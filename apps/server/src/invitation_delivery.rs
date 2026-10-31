// SPDX-License-Identifier: AGPL-3.0-or-later
//! Deliver committed invitation proof mail. Outbox records contain no token.
use krausen_app::invitation::{Proof, delivery_token};
use krausen_core::DbPool;

#[derive(sqlx::FromRow)]
struct Outbox {
    outbox_id: String,
    payload_json: String,
}

pub async fn deliver_pending(
    db: &DbPool,
    base_url: &str,
    key: &[u8],
    mail: &krausen_auth::email_service::EmailService,
) -> krausen_core::Result<()> {
    if !krausen_app::invitation::delivery_available(mail.is_configured(), base_url) {
        return Ok(());
    }
    deliver_pending_with(db, base_url, key, |email, url| async move {
        let text = format!("Confirm your workspace invitation request: {url}\nIf you did not request this, ignore this message.");
        let html = format!(r#"<p>Confirm your workspace invitation request:</p><p><a href="{url}">Open confirmation</a></p><p>If you did not request this, ignore this message.</p>"#);
        mail.send_email(&email, "Confirm workspace invitation request", &html, Some(&text), None, &[]).await
    }).await
}

/// Dispatch committed outbox work through the supplied delivery transport.
pub async fn deliver_pending_with<F, Fut>(
    db: &DbPool,
    base_url: &str,
    key: &[u8],
    mut send: F,
) -> krausen_core::Result<()>
where
    F: FnMut(String, String) -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let records = krausen_core::db_fetch_all!(
        db,
        Outbox,
        "SELECT outbox_id,payload_json FROM app_outbox WHERE operation='account.invitation.proof.v1' AND delivered_at IS NULL ORDER BY created_at LIMIT 50"
    )?;
    for record in records {
        let payload: serde_json::Value = serde_json::from_str(&record.payload_json)?;
        let Some(id) = payload.get("proof_id").and_then(|v| v.as_str()) else {
            tracing::error!(outbox_id = %record.outbox_id, "invalid proof outbox payload");
            continue;
        };
        let proof = krausen_core::db_fetch_optional!(
            db,
            Proof,
            "SELECT * FROM account_action_proofs WHERE proof_id=$1 AND consumed_at IS NULL AND expires_at>$2",
            id,
            chrono::Utc::now().timestamp()
        )?;
        let Some(proof) = proof else {
            krausen_core::db_execute!(
                db,
                "UPDATE app_outbox SET delivered_at=$1 WHERE outbox_id=$2 AND delivered_at IS NULL",
                chrono::Utc::now().timestamp(),
                &record.outbox_id
            )?;
            continue;
        };
        let token = delivery_token(key, &proof)
            .map_err(|_| krausen_core::Error::Internal("proof token generation failed".into()))?;
        let mut url = url::Url::parse(base_url)
            .map_err(|_| krausen_core::Error::Internal("invalid proof delivery origin".into()))?;
        url.set_path("/account/proof");
        url.set_fragment(Some(&format!(
            "interaction={}&code={}",
            proof.proof_id, token
        )));
        let url = url.to_string();
        if send(proof.recipient_email.clone(), url).await {
            krausen_core::db_execute!(
                db,
                "UPDATE app_outbox SET delivered_at=$1 WHERE outbox_id=$2 AND delivered_at IS NULL",
                chrono::Utc::now().timestamp(),
                &record.outbox_id
            )?;
        }
    }
    Ok(())
}
