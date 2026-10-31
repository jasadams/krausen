// SPDX-License-Identifier: AGPL-3.0-or-later

//! Expiry comparison against a real SQLite engine.
//!
//! The shape of the SQL string is not the interesting part — the bug this
//! guards against passed every string assertion. SQLite stores timestamps as
//! text, the codebase writes several different shapes of it, and comparing
//! those shapes lexically gives the wrong answer for anything that expired
//! earlier the same day. Only running the comparison catches that.

use krausen_core::sql_compat;
use sqlx::{Connection, Row, SqliteConnection};

/// The timestamp shapes this codebase actually writes into SQLite.
///
/// `chrono` RFC3339 with an offset is what the token services insert; the `Z`
/// form comes from column defaults; the space-separated form is what
/// `datetime('now')` itself produces.
const SHAPES: &[(&str, &str)] = &[
    ("rfc3339 with offset", "%Y-%m-%dT%H:%M:%S.000000+00:00"),
    ("iso with Z", "%Y-%m-%dT%H:%M:%SZ"),
    ("sqlite datetime()", "%Y-%m-%d %H:%M:%S"),
];

async fn evaluate(conn: &mut SqliteConnection, expires_at: &str) -> bool {
    let predicate = sql_compat::is_future(false, "?");
    let sql = format!("SELECT {predicate} AS still_valid");
    sqlx::query(&sql)
        .bind(expires_at)
        .fetch_one(conn)
        .await
        .expect("query runs")
        .get::<i64, _>("still_valid")
        == 1
}

/// Format an instant offset from now by `hours`, in the given strftime shape.
async fn stamp(conn: &mut SqliteConnection, shape: &str, hours: i64) -> String {
    let modifier = format!("{hours:+} hours");
    sqlx::query("SELECT strftime(?, datetime('now', ?)) AS t")
        .bind(shape)
        .bind(&modifier)
        .fetch_one(conn)
        .await
        .expect("query runs")
        .get("t")
}

#[tokio::test]
async fn an_expiry_an_hour_ago_is_not_in_the_future() {
    let mut conn = SqliteConnection::connect(":memory:").await.unwrap();

    for (label, shape) in SHAPES {
        let past = stamp(&mut conn, shape, -1).await;
        assert!(
            !evaluate(&mut conn, &past).await,
            "{label}: {past} expired an hour ago but read as still valid"
        );
    }
}

#[tokio::test]
async fn an_expiry_later_today_is_in_the_future() {
    let mut conn = SqliteConnection::connect(":memory:").await.unwrap();

    for (label, shape) in SHAPES {
        let soon = stamp(&mut conn, shape, 1).await;
        assert!(
            evaluate(&mut conn, &soon).await,
            "{label}: {soon} expires in an hour but read as already expired"
        );
    }
}

#[tokio::test]
async fn expiry_on_other_days_still_works() {
    let mut conn = SqliteConnection::connect(":memory:").await.unwrap();

    for (label, shape) in SHAPES {
        let last_week = stamp(&mut conn, shape, -24 * 7).await;
        let next_week = stamp(&mut conn, shape, 24 * 7).await;
        assert!(!evaluate(&mut conn, &last_week).await, "{label}: last week");
        assert!(evaluate(&mut conn, &next_week).await, "{label}: next week");
    }
}

/// The comparison this replaced, kept so the regression stays legible: a bare
/// `column > datetime('now')` accepts a credential that expired earlier today.
#[tokio::test]
async fn the_naive_comparison_is_why_this_helper_exists() {
    let mut conn = SqliteConnection::connect(":memory:").await.unwrap();

    let past = stamp(&mut conn, "%Y-%m-%dT%H:%M:%S.000000+00:00", -1).await;
    let naive: i64 = sqlx::query("SELECT (? > datetime('now')) AS still_valid")
        .bind(&past)
        .fetch_one(&mut conn)
        .await
        .unwrap()
        .get("still_valid");

    assert_eq!(
        naive, 1,
        "the naive comparison used to accept {past}; if this now fails the \
         storage format changed and is_future may no longer be needed"
    );
    assert!(!evaluate(&mut conn, &past).await, "is_future must reject it");
}
