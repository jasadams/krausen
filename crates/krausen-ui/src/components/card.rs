// SPDX-License-Identifier: AGPL-3.0-or-later

//! Card components.

use leptos::prelude::*;

/// Card container.
#[component]
pub fn Card(
    #[prop(optional, into)]
    class: String,
    children: Children,
) -> impl IntoView {
    let classes = format!(
        "rounded-lg border border-border bg-card text-card-foreground shadow {}",
        class
    );
    view! {
        <div class=classes>
            {children()}
        </div>
    }
}

/// Card header section.
#[component]
pub fn CardHeader(
    #[prop(optional, into)]
    class: String,
    children: Children,
) -> impl IntoView {
    let classes = format!("flex flex-col space-y-1.5 p-6 {}", class);
    view! { <div class=classes>{children()}</div> }
}

/// Card content section.
#[component]
pub fn CardContent(
    #[prop(optional, into)]
    class: String,
    children: Children,
) -> impl IntoView {
    let classes = format!("p-6 pt-0 {}", class);
    view! { <div class=classes>{children()}</div> }
}

/// Card title.
#[component]
pub fn CardTitle(
    #[prop(optional, into)]
    class: String,
    children: Children,
) -> impl IntoView {
    let classes = format!("font-semibold leading-none tracking-tight {}", class);
    view! { <div class=classes>{children()}</div> }
}

/// Card description.
#[component]
pub fn CardDescription(
    #[prop(optional, into)]
    class: String,
    children: Children,
) -> impl IntoView {
    let classes = format!("text-sm text-muted-foreground {}", class);
    view! { <div class=classes>{children()}</div> }
}

/// Card footer.
#[component]
pub fn CardFooter(children: Children) -> impl IntoView {
    view! { <div class="flex items-center p-6 pt-0">{children()}</div> }
}
