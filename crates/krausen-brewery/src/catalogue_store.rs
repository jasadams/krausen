// SPDX-License-Identifier: AGPL-3.0-or-later
//! Workspace-scoped catalogue helpers on the application's transaction.
//!
//! Starter rows are the explicit global-read exception and carry no workspace
//! ownership; workspace rows are always scoped to the effective workspace.
use krausen_core::{DbTx, Error, Result};
use krausen_types::catalogue::{
    CatalogueFilter, CatalogueScope, IngredientEntry, IngredientEntryDocumentV1,
    IngredientProvenance,
};
use krausen_types::brewery::IngredientSource;

macro_rules! execute { ($tx:expr,$sql:expr $(,$arg:expr)* $(,)?) => {{ match &mut *$tx {
    DbTx::Postgres(t)=>sqlx::query($sql)$(.bind($arg))*.execute(&mut **t).await?.rows_affected(),
    DbTx::Sqlite(t)=>sqlx::query($sql)$(.bind($arg))*.execute(&mut **t).await?.rows_affected(),
}}}; }

#[derive(sqlx::FromRow)]
struct Row {
    scope: String,
    workspace_id: Option<String>,
    entry_id: String,
    revision: i64,
    metadata_revision: i64,
    archived: i64,
    created_at: i64,
    document_json: String,
    provenance_json: String,
    saved_by: Option<String>,
    saved_at: i64,
}

impl Row {
    fn into_entry(self) -> Result<IngredientEntry> {
        let document: IngredientEntryDocumentV1 = serde_json::from_str(&self.document_json)?;
        if document.schema_version != 1 {
            return Err(Error::BadRequest("unsupported ingredient schema".into()));
        }
        let scope = match self.scope.as_str() {
            "starter" => CatalogueScope::Starter,
            "workspace" => CatalogueScope::Workspace,
            other => {
                return Err(Error::BadRequest(format!("unknown catalogue scope {other}")));
            }
        };
        Ok(IngredientEntry {
            scope,
            workspace_id: self.workspace_id,
            entry_id: self.entry_id,
            created_at: self.created_at,
            revision: self.revision,
            metadata_revision: self.metadata_revision,
            archived: self.archived != 0,
            document,
            provenance: serde_json::from_str(&self.provenance_json)?,
            saved_by: self.saved_by,
            saved_at: self.saved_at,
        })
    }
}

const SELECT_UNION: &str = "SELECT 'starter' AS scope, NULL AS workspace_id, entry_id, name, producer, category, version AS revision, CAST(1 AS BIGINT) AS metadata_revision, CAST(CASE WHEN archived THEN 1 ELSE 0 END AS BIGINT) AS archived, created_at, document_json, provenance_json, NULL AS saved_by, created_at AS saved_at FROM starter_ingredient_entries WHERE $1 AND NOT archived AND (created_at > $2 OR (created_at = $2 AND entry_id > $3)) AND (lower(name) LIKE $4 ESCAPE '\\' OR lower(producer) LIKE $4 ESCAPE '\\' OR lower(category) LIKE $4 ESCAPE '\\') UNION ALL SELECT 'workspace' AS scope, e.workspace_id, e.entry_id, e.name, e.producer, e.category, e.current_revision AS revision, e.metadata_revision, CAST(CASE WHEN e.archived THEN 1 ELSE 0 END AS BIGINT) AS archived, e.created_at, r.document_json, r.provenance_json, r.actor_id AS saved_by, r.recorded_at AS saved_at FROM ingredient_entries e JOIN ingredient_revisions r ON r.workspace_id = e.workspace_id AND r.entry_id = e.entry_id AND r.revision = e.current_revision WHERE e.workspace_id = $5 AND NOT e.archived AND (e.created_at > $2 OR (e.created_at = $2 AND e.entry_id > $3)) AND (lower(e.name) LIKE $4 ESCAPE '\\' OR lower(e.producer) LIKE $4 ESCAPE '\\' OR lower(e.category) LIKE $4 ESCAPE '\\') ORDER BY created_at, entry_id LIMIT $6";

/// Escape LIKE metacharacters so a search term is a literal substring match.
fn like_pattern(query: &str) -> String {
    let lowered = query.to_lowercase();
    let mut out = String::with_capacity(lowered.len() + 2);
    out.push('%');
    for ch in lowered.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
    out
}

/// Bounded search across starter and workspace entries in stable order.
/// Workspace rows are always scoped; starter rows follow the filter.
pub async fn search(
    tx: &mut DbTx<'_>,
    workspace: &str,
    filter: CatalogueFilter,
    query: &str,
    after_created_at: i64,
    after_id: &str,
    limit: i64,
) -> Result<Vec<IngredientEntry>> {
    let pattern = like_pattern(query);
    let include_starter = filter == CatalogueFilter::All;
    let rows = match &mut *tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, Row>(SELECT_UNION)
                .bind(include_starter)
                .bind(after_created_at)
                .bind(after_id)
                .bind(&pattern)
                .bind(workspace)
                .bind(limit)
                .fetch_all(&mut **t)
                .await?
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, Row>(SELECT_UNION)
                .bind(include_starter)
                .bind(after_created_at)
                .bind(after_id)
                .bind(&pattern)
                .bind(workspace)
                .bind(limit)
                .fetch_all(&mut **t)
                .await?
        }
    };
    rows.into_iter().map(Row::into_entry).collect()
}

const SELECT_ENTRY: &str = "SELECT 'workspace' AS scope, e.workspace_id, e.entry_id, e.name, e.producer, e.category, e.current_revision AS revision, e.metadata_revision, CAST(CASE WHEN e.archived THEN 1 ELSE 0 END AS BIGINT) AS archived, e.created_at, r.document_json, r.provenance_json, r.actor_id AS saved_by, r.recorded_at AS saved_at FROM ingredient_entries e JOIN ingredient_revisions r ON r.workspace_id = e.workspace_id AND r.entry_id = e.entry_id AND r.revision = e.current_revision WHERE e.workspace_id = $1 AND e.entry_id = $2";

const SELECT_STARTER: &str = "SELECT 'starter' AS scope, NULL AS workspace_id, entry_id, name, producer, category, version AS revision, CAST(1 AS BIGINT) AS metadata_revision, CAST(CASE WHEN archived THEN 1 ELSE 0 END AS BIGINT) AS archived, created_at, document_json, provenance_json, NULL AS saved_by, created_at AS saved_at FROM starter_ingredient_entries WHERE entry_id = $1";

async fn fetch_one(tx: &mut DbTx<'_>, sql: &str, a: &str, b: Option<&str>) -> Result<Option<IngredientEntry>> {
    let row = match &mut *tx {
        DbTx::Postgres(t) => {
            let q = sqlx::query_as::<_, Row>(sql).bind(a);
            let q = if let Some(b) = b { q.bind(b) } else { q };
            q.fetch_optional(&mut **t).await?
        }
        DbTx::Sqlite(t) => {
            let q = sqlx::query_as::<_, Row>(sql).bind(a);
            let q = if let Some(b) = b { q.bind(b) } else { q };
            q.fetch_optional(&mut **t).await?
        }
    };
    row.map(Row::into_entry).transpose()
}

/// Workspace-scoped entry lookup; never crosses workspace membership.
pub async fn load_entry(
    tx: &mut DbTx<'_>,
    workspace: &str,
    id: &str,
    lock: bool,
) -> Result<Option<IngredientEntry>> {
    if lock {
        if let DbTx::Postgres(t) = &mut *tx {
            let _locked = sqlx::query(
                "SELECT entry_id FROM ingredient_entries WHERE workspace_id=$1 AND entry_id=$2 FOR UPDATE",
            )
            .bind(workspace)
            .bind(id)
            .fetch_optional(&mut **t)
            .await?;
        }
    }
    fetch_one(tx, SELECT_ENTRY, workspace, Some(id)).await
}

/// Starter entry lookup (global-read exception; no workspace argument).
pub async fn load_starter(tx: &mut DbTx<'_>, id: &str) -> Result<Option<IngredientEntry>> {
    fetch_one(tx, SELECT_STARTER, id, None).await
}

/// Resolve a recipe row's ingredient source. Workspace sources are scoped to
/// the effective workspace and their aggregate row is locked while new copies
/// are being validated; starter sources are the explicit global exception.
/// Unknown scopes resolve to nothing rather than widening access.
pub async fn resolve_source(
    tx: &mut DbTx<'_>,
    workspace: &str,
    source: &IngredientSource,
) -> Result<Option<IngredientEntry>> {
    match source.scope.as_str() {
        "starter" => load_starter(tx, &source.entry_id).await,
        "workspace" => load_entry(tx, workspace, &source.entry_id, true).await,
        _ => Ok(None),
    }
}

/// True when another workspace entry already holds this normalized duplicate key.
pub async fn duplicate_key_exists(
    tx: &mut DbTx<'_>,
    workspace: &str,
    dup_key: &str,
    exclude_entry: &str,
) -> Result<bool> {
    let sql = "SELECT entry_id FROM ingredient_entries WHERE workspace_id = $1 AND dup_key = $2 AND entry_id <> $3";
    let found = match &mut *tx {
        DbTx::Postgres(t) => {
            sqlx::query_scalar::<_, String>(sql)
                .bind(workspace)
                .bind(dup_key)
                .bind(exclude_entry)
                .fetch_optional(&mut **t)
                .await?
        }
        DbTx::Sqlite(t) => {
            sqlx::query_scalar::<_, String>(sql)
                .bind(workspace)
                .bind(dup_key)
                .bind(exclude_entry)
                .fetch_optional(&mut **t)
                .await?
        }
    };
    Ok(found.is_some())
}

/// True when the ID already exists anywhere in the catalogue: another
/// workspace's entry or the immutable starter namespace. IDs are global.
pub async fn entry_id_in_use(tx: &mut DbTx<'_>, entry_id: &str) -> Result<bool> {
    let sql = "SELECT CASE WHEN EXISTS (SELECT 1 FROM ingredient_entries WHERE entry_id = $1) OR EXISTS (SELECT 1 FROM starter_ingredient_entries WHERE entry_id = $1) THEN 1 ELSE 0 END";
    let found = match &mut *tx {
        DbTx::Postgres(t) => {
            sqlx::query_scalar::<_, i64>(sql)
                .bind(entry_id)
                .fetch_one(&mut **t)
                .await?
        }
        DbTx::Sqlite(t) => {
            sqlx::query_scalar::<_, i64>(sql)
                .bind(entry_id)
                .fetch_one(&mut **t)
                .await?
        }
    };
    Ok(found == 1)
}

fn encode(
    document: &IngredientEntryDocumentV1,
    provenance: &IngredientProvenance,
) -> Result<(String, String, String, String, String)> {
    let value = krausen_core::canonical_json::from_serializable(document)
        .map_err(|e| Error::BadRequest(e.to_string()))?;
    let content =
        krausen_core::canonical_json::from_serializable(&(document, provenance))
            .map_err(|e| Error::BadRequest(e.to_string()))?;
    let digest = krausen_core::canonical_json::sha256(&content)
        .map_err(|e| Error::BadRequest(e.to_string()))?;
    let json = String::from_utf8(
        krausen_core::canonical_json::bytes(&value)
            .map_err(|e| Error::BadRequest(e.to_string()))?,
    )
    .map_err(|e| Error::Internal(e.to_string()))?;
    Ok((
        json,
        digest,
        document.specification.name.clone().unwrap_or_default(),
        document
            .specification
            .producer
            .clone()
            .unwrap_or_default(),
        document.category.clone(),
    ))
}

/// Normalized duplicate key: name and producer, case-folded.
pub fn duplicate_key(name: &str, producer: &str) -> String {
    format!(
        "{}|{}",
        name.to_lowercase(),
        producer.to_lowercase()
    )
}

/// Create a workspace entry at revision 1 with its first immutable revision.
#[allow(clippy::too_many_arguments)]
pub async fn create(
    tx: &mut DbTx<'_>,
    workspace: &str,
    id: &str,
    document: &IngredientEntryDocumentV1,
    provenance: &IngredientProvenance,
    actor: &str,
    now: i64,
) -> Result<()> {
    let (json, digest, name, producer, category) = encode(document, provenance)?;
    let provenance_json = serde_json::to_string(provenance)?;
    execute!(
        tx,
        "INSERT INTO ingredient_entries (workspace_id,entry_id,name,producer,category,dup_key,current_revision,metadata_revision,archived,created_at) VALUES ($1,$2,$3,$4,$5,$6,1,1,FALSE,$7)",
        workspace,
        id,
        &name,
        &producer,
        &category,
        duplicate_key(&name, &producer),
        now
    );
    execute!(
        tx,
        "INSERT INTO ingredient_revisions (workspace_id,entry_id,revision,schema_version,name,producer,category,document_json,provenance_json,content_digest,actor_id,recorded_at) VALUES ($1,$2,1,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        workspace,
        id,
        document.schema_version as i32,
        &name,
        &producer,
        &category,
        &json,
        &provenance_json,
        &digest,
        actor,
        now
    );
    Ok(())
}

/// Append a guarded content revision; updates derived search metadata.
/// Returns `false` when the observed revision no longer matches.
pub async fn append(
    tx: &mut DbTx<'_>,
    workspace: &str,
    id: &str,
    document: &IngredientEntryDocumentV1,
    provenance: &IngredientProvenance,
    actor: &str,
    now: i64,
    expected: i64,
) -> Result<bool> {
    let (json, digest, name, producer, category) = encode(document, provenance)?;
    let provenance_json = serde_json::to_string(provenance)?;
    let next = expected + 1;
    let changed = execute!(
        tx,
        "UPDATE ingredient_entries SET name=$4,producer=$5,category=$6,dup_key=$7,current_revision=$8,metadata_revision=metadata_revision+1 WHERE workspace_id=$1 AND entry_id=$2 AND current_revision=$3 AND archived=FALSE",
        workspace,
        id,
        expected,
        &name,
        &producer,
        &category,
        duplicate_key(&name, &producer),
        next
    );
    if changed == 0 {
        return Ok(false);
    }
    execute!(
        tx,
        "INSERT INTO ingredient_revisions (workspace_id,entry_id,revision,schema_version,name,producer,category,document_json,provenance_json,content_digest,actor_id,recorded_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        workspace,
        id,
        next,
        document.schema_version as i32,
        &name,
        &producer,
        &category,
        &json,
        &provenance_json,
        &digest,
        actor,
        now
    );
    Ok(true)
}

/// Guarded archive/unarchive with a retained immutable event.
/// Returns `false` when the observed metadata revision no longer matches.
pub async fn archive(
    tx: &mut DbTx<'_>,
    workspace: &str,
    id: &str,
    archived: bool,
    expected: i64,
    actor: &str,
    now: i64,
) -> Result<bool> {
    let next = expected + 1;
    let changed = execute!(
        tx,
        "UPDATE ingredient_entries SET archived=$3,metadata_revision=$4 WHERE workspace_id=$1 AND entry_id=$2 AND metadata_revision=$5",
        workspace,
        id,
        archived,
        next,
        expected
    );
    if changed == 0 {
        return Ok(false);
    }
    execute!(
        tx,
        "INSERT INTO ingredient_archive_events (workspace_id,entry_id,metadata_revision,archived,actor_id,recorded_at) VALUES ($1,$2,$3,$4,$5,$6)",
        workspace,
        id,
        next,
        archived,
        actor,
        now
    );
    Ok(true)
}
