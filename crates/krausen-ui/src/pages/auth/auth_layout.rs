// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared layout component for all auth pages (login, signup, recovery).
//!
//! Left panel: dark background with Krausen branding (desktop only).
//! Right panel: centered content slot with title, subtitle, footer.

use crate::components::brewery_kit::BreweryBrand;
use leptos::prelude::*;

/// Shared layout for all authentication pages.
///
/// Renders the two-panel auth layout:
/// - Left: dark branded panel (hidden on mobile, `lg:flex lg:w-1/2`)
/// - Right: centered content with mobile logo, title/subtitle, children slot, and footer
///
/// Title and subtitle are reactive (`Signal<String>`) so the login page can
/// toggle between "Welcome back" / "Create your account" without remounting.
#[component]
pub fn AuthLayout(
    /// Title text (e.g., "Welcome back" or "Create your account").
    #[prop(into)]
    title: Signal<String>,
    /// Subtitle text (e.g., "Sign in to your account to continue").
    #[prop(into)]
    subtitle: Signal<String>,
    /// Main content slot (form fields, buttons, etc.).
    children: Children,
) -> impl IntoView {
    view! {
        // Outer container — theme-aware, respects user's localStorage preference
        <div class="min-h-screen bg-background flex">

            // ── Left side — Full Krausen logo on warm-stone panel ─────────────
            // Hidden below 1024px (lg breakpoint); form fills the whole
            // viewport on mobile and tablets. Decorative — `aria-hidden`.
            <div
                class="hidden lg:flex lg:w-1/2 relative overflow-hidden auth-brand-panel items-center justify-center"
                aria-hidden="true"
                inert=""
            >
                <BreweryBrand inverse=true/>

                // Bottom marginalia
                <div class="absolute bottom-10 left-12 right-12 z-10 flex items-center justify-between font-mono text-[10px] uppercase text-[color:rgba(245,243,239,0.30)]" style="letter-spacing:0.18em;">
                    <span>"KRAUSEN"</span>
                    <span>"種 \u{00B7} KRAUSEN"</span>
                </div>
            </div>

            // ── Right side — Form content ───────────────────────────────────
            <div class="w-full lg:w-1/2 flex items-center justify-center p-8">
                <div class="w-full max-w-md">

                    // ── Mobile logo + title/subtitle ────────────────────────
                    <div class="text-center mb-8">
                        // Mobile logo
                        <div class="lg:hidden mb-6">
                            <BreweryBrand/>
                        </div>
                        // Title — page-level landmark, 2xl token = 30px = text-3xl, Instrument Serif
                        <h1 class="text-3xl font-display text-foreground mb-2">
                            {title}
                        </h1>
                        <p class="text-[18px] text-muted-foreground italic font-display leading-tight mb-8">
                            {subtitle}
                        </p>
                    </div>

                    // ── Main content slot ────────────────────────────────────
                    {children()}

                    // ── Footer ───────────────────────────────────────────────
                    <div class="mt-8 pt-6 border-t border-border space-y-3">
                        // Flex row — no `space-x-*`; each link carries its own
                        // `py-3 px-2` padding so the hit area reaches the WCAG
                        // 2.5.5 AAA minimum of 44x44px without changing visible
                        // text size. `py-3` = 12px top + 12px bottom + ~20px
                        // line-height on `text-sm` = 44px total.
                        <div class="flex justify-center items-center text-sm text-muted-foreground">
                            <a
                                href="https://krausen.ai/privacy"
                                target="_blank"
                                rel="noopener noreferrer"
                                class="inline-block py-3 px-2 hover:text-foreground transition-colors"
                            >
                                "Privacy"
                            </a>
                            <span aria-hidden="true">"·"</span>
                            <a
                                href="https://krausen.ai/terms"
                                target="_blank"
                                rel="noopener noreferrer"
                                class="inline-block py-3 px-2 hover:text-foreground transition-colors"
                            >
                                "Terms"
                            </a>
                            <span aria-hidden="true">"·"</span>
                            <a
                                href="https://krausen.ai"
                                target="_blank"
                                rel="noopener noreferrer"
                                class="inline-block py-3 px-2 hover:text-foreground transition-colors"
                            >
                                "About"
                            </a>
                            <span aria-hidden="true">"·"</span>
                            <a
                                href="https://status.krausen.ai"
                                target="_blank"
                                rel="noopener noreferrer"
                                class="inline-block py-3 px-2 hover:text-foreground transition-colors"
                            >
                                "Status"
                            </a>
                        </div>
                        <p class="text-xs text-muted-foreground text-center">
                            "All trademarks are property of their respective owners."
                        </p>
                    </div>
                </div>
            </div>
        </div>
    }
}
