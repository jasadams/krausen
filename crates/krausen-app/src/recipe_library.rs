// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authorized recipe library, archive and retained revision queries.
use crate::{
    ActorContext, Error, RequestIdentity, authorize_workspace, check_intent, record_change,
    record_request, recover_request,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use krausen_core::{DbPool, DbTx, canonical_json};
use krausen_types::recipe_library::{
    ArchiveRecipe, HistoryPage, HistoryQuery, LibraryOutcome, LibraryPage, LibraryQuery,
    RecipeSummary, RecoveryQuery, RevisionQuery, RevisionSummary,
};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cursor {
    pub(crate) version: u8,
    pub(crate) workspace: String,
    pub(crate) kind: String,
    pub(crate) filter: String,
    pub(crate) order: String,
    pub(crate) position: String,
    pub(crate) id: String,
}
pub(crate) fn decode(
    value: &str,
    actor: &ActorContext,
    kind: &str,
    filter: &str,
    order: &str,
) -> Result<Cursor, Error> {
    if value.len() > 2048 {
        return Err(Error::Invalid("cursor"));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| Error::Invalid("cursor"))?;
    let c: Cursor = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("cursor"))?;
    if c.version != 1
        || c.workspace != actor.workspace_id
        || c.kind != kind
        || c.filter != filter
        || c.order != order
    {
        return Err(Error::Invalid("cursor"));
    }
    Ok(c)
}
pub(crate) fn cursor(
    actor: &ActorContext,
    kind: &str,
    filter: &str,
    order: &str,
    position: String,
    id: String,
) -> Result<String, Error> {
    Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&Cursor {
        version: 1,
        workspace: actor.workspace_id.clone(),
        kind: kind.into(),
        filter: filter.into(),
        order: order.into(),
        position,
        id,
    })?))
}
pub(crate) fn limit(n: i64) -> Result<(), Error> {
    if !(1..=100).contains(&n) {
        Err(Error::Invalid("limit"))
    } else {
        Ok(())
    }
}
#[derive(sqlx::FromRow)]
struct SummaryRow {
    recipe_id: String,
    title: String,
    style: Option<String>,
    batch_size_l: Option<f64>,
    revision: i64,
    metadata_revision: i64,
    archived: i64,
    needs_recovery: i64,
    created_at: String,
}
impl From<SummaryRow> for RecipeSummary {
    fn from(r: SummaryRow) -> Self {
        Self {
            recipe_id: r.recipe_id,
            title: r.title,
            style: r.style,
            batch_size_l: r.batch_size_l,
            revision: r.revision,
            metadata_revision: r.metadata_revision,
            archived: r.archived != 0,
            needs_recovery: r.needs_recovery != 0,
            created_at: r.created_at,
        }
    }
}
async fn list_inner(
    db: &DbPool,
    actor: &ActorContext,
    q: LibraryQuery,
) -> Result<LibraryOutcome, Error> {
    limit(q.limit)?;
    let search = q.search.trim().to_owned();
    if search.chars().count() > 200 {
        return Err(Error::Invalid("search"));
    }
    let filter = serde_json::to_string(&(&search, q.archived))?;
    let mut tx = db.begin_write().await.map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &q.intent)?;
    let (position, id) = if let Some(c) = q.cursor.as_deref() {
        let c = decode(c, actor, "recipes", &filter, "created_at,id")?;
        (c.position, c.id)
    } else {
        ("0001-01-01 00:00:00".into(), String::new())
    };
    let predicate = if matches!(&tx, DbTx::Postgres(_)) {
        "(created_at > CAST($2 AS TIMESTAMPTZ) OR (created_at = CAST($2 AS TIMESTAMPTZ) AND recipe_id > $3)) AND POSITION(LOWER($4) IN LOWER(name)) > 0"
    } else {
        "(created_at > $2 OR (created_at = $2 AND recipe_id > $3)) AND instr(lower(name), lower($4)) > 0"
    };
    let sql = format!(
        "SELECT recipe_id,name AS title,style_name AS style,batch_size_l,native_head_revision AS revision,metadata_revision,CAST(CASE WHEN deleted_at IS NULL THEN 0 ELSE 1 END AS BIGINT) AS archived,CAST(CASE WHEN native_migration_error IS NULL THEN 0 ELSE 1 END AS BIGINT) AS needs_recovery,CAST(created_at AS TEXT) AS created_at FROM recipes WHERE workspace_id=$1 AND (native_head_revision>0 OR native_migration_error IS NOT NULL) AND (deleted_at IS NOT NULL)=$5 AND {predicate} ORDER BY created_at,recipe_id LIMIT $6"
    );
    let rows = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, SummaryRow>(&sql)
                .bind(&actor.workspace_id)
                .bind(&position)
                .bind(&id)
                .bind(&search)
                .bind(q.archived)
                .bind(q.limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, SummaryRow>(&sql)
                .bind(&actor.workspace_id)
                .bind(&position)
                .bind(&id)
                .bind(&search)
                .bind(q.archived)
                .bind(q.limit + 1)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(crate::db_error)?;
    let mut items: Vec<RecipeSummary> = rows.into_iter().map(Into::into).collect();
    let more = items.len() > q.limit as usize;
    items.truncate(q.limit as usize);
    let next_cursor = if more {
        let last = items.last().ok_or(Error::Invalid("cursor"))?;
        Some(cursor(
            actor,
            "recipes",
            &filter,
            "created_at,id",
            last.created_at.clone(),
            last.recipe_id.clone(),
        )?)
    } else {
        None
    };
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(LibraryOutcome::Recipes(LibraryPage { items, next_cursor }))
}
#[derive(sqlx::FromRow)]
struct RevisionRow {
    revision: i64,
    saved_by: Option<String>,
    saved_at: Option<String>,
    restored_from_revision: Option<i64>,
}
async fn history_inner(
    db: &DbPool,
    actor: &ActorContext,
    q: HistoryQuery,
) -> Result<LibraryOutcome, Error> {
    limit(q.limit)?;
    let mut tx = db.begin_write().await.map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &q.intent)?;
    let before = if let Some(c) = q.cursor.as_deref() {
        let c = decode(c, actor, "history", &q.recipe_id, "revision-desc")?;
        c.position
            .parse::<i64>()
            .map_err(|_| Error::Invalid("cursor"))?
    } else {
        i64::MAX
    };
    let exists = scalar!(&mut tx, i64, "SELECT native_head_revision FROM recipes WHERE workspace_id=$1 AND recipe_id=$2 AND native_head_revision>0", &actor.workspace_id, &q.recipe_id).map_err(crate::db_error)?;
    if exists.is_none() {
        return Ok(LibraryOutcome::NotFound);
    }
    let sql = "SELECT revision,saved_by,CAST(COALESCE(saved_at,migrated_at) AS TEXT) AS saved_at,restored_from_revision FROM recipe_revisions WHERE workspace_id=$1 AND recipe_id=$2 AND revision<$3 ORDER BY revision DESC LIMIT $4";
    let rows = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, RevisionRow>(sql)
                .bind(&actor.workspace_id)
                .bind(&q.recipe_id)
                .bind(before)
                .bind(q.limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, RevisionRow>(sql)
                .bind(&actor.workspace_id)
                .bind(&q.recipe_id)
                .bind(before)
                .bind(q.limit + 1)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(crate::db_error)?;
    let mut items: Vec<RevisionSummary> = rows
        .into_iter()
        .map(|r| RevisionSummary {
            revision: r.revision,
            saved_by: r.saved_by,
            saved_at: r.saved_at,
            restored_from_revision: r.restored_from_revision,
        })
        .collect();
    let more = items.len() > q.limit as usize;
    items.truncate(q.limit as usize);
    let next_cursor = if more {
        Some(cursor(
            actor,
            "history",
            &q.recipe_id,
            "revision-desc",
            items
                .last()
                .ok_or(Error::Invalid("cursor"))?
                .revision
                .to_string(),
            String::new(),
        )?)
    } else {
        None
    };
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(LibraryOutcome::History(HistoryPage { items, next_cursor }))
}
async fn revision_inner(
    db: &DbPool,
    actor: &ActorContext,
    q: RevisionQuery,
) -> Result<LibraryOutcome, Error> {
    if q.revision < 1 {
        return Err(Error::Invalid("revision"));
    }
    let mut tx = db.begin_write().await.map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &q.intent)?;
    let result = krausen_brewery::native_store::load_document(
        &mut tx,
        &actor.workspace_id,
        &q.recipe_id,
        q.revision,
    )
    .await
    .map_err(|_| Error::Invalid("stored revision unavailable"))?;
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(match result {
        Some(doc) => LibraryOutcome::Revision(doc),
        None => LibraryOutcome::NotFound,
    })
}
#[derive(sqlx::FromRow)]
struct ArchiveRow {
    metadata_revision: i64,
    archived: i64,
}
async fn archive_inner(
    db: &DbPool,
    actor: &ActorContext,
    q: ArchiveRecipe,
) -> Result<LibraryOutcome, Error> {
    uuid::Uuid::parse_str(&q.request_id).map_err(|_| Error::Invalid("request id"))?;
    if q.recipe_id.is_empty() || q.recipe_id.len() > 256 || q.expected_metadata_revision < 0 {
        return Err(Error::Invalid("archive request"));
    }
    let payload = canonical_json::from_serializable(&(
        &q.intent,
        &q.recipe_id,
        q.expected_metadata_revision,
        q.archived,
    ))
    .map_err(|_| Error::Invalid("request"))?;
    let digest = canonical_json::sha256(&payload).map_err(|_| Error::Invalid("request"))?;
    let mut tx = db.begin_write().await.map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &q.intent)?;
    let ws = if matches!(&tx, DbTx::Postgres(_)) {
        "SELECT workspace_id FROM workspaces WHERE workspace_id=$1 AND status='active' FOR UPDATE"
    } else {
        "SELECT workspace_id FROM workspaces WHERE workspace_id=$1 AND status='active'"
    };
    if scalar!(&mut tx, String, ws, &actor.workspace_id)
        .map_err(crate::db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    authorize_workspace(&mut tx, actor).await?;
    let key = RequestIdentity {
        actor,
        operation: "recipe.archive.v1",
        request_id: &q.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        let outcome = serde_json::from_value(result)?;
        tx.commit().await.map_err(crate::boundary_error)?;
        return Ok(outcome);
    }
    let sql = format!(
        "SELECT metadata_revision,CAST(CASE WHEN deleted_at IS NULL THEN 0 ELSE 1 END AS BIGINT) AS archived FROM recipes WHERE workspace_id=$1 AND recipe_id=$2 AND native_head_revision>0{}",
        if matches!(&tx, DbTx::Postgres(_)) {
            " FOR UPDATE"
        } else {
            ""
        }
    );
    let row = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, ArchiveRow>(&sql)
                .bind(&actor.workspace_id)
                .bind(&q.recipe_id)
                .fetch_optional(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, ArchiveRow>(&sql)
                .bind(&actor.workspace_id)
                .bind(&q.recipe_id)
                .fetch_optional(&mut **t)
                .await
        }
    }
    .map_err(crate::db_error)?;
    authorize_workspace(&mut tx, actor).await?;
    let Some(row) = row else {
        return Ok(LibraryOutcome::NotFound);
    };
    if row.metadata_revision != q.expected_metadata_revision {
        return Ok(LibraryOutcome::Conflict {
            expected: q.expected_metadata_revision,
            actual: row.metadata_revision,
        });
    }
    let changed = (row.archived != 0) != q.archived;
    let revision = if changed {
        krausen_brewery::native_store::set_archived(
            &mut tx,
            &actor.workspace_id,
            &q.recipe_id,
            row.metadata_revision,
            q.archived.then(chrono::Utc::now),
        )
        .await
        .map_err(crate::boundary_error)?
    } else {
        row.metadata_revision
    };
    let outcome = LibraryOutcome::Archive {
        recipe_id: q.recipe_id.clone(),
        metadata_revision: revision,
        archived: q.archived,
    };
    if changed {
        let sql = "INSERT INTO recipe_archive_events (workspace_id,recipe_id,metadata_revision,archived,actor_id,recorded_at) VALUES ($1,$2,$3,$4,$5,$6)";
        let now = chrono::Utc::now().timestamp();
        match &mut tx {
            DbTx::Postgres(t) => sqlx::query(sql)
                .bind(&actor.workspace_id)
                .bind(&q.recipe_id)
                .bind(revision)
                .bind(q.archived)
                .bind(&actor.actor_id)
                .bind(now)
                .execute(&mut **t)
                .await
                .map(|result| result.rows_affected()),
            DbTx::Sqlite(t) => sqlx::query(sql)
                .bind(&actor.workspace_id)
                .bind(&q.recipe_id)
                .bind(revision)
                .bind(q.archived)
                .bind(&actor.actor_id)
                .bind(now)
                .execute(&mut **t)
                .await
                .map(|result| result.rows_affected()),
        }
        .map_err(crate::db_error)?;
        record_change(&mut tx, actor, "recipe.archive.v1", &q.recipe_id, revision).await?;
    }
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&q.recipe_id),
    )
    .await?;
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(outcome)
}
fn project(result: Result<LibraryOutcome, Error>) -> LibraryOutcome {
    match result {
        Ok(value) => value,
        Err(Error::Forbidden) => LibraryOutcome::Forbidden,
        Err(Error::ContextChanged) => LibraryOutcome::ContextChanged,
        Err(Error::Invalid(_)) => LibraryOutcome::Invalid,
        Err(Error::RequestIdentityConflict) => LibraryOutcome::RequestIdentityConflict,
        Err(Error::ResultExpired { .. }) => LibraryOutcome::ResultExpired,
        Err(error) => {
            tracing::warn!(error=%error, "recipe library unavailable");
            LibraryOutcome::Unavailable
        }
    }
}
pub async fn list(db: &DbPool, actor: &ActorContext, query: LibraryQuery) -> LibraryOutcome {
    project(list_inner(db, actor, query).await)
}
pub async fn history(db: &DbPool, actor: &ActorContext, query: HistoryQuery) -> LibraryOutcome {
    project(history_inner(db, actor, query).await)
}
pub async fn revision(db: &DbPool, actor: &ActorContext, query: RevisionQuery) -> LibraryOutcome {
    project(revision_inner(db, actor, query).await)
}
pub async fn archive(db: &DbPool, actor: &ActorContext, query: ArchiveRecipe) -> LibraryOutcome {
    project(archive_inner(db, actor, query).await)
}

/// Preserve the existing authorized legacy-source recovery operation while
/// enforcing the browser's pinned account/workspace precondition.
pub async fn recover(db: &DbPool, actor: &ActorContext, query: RecoveryQuery) -> LibraryOutcome {
    if let Err(error) = check_intent(actor, &query.intent) {
        return project(Err(error));
    }
    match crate::recipe_migration::recover_legacy_recipe(db, actor, &query.recipe_id).await {
        Ok(Some((source, diagnostic))) => LibraryOutcome::LegacySource { source, diagnostic },
        Ok(None) => LibraryOutcome::NotFound,
        Err(error) => project(Err(error)),
    }
}
