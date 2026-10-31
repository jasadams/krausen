// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authorized formulation previews and the pool-owning preview wrapper.
//!
//! The shared transaction-aware resolver lives in `formulation_kernel`; both
//! preview and start-brew use it so targets, assumptions and context IDs have
//! exactly one producer.
use crate::{ActorContext, Error, authorize_workspace};
use krausen_core::DbPool;
use krausen_types::formulation::{PreviewOutcome, PreviewRequest};

/// Run the shared preview after verifying current workspace membership. The
/// equipment draft has explicit caller-supplied provenance and is not a profile.
pub async fn preview(
    pool: &DbPool,
    actor: &ActorContext,
    request: PreviewRequest,
) -> Result<PreviewOutcome, Error> {
    let mut tx = pool.begin_write().await?;
    let outcome = crate::formulation_kernel::resolve_in_tx(&mut tx, actor, request).await;
    if matches!(outcome, Ok(PreviewOutcome::Preview(_))) {
        tx.commit().await?;
    }
    outcome
}

/// Bounded list of supported style identities under the same workspace policy.
pub async fn list_styles(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &krausen_types::request_context::OperationIntent,
) -> Result<Vec<krausen_types::brewery::StyleReference>, Error> {
    let mut tx = pool.begin_write().await?;
    authorize_workspace(&mut tx, actor).await?;
    crate::check_intent(actor, intent)?;
    let styles = crate::style_reference::identities();
    tx.commit().await?;
    Ok(styles)
}

/// Shared sanitized result mapping for both adapters.
pub fn project_preview(result: Result<PreviewOutcome, Error>) -> PreviewOutcome {
    match result {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => PreviewOutcome::Forbidden,
        Err(Error::ContextChanged) => PreviewOutcome::ContextChanged,
        Err(Error::Invalid(_)) => PreviewOutcome::Invalid,
        Err(error) => {
            tracing::warn!(%error, "formulation preview unavailable");
            PreviewOutcome::Unavailable
        }
    }
}
pub fn project_styles(
    result: Result<Vec<krausen_types::brewery::StyleReference>, Error>,
) -> krausen_types::formulation::StyleListOutcome {
    use krausen_types::formulation::StyleListOutcome;
    match result {
        Ok(styles) => StyleListOutcome::Styles(styles),
        Err(Error::Forbidden) => StyleListOutcome::Forbidden,
        Err(Error::ContextChanged) => StyleListOutcome::ContextChanged,
        Err(error) => {
            tracing::warn!(%error, "style list unavailable");
            StyleListOutcome::Unavailable
        }
    }
}
