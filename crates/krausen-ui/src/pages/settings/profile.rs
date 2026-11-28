// SPDX-License-Identifier: AGPL-3.0-or-later

//! Profile settings page — the first Leptos-rendered page in Krausen.
//!
//! All data fetching uses server functions instead of REST API calls.

use leptos::prelude::*;

use crate::components::{
    ActionStatus, Card, CardContent, CardDescription, CardHeader, CardTitle, Label,
    SettingsPageSkeleton, INPUT_CLASS,
};
use crate::server_fns::profile::*;
use crate::server_fns::user_settings::{get_user_settings, save_user_settings};
use crate::types::ProfileData;

// ─────────────────────────────────────────────────────────────────────────────
// Main page
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn ProfilePage() -> impl IntoView {
    let profile = Resource::new(|| (), |_| get_profile());
    // Auth error handling (expired access_token) is done globally in Layout.
    let invitations = Resource::new(|| (), |_| get_pending_invitations());

    view! {
        <div class="p-4 sm:p-6">
            <h2 class="text-xl font-display text-foreground mb-6">"Profile Settings"</h2>

            <Transition fallback=move || view! { <SettingsPageSkeleton /> }>
                {move || {
                    let profile_result = profile.get();
                    let invitations_result = invitations.get();

                    profile_result.map(|result| match result {
                        Ok(data) => {
                            let inv_list = invitations_result
                                .and_then(|r| r.ok())
                                .unwrap_or_default();

                            let is_personal = data.is_personal_mode;
                            let has_invitations = !inv_list.is_empty();
                            let data_profile = data.clone();
                            let data_appearance = data.clone();
                            view! {
                                <div class="space-y-6">
                                    <Show when=move || !is_personal>
                                        <ProfileInfoCard data=data_profile.clone()/>
                                    </Show>
                                    <div id="appearance"><AppearanceCard data=data_appearance/></div>
                                    <div id="recipe-defaults"><RecipeDefaultsCard/></div>
                                    <McpConnectionCard is_personal=is_personal/>
                                    <Show when=move || !is_personal && has_invitations>
                                        <InvitationsCard invitations=inv_list.clone()/>
                                    </Show>
                                </div>
                            }.into_any()
                        },
                        Err(e) => {
                            let msg = e.to_string();
                            view! {
                                <Card>
                                    <div class="p-6">
                                        <p class="text-error-foreground">"Failed to load profile: " {msg}</p>
                                    </div>
                                </Card>
                            }.into_any()
                        },
                    })
                }}
            </Transition>
        </div>
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Profile Info Card
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn ProfileInfoCard(data: ProfileData) -> impl IntoView {
    let (name, set_name) = signal(data.name.clone().unwrap_or_default());
    let save_action = Action::new(|name: &String| {
        let name = name.clone();
        async move { update_profile_name(name).await }
    });

    let on_blur = move |_| {
        let current = name.get();
        if !current.trim().is_empty() {
            save_action.dispatch(current);
        }
    };

    view! {
        <Card>
            <CardHeader>
                <div class="flex items-center justify-between">
                    <div>
                        <CardTitle>"Profile Information"</CardTitle>
                    </div>
                    <ActionStatus action=save_action/>
                </div>
            </CardHeader>
            <CardContent>
                <div class="space-y-4">
                    <div class="space-y-2">
                        <Label>"Name"</Label>
                        <input
                            type="text"
                            class=INPUT_CLASS
                            placeholder="Your name"
                            prop:value=name
                            on:input=move |ev| set_name.set(event_target_value(&ev))
                            on:blur=on_blur
                        />
                    </div>
                    <div class="space-y-2">
                        <Label>"Email"</Label>
                        <input
                            type="email"
                            class="w-full px-3 py-2 border border-input rounded-md bg-muted text-foreground cursor-not-allowed"
                            disabled=true
                            prop:value=data.email.clone()
                        />
                        <p class="text-xs text-muted-foreground">"Email cannot be changed"</p>
                    </div>
                </div>
            </CardContent>
        </Card>
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Appearance Card
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn AppearanceCard(data: ProfileData) -> impl IntoView {
    // Get the theme state from context (provided by ThemeProvider at app root)
    let theme_state = crate::components::theme::use_theme();

    // Initialize with the user's saved preference
    if let Some(state) = theme_state {
        state.preference.set(data.theme.clone());
    }

    let (current_theme, set_current_theme) = signal(data.theme.clone());
    let save_action = Action::new(|theme: &String| {
        let theme = theme.clone();
        async move { update_theme(theme).await }
    });

    static THEME_OPTIONS: &[(&str, &str, phosphor_leptos::IconData)] = &[
        ("light", "Light", phosphor_leptos::SUN),
        ("dark", "Dark", phosphor_leptos::MOON),
        ("system", "System", phosphor_leptos::MONITOR),
    ];

    view! {
        <Card>
            <CardHeader>
                <CardTitle>"Appearance"</CardTitle>
                <CardDescription>"Choose how Krausen looks to you."</CardDescription>
            </CardHeader>
            <CardContent>
                <div class="flex flex-wrap gap-3">
                    {THEME_OPTIONS.iter().map(|(value, label, icon_data)| {
                        let value_str = value.to_string();
                        let value_for_weight = value.to_string();
                        let value_for_click = value.to_string();
                        let label_str = label.to_string();
                        let icon = *icon_data;
                        let theme_weight = Memo::new(move |_| {
                            if current_theme.get() == value_for_weight {
                                phosphor_leptos::IconWeight::Fill
                            } else {
                                phosphor_leptos::IconWeight::Light
                            }
                        });
                        view! {
                            <button
                                class=move || {
                                    let base = "flex items-center gap-2 px-4 py-2 rounded-lg border-2 text-sm font-medium transition-all";
                                    if current_theme.get() == value_str {
                                        format!("{base} border-primary bg-primary/10 text-primary")
                                    } else {
                                        format!("{base} border-border text-muted-foreground hover:border-border/80 hover:text-foreground")
                                    }
                                }
                                on:click={
                                    let set_local = set_current_theme;
                                    let action = save_action;
                                    move |_| {
                                        // Update local UI state
                                        set_local.set(value_for_click.clone());
                                        // Apply theme to DOM + localStorage immediately
                                        crate::components::theme::set_theme(&value_for_click);
                                        crate::components::theme::save_theme_to_local_storage(&value_for_click);
                                        // Persist to server
                                        action.dispatch(value_for_click.clone());
                                    }
                                }
                            >
                                <phosphor_leptos::Icon icon=icon weight=theme_weight size="16px"/>
                                <span>{label_str}</span>
                            </button>
                        }
                    }).collect_view()}
                </div>
            </CardContent>
        </Card>
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Recipe Defaults Card
// ─────────────────────────────────────────────────────────────────────────────

/// What a field's text means as a default batch size: nothing (unset) or a number to send.
/// Whether the number is a valid batch size is the application's rule, answered by `Invalid`;
/// only text that is not a finite number cannot be sent at all.
fn parse_default_batch(text: &str) -> Result<Option<f64>, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    match text.parse::<f64>() {
        Ok(litres) if litres.is_finite() => Ok(Some(litres)),
        _ => Err("Enter a number of litres."),
    }
}

#[component]
fn RecipeDefaultsCard() -> impl IntoView {
    use krausen_types::user_settings::{UserSettings, UserSettingsOutcome};

    let settings = Resource::new(|| (), |_| get_user_settings());
    let (text, set_text) = signal(String::new());
    // What the server holds: `None` until it has been read, so nothing is saved over an
    // unread setting; `Some(None)` is a setting the user has not made.
    let (stored, set_stored) = signal(None::<Option<f64>>);
    let (message, set_message) = signal(None::<(bool, String)>);
    // The text of the save in flight, so its answer never replaces what was typed since.
    let submitted = StoredValue::new(String::new());
    let save_action = Action::new(|settings: &UserSettings| {
        let settings = settings.clone();
        async move { save_user_settings(settings).await }
    });
    let saving = save_action.pending();

    // Show what is stored once it has loaded, and say so when it cannot be.
    Effect::new(move |_| match settings.get() {
        Some(Ok(UserSettingsOutcome::Settings(loaded))) => {
            set_text.set(loaded.default_batch_size_l.map(|l| l.to_string()).unwrap_or_default());
            set_stored.set(Some(loaded.default_batch_size_l));
        }
        Some(_) => set_message.set(Some((false, "Your default batch size could not be loaded. Reload the page to change it.".to_string()))),
        None => {}
    });
    // Report what the server did with the last save.
    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            set_message.set(Some(match result {
                Ok(UserSettingsOutcome::Settings(saved)) => {
                    set_stored.set(Some(saved.default_batch_size_l));
                    if text.get_untracked() == submitted.get_value() {
                        set_text.set(saved.default_batch_size_l.map(|l| l.to_string()).unwrap_or_default());
                    }
                    (true, "Saved".to_string())
                }
                Ok(UserSettingsOutcome::Invalid) => (false, "Enter a batch size greater than zero, in litres.".to_string()),
                Ok(_) => (false, "Your default batch size could not be saved. Try again.".to_string()),
                Err(e) => (false, e.to_string()),
            }));
        }
    });
    let on_blur = move |_| {
        // Nothing to save before the setting is read, while a save is in flight, or when the
        // text still means what is stored.
        let Some(stored_value) = stored.get_untracked() else { return };
        if saving.get_untracked() {
            return;
        }
        match parse_default_batch(&text.get_untracked()) {
            Ok(value) if value == stored_value => {}
            Ok(default_batch_size_l) => {
                submitted.set_value(text.get_untracked());
                save_action.dispatch(UserSettings { default_batch_size_l });
            }
            Err(problem) => set_message.set(Some((false, problem.to_string()))),
        }
    };

    view! {
        <Card>
            <CardHeader>
                <div class="flex items-center justify-between">
                    <div>
                        <CardTitle>"Recipe defaults"</CardTitle>
                        <CardDescription>"What every new recipe you start begins with. You can still change it in each recipe."</CardDescription>
                    </div>
                    <span class="text-xs" aria-live="polite" data-testid="default-batch-status">
                        {move || message.get().map(|(ok, msg)| {
                            let class = if ok { "text-success-foreground" } else { "text-error-foreground" };
                            view! { <span class=class>{msg}</span> }
                        })}
                    </span>
                </div>
            </CardHeader>
            <CardContent>
                <div class="space-y-2">
                    <Label html_for="default-batch-size">"Default batch size (L)"</Label>
                    <input
                        id="default-batch-size"
                        type="text"
                        inputmode="decimal"
                        class=INPUT_CLASS
                        placeholder="Not set"
                        disabled=move || stored.get().is_none() || saving.get()
                        prop:value=text
                        on:input=move |ev| {
                            set_text.set(event_target_value(&ev));
                            set_message.set(None);
                        }
                        on:blur=on_blur
                    />
                    <p class="text-xs text-muted-foreground">"Leave empty and new recipes start with no batch size."</p>
                </div>
            </CardContent>
        </Card>
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MCP Connection Card
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn McpConnectionCard(is_personal: bool) -> impl IntoView {
    // Derive MCP URL from window.location on the client.
    // On the server (SSR), use a sensible default that will be replaced on hydration.
    let mcp_url = {
        #[cfg(target_arch = "wasm32")]
        {
            let window = web_sys::window().expect("no global window");
            let location = window.location();
            let port = location.port().unwrap_or_default();
            let hostname = location.hostname().unwrap_or_default();
            let origin = location.origin().unwrap_or_default();

            if is_personal {
                let p = if port.is_empty() { "3000".to_string() } else { port };
                format!("http://localhost:{p}/mcp")
            } else if hostname == "localhost" || hostname == "127.0.0.1" {
                format!("http://{hostname}:8002/mcp")
            } else {
                format!("{origin}/mcp")
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            "/mcp".to_string()
        }
    };

    let mcp_port = {
        #[cfg(target_arch = "wasm32")]
        {
            let port = web_sys::window()
                .and_then(|w| w.location().port().ok())
                .unwrap_or_default();
            if port.is_empty() { "3000".to_string() } else { port }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            "3000".to_string()
        }
    };

    let claude_code_command = format!(
        "claude mcp add --transport http krausen http://localhost:{mcp_port}/mcp"
    );

    let claude_desktop_config = format!(
        "{{\n  \"mcpServers\": {{\n    \"krausen\": {{\n      \"url\": \"{mcp_url}\"\n    }}\n  }}\n}}"
    );

    // Build the Cursor deep-link URL (base64-encoded config, client-only)
    #[cfg(target_arch = "wasm32")]
    let cursor_config_b64 = {
        use base64::Engine;
        let cursor_config_json = format!("{{\"type\":\"http\",\"url\":\"{mcp_url}\"}}");
        base64::engine::general_purpose::STANDARD.encode(cursor_config_json.as_bytes())
    };

    // Clones for closures
    let url_for_copy = mcp_url.clone();
    let cmd_for_copy = claude_code_command.clone();
    let config_for_copy = claude_desktop_config.clone();

    view! {
        <Card>
            <CardHeader>
                <CardTitle>
                    <span class="flex items-center gap-2">
                        <phosphor_leptos::Icon icon=phosphor_leptos::PLUG weight=phosphor_leptos::IconWeight::Light size="20px"/>
                        "MCP Connection"
                    </span>
                </CardTitle>
                <CardDescription>
                    "Connect Krausen to any MCP-compatible client for AI-powered data analysis."
                </CardDescription>
            </CardHeader>
            <CardContent>
                <div class="space-y-6">
                    // -- Server URL --
                    <div class="space-y-3">
                        <h4 class="font-medium text-foreground">"Server URL"</h4>
                        {if !is_personal {
                            view! {
                                <p class="text-sm text-muted-foreground">
                                    "Use this URL to connect from any MCP client. You\u{2019}ll be prompted to authorize via your browser."
                                </p>
                            }.into_any()
                        } else {
                            view! {
                                <p class="text-sm text-muted-foreground">
                                    "Use this URL to connect from any MCP client."
                                </p>
                            }.into_any()
                        }}
                        <div class="relative">
                            <pre class="p-4 bg-muted rounded-md text-sm overflow-x-auto pr-12">
                                {mcp_url.clone()}
                            </pre>
                            <CopyButton text=url_for_copy/>
                        </div>
                    </div>

                    // -- Claude Code (personal mode only) --
                    {if is_personal {
                        view! {
                            <div class="space-y-3 pt-4 border-t border-border">
                                <h4 class="font-medium text-foreground">"Claude Code"</h4>
                                <p class="text-sm text-muted-foreground">
                                    "Run this command in your terminal to connect Claude Code."
                                </p>
                                <div class="relative">
                                    <pre class="p-4 bg-muted rounded-md text-sm overflow-x-auto pr-12">
                                        {claude_code_command}
                                    </pre>
                                    <CopyButton text=cmd_for_copy/>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! { <span class="hidden"></span> }.into_any()
                    }}

                    // -- Claude Desktop (personal mode only) --
                    {if is_personal {
                        view! {
                            <div class="space-y-3 pt-4 border-t border-border">
                                <h4 class="font-medium text-foreground">"Claude Desktop"</h4>
                                <p class="text-sm text-muted-foreground">
                                    "Add this to your Claude Desktop configuration file."
                                </p>
                                <div class="relative">
                                    <pre class="p-4 bg-muted rounded-md text-sm overflow-x-auto pr-12">
                                        {claude_desktop_config}
                                    </pre>
                                    <CopyButton text=config_for_copy/>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! { <span class="hidden"></span> }.into_any()
                    }}

                    // -- Cursor One-Click --
                    <div class="space-y-3 pt-4 border-t border-border">
                        <h4 class="font-medium text-foreground">"Cursor"</h4>
                        <p class="text-sm text-muted-foreground">
                            "One-click install for Cursor users."
                        </p>
                        {
                            #[cfg(target_arch = "wasm32")]
                            {
                                let cursor_url = format!(
                                    "cursor://anysphere.cursor-deeplink/mcp/install?name=krausen&config={cursor_config_b64}"
                                );
                                view! {
                                    <a
                                        href=cursor_url
                                        target="_blank"
                                        class="inline-flex items-center gap-2 px-4 py-2 rounded-md border border-border text-sm font-medium text-foreground hover:bg-secondary transition-colors"
                                    >
                                        <phosphor_leptos::Icon icon=phosphor_leptos::ARROW_SQUARE_OUT weight=phosphor_leptos::IconWeight::Light size="16px"/>
                                        "Connect with Cursor"
                                    </a>
                                }.into_any()
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                view! {
                                    <a
                                        href="#"
                                        class="inline-flex items-center gap-2 px-4 py-2 rounded-md border border-border text-sm font-medium text-foreground hover:bg-secondary transition-colors"
                                    >
                                        <phosphor_leptos::Icon icon=phosphor_leptos::ARROW_SQUARE_OUT weight=phosphor_leptos::IconWeight::Light size="16px"/>
                                        "Connect with Cursor"
                                    </a>
                                }.into_any()
                            }
                        }
                    </div>
                </div>
            </CardContent>
        </Card>
    }
}

/// Small copy-to-clipboard button used inside MCP Connection card.
#[component]
fn CopyButton(text: String) -> impl IntoView {
    let (copied, set_copied) = signal(false);
    let text = text.clone();

    let on_click = move |_| {
        let text = text.clone();
        let set_copied = set_copied;

        #[cfg(target_arch = "wasm32")]
        {
            leptos::task::spawn_local(async move {
                if let Some(window) = web_sys::window() {
                    let clipboard = window.navigator().clipboard();
                    let promise = clipboard.write_text(&text);
                    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                    set_copied.set(true);
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    set_copied.set(false);
                }
            });
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (text, set_copied);
        }
    };

    view! {
        <button
            class="absolute top-2 right-2 p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
            on:click=on_click
            title="Copy to clipboard"
        >
            {move || {
                if copied.get() {
                    view! { <phosphor_leptos::Icon icon=phosphor_leptos::CHECK weight=phosphor_leptos::IconWeight::Bold size="16px"/> }.into_any()
                } else {
                    view! { <phosphor_leptos::Icon icon=phosphor_leptos::COPY weight=phosphor_leptos::IconWeight::Light size="16px"/> }.into_any()
                }
            }}
        </button>
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invitations Card
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn InvitationsCard(invitations: Vec<crate::types::InvitationData>) -> impl IntoView {
    let (inv_list, set_inv_list) = signal(invitations);

    let accept_action = Action::new(|id: &String| {
        let id = id.clone();
        async move { accept_invitation(id).await }
    });
    let decline_action = Action::new(|id: &String| {
        let id = id.clone();
        async move { decline_invitation(id).await }
    });

    view! {
        <Card>
            <CardHeader>
                <CardTitle>"Pending Invitations"</CardTitle>
                <CardDescription>"Workspace invitations waiting for your response."</CardDescription>
            </CardHeader>
            <CardContent>
                <div class="space-y-3">
                    <For
                        each=move || inv_list.get()
                        key=|inv| inv.invitation_id.clone()
                        let:inv
                    >
                        {
                            let inv_id_accept = inv.invitation_id.clone();
                            let inv_id_decline = inv.invitation_id.clone();
                            let inv_id_remove = inv.invitation_id.clone();
                            view! {
                                <div class="flex items-center justify-between p-4 border border-border rounded-lg">
                                    <div>
                                        <p class="text-sm font-medium text-foreground">
                                            "Workspace: " {inv.workspace_id.clone()}
                                        </p>
                                        <p class="text-xs text-muted-foreground">
                                            "Role: " {inv.role.clone()}
                                        </p>
                                    </div>
                                    <div class="flex gap-2">
                                        <button
                                            class="px-3 py-1.5 rounded-md text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
                                            on:click={
                                                let set_list = set_inv_list;
                                                let id = inv_id_accept.clone();
                                                let remove_id = inv_id_remove.clone();
                                                move |_| {
                                                    accept_action.dispatch(id.clone());
                                                    set_list.update(|list| list.retain(|i| i.invitation_id != remove_id));
                                                }
                                            }
                                        >
                                            "Accept"
                                        </button>
                                        <button
                                            class="px-3 py-1.5 rounded-md text-xs font-medium border border-border text-muted-foreground hover:text-foreground transition-colors"
                                            on:click={
                                                let set_list = set_inv_list;
                                                let id = inv_id_decline.clone();
                                                let remove_id = inv_id_remove;
                                                move |_| {
                                                    decline_action.dispatch(id.clone());
                                                    set_list.update(|list| list.retain(|i| i.invitation_id != remove_id));
                                                }
                                            }
                                        >
                                            "Decline"
                                        </button>
                                    </div>
                                </div>
                            }
                        }
                    </For>
                </div>
            </CardContent>
        </Card>
    }
}


// ─────────────────────────────────────────────────────────────────────────────
// Clear local data — wipes IndexedDB cache and reloads
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn ClearLocalDataCard() -> impl IntoView {
    #[cfg(target_arch = "wasm32")]
    {
        use leptos::task::spawn_local;

        let on_clear = move |_: leptos::ev::MouseEvent| {
            spawn_local(async move {
                // TODO: implement client-side cache clearing
                let _ = web_sys::window()
                    .and_then(|w| w.location().reload().ok());
            });
        };

        view! {
            <Card>
                <CardHeader>
                    <CardTitle>"Local Data"</CardTitle>
                    <CardDescription>
                        "Clear cached data stored in your browser. Use this if data appears missing or stale. Your account data is not affected."
                    </CardDescription>
                </CardHeader>
                <CardContent>
                    <crate::components::Button
                        variant=crate::components::ButtonVariant::Secondary
                        size=crate::components::ButtonSize::Sm
                        on:click=on_clear
                    >
                        "Clear local data and reload"
                    </crate::components::Button>
                </CardContent>
            </Card>
        }.into_any()
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        view! { <span></span> }.into_any()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_default_batch;

    #[test]
    fn default_batch_text_is_unset_litres_or_refused() {
        assert_eq!(parse_default_batch(""), Ok(None));
        assert_eq!(parse_default_batch("  "), Ok(None));
        assert_eq!(parse_default_batch(" 23 "), Ok(Some(23.0)));
        assert_eq!(parse_default_batch("22.5"), Ok(Some(22.5)));
        for bad in ["abc", "1e", "NaN", "inf"] {
            assert!(parse_default_batch(bad).is_err(), "{bad}");
        }
    }
}
