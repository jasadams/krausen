// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server functions for recipes.
//!
//! Legacy recipe reads and migration recovery. Native authoring and estimates
//! use the shared recipe editor and formulation operations.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use super::AuthenticatedContext;

/// A recipe in a list, with the numbers Krausen derives for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecipeListItem {
    pub recipe_id: String,
    pub name: String,
    pub style_name: Option<String>,
    pub batch_size_l: Option<f64>,
    pub boil_minutes: Option<i64>,
    /// `"krausen"` or `"beerxml"`.
    pub source: String,
    pub updated_at: String,
}

/// Legacy recipe read retained for source recovery and transition screens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecipeDetail {
    pub recipe_id: String,
    pub name: String,
    pub source: String,
    /// The complete recipe. Everything the file had, including what Krausen
    /// does not model.
    pub beerxml: String,
    /// Retained for legacy transport compatibility; estimates use shared preview.
    pub derived: Option<DerivedSummary>,
    /// What conversion could not do faithfully — §3.5 wants these visible.
    pub warnings: Vec<String>,
}

/// The headline figures, flattened for display.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DerivedSummary {
    pub original_gravity: f64,
    pub final_gravity: f64,
    pub pre_boil_gravity: f64,
    pub abv_percent: f64,
    pub colour_ebc: f64,
    pub ibu_total: f64,
    /// What boil-only Tinseth would have said, for comparison with software
    /// that stops at flameout.
    pub ibu_boil_only: f64,
    pub pre_boil_l: f64,
    pub post_boil_l: f64,
    pub mash_water_l: f64,
    pub sparge_water_l: f64,
}

#[cfg(feature = "ssr")]
fn parse_one(beerxml: &str) -> Result<krausen_beerxml::Recipe, ServerFnError> {
    let mut doc = krausen_beerxml::parse(beerxml)
        .map_err(|e| ServerFnError::new(format!("could not read that BeerXML: {e}")))?;
    if doc.recipes.is_empty() {
        return Err(ServerFnError::new("that BeerXML contains no recipes"));
    }
    Ok(doc.recipes.remove(0))
}

#[cfg(feature = "ssr")]
fn detail(stored: krausen_brewery::Recipe) -> RecipeDetail {
    // Calculation is available only through the shared formulation preview.
    let mut warnings = Vec::new();
    if let Some(error) = &stored.native_migration_error {
        warnings.push(format!(
            "Native migration failed: {error}. This legacy record is read-only."
        ));
    }
    RecipeDetail {
        recipe_id: stored.recipe_id,
        name: stored.name,
        source: stored.source.as_str().to_string(),
        beerxml: krausen_beerxml::write_recipe_document(&stored.document),
        derived: None,
        warnings,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Read
// ─────────────────────────────────────────────────────────────────────────

/// Every recipe in the caller's workspace.
#[server(prefix = "/leptos-api")]
pub async fn list_recipes() -> Result<Vec<RecipeListItem>, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;

    let rows = krausen_brewery::recipe_service::list_recipes(&ac.ctx.db, &ac.ws_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "list_recipes failed");
            ServerFnError::new("Could not list recipes")
        })?;

    Ok(rows
        .into_iter()
        .map(|r| RecipeListItem {
            recipe_id: r.recipe_id,
            name: r.name,
            style_name: r.style_name,
            batch_size_l: r.batch_size_l,
            boil_minutes: r.boil_minutes,
            source: r.source.as_str().to_string(),
            updated_at: r.updated_at.to_rfc3339(),
        })
        .collect())
}

/// Read a legacy recipe or recover its original source.
/// Recover the stored legacy payload when migration cannot parse it.
#[server(prefix = "/leptos-api")]
pub async fn recover_legacy_recipe(
    recipe_id: String,
) -> Result<Option<(String, String)>, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    krausen_app::recipe_migration::recover_legacy_recipe(&ac.ctx.db, &actor, &recipe_id)
        .await
        .map_err(|_| ServerFnError::new("Could not recover that recipe"))
}

/// Read a legacy recipe or recover its original source.
#[server(prefix = "/leptos-api")]
pub async fn get_recipe(recipe_id: String) -> Result<Option<RecipeDetail>, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;

    let stored = krausen_brewery::recipe_service::get_recipe(&ac.ctx.db, &ac.ws_id, &recipe_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "get_recipe failed");
            match e {
                krausen_core::Error::Conflict(message) => ServerFnError::new(message),
                _ => ServerFnError::new("Could not load that recipe"),
            }
        })?;

    Ok(stored.map(detail))
}

/// Search recipes by name.
#[server(prefix = "/leptos-api")]
pub async fn search_recipes(query: String) -> Result<Vec<RecipeListItem>, ServerFnError> {
    if query.trim().is_empty() {
        return list_recipes().await;
    }
    let ac = AuthenticatedContext::extract().await?;

    let rows = krausen_brewery::recipe_service::search_recipes(&ac.ctx.db, &ac.ws_id, query.trim())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "search_recipes failed");
            ServerFnError::new("Could not search recipes")
        })?;

    Ok(rows
        .into_iter()
        .map(|r| RecipeListItem {
            recipe_id: r.recipe_id,
            name: r.name,
            style_name: r.style_name,
            batch_size_l: r.batch_size_l,
            boil_minutes: r.boil_minutes,
            source: r.source.as_str().to_string(),
            updated_at: r.updated_at.to_rfc3339(),
        })
        .collect())
}

// ─────────────────────────────────────────────────────────────────────────
// Write
// ─────────────────────────────────────────────────────────────────────────

/// Store a recipe given as BeerXML.
#[server(prefix = "/leptos-api")]
pub async fn create_recipe(beerxml: String) -> Result<RecipeDetail, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let _ = beerxml;
    let actor = super::verified_actor(&ac.auth)?;
    match krausen_app::recipe_migration::reject_legacy_authoring(&ac.ctx.db, &actor).await {
        Err(krausen_app::Error::LegacyAuthoringUnavailable) => Err(ServerFnError::new(
            "Recipe creation is unavailable while native authoring is being introduced",
        )),
        _ => Err(ServerFnError::new("Recipe creation is unavailable")),
    }
}

/// Import a BeerXML file, which may hold several recipes.
///
/// Returns what was stored, so the caller can say how many arrived.
#[server(prefix = "/leptos-api")]
pub async fn import_beerxml(beerxml: String) -> Result<Vec<RecipeListItem>, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let _ = beerxml;
    let actor = super::verified_actor(&ac.auth)?;
    match krausen_app::recipe_migration::reject_legacy_authoring(&ac.ctx.db, &actor).await {
        Err(krausen_app::Error::LegacyAuthoringUnavailable) => Err(ServerFnError::new(
            "Recipe import is unavailable while native authoring is being introduced",
        )),
        _ => Err(ServerFnError::new("Recipe import is unavailable")),
    }
}

/// Replace a recipe with the BeerXML given.
#[server(prefix = "/leptos-api")]
pub async fn update_recipe(
    recipe_id: String,
    beerxml: String,
) -> Result<Option<RecipeDetail>, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let document = parse_one(&beerxml)?;

    let stored = krausen_brewery::recipe_service::update_recipe(
        &ac.ctx.db, &ac.ws_id, &recipe_id, &document,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "update_recipe failed");
        match e {
            krausen_core::Error::Conflict(message) => ServerFnError::new(message),
            _ => ServerFnError::new("Could not save that recipe"),
        }
    })?;

    Ok(stored.map(detail))
}

/// Delete a recipe. Recoverable — see [`restore_recipe`].
#[server(prefix = "/leptos-api")]
pub async fn delete_recipe(recipe_id: String) -> Result<bool, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;

    krausen_brewery::recipe_service::delete_recipe(&ac.ctx.db, &ac.ws_id, &recipe_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "delete_recipe failed");
            match e {
                krausen_core::Error::Conflict(message) => ServerFnError::new(message),
                _ => ServerFnError::new("Could not delete that recipe"),
            }
        })
}

/// Undo a delete.
#[server(prefix = "/leptos-api")]
pub async fn restore_recipe(recipe_id: String) -> Result<bool, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;

    krausen_brewery::recipe_service::restore_recipe(&ac.ctx.db, &ac.ws_id, &recipe_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "restore_recipe failed");
            match e {
                krausen_core::Error::Conflict(message) => ServerFnError::new(message),
                _ => ServerFnError::new("Could not restore that recipe"),
            }
        })
}

#[cfg(all(test, feature = "ssr"))]
mod principal_tests {
    use axum::extract::FromRequestParts;
    use krausen_auth::middleware::{AuthState, AuthUser};
    use krausen_core::DbPool;

    #[tokio::test]
    async fn verified_browser_principal_is_required_for_recovery() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../apps/server/migrations-sqlite")
            .run(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO users (user_id, email) VALUES ('user-local', 'local@example.test')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO workspaces (workspace_id, name, owner_user_id) VALUES ('workspace-local', 'Local', 'user-local')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO workspace_users (workspace_id, user_id, role) VALUES ('workspace-local', 'user-local', 'workspace_admin')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO recipes (workspace_id, recipe_id, name, document, native_migration_error) VALUES ('workspace-local', 'broken', 'Broken', 'original', 'Recovery required')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ('c','user-local','browser','krausen-browser','krausen-ui',4102444800)").execute(&pool).await.unwrap();
        let db = DbPool::Sqlite(pool);
        let secret = "test-only-jwt-secret-for-principal-boundary";
        let personal = AuthState {
            jwt_secret: secret.into(),
            db: db.clone(),
        };
        let (mut parts, _) = axum::http::Request::builder()
            .body(())
            .unwrap()
            .into_parts();
        assert!(
            AuthUser::from_request_parts(&mut parts, &personal)
                .await
                .is_err()
        );
        let mut claims = std::collections::HashMap::new();
        claims.insert("workspace_id".into(), serde_json::json!("workspace-local"));
        claims.insert("credential_id".into(), serde_json::json!("c"));
        claims.insert(
            "iss".into(),
            serde_json::json!(krausen_auth::jwt::BROWSER_ISSUER),
        );
        claims.insert(
            "aud".into(),
            serde_json::json!(krausen_auth::jwt::BROWSER_AUDIENCE),
        );
        let token =
            krausen_auth::jwt::create_access_token_str("user-local", secret, 10, claims).unwrap();
        let state = AuthState {
            jwt_secret: secret.into(),
            db: db.clone(),
        };
        let (mut parts, _) = axum::http::Request::builder()
            .header("authorization", format!("Bearer {token}"))
            .body(())
            .unwrap()
            .into_parts();
        let verified = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        let actor = super::super::verified_actor(&verified).unwrap();
        let source = krausen_app::recipe_migration::recover_legacy_recipe(&db, &actor, "broken")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(source.0, "original");
    }
}
