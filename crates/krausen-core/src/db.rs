// SPDX-License-Identifier: AGPL-3.0-or-later

//! Database pool abstraction — supports Postgres and SQLite at runtime.

use std::str::FromStr;

use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use tracing::Instrument;

/// Runtime-selected database pool.
///
/// `DATABASE_URL` prefix determines which backend is used:
/// - `postgresql://` or `postgres://` → Postgres
/// - Anything else (e.g. `sqlite://path.db`) → SQLite
#[derive(Clone, Debug)]
pub enum DbPool {
    Postgres(PgPool),
    Sqlite(SqlitePool),
}

/// A single application-owned write transaction. Helpers borrow this value and
/// must not begin, commit, or acquire another connection.
pub enum DbTx<'a> {
    Postgres(sqlx::Transaction<'a, sqlx::Postgres>),
    Sqlite(sqlx::Transaction<'a, sqlx::Sqlite>),
}

impl DbTx<'_> {
    pub async fn commit(self) -> crate::Result<()> {
        match self {
            Self::Postgres(tx) => tx.commit().await?,
            Self::Sqlite(tx) => tx.commit().await?,
        }
        Ok(())
    }

    pub async fn rollback(self) -> crate::Result<()> {
        match self {
            Self::Postgres(tx) => tx.rollback().await?,
            Self::Sqlite(tx) => tx.rollback().await?,
        }
        Ok(())
    }
}

impl DbPool {
    /// Acquire the write boundary before any authorization or source reads.
    pub async fn begin_write(&self) -> crate::Result<DbTx<'_>> {
        Ok(match self {
            Self::Postgres(pool) => DbTx::Postgres(pool.begin().await?),
            Self::Sqlite(pool) => {
                // sqlx's custom-statement `begin_with` awaits again after `BEGIN IMMEDIATE` has
                // run and before the `Transaction` guard exists. A caller dropped there (an axum
                // handler is dropped when its client disconnects) would leave the pool's only
                // connection inside an open transaction with nothing to roll it back, and every
                // later `begin_write` would fail with "non-zero transaction depth" until restart.
                // Running the begin on its own task lets it always finish; if nobody is waiting
                // any more the finished guard is simply dropped, which rolls back normally.
                //
                // Caveat, accepted: a caller dropped while still QUEUED for the single
                // connection leaves its spawned begin in the pool's queue. It later takes the
                // connection, runs `BEGIN IMMEDIATE` and the finished guard is dropped, which
                // rolls back immediately. That wasted work is short and bounded by the pool's
                // acquire timeout (30 s default, sqlx `acquire_timeout`) for a task that never
                // got a connection. Aborting the task instead would reintroduce the bug
                // whenever the abort lands between BEGIN and the guard's creation.
                let pool = pool.clone();
                let begun = tokio::spawn(
                    async move { pool.begin_with("BEGIN IMMEDIATE").await }.in_current_span(),
                )
                .await
                .map_err(|error| {
                    crate::Error::Internal(format!("write transaction task failed: {error}"))
                })?;
                DbTx::Sqlite(begun?)
            }
        })
    }
    /// Connect to the database. The composition root runs migrations.
    pub async fn connect(url: &str) -> crate::Result<Self> {
        if url.starts_with("postgresql://") || url.starts_with("postgres://") {
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .acquire_timeout(std::time::Duration::from_secs(5))
                .connect(url)
                .await?;
            tracing::info!("PostgreSQL pool connected");
            Ok(Self::Postgres(pool))
        } else {
            // SQLite WAL mode serialises writes; a single connection avoids contention.
            let opts = sqlx::sqlite::SqliteConnectOptions::from_str(url)?.create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(opts)
                .await?;
            sqlx::query("PRAGMA journal_mode=WAL")
                .execute(&pool)
                .await
                .map_err(crate::Error::Sqlx)?;
            sqlx::query("PRAGMA foreign_keys=ON")
                .execute(&pool)
                .await
                .map_err(crate::Error::Sqlx)?;
            tracing::info!("SQLite pool connected");
            Ok(Self::Sqlite(pool))
        }
    }

    pub fn is_postgres(&self) -> bool {
        matches!(self, Self::Postgres(_))
    }

    pub fn is_sqlite(&self) -> bool {
        matches!(self, Self::Sqlite(_))
    }

    /// Extract the inner `PgPool` for Postgres-only code paths.
    ///
    /// Panics if called on a SQLite pool.
    pub fn pg_pool(&self) -> &PgPool {
        match self {
            Self::Postgres(pg) => pg,
            Self::Sqlite(_) => panic!("pg_pool() called on SQLite pool"),
        }
    }
}

/// Backwards-compatible pool constructor for tests.
pub async fn create_pool(url: &str) -> crate::Result<DbPool> {
    DbPool::connect(url).await
}

/// Run a quick connectivity check — useful for health endpoints.
pub async fn ping(pool: &DbPool) -> crate::Result<()> {
    match pool {
        DbPool::Postgres(pg) => {
            sqlx::query_scalar::<_, i32>("SELECT 1")
                .fetch_one(pg)
                .await?;
        }
        DbPool::Sqlite(sq) => {
            sqlx::query_scalar::<_, i32>("SELECT 1")
                .fetch_one(sq)
                .await?;
        }
    }
    Ok(())
}

/// Backend-agnostic query result for `db_execute!`.
///
/// Wraps the `rows_affected()` value from either `PgQueryResult` or
/// `SqliteQueryResult` so callers don't need to care about the backend.
pub struct DbQueryResult {
    rows: u64,
}

/// Execute on a borrowed application transaction without acquiring a pool lease.
#[macro_export]
macro_rules! dbtx_execute {
    ($tx:expr, $query:expr $(, $bind:expr)*) => {
        match $tx {
            $crate::DbTx::Postgres(tx) => sqlx::query($query)$(.bind($bind))*.execute(&mut **tx).await.map($crate::db::DbQueryResult::from_pg),
            $crate::DbTx::Sqlite(tx) => sqlx::query($query)$(.bind($bind))*.execute(&mut **tx).await.map($crate::db::DbQueryResult::from_sqlite),
        }
    }
}

#[macro_export]
macro_rules! dbtx_fetch_optional {
    ($tx:expr, $type:ty, $query:expr $(, $bind:expr)*) => {
        match $tx {
            $crate::DbTx::Postgres(tx) => sqlx::query_as::<_, $type>($query)$(.bind($bind))*.fetch_optional(&mut **tx).await,
            $crate::DbTx::Sqlite(tx) => sqlx::query_as::<_, $type>($query)$(.bind($bind))*.fetch_optional(&mut **tx).await,
        }
    }
}

#[macro_export]
macro_rules! dbtx_fetch_scalar {
    ($tx:expr, $type:ty, $query:expr $(, $bind:expr)*) => {
        match $tx {
            $crate::DbTx::Postgres(tx) => sqlx::query_scalar::<_, $type>($query)$(.bind($bind))*.fetch_one(&mut **tx).await,
            $crate::DbTx::Sqlite(tx) => sqlx::query_scalar::<_, $type>($query)$(.bind($bind))*.fetch_one(&mut **tx).await,
        }
    }
}

impl DbQueryResult {
    pub fn from_pg(r: sqlx::postgres::PgQueryResult) -> Self {
        Self {
            rows: r.rows_affected(),
        }
    }
    pub fn from_sqlite(r: sqlx::sqlite::SqliteQueryResult) -> Self {
        Self {
            rows: r.rows_affected(),
        }
    }
    pub fn rows_affected(&self) -> u64 {
        self.rows
    }
}

/// Execute a query that returns typed rows via `query_as`. Fetches all rows.
#[macro_export]
macro_rules! db_fetch_all {
    ($pool:expr, $type:ty, $query:expr $(, $bind:expr)*) => {
        match &$pool {
            $crate::db::DbPool::Postgres(pg) =>
                sqlx::query_as::<_, $type>($query)$(.bind($bind))*.fetch_all(pg).await,
            $crate::db::DbPool::Sqlite(sq) =>
                sqlx::query_as::<_, $type>($query)$(.bind($bind))*.fetch_all(sq).await,
        }
    }
}

/// Fetch exactly one row.
#[macro_export]
macro_rules! db_fetch_one {
    ($pool:expr, $type:ty, $query:expr $(, $bind:expr)*) => {
        match &$pool {
            $crate::db::DbPool::Postgres(pg) =>
                sqlx::query_as::<_, $type>($query)$(.bind($bind))*.fetch_one(pg).await,
            $crate::db::DbPool::Sqlite(sq) =>
                sqlx::query_as::<_, $type>($query)$(.bind($bind))*.fetch_one(sq).await,
        }
    }
}

/// Fetch zero or one row.
#[macro_export]
macro_rules! db_fetch_optional {
    ($pool:expr, $type:ty, $query:expr $(, $bind:expr)*) => {
        match &$pool {
            $crate::db::DbPool::Postgres(pg) =>
                sqlx::query_as::<_, $type>($query)$(.bind($bind))*.fetch_optional(pg).await,
            $crate::db::DbPool::Sqlite(sq) =>
                sqlx::query_as::<_, $type>($query)$(.bind($bind))*.fetch_optional(sq).await,
        }
    }
}

/// Execute a query without returning rows (INSERT, UPDATE, DELETE).
///
/// Returns `Result<DbQueryResult, sqlx::Error>` which provides
/// `rows_affected()` regardless of the backend.
#[macro_export]
macro_rules! db_execute {
    ($pool:expr, $query:expr $(, $bind:expr)*) => {
        match &$pool {
            $crate::db::DbPool::Postgres(pg) =>
                sqlx::query($query)$(.bind($bind))*.execute(pg).await
                    .map($crate::db::DbQueryResult::from_pg),
            $crate::db::DbPool::Sqlite(sq) =>
                sqlx::query($query)$(.bind($bind))*.execute(sq).await
                    .map($crate::db::DbQueryResult::from_sqlite),
        }
    }
}

/// Dispatch to whichever pool variant is active.
///
/// Use this when both the Postgres and SQLite arms would be **identical**
/// except for the pool variable name.  The closure receives `p` which is
/// either a `&PgPool` or a `&SqlitePool`.
///
/// ```rust,ignore
/// let row = db_with_pool!(pool, |p| {
///     sqlx::query("SELECT 1").fetch_one(p).await?
/// });
/// ```
#[macro_export]
macro_rules! db_with_pool {
    ($pool:expr, |$p:ident| $body:expr) => {
        match &$pool {
            $crate::db::DbPool::Postgres($p) => $body,
            $crate::db::DbPool::Sqlite($p) => $body,
        }
    };
}

/// Fetch a single scalar value.
#[macro_export]
macro_rules! db_fetch_scalar {
    ($pool:expr, $type:ty, $query:expr $(, $bind:expr)*) => {
        match &$pool {
            $crate::db::DbPool::Postgres(pg) =>
                sqlx::query_scalar::<_, $type>($query)$(.bind($bind))*.fetch_one(pg).await,
            $crate::db::DbPool::Sqlite(sq) =>
                sqlx::query_scalar::<_, $type>($query)$(.bind($bind))*.fetch_one(sq).await,
        }
    }
}

/// Build a SQL IN clause with numbered placeholders starting at `start_idx`.
/// Returns the clause `($N, $N+1, ...)` and the next available index.
pub fn in_clause_placeholders(count: usize, start_idx: usize) -> (String, usize) {
    let placeholders: Vec<String> = (0..count).map(|i| format!("${}", start_idx + i)).collect();
    let clause = format!("({})", placeholders.join(", "));
    (clause, start_idx + count)
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use std::time::Duration;

    /// The request future that owns `begin_write` is dropped when a client disconnects.
    /// Dropping it at ANY point must leave the pool usable: the single SQLite connection
    /// must never be handed back while still inside the `BEGIN IMMEDIATE` it started.
    ///
    /// The future is polled by hand: after `polls` polls (letting the connection's worker
    /// thread run for `pause` between polls) it is dropped, which is exactly what axum does
    /// to a handler when the client goes away.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelling_begin_write_never_leaves_the_pooled_connection_in_a_transaction() {
        let db = DbPool::connect("sqlite::memory:").await.unwrap();
        let mut cancelled_midway = 0;
        for polls in 1..=8 {
            for pause_micros in [0_u64, 5, 20, 50, 100, 250, 500, 1_000] {
                for _ in 0..5 {
                    let mut begin = Box::pin(db.begin_write());
                    let mut finished = false;
                    for _ in 0..polls {
                        if let std::task::Poll::Ready(result) = futures_util::poll!(begin.as_mut()) {
                            result.unwrap().rollback().await.unwrap();
                            finished = true;
                            break;
                        }
                        // Deliberately blocking: the connection's worker is an OS thread, and the
                        // window being probed is microseconds wide. `tokio::time::sleep` has ~1 ms
                        // granularity and would only ever cancel outside it; blocking this worker
                        // (the test runs on a multi-thread runtime, so nothing else depends on it)
                        // lets the worker thread run for exactly `pause_micros` between polls.
                        std::thread::sleep(Duration::from_micros(pause_micros));
                    }
                    if !finished {
                        cancelled_midway += 1;
                    }
                    drop(begin);
                    let next = tokio::time::timeout(Duration::from_secs(5), db.begin_write())
                        .await
                        .expect("the pool must not hang after a cancelled begin_write");
                    match next {
                        Ok(tx) => tx.rollback().await.unwrap(),
                        Err(error) => panic!(
                            "begin_write failed after one cancelled after {polls} poll(s) \
                             with {pause_micros}us pauses: {error}"
                        ),
                    }
                }
            }
        }
        assert!(cancelled_midway > 0, "the test never actually cancelled a begin_write");
    }
}
