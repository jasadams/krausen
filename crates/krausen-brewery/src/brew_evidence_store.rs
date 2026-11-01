// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transaction-participating brew evidence, selection and lifecycle storage.
//! The caller owns `DbTx` and commit/rollback.

use krausen_core::{DbTx, Error, Result};
use krausen_types::brewery::{
    ActualFormulationRevision, ActualFormulationV1, ActualSelection, ActualSelectionEvent,
    BrewArchiveEvent, BrewLifecycleEvent, BrewStatusV1, MeasurementEntry, MeasurementMetadataV1,
    ObservationNote, ObservationNoteV1, ObservationRevision,
};
use std::collections::BTreeMap;

macro_rules! execute {
    ($tx:expr, $sql:expr $(, $arg:expr)* $(,)?) => {{
        match &mut *$tx {
            DbTx::Postgres(inner) => sqlx::query($sql)$(.bind($arg))*.execute(&mut **inner).await?.rows_affected(),
            DbTx::Sqlite(inner) => sqlx::query($sql)$(.bind($arg))*.execute(&mut **inner).await?.rows_affected(),
        }
    }};
}

macro_rules! scalar {
    ($tx:expr, $ty:ty, $sql:expr $(, $arg:expr)* $(,)?) => {{
        match &mut *$tx {
            DbTx::Postgres(inner) => sqlx::query_scalar::<_, $ty>($sql)$(.bind($arg))*.fetch_optional(&mut **inner).await,
            DbTx::Sqlite(inner) => sqlx::query_scalar::<_, $ty>($sql)$(.bind($arg))*.fetch_optional(&mut **inner).await,
        }
    }};
}

#[derive(Debug, Clone, PartialEq)]
pub struct BrewEvidenceState {
    pub status: BrewStatusV1,
    pub revision: i64,
    pub metadata_revision: i64,
    pub archived: bool,
    pub actual_revision: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct BrewStateRow {
    status: String,
    revision: i64,
    metadata_revision: i64,
    archived: i64,
    actual_revision: Option<i64>,
}

fn state_from_row(row: BrewStateRow) -> Result<BrewEvidenceState> {
    let status = BrewStatusV1::parse(&row.status)
        .ok_or_else(|| Error::Internal("stored brew status is not supported".into()))?;
    Ok(BrewEvidenceState {
        status,
        revision: row.revision,
        metadata_revision: row.metadata_revision,
        archived: row.archived != 0,
        actual_revision: row.actual_revision,
    })
}

const STATE_SELECT: &str = "SELECT status, revision, metadata_revision, CAST(CASE WHEN archived THEN 1 ELSE 0 END AS BIGINT) AS archived, actual_revision FROM brews WHERE workspace_id = $1 AND brew_id = $2";

/// Lock the brew aggregate and return its lifecycle/archive/actual pointers.
pub async fn lock(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
) -> Result<Option<BrewEvidenceState>> {
    let sql = if matches!(&*tx, DbTx::Postgres(_)) {
        format!("{STATE_SELECT} FOR UPDATE")
    } else {
        STATE_SELECT.to_owned()
    };
    let row = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as::<_, BrewStateRow>(&sql)
                .bind(workspace_id)
                .bind(brew_id)
                .fetch_optional(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as::<_, BrewStateRow>(&sql)
                .bind(workspace_id)
                .bind(brew_id)
                .fetch_optional(&mut **inner)
                .await?
        }
    };
    row.map(state_from_row).transpose()
}

/// Read the brew aggregate without taking a row lock.
pub async fn load_state(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
) -> Result<Option<BrewEvidenceState>> {
    let row = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as::<_, BrewStateRow>(STATE_SELECT)
                .bind(workspace_id)
                .bind(brew_id)
                .fetch_optional(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as::<_, BrewStateRow>(STATE_SELECT)
                .bind(workspace_id)
                .bind(brew_id)
                .fetch_optional(&mut **inner)
                .await?
        }
    };
    row.map(state_from_row).transpose()
}

#[derive(sqlx::FromRow)]
struct ActualRow {
    workspace_id: String,
    brew_id: String,
    revision: i64,
    predecessor_revision: Option<i64>,
    schema_version: i32,
    document_json: String,
    content_digest: String,
    actor_id: String,
    recorded_at: i64,
    reason: Option<String>,
}

impl ActualRow {
    fn into_revision(self) -> Result<ActualFormulationRevision> {
        if self.schema_version != 1 {
            return Err(Error::Internal("unsupported actual formulation schema".into()));
        }
        Ok(ActualFormulationRevision {
            workspace_id: self.workspace_id,
            brew_id: self.brew_id,
            revision: self.revision,
            predecessor_revision: self.predecessor_revision,
            document: serde_json::from_str::<ActualFormulationV1>(&self.document_json)?,
            content_digest: self.content_digest,
            actor_id: self.actor_id,
            recorded_at: self.recorded_at,
            reason: self.reason,
        })
    }
}

const ACTUAL_SELECT: &str = "SELECT workspace_id, brew_id, revision, predecessor_revision, schema_version, document_json, content_digest, actor_id, recorded_at, reason FROM brew_actual_formulations WHERE workspace_id = $1 AND brew_id = $2 AND revision = $3";

pub async fn insert_actual_formulation(
    tx: &mut DbTx<'_>,
    revision: &ActualFormulationRevision,
) -> Result<()> {
    let document_json = serde_json::to_string(&revision.document)?;
    execute!(
        tx,
        "INSERT INTO brew_actual_formulations (workspace_id, brew_id, revision, predecessor_revision, schema_version, document_json, content_digest, actor_id, recorded_at, reason) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        &revision.workspace_id,
        &revision.brew_id,
        revision.revision,
        revision.predecessor_revision,
        revision.document.schema_version as i32,
        &document_json,
        &revision.content_digest,
        &revision.actor_id,
        revision.recorded_at,
        &revision.reason
    );
    Ok(())
}

pub async fn load_actual_formulation(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    revision: i64,
) -> Result<Option<ActualFormulationRevision>> {
    let row = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as::<_, ActualRow>(ACTUAL_SELECT)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(revision)
                .fetch_optional(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as::<_, ActualRow>(ACTUAL_SELECT)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(revision)
                .fetch_optional(&mut **inner)
                .await?
        }
    };
    row.map(ActualRow::into_revision).transpose()
}

pub async fn load_current_actual(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
) -> Result<Option<ActualFormulationRevision>> {
    let revision = scalar!(
        tx,
        Option<i64>,
        "SELECT actual_revision FROM brews WHERE workspace_id = $1 AND brew_id = $2",
        workspace_id,
        brew_id
    )?
    .flatten();
    match revision {
        Some(revision) => load_actual_formulation(tx, workspace_id, brew_id, revision).await,
        None => Ok(None),
    }
}

pub async fn set_actual_revision(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    revision: i64,
) -> Result<()> {
    execute!(
        tx,
        "UPDATE brews SET actual_revision = $3 WHERE workspace_id = $1 AND brew_id = $2",
        workspace_id,
        brew_id,
        revision
    );
    Ok(())
}

#[derive(sqlx::FromRow)]
struct MeasurementRow {
    workspace_id: String,
    brew_id: String,
    entry_id: String,
    quantity_id: String,
    unit: String,
    basis: String,
    stage: String,
    value: Option<f64>,
    observed_at: i64,
    time_source: String,
    recorded_at: i64,
    actor_id: String,
    source: String,
    root_observation_id: String,
    supersedes_entry_id: Option<String>,
    schema_version: i32,
    metadata_json: String,
    extensions_json: String,
}

impl MeasurementRow {
    fn into_entry(self) -> Result<MeasurementEntry> {
        if self.schema_version != 1 {
            return Err(Error::Internal("unsupported measurement schema".into()));
        }
        let mut metadata: MeasurementMetadataV1 = serde_json::from_str(&self.metadata_json)?;
        metadata.extensions = serde_json::from_str::<BTreeMap<String, serde_json::Value>>(
            &self.extensions_json,
        )?;
        Ok(MeasurementEntry {
            workspace_id: self.workspace_id,
            brew_id: self.brew_id,
            entry_id: self.entry_id,
            quantity_id: self.quantity_id,
            unit: self.unit,
            basis: self.basis,
            stage: self.stage,
            value: self.value,
            observed_at: self.observed_at,
            time_source: self.time_source,
            recorded_at: self.recorded_at,
            actor_id: self.actor_id,
            source: self.source,
            root_observation_id: self.root_observation_id,
            supersedes_entry_id: self.supersedes_entry_id,
            schema_version: self.schema_version as u32,
            metadata,
        })
    }
}

const MEASUREMENT_SELECT: &str = "SELECT workspace_id, brew_id, entry_id, quantity_id, unit, basis, stage, value, observed_at, time_source, recorded_at, actor_id, source, root_observation_id, supersedes_entry_id, schema_version, metadata_json, extensions_json FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2";

pub async fn insert_measurement(tx: &mut DbTx<'_>, entry: &MeasurementEntry) -> Result<()> {
    let extensions_json = serde_json::to_string(&entry.metadata.extensions)?;
    let mut metadata = entry.metadata.clone();
    metadata.extensions.clear();
    let metadata_json = serde_json::to_string(&metadata)?;
    execute!(
        tx,
        "INSERT INTO brew_measurement_entries (workspace_id, brew_id, entry_id, quantity_id, unit, basis, stage, value, observed_at, time_source, recorded_at, actor_id, source, root_observation_id, supersedes_entry_id, schema_version, metadata_json, extensions_json) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)",
        &entry.workspace_id,
        &entry.brew_id,
        &entry.entry_id,
        &entry.quantity_id,
        &entry.unit,
        &entry.basis,
        &entry.stage,
        entry.value,
        entry.observed_at,
        &entry.time_source,
        entry.recorded_at,
        &entry.actor_id,
        &entry.source,
        &entry.root_observation_id,
        &entry.supersedes_entry_id,
        entry.schema_version as i32,
        &metadata_json,
        &extensions_json
    );
    Ok(())
}

pub async fn load_measurement(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    entry_id: &str,
) -> Result<Option<MeasurementEntry>> {
    let sql = format!("{MEASUREMENT_SELECT} AND entry_id = $3");
    let row = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as::<_, MeasurementRow>(&sql)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(entry_id)
                .fetch_optional(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as::<_, MeasurementRow>(&sql)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(entry_id)
                .fetch_optional(&mut **inner)
                .await?
        }
    };
    row.map(MeasurementRow::into_entry).transpose()
}

pub async fn load_quantity_entries(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    quantity_id: &str,
) -> Result<Vec<MeasurementEntry>> {
    let sql = format!("{MEASUREMENT_SELECT} AND quantity_id = $3 ORDER BY recorded_at, entry_id");
    let rows = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as::<_, MeasurementRow>(&sql)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(quantity_id)
                .fetch_all(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as::<_, MeasurementRow>(&sql)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(quantity_id)
                .fetch_all(&mut **inner)
                .await?
        }
    };
    rows.into_iter().map(MeasurementRow::into_entry).collect()
}

/// Resolve the current unsuperseded entry for one observation root.
pub async fn load_current_entry(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    root_observation_id: &str,
) -> Result<Option<MeasurementEntry>> {
    let sql = format!(
        "{MEASUREMENT_SELECT} AND root_observation_id = $3 AND entry_id NOT IN (SELECT supersedes_entry_id FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2 AND supersedes_entry_id IS NOT NULL)"
    );
    let row = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as::<_, MeasurementRow>(&sql)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(root_observation_id)
                .fetch_optional(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as::<_, MeasurementRow>(&sql)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(root_observation_id)
                .fetch_optional(&mut **inner)
                .await?
        }
    };
    row.map(MeasurementRow::into_entry).transpose()
}

#[derive(sqlx::FromRow)]
struct SelectionRow {
    workspace_id: String,
    brew_id: String,
    quantity_id: String,
    revision: i64,
    selected_root_observation_id: Option<String>,
    updated_at: i64,
    updated_by: String,
}

impl SelectionRow {
    fn into_selection(self) -> ActualSelection {
        ActualSelection {
            workspace_id: self.workspace_id,
            brew_id: self.brew_id,
            quantity_id: self.quantity_id,
            revision: self.revision,
            selected_root_observation_id: self.selected_root_observation_id,
            updated_at: self.updated_at,
            updated_by: self.updated_by,
        }
    }
}

const SELECTION_SELECT: &str = "SELECT workspace_id, brew_id, quantity_id, revision, selected_root_observation_id, updated_at, updated_by FROM brew_actual_selections WHERE workspace_id = $1 AND brew_id = $2 AND quantity_id = $3";

pub async fn load_selection(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    quantity_id: &str,
) -> Result<Option<ActualSelection>> {
    let row = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as::<_, SelectionRow>(SELECTION_SELECT)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(quantity_id)
                .fetch_optional(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as::<_, SelectionRow>(SELECTION_SELECT)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(quantity_id)
                .fetch_optional(&mut **inner)
                .await?
        }
    };
    Ok(row.map(SelectionRow::into_selection))
}

pub async fn insert_selection(tx: &mut DbTx<'_>, selection: &ActualSelection) -> Result<()> {
    execute!(
        tx,
        "INSERT INTO brew_actual_selections (workspace_id, brew_id, quantity_id, revision, selected_root_observation_id, updated_at, updated_by) VALUES ($1,$2,$3,$4,$5,$6,$7)",
        &selection.workspace_id,
        &selection.brew_id,
        &selection.quantity_id,
        selection.revision,
        &selection.selected_root_observation_id,
        selection.updated_at,
        &selection.updated_by
    );
    Ok(())
}

pub async fn update_selection(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    quantity_id: &str,
    expected_revision: i64,
    next: &ActualSelection,
) -> Result<bool> {
    let changed = execute!(
        tx,
        "UPDATE brew_actual_selections SET revision = $5, selected_root_observation_id = $6, updated_at = $7, updated_by = $8 WHERE workspace_id = $1 AND brew_id = $2 AND quantity_id = $3 AND revision = $4",
        workspace_id,
        brew_id,
        quantity_id,
        expected_revision,
        next.revision,
        &next.selected_root_observation_id,
        next.updated_at,
        &next.updated_by
    );
    Ok(changed == 1)
}

pub async fn insert_selection_event(
    tx: &mut DbTx<'_>,
    event: &ActualSelectionEvent,
) -> Result<()> {
    execute!(
        tx,
        "INSERT INTO brew_actual_selection_events (workspace_id, brew_id, quantity_id, revision, selected_root_observation_id, actor_id, recorded_at) VALUES ($1,$2,$3,$4,$5,$6,$7)",
        &event.workspace_id,
        &event.brew_id,
        &event.quantity_id,
        event.revision,
        &event.selected_root_observation_id,
        &event.actor_id,
        event.recorded_at
    );
    Ok(())
}

#[derive(sqlx::FromRow)]
struct NoteRow {
    workspace_id: String,
    brew_id: String,
    note_id: String,
    current_revision: i64,
    created_at: i64,
    created_by: String,
}

impl NoteRow {
    fn into_note(self) -> ObservationNote {
        ObservationNote {
            workspace_id: self.workspace_id,
            brew_id: self.brew_id,
            note_id: self.note_id,
            current_revision: self.current_revision,
            created_at: self.created_at,
            created_by: self.created_by,
        }
    }
}

const NOTE_SELECT: &str = "SELECT workspace_id, brew_id, note_id, current_revision, created_at, created_by FROM brew_notes WHERE workspace_id = $1 AND brew_id = $2 AND note_id = $3";

pub async fn insert_note(tx: &mut DbTx<'_>, note: &ObservationNote) -> Result<()> {
    execute!(
        tx,
        "INSERT INTO brew_notes (workspace_id, brew_id, note_id, current_revision, created_at, created_by) VALUES ($1,$2,$3,$4,$5,$6)",
        &note.workspace_id,
        &note.brew_id,
        &note.note_id,
        note.current_revision,
        note.created_at,
        &note.created_by
    );
    Ok(())
}

pub async fn load_note(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    note_id: &str,
) -> Result<Option<ObservationNote>> {
    let row = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as::<_, NoteRow>(NOTE_SELECT)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(note_id)
                .fetch_optional(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as::<_, NoteRow>(NOTE_SELECT)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(note_id)
                .fetch_optional(&mut **inner)
                .await?
        }
    };
    Ok(row.map(NoteRow::into_note))
}

pub async fn update_note_revision(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    note_id: &str,
    expected_revision: i64,
    next_revision: i64,
) -> Result<bool> {
    let changed = execute!(
        tx,
        "UPDATE brew_notes SET current_revision = $5 WHERE workspace_id = $1 AND brew_id = $2 AND note_id = $3 AND current_revision = $4",
        workspace_id,
        brew_id,
        note_id,
        expected_revision,
        next_revision
    );
    Ok(changed == 1)
}

#[derive(sqlx::FromRow)]
struct NoteRevisionRow {
    workspace_id: String,
    brew_id: String,
    note_id: String,
    revision: i64,
    schema_version: i32,
    document_json: String,
    content_digest: String,
    supersedes_revision: Option<i64>,
    actor_id: String,
    recorded_at: i64,
    reason: Option<String>,
}

impl NoteRevisionRow {
    fn into_revision(self) -> Result<ObservationRevision> {
        if self.schema_version != 1 {
            return Err(Error::Internal("unsupported note schema".into()));
        }
        Ok(ObservationRevision {
            workspace_id: self.workspace_id,
            brew_id: self.brew_id,
            note_id: self.note_id,
            revision: self.revision,
            document: serde_json::from_str::<ObservationNoteV1>(&self.document_json)?,
            content_digest: self.content_digest,
            supersedes_revision: self.supersedes_revision,
            actor_id: self.actor_id,
            recorded_at: self.recorded_at,
            reason: self.reason,
        })
    }
}

const NOTE_REVISION_SELECT: &str = "SELECT workspace_id, brew_id, note_id, revision, schema_version, document_json, content_digest, supersedes_revision, actor_id, recorded_at, reason FROM brew_note_revisions WHERE workspace_id = $1 AND brew_id = $2 AND note_id = $3 AND revision = $4";

pub async fn insert_note_revision(tx: &mut DbTx<'_>, revision: &ObservationRevision) -> Result<()> {
    let document_json = serde_json::to_string(&revision.document)?;
    execute!(
        tx,
        "INSERT INTO brew_note_revisions (workspace_id, brew_id, note_id, revision, schema_version, document_json, content_digest, supersedes_revision, actor_id, recorded_at, reason) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        &revision.workspace_id,
        &revision.brew_id,
        &revision.note_id,
        revision.revision,
        revision.document.schema_version as i32,
        &document_json,
        &revision.content_digest,
        revision.supersedes_revision,
        &revision.actor_id,
        revision.recorded_at,
        &revision.reason
    );
    Ok(())
}

pub async fn load_note_revision(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    note_id: &str,
    revision: i64,
) -> Result<Option<ObservationRevision>> {
    let row = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as::<_, NoteRevisionRow>(NOTE_REVISION_SELECT)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(note_id)
                .bind(revision)
                .fetch_optional(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as::<_, NoteRevisionRow>(NOTE_REVISION_SELECT)
                .bind(workspace_id)
                .bind(brew_id)
                .bind(note_id)
                .bind(revision)
                .fetch_optional(&mut **inner)
                .await?
        }
    };
    row.map(NoteRevisionRow::into_revision).transpose()
}

pub async fn insert_lifecycle_event(tx: &mut DbTx<'_>, event: &BrewLifecycleEvent) -> Result<()> {
    execute!(
        tx,
        "INSERT INTO brew_lifecycle_events (workspace_id, brew_id, brew_revision, from_status, to_status, reason, actor_id, recorded_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        &event.workspace_id,
        &event.brew_id,
        event.brew_revision,
        event.from_status.as_str(),
        event.to_status.as_str(),
        &event.reason,
        &event.actor_id,
        event.recorded_at
    );
    Ok(())
}

pub async fn update_status(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    expected_revision: i64,
    expected_status: BrewStatusV1,
    next_revision: i64,
    next_status: BrewStatusV1,
) -> Result<bool> {
    let changed = execute!(
        tx,
        "UPDATE brews SET status = $5, revision = $6 WHERE workspace_id = $1 AND brew_id = $2 AND revision = $3 AND status = $4",
        workspace_id,
        brew_id,
        expected_revision,
        expected_status.as_str(),
        next_status.as_str(),
        next_revision
    );
    Ok(changed == 1)
}

pub async fn insert_archive_event(tx: &mut DbTx<'_>, event: &BrewArchiveEvent) -> Result<()> {
    execute!(
        tx,
        "INSERT INTO brew_archive_events (workspace_id, brew_id, metadata_revision, archived, actor_id, recorded_at) VALUES ($1,$2,$3,$4,$5,$6)",
        &event.workspace_id,
        &event.brew_id,
        event.metadata_revision,
        event.archived,
        &event.actor_id,
        event.recorded_at
    );
    Ok(())
}

pub async fn update_archive(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    expected_metadata_revision: i64,
    next_metadata_revision: i64,
    archived: bool,
) -> Result<bool> {
    let changed = execute!(
        tx,
        "UPDATE brews SET archived = $4, metadata_revision = $5 WHERE workspace_id = $1 AND brew_id = $2 AND metadata_revision = $3",
        workspace_id,
        brew_id,
        expected_metadata_revision,
        archived,
        next_metadata_revision
    );
    Ok(changed == 1)
}
