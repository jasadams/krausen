// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Equipment destination: reusable workspace profiles, without any recipe.
use crate::{
    components::{BreweryNotice, ButtonLink, ButtonVariant},
    pages::equipment_profiles::EquipmentManager,
};
use krausen_types::request_context::OperationIntent;
use leptos::prelude::*;

/// Only an in-app recipe editor path may be a return target; never an external URL.
pub fn safe_return(value: Option<String>) -> Option<String> {
    let value = value?;
    let rest = value.strip_prefix("/recipe-editor/")?;
    (!rest.is_empty()
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    .then_some(value)
}

#[component]
pub fn EquipmentPage() -> impl IntoView {
    let context = LocalResource::new(|| async {
        let context = crate::server_fns::context::get_user_context().await?;
        Ok::<_, ServerFnError>(OperationIntent {
            account_id: context.user_id,
            workspace_id: context
                .workspace_id
                .ok_or_else(|| ServerFnError::new("Workspace required"))?,
        })
    });
    view! { <Suspense fallback=|| view! { <p>"Loading equipment…"</p> }>
        {move || Suspend::new(async move { match context.await {
            Ok(identity) => view! { <EquipmentContent identity=identity/> }.into_any(),
            Err(_) => view! { <BreweryNotice title="Unable to load equipment" body="Sign in and select a workspace." warning=true/> }.into_any(),
        } })}
    </Suspense> }
}

#[component]
fn EquipmentContent(identity: OperationIntent) -> impl IntoView {
    let identity = RwSignal::new(identity);
    let context_visible = RwSignal::new(true);
    let query = leptos_router::hooks::use_query_map();
    let initial = query.with_untracked(|q| q.get("profile")).unwrap_or_default();
    let back = safe_return(query.with_untracked(|q| q.get("return")));
    view! {
        <div class="recipe-editor">
            <header class="recipe-editor__header">
                <div>
                    <p class="kit-showcase__eyebrow">"Your brewery"</p>
                    <h1 id="equipment-heading" tabindex="-1">"Equipment"</h1>
                    <p>"Profiles describe your vessels, losses and chilling. Recipes choose a profile and may adjust it for that recipe."</p>
                </div>
                {back.map(|href| view! { <ButtonLink href=href variant=ButtonVariant::Secondary>"Back to recipe"</ButtonLink> })}
            </header>
            <Show when=move || !context_visible.get()>
                <BreweryNotice title="Workspace changed" body="Return to the account and workspace where you opened Equipment, then reload." warning=true/>
            </Show>
            <Show when=move || context_visible.get()>
                <EquipmentManager identity=identity context_visible=context_visible initial_profile=initial.clone()/>
            </Show>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::safe_return;

    #[test]
    fn only_in_app_recipe_editor_paths_are_return_targets() {
        assert_eq!(safe_return(Some("/recipe-editor/new".into())).as_deref(), Some("/recipe-editor/new"));
        assert_eq!(safe_return(Some("/recipe-editor/abc-123".into())).as_deref(), Some("/recipe-editor/abc-123"));
        for bad in ["https://evil.example", "//evil.example", "/recipe-editor/", "/recipe-editor/../x", "/recipe-editor/a?x=1", "/settings"] {
            assert_eq!(safe_return(Some(bad.into())), None, "{bad}");
        }
        assert_eq!(safe_return(None), None);
    }
}
