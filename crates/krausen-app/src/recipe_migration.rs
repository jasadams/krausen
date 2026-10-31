// SPDX-License-Identifier: AGPL-3.0-or-later

//! Startup conversion and authenticated access to failed legacy payloads.
use crate::{ActorContext, Error, authorize_workspace};
use krausen_core::{DbPool, DbTx};

#[derive(Debug, Default)]
pub struct MigrationReport {
    pub migrated: usize,
    pub failed: Vec<(String, String)>,
}

/// Reject legacy create/import only after checking the caller's current access.
pub async fn reject_legacy_authoring(pool: &DbPool, actor: &ActorContext) -> Result<(), Error> {
    let mut tx = pool.begin_write().await.map_err(Error::Database)?;
    authorize_workspace(&mut tx, actor).await?;
    tx.rollback().await.map_err(Error::Database)?;
    Err(Error::LegacyAuthoringUnavailable)
}

/// Run after schema migrations and before serving. Each source row is committed
/// independently; malformed source stays untouched with a recoverable marker.
pub async fn backfill_legacy_recipes(pool: &DbPool) -> Result<MigrationReport, Error> {
    let sql = "SELECT workspace_id, recipe_id FROM recipes WHERE native_head_revision = 0 ORDER BY workspace_id, recipe_id";
    let rows: Vec<(String, String)> = match pool {
        DbPool::Postgres(p) => sqlx::query_as(sql).fetch_all(p).await,
        DbPool::Sqlite(p) => sqlx::query_as(sql).fetch_all(p).await,
    }
    .map_err(|e| Error::Database(e.into()))?;
    let mut report = MigrationReport::default();
    for (workspace, recipe) in rows {
        let mut tx = pool.begin_write().await.map_err(Error::Database)?;
        match krausen_brewery::native_store::backfill(
            &mut tx,
            &workspace,
            &recipe,
            chrono::Utc::now(),
        )
        .await
        {
            Ok(changed) => {
                tx.commit().await.map_err(Error::Database)?;
                if changed {
                    report.migrated += 1;
                }
            }
            Err(krausen_core::Error::BadRequest(_)) => {
                tx.rollback().await.map_err(Error::Database)?;
                let mut mark = pool.begin_write().await.map_err(Error::Database)?;
                let sql = "UPDATE recipes SET native_migration_error = $1 WHERE workspace_id = $2 AND recipe_id = $3 AND native_head_revision = 0";
                let message =
                    "Native migration failed; original legacy record is available for recovery";
                match &mut mark {
                    DbTx::Postgres(t) => sqlx::query(sql)
                        .bind(message)
                        .bind(&workspace)
                        .bind(&recipe)
                        .execute(&mut **t)
                        .await
                        .map(|r| r.rows_affected()),
                    DbTx::Sqlite(t) => sqlx::query(sql)
                        .bind(message)
                        .bind(&workspace)
                        .bind(&recipe)
                        .execute(&mut **t)
                        .await
                        .map(|r| r.rows_affected()),
                }
                .map_err(|e| Error::Database(e.into()))?;
                mark.commit().await.map_err(Error::Database)?;
                report.failed.push((workspace, recipe));
            }
            Err(error) => {
                tx.rollback().await.map_err(Error::Database)?;
                return Err(Error::Database(error));
            }
        }
    }
    Ok(report)
}

/// Recover raw source only after rechecking current membership within DbTx.
pub async fn recover_legacy_recipe(
    pool: &DbPool,
    actor: &ActorContext,
    recipe: &str,
) -> Result<Option<(String, String)>, Error> {
    let mut tx = pool.begin_write().await.map_err(Error::Database)?;
    authorize_workspace(&mut tx, actor).await?;
    let sql = "SELECT document, native_migration_error FROM recipes WHERE workspace_id = $1 AND recipe_id = $2 AND native_head_revision = 0 AND native_migration_error IS NOT NULL";
    let row = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, (String, String)>(sql)
                .bind(&actor.workspace_id)
                .bind(recipe)
                .fetch_optional(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, (String, String)>(sql)
                .bind(&actor.workspace_id)
                .bind(recipe)
                .fetch_optional(&mut **t)
                .await
        }
    }
    .map_err(|e| Error::Database(e.into()))?;
    tx.commit().await.map_err(Error::Database)?;
    Ok(row)
}
