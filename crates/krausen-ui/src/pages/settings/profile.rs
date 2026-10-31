// SPDX-License-Identifier: AGPL-3.0-or-later

//! Profile settings page — the first Leptos-rendered page in Krausen.
//!
//! All data fetching uses server functions instead of REST API calls.

use leptos::prelude::*;

use crate::components::{
    ActionStatus, Button, ButtonSize, Card, CardContent, CardDescription, CardHeader, CardTitle,
    INPUT_CLASS, Label, SettingsPageSkeleton,
};
use crate::server_fns::profile::*;
use crate::types::ProfileData;

// ─────────────────────────────────────────────────────────────────────────────
// Main page
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn ProfilePage() -> impl IntoView {
    let profile = Resource::new(|| (), |_| get_profile());
    // Auth error handling (expired access_token) is done globally in Layout.
    let query = leptos_router::hooks::use_query_map();
    let discovery = move || query.with(|q| q.get("invitation_discovery"));
    let invitations = Resource::new(discovery, |proof| async move {
        match proof {
            Some(proof) => get_pending_invitations(proof).await,
            None => Ok(crate::types::InvitationDataList {
                invitations: Vec::new(),
                truncated: false,
            }),
        }
    });

    view! {
        <div class="p-4 sm:p-6">
            <h2 class="text-xl font-display text-foreground mb-6">"Profile Settings"</h2>

            <Transition fallback=move || view! { <SettingsPageSkeleton /> }>
                {move || {
                    let profile_result = profile.get();
                    let invitations_result = invitations.get();

                    profile_result.map(|result| match result {
                        Ok(data) => {
                            let (inv_list, inv_truncated) = invitations_result
                                .and_then(|r| r.ok())
                                .map(|page| (page.invitations, page.truncated))
                                .unwrap_or_default();

                            let is_personal = data.is_personal_mode;
                            let discovery_id = discovery();
                            let data_profile = data.clone();
                            let data_appearance = data.clone();
                            let revision = RwSignal::new(data.revision);
                            view! {
                                <div class="space-y-6">
                                    <Show when=move || !is_personal>
                                        <ProfileInfoCard data=data_profile.clone() revision=revision/>
                                    </Show>
                                    <AppearanceCard data=data_appearance revision=revision/>
                                    <McpConnectionCard is_personal=is_personal/>
                                    <InvitationsCard invitations=inv_list.clone() truncated=inv_truncated discovery_id=discovery_id.clone()/>

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
fn ProfileInfoCard(data: ProfileData, revision: RwSignal<i64>) -> impl IntoView {
    let (name, set_name) = signal(data.name.clone().unwrap_or_default());
    let pending_request = RwSignal::new(None::<(String, i64, String)>);
    let save_action = Action::new(move |request: &(String, i64, String)| {
        let (name, expected, id) = request.clone();
        async move {
            let profile = save_profile_change(
                krausen_types::account::ProfileChange::Name { name },
                expected,
                id,
            )
            .await?;
            revision.set(profile.revision);
            pending_request.set(None);
            Ok::<_, ServerFnError>(profile)
        }
    });

    let on_blur = move |_| {
        let current = name.get();
        if !current.trim().is_empty() {
            let expected = revision.get_untracked();
            let request = pending_request
                .get_untracked()
                .filter(|(value, rev, _)| value == &current && *rev == expected)
                .or_else(|| {
                    crate::utils::request_id::new_request_id().map(|id| (current, expected, id))
                });
            if let Some(request) = request {
                pending_request.set(Some(request.clone()));
                save_action.dispatch(request);
            }
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
fn AppearanceCard(data: ProfileData, revision: RwSignal<i64>) -> impl IntoView {
    // Get the theme state from context (provided by ThemeProvider at app root)
    let theme_state = crate::components::theme::use_theme();

    // Initialize with the user's saved preference
    if let Some(state) = theme_state {
        state.preference.set(data.theme.clone());
    }

    let (current_theme, set_current_theme) = signal(data.theme.clone());
    let pending_request = RwSignal::new(None::<(String, i64, String)>);
    let save_action = Action::new(move |request: &(String, i64, String)| {
        let (theme, expected, id) = request.clone();
        async move {
            let profile = save_profile_change(
                krausen_types::account::ProfileChange::Theme { theme },
                expected,
                id,
            )
            .await?;
            revision.set(profile.revision);
            pending_request.set(None);
            Ok::<_, ServerFnError>(profile)
        }
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
                <ActionStatus action=save_action/>
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
                                        let expected = revision.get_untracked();
                                        let request = pending_request.get_untracked().filter(|(value, rev, _)| value == &value_for_click && *rev == expected)
                                            .or_else(|| crate::utils::request_id::new_request_id().map(|id| (value_for_click.clone(), expected, id)));
                                        if let Some(request) = request { pending_request.set(Some(request.clone())); action.dispatch(request); }
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

async fn save_profile_change(
    change: krausen_types::account::ProfileChange,
    expected: i64,
    request_id: String,
) -> Result<krausen_types::account::Profile, ServerFnError> {
    use krausen_types::account::ProfileSaveOutcome;
    let change_json = serde_json::to_string(&change)
        .map_err(|_| ServerFnError::new("Invalid profile change."))?;
    match update_profile(change_json, expected, request_id).await? {
        ProfileSaveOutcome::Saved(profile) => Ok(profile),
        ProfileSaveOutcome::Conflict { .. } => Err(ServerFnError::new(
            "Profile changed elsewhere. Reload to continue.",
        )),
        ProfileSaveOutcome::RequestIdentityConflict => {
            Err(ServerFnError::new("Request identity conflict."))
        }
        ProfileSaveOutcome::ResultExpired => Err(ServerFnError::new(
            "Request result expired. Reload to continue.",
        )),
        ProfileSaveOutcome::Forbidden => Err(ServerFnError::new("Access denied.")),
        ProfileSaveOutcome::Invalid => Err(ServerFnError::new("Invalid profile value.")),
        ProfileSaveOutcome::Retryable | ProfileSaveOutcome::Unavailable => {
            Err(ServerFnError::new("Could not save. Try again."))
        }
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
                let p = if port.is_empty() {
                    "3000".to_string()
                } else {
                    port
                };
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
            if port.is_empty() {
                "3000".to_string()
            } else {
                port
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            "3000".to_string()
        }
    };

    let claude_code_command =
        format!("claude mcp add --transport http krausen http://localhost:{mcp_port}/mcp");

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
fn InvitationsCard(
    invitations: Vec<crate::types::InvitationData>,
    truncated: bool,
    discovery_id: Option<String>,
) -> impl IntoView {
    let discovery_request = RwSignal::new(None::<String>);
    let response_requests = RwSignal::new(std::collections::HashMap::<String, String>::new());
    let identity_error = RwSignal::new(false);
    let discovery_action = Action::new(|request: &String| {
        let request = request.clone();
        async move { begin_invitation_discovery(request).await }
    });
    let response_action = Action::new(
        |args: &(
            String,
            String,
            String,
            krausen_types::account::InvitationAction,
            String,
        )| {
            let (workspace, invitation, proof, action, request) = args.clone();
            async move { begin_invitation_response(workspace, invitation, proof, action, request).await }
        },
    );
    let proof = discovery_id.clone();
    let begin_discovery = move |_| {
        let request = match discovery_request
            .get_untracked()
            .or_else(crate::utils::request_id::new_request_id)
        {
            Some(request) => request,
            None => {
                identity_error.set(true);
                return;
            }
        };
        identity_error.set(false);
        discovery_request.set(Some(request.clone()));
        discovery_action.dispatch(request);
    };
    view! {
        <Card><CardHeader><CardTitle>"Workspace invitations"</CardTitle>
            <CardDescription>"Verify your mailbox to view and respond to invitations."</CardDescription></CardHeader>
            <CardContent>
                <Button size=ButtonSize::Sm on:click=begin_discovery>"Verify mailbox"</Button>
                {move || identity_error.get().then(|| view! { <p>"Unable to create a secure request identity."</p> })}
                {move || discovery_action.value().get().map(|result| match result {
                    Ok(_) => view! { <p>"Confirmation is pending. Check your email when delivery is available; no invitation response has been made."</p> }.into_any(),
                    Err(error) => view! { <p>{error.to_string()}</p> }.into_any(),
                })}
                {move || response_action.value().get().map(|result| match result {
                    Ok(_) => view! { <p>"Confirmation is pending. Check your email when delivery is available; your invitation remains pending."</p> }.into_any(),
                    Err(error) => view! { <p>{error.to_string()}</p> }.into_any(),
                })}
                {truncated.then(|| view! {
                    <p class="text-sm text-muted-foreground">"Showing the first 100 pending invitations."</p>
                })}
                <div class="space-y-3">
                {invitations.into_iter().map(|inv| {
                    let invite_id = inv.invitation_id.clone(); let workspace = inv.workspace_id.clone();
                    let proof = proof.clone().unwrap_or_default();
                    let send = move |action: krausen_types::account::InvitationAction| {
                        let key = format!("{}:{}", action.as_str(), invite_id);
                        let request = response_requests.get_untracked().get(&key).cloned().or_else(crate::utils::request_id::new_request_id);
                        let Some(request) = request else { identity_error.set(true); return; };
                        response_requests.update(|items| { items.insert(key, request.clone()); });
                        response_action.dispatch((workspace.clone(), invite_id.clone(), proof.clone(), action, request));
                    };
                    let send_decline = send.clone();
                    view! { <div class="flex items-center justify-between p-4 border border-border rounded-lg">
                        <div><p>"Workspace: " {inv.workspace_id}</p><p>"Role: " {inv.role}</p></div>
                        <div class="flex gap-2"><Button size=ButtonSize::Sm on:click=move |_| send(krausen_types::account::InvitationAction::Accept)>"Accept"</Button>
                        <Button size=ButtonSize::Sm on:click=move |_| send_decline(krausen_types::account::InvitationAction::Decline)>"Decline"</Button></div></div> }
                }).collect_view()}
                </div>
            </CardContent></Card>
    }
}
