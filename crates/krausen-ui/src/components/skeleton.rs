// SPDX-License-Identifier: AGPL-3.0-or-later

//! Skeleton loading placeholder.
//!
//! Renders an animated placeholder div used while content is loading.

use leptos::prelude::*;

/// Base classes.
const BASE: &str = "bg-accent animate-pulse rounded-md";

/// Skeleton component.
///
/// A simple animated placeholder that indicates content is loading.
/// Used by Data Sources and other views that need loading states.
#[component]
pub fn Skeleton(
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let classes = format!("{} {}", BASE, class);

    view! {
        <div data-slot="skeleton" class=classes />
    }
}

/// Settings page skeleton — displayed while settings data loads.
///
/// Shows a series of card-shaped placeholders matching the settings page layout.
#[component]
pub fn SettingsPageSkeleton() -> impl IntoView {
    view! {
        <div class="space-y-6">
            <Skeleton class="h-48 w-full"/>
            <Skeleton class="h-32 w-full"/>
            <Skeleton class="h-32 w-full"/>
        </div>
    }
}
