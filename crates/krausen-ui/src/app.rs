// SPDX-License-Identifier: AGPL-3.0-or-later

//! Root application component and router.

use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::{
    components::{ParentRoute, Redirect, Route, Router, Routes},
    path,
};

use crate::pages::accept_invite::AcceptInvitePage;
use crate::pages::accept_ownership::AcceptOwnershipPage;
use crate::pages::auth::{
    account_recovery::AccountRecoveryPage, account_recovery_complete::AccountRecoveryCompletePage,
    google_callback::GoogleCallbackPage, login::LoginPage, oauth_complete::OAuthCompletePage,
    passkey_recovery::PasskeyRecoveryPage, passkey_recovery_complete::PasskeyRecoveryCompletePage,
    passkey_signup_complete::PasskeySignupCompletePage, signup_complete::SignupCompletePage,
};
use crate::pages::beerxml::BeerXmlPage;
use crate::pages::brew_evidence::BrewEvidencePage;
use crate::pages::brew_history::{BrewAnalysisPage, RecipeBrewHistoryPage};
use crate::pages::design_system::DesignSystemPage;
use crate::pages::onboarding::OnboardingPage;
use crate::pages::recipe_editor::RecipeEditorPage;
use crate::pages::equipment::EquipmentPage;
use crate::pages::recipes::RecipesPage;
use crate::pages::settings::{
    profile::ProfilePage, security::security_tab::SecurityTab, settings_shell::{SettingsIndex, SettingsShell},
    team::TeamPage, workspace::WorkspacePage,
};
use crate::pages::welcome::WelcomePage;

/// Shell HTML page that loads the WASM bundle.
#[component]
pub fn Shell(#[prop(optional)] children: Option<Children>) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover"/>
                <title>"Krausen"</title>
                <leptos_meta::MetaTags/>
            </head>
            <body class="min-h-screen bg-background text-foreground antialiased">
                {children.map(|c| c())}
            </body>
        </html>
    }
}

use crate::components::layout::Layout;

/// Root Leptos application component.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    let editor_guard = crate::components::layout::EditorGuard::new();
    provide_context(editor_guard);
    #[cfg(target_arch = "wasm32")]
    editor_guard.install_history_guard();

    let auth_version = RwSignal::new(0u64);
    provide_context(auth_version);
    let user_ctx = LocalResource::new(move || {
        auth_version.get();
        crate::server_fns::context::get_user_context()
    });
    provide_context(user_ctx);

    view! {
        <crate::components::theme::ThemeProvider initial_preference="system".to_string()>
        <Router>
            <Routes fallback=|| view! {
                <div class="min-h-screen bg-background flex items-center justify-center p-8">
                    <div class="text-center max-w-md">
                        <h1 class="text-6xl font-display text-foreground mb-4">"404"</h1>
                        <p class="text-lg text-muted-foreground mb-8">"This page doesn\u{2019}t exist."</p>
                        <a href="/" class="inline-flex items-center justify-center px-5 py-3 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90 transition-colors">
                            "Go home"
                        </a>
                    </div>
                </div>
            }>
                // ── Public routes (no auth required) ─────────────────────────
                <Route path=path!("/login") view=|| view! { <LoginPage/> }/>
                <Route path=path!("/signup") view=|| view! { <LoginPage signup_mode=true/> }/>
                <Route path=path!("/signup/complete") view=SignupCompletePage/>
                <Route path=path!("/auth/google/callback") view=GoogleCallbackPage/>
                <Route path=path!("/account/recover") view=AccountRecoveryPage/>
                <Route path=path!("/account/recover/complete") view=AccountRecoveryCompletePage/>
                <Route path=path!("/auth/passkey-signup") view=PasskeySignupCompletePage/>
                <Route path=path!("/auth/recover-passkey") view=PasskeyRecoveryPage/>
                <Route path=path!("/auth/recover-passkey/complete") view=PasskeyRecoveryCompletePage/>
                <Route path=path!("/oauth-complete") view=OAuthCompletePage/>
                <Route path=path!("/welcome") view=WelcomePage/>
                // Flow page: no sidebar, requires auth (the server sends a
                // signed-out visit to /login?redirect=… and back).
                <Route path=path!("/accept-invite/:invitation_id") view=AcceptInvitePage/>

                // ── Authenticated routes (Layout provides sidebar + auth guard) ────
                <ParentRoute path=path!("") view=Layout>
                    <Route path=path!("/") view=|| view! { <Redirect path="/recipes"/> }/>
                    <Route path=path!("/onboarding") view=OnboardingPage/>
                    <Route path=path!("/recipes") view=RecipesPage/>
                    <Route path=path!("/equipment") view=EquipmentPage/>
                    <Route path=path!("/recipe-editor/:id") view=RecipeEditorPage/>
                    <Route path=path!("/brews/:id") view=BrewEvidencePage/>
                    <Route path=path!("/brews/:id/analysis") view=BrewAnalysisPage/>
                    <Route path=path!("/recipes/:id/brews") view=RecipeBrewHistoryPage/>
                    <Route path=path!("/beerxml") view=BeerXmlPage/>
                    <Route path=path!("/design-system") view=DesignSystemPage/>
                    <Route path=path!("/accept-ownership/:transfer_id") view=AcceptOwnershipPage/>

                    // Settings: scoped route navigation, safe return (IA-008)
                    <ParentRoute path=path!("/settings") view=SettingsShell>
                        <Route path=path!("") view=SettingsIndex/>
                        <Route path=path!("/profile") view=ProfilePage/>
                        <Route path=path!("/security") view=SecurityTab/>
                        <Route path=path!("/workspace") view=WorkspacePage/>
                        <Route path=path!("/team") view=TeamPage/>
                    </ParentRoute>
                </ParentRoute>
            </Routes>
        </Router>
        </crate::components::theme::ThemeProvider>
    }
}
