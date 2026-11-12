# Graph Report - kra40-import-spacing  (2026-11-12)

## Corpus Check
- 389 files · ~366,695 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 30 file(s) not represented in the graph (top: .ttf 16, .css 7, (none) 5)

## Summary
- 5327 nodes · 12564 edges · 227 communities (194 shown, 33 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 444 edges (avg confidence: 0.82)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `1ae09524`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- src/brew_evidence.rs
- ActorContext
- extract_context
- mcp.rs
- tests/process_summary.rs
- src/components/mod.rs
- brew_evidence_store.rs
- krausen-app/src/catalogue.rs
- printable_recipe.rs
- auth.rs
- ButtonVariant
- TransitionBrewRequest
- auth_service.rs
- Error
- RecipeRevisionHistory
- encryption.rs
- operations.rs
- RecipeSaveOutcome
- E2E Test Plan
- presets.rs
- pages/brew_history.rs
- recover_request
- krausen-types/src/brew_history.rs
- workspace_service.rs
- native_store.rs
- prelude
- mcp_session_manager.rs
- app.js
- krausen-app/src/scaling.rs
- settings/team.rs
- oauth.rs
- app.rs
- ScaleChange
- convert.rs
- mibu.rs
- sql_compat.rs
- accept_ownership.rs
- RecipeDocumentV1
- email_service.rs
- super
- native.rs
- project_xml
- StartBrewOutcome
- popover.rs
- recipe_crud.rs
- ActorContext
- RecipeComposition
- mcp_auth.rs
- pages/brew_evidence.rs
- parse.rs
- server_fns/brew.rs
- krausen-calc/src/lib.rs
- ScaleApplyOutcome
- foundation
- manager.rs
- reference_recipe.rs
- Validation
- RecipeDraft
- @playwright/test
- DbTx
- krausen-app/src/interchange.rs
- Recipe organization
- recipe_comparison.rs
- BadgeVariant
- catalogue_picker.rs
- KVPool
- token_service.rs
- serialize
- user_service.rs
- layout.rs
- icon
- scripts
- onboarding_service.rs
- tests/brew_history.rs
- oauth_service.rs
- Equipment
- PreviewOutcome
- jwt.rs
- Chilling
- brew-start.spec.ts
- db
- PageState
- tests/brew_evidence.rs
- pages/recipe_editor.rs
- BrewStatusV1
- CalcError
- beerxml.rs
- src/webauthn.rs
- brewery.rs
- account_recovery_complete.rs
- pages/brew_start.rs
- websocket.rs
- constants.rs
- ExportOutcome
- AppState
- loginUser
- middleware/mod.rs
- leptos_frontend.rs
- signup_flow.rs
- BrewFeedbackApplyOutcome
- RecipeReadOutcome
- retry.rs
- with
- 4. Architecture
- server_fns/recipes.rs
- google_oauth.rs
- Recipe
- record.rs
- krausen-types/src/interchange.rs
- Button
- docs/README.md
- extract_context
- McpHandler
- tests/brew_start.rs
- Target
- WebSocketManager
- ProcessSummaryOutcome
- MeasurementOutcome
- signup_complete.rs
- krausen-app/src/process_summary.rs
- krausen-types/src/process_summary.rs
- MeasurementOutcome
- InMemoryKVStore
- EquipmentSettingsV1
- toast.rs
- Shared application architecture
- MessageType
- Field
- server_fns/brew_history.rs
- totp.rs
- Navigation requirements
- krausen-calc/src/formulation.rs
- db.rs
- draft_validation.rs
- write.rs
- DbPool
- EquipmentOutcome
- PreviewGate
- Krausen Coding Standards
- DEFAULT_TEST_USER
- server/src/lib.rs
- ScaleOutcome
- bootstrap.rs
- migration_orders.rs
- RedisKVStore
- middleware.rs
- Recipe
- 4. Architecture
- onboarding_service.rs
- kv_store_memory.rs
- tests/brew_start.rs
- EmptyStateVariant
- PreviewOutcome
- Validation
- Krausen design system
- Recipes, revisions and historical brews
- components.js
- Prioritized findings
- shell-source-check.cjs
- steps
- BrewLifecycleOutcome
- theme.rs
- BrewFeedbackPreviewOutcome
- switch.rs
- ImportCommitOutcome
- manager
- RawElement
- native_recipe.rs
- select.rs
- StatusBadgeVariant
- SearchInput
- Calculation and interchange contracts
- verify-deployment-app
- browser_refresh.rs
- OriginalOutcome
- IngredientSection
- server/src/main.rs
- settings_shell.rs
- TransferStatus
- krausen-ui/package.json
- import_report.rs
- App Starter Skeleton — historical plan
- profile-browser.sh
- Krausen
- colour.rs
- bootstrap.rs
- mode
- krausen-ui
- rate_limiter.rs
- deserialize
- server_fns/scaling.rs
- round_trip.rs
- invitation-expiry.spec.ts
- Krausen recipe and component prototype
- Krausen agent guidance
- Scalar<T>
- Targets
- KVStore
- DuplicateOutcome
- Architecture adoption and implementation contract
- icon
- image
- NativeMigration
- krausen-app/src/recipe_library.rs
- Krausen documentation
- deploy-notify
- Krausen release configuration
- full_table_name_expr_prefixed
- auth_refresh.rs
- EditorAccess
- krausen-app/README.md
- kra20_baseline_4fc8313/README.md
- build.sh
- fonts/README.md
- AdditionalText
- Tooltip
- native_transactions.rs

## God Nodes (most connected - your core abstractions)
1. `DbPool` - 331 edges
2. `ActorContext` - 149 edges
3. `RecipeDocumentV1` - 112 edges
4. `DbTx` - 109 edges
5. `OperationIntent` - 105 edges
6. `authorize_workspace()` - 52 edges
7. `KVPool` - 52 edges
8. `RecipeDraft` - 49 edges
9. `@playwright/test` - 48 edges
10. `ButtonVariant` - 46 edges

## Surprising Connections (you probably didn't know these)
- `Style references while formulating` --references--> `StyleRange()`  [INFERRED]
  DESIGN.md → docs/design/prototype/components.js
- `Searchable ingredients and a personal workspace library` --references--> `CatalogueItem()`  [INFERRED]
  DESIGN.md → docs/design/prototype/components.js
- `Component families and production mapping` --references--> `Button()`  [INFERRED]
  DESIGN.md → docs/design/prototype/components.js
- `Component families and production mapping` --references--> `Badge()`  [INFERRED]
  DESIGN.md → docs/design/prototype/components.js
- `Style reference data` --references--> `StyleRange()`  [INFERRED]
  docs/design/prototype/README.md → docs/design/prototype/components.js

## Import Cycles
- 1-file cycle: `apps/server/src/leptos_frontend.rs -> apps/server/src/leptos_frontend.rs`
- 1-file cycle: `apps/server/src/routes/oauth.rs -> apps/server/src/routes/oauth.rs`
- 1-file cycle: `crates/krausen-ui/src/pages/recipe_recovery.rs -> crates/krausen-ui/src/pages/recipe_recovery.rs`
- 1-file cycle: `crates/krausen-ui/src/pages/recipe_preview_gate.rs -> crates/krausen-ui/src/pages/recipe_preview_gate.rs`
- 2-file cycle: `crates/krausen-types/src/brewery.rs -> crates/krausen-types/src/formulation.rs -> crates/krausen-types/src/brewery.rs`

## Communities (227 total, 33 thin omitted)

### Community 0 - "src/brew_evidence.rs"
Cohesion: 0.05
Nodes (44): actual(), decode_cursor(), encode_cursor(), EvidenceCursor, execute(), finite(), finite_non_negative(), get_actual() (+36 more)

### Community 1 - "ActorContext"
Cohesion: 0.09
Nodes (56): begin(), feedback_apply(), feedback_apply_inner(), recipe_head(), digest(), get(), identifiers(), lock_workspace() (+48 more)

### Community 2 - "extract_context"
Cohesion: 0.06
Nodes (44): ConfirmDialog(), add_passkey_flow(), arraybuffer_to_base64url(), base64url_decode(), browser_create_credential(), check_webauthn_support(), detect_device_icon(), DeviceIconKind (+36 more)

### Community 3 - "mcp.rs"
Cohesion: 0.03
Nodes (32): authenticate(), BeerXmlExport, BeerXmlImport, BeerXmlOriginal, BeerXmlPreview, BrewActualInput, BrewArchiveInput, BrewComparisonInput (+24 more)

### Community 4 - "tests/process_summary.rs"
Cohesion: 0.17
Nodes (9): complete_draft(), create_equipment(), equipment_request(), exercise(), intent(), postgres_preview_rechecks_authority_after_profile_lock_wait(), postgres_process_summary(), save_recipe() (+1 more)

### Community 5 - "src/components/mod.rs"
Cohesion: 0.12
Nodes (26): BreweryActionGroup(), BreweryAddition(), BreweryBadge(), BreweryBeerPortrait(), BreweryBrand(), BreweryDialog(), BreweryDisclosure(), BreweryField() (+18 more)

### Community 6 - "brew_evidence_store.rs"
Cohesion: 0.06
Nodes (60): resolve_draft_sources(), ACTUAL_BASE_SELECT, actual_projection_retains_boil_duration_and_does_not_mutate_plan(), ACTUAL_SELECT, ActualRow, BrewEvidenceState, BrewStateRow, insert_actual_formulation() (+52 more)

### Community 7 - "krausen-app/src/catalogue.rs"
Cohesion: 0.13
Nodes (12): backfill_legacy_recipes(), MigrationReport, recover_legacy_recipe(), reject_legacy_authoring(), exercise(), postgres_foundation(), sqlite_foundation(), exercise() (+4 more)

### Community 8 - "printable_recipe.rs"
Cohesion: 0.09
Nodes (21): ChillingV1, Counterflow, Forced, Natural, RangeStatus, Above, Below, Within (+13 more)

### Community 9 - "auth.rs"
Cohesion: 0.05
Nodes (58): CheckEmailView(), CredentialsView(), LOGIN_SUBTITLES, LoginPage(), LoginView, CheckEmail, Credentials, Signup (+50 more)

### Community 10 - "ButtonVariant"
Cohesion: 0.04
Nodes (42): BASE, Button(), ButtonLink(), ButtonSize, Default, Icon, IconSm, IconXs (+34 more)

### Community 11 - "TransitionBrewRequest"
Cohesion: 0.06
Nodes (35): IngredientOutcome, Archived, Conflict, ContextChanged, Duplicate, Entries, Entry, Forbidden (+27 more)

### Community 12 - "auth_service.rs"
Cohesion: 0.05
Nodes (73): build_exclude_ids(), ensure_google_oauth_auth_method(), ensure_user_has_workspace(), establish_verified_account(), EstablishVerifiedAccountParams, get_passkeys_for_user(), google_oauth_callback_service(), GoogleOAuthCallbackParams (+65 more)

### Community 13 - "Error"
Cohesion: 0.09
Nodes (15): Error, BadRequest, Conflict, Forbidden, Internal, Migrate, NotFound, NotImplemented (+7 more)

### Community 15 - "encryption.rs"
Cohesion: 0.09
Nodes (30): decrypt(), decrypt_json(), decrypt_slack_token(), derive_key(), derive_key_correct_length(), derive_key_invalid_base64_rejected(), derive_key_wrong_length_rejected(), each_encryption_produces_different_ciphertext() (+22 more)

### Community 16 - "operations.rs"
Cohesion: 0.08
Nodes (45): BEERXML_EXPORT, BEERXML_IMPORT, BEERXML_ORIGINAL, BEERXML_PREVIEW, BREW_ACTUAL, BREW_ARCHIVE, BREW_COMPARISON, BREW_EVIDENCE_VIEW (+37 more)

### Community 17 - "RecipeSaveOutcome"
Cohesion: 0.10
Nodes (25): check_editor_access(), ContextCheckGeneration, current_context_matches(), EditorAccess, Checking, Expired, Forbidden, Mismatch (+17 more)

### Community 18 - "E2E Test Plan"
Cohesion: 0.04
Nodes (46): Current acceptance scope, E2E Test Plan, Notes for Test Implementation, TC-001: Signup Flow (Email Verification Path), TC-002: Signup With Invitation, TC-003: Login Flow (Password), TC-004: Login Flow (Passkey), TC-005: Logout (+38 more)

### Community 19 - "presets.rs"
Cohesion: 0.08
Nodes (26): a_later_preset_overrides_an_earlier_one_with_the_same_id(), a_preset_can_be_plugged_in_at_runtime(), a_preset_profile_derives_a_recipe(), adopting_a_preset_records_where_the_profile_came_from(), builtin_presets(), builtins_are_available_and_uniquely_identified(), chilling_choice_survives_a_round_trip_through_a_preset(), EquipmentField (+18 more)

### Community 20 - "pages/brew_history.rs"
Cohesion: 0.08
Nodes (40): BrewChangeKind, Added, Changed, Removed, BrewFormulationChange, BrewHistorySection, Extensions, Fermentable (+32 more)

### Community 21 - "recover_request"
Cohesion: 0.10
Nodes (56): amend_actual(), archive_inner(), begin(), canonical_document(), correct_measurement(), correct_note(), digest_of(), list_notes() (+48 more)

### Community 22 - "krausen-types/src/brew_history.rs"
Cohesion: 0.13
Nodes (15): measurement_comparison(), list_current_entries(), list_root_lineage(), load_current_entry(), load_measurement(), MeasurementRow, BrewComparison, BrewMeasurementComparison (+7 more)

### Community 23 - "workspace_service.rs"
Cohesion: 0.08
Nodes (47): accept_invitation(), accept_invitation_for_user(), check_existing_member_by_email(), check_pending_invitation(), complete_ownership_transfer(), count_admins(), count_pending_invitations(), count_workspace_users() (+39 more)

### Community 24 - "native_store.rs"
Cohesion: 0.08
Nodes (41): BackfillError, Encode, Legacy, encode_document(), EncodeError, Canonical, UnsupportedVersion, prepare_backfill() (+33 more)

### Community 25 - "prelude"
Cohesion: 0.11
Nodes (19): Card(), CardContent(), CardDescription(), CardFooter(), CardHeader(), CardTitle(), INPUT_CLASS, Label() (+11 more)

### Community 26 - "mcp_session_manager.rs"
Cohesion: 0.16
Nodes (27): cleanup(), clone_shares_state(), create_and_validate_session(), invalidate_empty_workspace_is_noop(), invalidate_workspace_clears_all_workspace_sessions(), mcp_apps_support_default_false(), mcp_apps_support_unknown_session_returns_false(), MCPSessionManager (+19 more)

### Community 27 - "app.js"
Cohesion: 0.09
Nodes (35): beerGlass(), brews, catalogue, changeState(), clone(), closeDialog(), commitNavigation(), continuePending() (+27 more)

### Community 28 - "krausen-app/src/scaling.rs"
Cohesion: 0.13
Nodes (29): absent_values_stay_absent(), accepted_digest(), accepted_digest_binds_revision_target_policy_and_context(), apply(), apply_inner(), build_preview(), capacity_and_fixed_losses_are_explicit(), capacity_warning_checks_pre_boil_fill_too() (+21 more)

### Community 29 - "settings/team.rs"
Cohesion: 0.13
Nodes (27): format_date(), InvitationRow(), MemberRow(), PendingAction, CancelInvitation, CancelTransfer, RemoveMember, TeamPage() (+19 more)

### Community 30 - "oauth.rs"
Cohesion: 0.16
Nodes (23): approve(), auth_failure(), authorization_metadata(), authorize(), client_auth(), continue_authorize(), html_escape(), invalid_client() (+15 more)

### Community 31 - "app.rs"
Cohesion: 0.05
Nodes (12): App(), Shell(), AccountRecoveryPage(), AuthLayout(), OAuthCompletePage(), PasskeyRecoveryPage(), OnboardingPage(), SettingsShell() (+4 more)

### Community 32 - "ScaleChange"
Cohesion: 0.11
Nodes (13): document(), exercise(), postgres_scaling(), run(), settings(), sqlite_scaling(), capacity_warning_names_values_and_explicit_geometry_policy(), change_text() (+5 more)

### Community 33 - "convert.rs"
Cohesion: 0.13
Nodes (30): a_converted_recipe_derives(), a_hop_use_the_engine_cannot_model_is_reported_not_hidden(), a_missing_value_is_defaulted_and_declared(), a_value_that_is_present_but_unreadable_is_reported_as_such(), an_aroma_addition_becomes_a_whirlpool_addition(), an_imported_equipment_profile_says_what_beerxml_could_not_supply(), attenuation_becomes_a_fraction(), convert() (+22 more)

### Community 34 - "mibu.rs"
Cohesion: 0.10
Nodes (30): a_flameout_addition_still_gains_bitterness(), a_held_hop_stand_extracts_less_than_a_boiling_one(), a_lidded_kettle_cools_slower_than_an_open_one(), a_longer_stand_extracts_more_when_chilling_would_otherwise_have_started(), a_wider_kettle_cools_faster(), AA_DECAY_PER_MIN, additions_are_independent_of_each_other(), an_addition_already_spent_in_the_boil_gains_little() (+22 more)

### Community 35 - "sql_compat.rs"
Cohesion: 0.05
Nodes (9): full_table_name_expr_prefixed(), test_full_table_name_expr_prefixed(), an_expiry_an_hour_ago_is_not_in_the_future(), an_expiry_later_today_is_in_the_future(), evaluate(), expiry_on_other_days_still_works(), SHAPES, stamp() (+1 more)

### Community 36 - "accept_ownership.rs"
Cohesion: 0.13
Nodes (23): capability_item(), error_view(), format_date_script(), icon_alert_circle_large(), icon_alert_circle_sm(), icon_arrow_right_left(), icon_building2(), icon_check_circle_large() (+15 more)

### Community 37 - "RecipeDocumentV1"
Cohesion: 0.16
Nodes (16): AdditionalFields(), AdditionalGroup(), AdditionalIngredients(), AdditionalKind, Other, Yeast, AdditionalNumberField(), AdditionalTextField() (+8 more)

### Community 38 - "email_service.rs"
Cohesion: 0.10
Nodes (11): admin_email_brand_preserves_escaped_sections(), admin_email_template(), assert_email_brand(), email_template(), EmailService, html_escape(), LOGO_BYTES, LOGO_CID (+3 more)

### Community 39 - "super"
Cohesion: 0.10
Nodes (9): a_leaf_carries_its_text(), checked_json(), WORKSPACE_SETTINGS, find(), RecoveryRecord, RecoveryScope, write(), new_request_id() (+1 more)

### Community 40 - "native.rs"
Cohesion: 0.13
Nodes (30): beerxml_dry_hop_time_is_interpreted_as_minutes(), edited_export_uses_current_native_values_and_reports_source_omission(), envelope_fragments_are_only_reused_for_unchanged_single_recipe(), explicit_choices_project_colour_and_efficiency_without_changing_source(), explicit_style_identity_survives_native_projection_and_export(), extension_follows_row_identity_after_reordering(), from_xml(), from_xml_at() (+22 more)

### Community 41 - "project_xml"
Cohesion: 0.21
Nodes (15): exact_number(), from_legacy(), NativeMigration, note_unmapped_number(), OriginalSource, preserved_extensions(), preserved_fields(), project_xml() (+7 more)

### Community 43 - "popover.rs"
Cohesion: 0.08
Nodes (27): BreweryPopover(), return_popover_focus(), Align, End, Start, bottom_end_aligns_right_edges(), bottom_start_fits_below_no_flip(), bottom_start_flips_when_no_room_below() (+19 more)

### Community 44 - "recipe_crud.rs"
Cohesion: 0.08
Nodes (41): create_recipe(), create_recipe(), delete_recipe(), get_legacy_recovery(), get_recipe(), index(), Indexed, LegacyRecovery (+33 more)

### Community 45 - "ActorContext"
Cohesion: 0.12
Nodes (21): doc(), exercise(), postgres_ingredient_catalogue(), recipe_with(), save_request(), search(), sqlite_ingredient_catalogue(), INGREDIENT_ENTRY_KINDS (+13 more)

### Community 46 - "RecipeComposition"
Cohesion: 0.09
Nodes (3): EmailBrandSourceTests, RecipeComposition, data_url()

### Community 47 - "mcp_auth.rs"
Cohesion: 0.12
Nodes (9): https_authorization_and_sdk_through_reverse_proxy(), Proxy, brew_test_document(), brew_test_profile(), brew_ui_outcome(), exercise(), postgres_sdk_clients(), run_mcp_exercise() (+1 more)

### Community 48 - "pages/brew_evidence.rs"
Cohesion: 0.04
Nodes (56): map_note_error(), BrewLifecycleOutcome, Archived, Conflict, ContextChanged, Forbidden, Invalid, InvalidTransition (+48 more)

### Community 49 - "parse.rs"
Cohesion: 0.12
Nodes (21): ParseError, NoRootElement, Xml, accepts_a_bare_recipe_without_the_recipes_wrapper(), an_empty_element_is_recorded_as_empty_not_absent(), cdata_survives(), collect(), collections_keep_their_order() (+13 more)

### Community 50 - "server_fns/brew.rs"
Cohesion: 0.05
Nodes (56): archive(), map_archive_error(), ActualFormulationOutcome, Amended, Archived, Conflict, ContextChanged, Forbidden (+48 more)

### Community 51 - "krausen-calc/src/lib.rs"
Cohesion: 0.12
Nodes (20): Corrections, Derived, FermentablePoints, IbuContribution, Volumes, abv(), compute(), corrections() (+12 more)

### Community 52 - "ScaleApplyOutcome"
Cohesion: 0.24
Nodes (11): ScalePreviewRequest, DuplicateDialog(), duplicate_recipe_shared(), execute_apply(), execute_duplicate(), execute_duplicate_recipe(), execute_preview(), execute_scale_apply() (+3 more)

### Community 53 - "foundation"
Cohesion: 0.06
Nodes (31): concurrency, cancel-in-progress, group, KRAUSEN_TEST_POSTGRES_URL, POSTGRES_DB, POSTGRES_PASSWORD, POSTGRES_USER, env (+23 more)

### Community 54 - "manager.rs"
Cohesion: 0.12
Nodes (8): Inner, MAX_CONNECTIONS_PER_USER, NEXT_CONNECTION_ID, REDIS_CHANNEL_PREFIX, sqlite_memory_pool(), TrackedConnection, WS_CHANNEL_CAPACITY, WsSender

### Community 55 - "reference_recipe.rs"
Cohesion: 0.22
Nodes (30): derive(), ABV_EPS, assert_close(), boil_time_moves_everything_downstream_of_volume(), chilling_sooner_means_less_bitterness(), colour_uses_post_boil_volume_not_batch_volume(), CORRECTION_EPS, EBC_EPS (+22 more)

### Community 56 - "Validation"
Cohesion: 0.15
Nodes (10): draft(), exercise(), postgres_profiles(), settings(), sqlite_profiles(), merge_defaults(), record_default(), targets() (+2 more)

### Community 57 - "RecipeDraft"
Cohesion: 0.12
Nodes (11): RecipeDocumentV1, DraftContents, editing_clears_stale_server_diagnostics_and_preview(), explicit_submission_settles_raw_fields_and_old_recovery_remains_readable(), raw_numbers_preserve_empty_invalid_and_real_zero(), RecipeDraft, replacing_clean_saved_document_invalidates_existing_preview_generation(), settled_feedback_tracks_correction_without_content_or_generation_changes_on_blur() (+3 more)

### Community 58 - "@playwright/test"
Cohesion: 0.13
Nodes (9): gotoAuthenticated(), AUTH_STATE, hasAuthState, navigateToSecurity(), navigateToSecurity(), disable2FAIfEnabled(), ensureUserHasPassword(), navigateToSecurity() (+1 more)

### Community 59 - "DbTx"
Cohesion: 0.11
Nodes (22): append(), archive(), create(), duplicate_key_exists(), encode(), entry_id_in_use(), fetch_one(), like_pattern() (+14 more)

### Community 60 - "krausen-app/src/interchange.rs"
Cohesion: 0.12
Nodes (26): export(), export_inner(), preview(), preview_inner(), project(), ExportOutcome, ContextChanged, Exported (+18 more)

### Community 61 - "Recipe organization"
Cohesion: 0.05
Nodes (38): ACC-001: Names, fields and local states, ACC-002: Chosen account/actions popup pattern, ACC-003: True modal and preview boundaries, ACC-004: Add, remove and Undo retain orientation, ACC-005: Announce meaningful settled outcomes, ACC-006: Reachability, enlargement and honest evidence, Accessibility contracts, Approved reference and visual contracts (+30 more)

### Community 62 - "recipe_comparison.rs"
Cohesion: 0.18
Nodes (19): changes(), compare(), display(), entered_text(), field_label(), ingredient_and_process_changes_use_names_and_keep_preserved_source_values(), native_row(), native_rows() (+11 more)

### Community 63 - "BadgeVariant"
Cohesion: 0.20
Nodes (9): Badge(), BadgeVariant, Default, Destructive, Outline, Secondary, Warning, BASE (+1 more)

### Community 64 - "catalogue_picker.rs"
Cohesion: 0.12
Nodes (16): multiplied(), ScaleOutcome, Archived, ContextChanged, Forbidden, Invalid, NotFound, Preview (+8 more)

### Community 65 - "KVPool"
Cohesion: 0.12
Nodes (29): delete_recovery_session(), delete_webauthn_challenge(), get_pending_signup(), get_pending_terms(), get_pending_totp(), get_recovery_session(), get_webauthn_challenge(), peek_recovery_session() (+21 more)

### Community 66 - "token_service.rs"
Cohesion: 0.13
Nodes (20): create_verification_token(), create_verification_token_with_expiry(), get_user_sessions(), hash_refresh_token(), RefreshTokenUserData, RefreshTokenVerifyResult, GracePeriod, Invalid (+12 more)

### Community 67 - "serialize"
Cohesion: 0.11
Nodes (16): action_type_to_str(), get_entries_since(), get_latest_sync_id(), is_sync_id_available(), normalise_timestamp(), parse_action_type(), prune_old_entries(), SyncLogRow (+8 more)

### Community 68 - "user_service.rs"
Cohesion: 0.08
Nodes (45): add_passkey_to_user(), admin_create_user(), admin_update_user(), create_api_token(), create_user(), create_workspace_for_user(), default_workspace_name(), delete_passkey_from_user() (+37 more)

### Community 69 - "layout.rs"
Cohesion: 0.12
Nodes (15): CONTEXT_CHANGED_KEY, EditorDestination, Href, Logout, Workspace, EditorGuard, Layout(), Sidebar() (+7 more)

### Community 70 - "icon"
Cohesion: 0.22
Nodes (27): Component families and production mapping, brewsPage(), compactSummary(), composition(), editor(), fmt(), historyPage(), ingredientPanel() (+19 more)

### Community 71 - "scripts"
Cohesion: 0.10
Nodes (21): dependencies, argon2@^0.44.0, devDependencies, otplib@^12.0.1, @playwright/test@^1.49.0, name, private, scripts (+13 more)

### Community 72 - "onboarding_service.rs"
Cohesion: 0.19
Nodes (6): Config, KrausenMode, Personal, Saas, SelfHosted, required_env()

### Community 73 - "tests/brew_history.rs"
Cohesion: 0.07
Nodes (15): actual(), correct_request(), document(), evidence_view(), exercise(), fixture(), integrated_acceptance(), intent() (+7 more)

### Community 74 - "oauth_service.rs"
Cohesion: 0.15
Nodes (24): http_client(), approve(), authenticate_client(), AuthorizationRequest, begin_authorization(), bind_pending(), Client, Code (+16 more)

### Community 75 - "Equipment"
Cohesion: 0.19
Nodes (17): a_whirlpool_with_no_stand_still_isomerises_while_the_wort_chills(), AA_DECAY_PER_MIN, BIGNESS_BASE, BIGNESS_COEFFICIENT, bigness_factor(), bitterness_scales_linearly_with_hop_weight(), contributions(), dry_hops_contribute_no_bitterness_and_no_row() (+9 more)

### Community 76 - "PreviewOutcome"
Cohesion: 0.06
Nodes (38): effective(), engine_source_hash(), policy_source_hash(), POLICY_VERSION, resolve_in_tx(), list_styles(), preview(), project_preview() (+30 more)

### Community 77 - "jwt.rs"
Cohesion: 0.15
Nodes (18): BROWSER_AUDIENCE, BROWSER_ISSUER, Claims, create_access_token(), create_access_token_str(), create_refresh_token(), decode_python_shaped_jwt_with_extra_fields(), empty_token_rejected() (+10 more)

### Community 78 - "Chilling"
Cohesion: 0.20
Nodes (9): Equipment, WithChilling, Chilling, Counterflow, Forced, None, chilling_rates_fall_as_volume_rises(), immersion_and_ice_bath_differ_in_the_expected_direction() (+1 more)

### Community 79 - "brew-start.spec.ts"
Cohesion: 0.10
Nodes (11): { chromium }, path, SCREENSHOT_DIR, AUTH_STATE_PATH, getKrausenMode(), globalSetup(), sqliteExec(), importRecipe() (+3 more)

### Community 80 - "db"
Cohesion: 0.26
Nodes (17): allowSignup(), cleanTestData(), createSecondUser(), db, generateEmail(), generateUserId(), getDefaultUserId(), getDefaultWorkspaceId() (+9 more)

### Community 81 - "PageState"
Cohesion: 0.07
Nodes (31): Alert(), AlertDescription(), AlertTitle(), AlertVariant, Default, Error, Info, Success (+23 more)

### Community 82 - "tests/brew_evidence.rs"
Cohesion: 0.06
Nodes (50): amend_document(), complete_document(), correct_request(), equipment_request(), exercise(), exercise_acceptance_cases(), fixture(), intent() (+42 more)

### Community 83 - "pages/recipe_editor.rs"
Cohesion: 0.15
Nodes (19): ingredient_number(), ingredient_text(), IngredientFields(), IngredientKind, Fermentable, Hop, IngredientNumber, Amount (+11 more)

### Community 84 - "BrewStatusV1"
Cohesion: 0.10
Nodes (24): load_brew(), update_status(), BREW_PLAN_SCHEMA_VERSION, BrewRow, BrewSummaryRow, encode_plan(), insert(), list_for_recipe() (+16 more)

### Community 85 - "CalcError"
Cohesion: 0.16
Nodes (20): CalcError, AttenuationOutOfRange, BoilOffExceedsPreBoil, EfficiencyOutOfRange, EmptyGrist, InvalidInput, MashWaterExceedsTotal, NegativeBoilOffRate (+12 more)

### Community 86 - "beerxml.rs"
Cohesion: 0.11
Nodes (6): exercise(), postgres_interchange(), sqlite_interchange(), BeerXmlExport(), BeerXmlPage(), hand_off_import_action_focus()

### Community 87 - "src/webauthn.rs"
Cohesion: 0.09
Nodes (16): build_webauthn(), finish_authentication(), finish_discoverable_authentication(), finish_registration(), start_authentication(), start_discoverable_authentication(), start_registration(), array_buffer_to_base64url() (+8 more)

### Community 88 - "brewery.rs"
Cohesion: 0.12
Nodes (21): resolve_one(), actual_from_plan(), actual_from_recipe(), EffectiveActualFormulation, ActualFormulationV1, AdditionStage, FermentableAddition, FermentableSpecification (+13 more)

### Community 89 - "account_recovery_complete.rs"
Cohesion: 0.11
Nodes (22): AccountRecoveryCompletePage(), error_view(), PageState, Error, Ready, Submitting, Success, Verifying (+14 more)

### Community 90 - "pages/brew_start.rs"
Cohesion: 0.16
Nodes (13): build_start_request(), HowToStart(), invalidate_selection(), may_prepare(), metric(), prepared_current_selection_keeps_original_head_guard(), prepared_head_guard(), refreshed_metrics() (+5 more)

### Community 91 - "websocket.rs"
Cohesion: 0.16
Nodes (10): CLOSE_AUTH_REQUIRED, CLOSE_FORBIDDEN, CLOSE_TOO_MANY_CONNECTIONS, close_with_code(), handle_authenticated_ws(), MAX_MESSAGE_SIZE, PING_INTERVAL_SECS, valid_credential() (+2 more)

### Community 92 - "constants.rs"
Cohesion: 0.16
Nodes (13): Constants, CONSTANTS, CookieConstants, get(), JwtConstants, load_with_fallback(), RateLimitBucket, RateLimitConstants (+5 more)

### Community 93 - "ExportOutcome"
Cohesion: 0.14
Nodes (14): ImportCommitOutcome, ChoicesRequired, ContextChanged, Forbidden, Imported, Invalid, InvalidRequest, InvalidSource (+6 more)

### Community 94 - "AppState"
Cohesion: 0.12
Nodes (15): WorkspaceRenameOutcome, Conflict, Forbidden, Invalid, RequestIdentityConflict, ResultExpired, Retryable, Saved (+7 more)

### Community 95 - "loginUser"
Cohesion: 0.18
Nodes (14): ensureLoggedIn(), fetchAuthConfig(), getCookies(), getKrausenMode(), hasCookie(), isAuthenticated(), isPersonalMode(), isSelfHostedMode() (+6 more)

### Community 96 - "middleware/mod.rs"
Cohesion: 0.15
Nodes (7): cors_layer(), security_headers(), clear_recovery_session_cookie(), clear_token_cookies(), get_cookie_value(), set_recovery_session_cookie(), set_token_cookies()

### Community 97 - "leptos_frontend.rs"
Cohesion: 0.15
Nodes (9): ANYTHING, ASSET, BROWSER, dist_dir(), serve(), serve_asset_or_shell(), serve_leptos_shell(), static_files_service() (+1 more)

### Community 98 - "signup_flow.rs"
Cohesion: 0.28
Nodes (14): a_known_address_is_answered_the_same_as_an_unknown_one(), a_second_link_does_not_make_a_second_account(), an_unaccepted_token_leaves_nothing_behind(), count_users(), device(), emailed_token(), JWT_SECRET, kv() (+6 more)

### Community 99 - "BrewFeedbackApplyOutcome"
Cohesion: 0.10
Nodes (20): BrewFeedbackApplyOutcome, ActualConflict, Applied, ContextChanged, ContextConflict, Forbidden, HeadConflict, Invalid (+12 more)

### Community 100 - "RecipeReadOutcome"
Cohesion: 0.25
Nodes (8): read(), RecipeReadOutcome, ContextChanged, Forbidden, Found, NotFound, Unavailable, RecipeSnapshot

### Community 101 - "retry.rs"
Cohesion: 0.11
Nodes (11): backoff_delay(), INITIAL_INTERVAL_SECS, MAX_INTERVAL, MAX_RETRIES, max_retries_exhausted_returns_last_error(), MULTIPLIER, non_retryable_error_fails_immediately(), retry_with_backoff() (+3 more)

### Community 102 - "with"
Cohesion: 0.10
Nodes (20): with, cache-from, cache-to, client-id, context, file, owner, password (+12 more)

### Community 103 - "4. Architecture"
Cohesion: 0.11
Nodes (18): 1. The thesis, 2. What Krausen is not, 3.1 Hide questions, not capabilities, 3.2 The software has a point of view, 3.3 Learn the environment, never silently change it, 3.4 Flag runs, not misses, 3.5 Say "I don't know yet", 3.6 AI never produces a number (+10 more)

### Community 104 - "server_fns/recipes.rs"
Cohesion: 0.23
Nodes (15): create_recipe(), delete_recipe(), DerivedSummary, detail(), get_recipe(), import_beerxml(), list_recipes(), parse_one() (+7 more)

### Community 105 - "google_oauth.rs"
Cohesion: 0.18
Nodes (17): authorization_url_targets_google_with_login_scopes(), build_authorization_url(), build_oauth_data(), exchange_code_for_tokens(), get_user_info(), GOOGLE_AUTH_URI, GOOGLE_TOKEN_URI, GOOGLE_USER_INFO_URI (+9 more)

### Community 106 - "Recipe"
Cohesion: 0.22
Nodes (3): BeerXml, Mash, Recipe

### Community 107 - "record.rs"
Cohesion: 0.14
Nodes (8): a_number_with_a_unit_suffix_parses_and_still_writes_verbatim(), an_empty_element_survives_the_round_trip(), an_unparseable_number_is_kept_rather_than_dropped_or_defaulted(), fields_are_written_in_declaration_order(), record!, only_fields_that_were_set_are_written(), Record, .TAG

### Community 108 - "krausen-types/src/interchange.rs"
Cohesion: 0.16
Nodes (14): ColourUnit, Lovibond, Srm, EfficiencyChoice, RecipeOverride, SelectedEquipment, ExportSelection, Draft (+6 more)

### Community 109 - "Button"
Cohesion: 0.27
Nodes (19): actualsDialog(), allFermentables(), catalogueResults(), createFermentableDialog(), equipmentDialog(), inspectRevision(), openAdd(), openFermentablePicker() (+11 more)

### Community 111 - "extract_context"
Cohesion: 0.08
Nodes (28): AcceptOwnershipPage(), fetch_ownership_transfer(), AppearanceCard(), InvitationsCard(), ProfileInfoCard(), ProfilePage(), get_user_context(), AuthenticatedContext (+20 more)

### Community 112 - "McpHandler"
Cohesion: 0.20
Nodes (8): BreweryCompactIngredientRow(), BreweryDraftArea(), BreweryDraftField(), BreweryDraftSelect(), CatalogueItem(), authority_failed(), RecipeLibrary(), RecipesPage()

### Community 113 - "tests/brew_start.rs"
Cohesion: 0.08
Nodes (26): baseline_v1_golden_refused_and_v2_matches_targets(), CONTEXT, DIGEST, FIXTURE, STYLE, TARGETS, VALIDATION, complete_document() (+18 more)

### Community 114 - "Target"
Cohesion: 0.10
Nodes (19): ALL, EXTRACT, IBU, Rule, Addition, Finite, Fraction, Nonnegative (+11 more)

### Community 115 - "WebSocketManager"
Cohesion: 0.20
Nodes (11): send_error(), send_member_joined(), send_member_role_changed(), send_ownership_transfer_completed(), send_ownership_transfer_declined(), send_ownership_transfer_offered(), send_sync_action(), send_workspace_invitation() (+3 more)

### Community 116 - "ProcessSummaryOutcome"
Cohesion: 0.12
Nodes (22): read(), read_inner(), summary(), summary_request(), ProcessSummaryOutcome, ContextChanged, Forbidden, Invalid (+14 more)

### Community 118 - "signup_complete.rs"
Cohesion: 0.08
Nodes (22): Checkbox(), CHECKBOX_ACTIVE, CHECKBOX_BASE, CHECKBOX_UNCHECKED, creating_view(), error_view(), PageState, Creating (+14 more)

### Community 119 - "krausen-app/src/process_summary.rs"
Cohesion: 0.11
Nodes (12): load_saved(), project(), project_step(), ResolvedSummary, SavedRead, Found, Missing, UnsupportedVersion (+4 more)

### Community 120 - "krausen-types/src/process_summary.rs"
Cohesion: 0.16
Nodes (14): compare(), identities(), identities_resolve_and_bounds_are_inclusive_at_full_precision(), resolve(), StyleReference, Preview, Range, StyleComparison (+6 more)

### Community 121 - "MeasurementOutcome"
Cohesion: 0.11
Nodes (19): map_measurement_error(), MeasurementOutcome, Archived, Conflict, ContextChanged, Corrected, EntryConflict, Forbidden (+11 more)

### Community 123 - "EquipmentSettingsV1"
Cohesion: 0.23
Nodes (13): append(), archive(), create(), encode(), list(), load(), load_revision(), execute! (+5 more)

### Community 124 - "toast.rs"
Cohesion: 0.20
Nodes (13): add_toast(), Toast, toast_error(), toast_info(), toast_success(), ToastContainer(), ToastItem(), ToastProvider() (+5 more)

### Community 125 - "Shared application architecture"
Cohesion: 0.12
Nodes (17): Application architecture, Authentication and authorization, Capability matrix, Common execution and persistence contract, Confirmed product contracts, Dependency and implementation acceptance, Existing implementation and gaps, Installation and credential contract (+9 more)

### Community 126 - "MessageType"
Cohesion: 0.15
Nodes (12): MessageType, Error, Heartbeat, MemberJoined, MemberRoleChanged, OwnershipTransferCompleted, OwnershipTransferDeclined, OwnershipTransferOffered (+4 more)

### Community 127 - "Field"
Cohesion: 0.15
Nodes (6): bool, f64, Field, leading_number(), String, u32

### Community 128 - "server_fns/brew_history.rs"
Cohesion: 0.09
Nodes (33): BrewComparisonOutcome, Comparison, ContextChanged, Forbidden, NotFound, Unavailable, BrewPrintOutcome, ContextChanged (+25 more)

### Community 129 - "totp.rs"
Cohesion: 0.19
Nodes (14): build_totp(), DIGITS, generate_qr_code(), generate_secret(), generate_secret_returns_valid_base32(), ISSUER, provisioning_uri(), provisioning_uri_contains_issuer() (+6 more)

### Community 130 - "Navigation requirements"
Cohesion: 0.11
Nodes (18): Authority and use, Current-state boundary, IA-001: Recipe-first home and truthful destinations, IA-002: One compact account dropdown, IA-003: Workspace switching is exceptional account context, IA-004: Mobile navigation has one modal owner, IA-005: Internal showcase policy, IA-006: Object-local vocabulary and source context (+10 more)

### Community 131 - "krausen-calc/src/formulation.rs"
Cohesion: 0.17
Nodes (20): calculate(), colour_ebc(), ENGINE_BUILD, ENGINE_SOURCE, ENGINE_VERSION, FermentableInput, FermentableStage, Boil (+12 more)

### Community 132 - "db.rs"
Cohesion: 0.09
Nodes (12): create_pool(), DbQueryResult, db_execute!, db_fetch_all!, db_fetch_one!, db_fetch_optional!, db_fetch_scalar!, db_with_pool! (+4 more)

### Community 133 - "draft_validation.rs"
Cohesion: 0.26
Nodes (12): validate_draft(), attenuation_is_explicit_and_never_inferred_from_yeast(), colour_and_yield_have_distinct_target_dependencies(), complete(), extract_only_needs_no_mash_inputs(), missing_hop_input_does_not_hide_water_and_no_hops_is_valid(), nonfinite_and_conflicting_schedule_are_invalid(), profile_thickness_and_top_up_must_fit_water_budget() (+4 more)

### Community 134 - "write.rs"
Cohesion: 0.27
Nodes (14): empty_collections_are_still_written(), indent(), INDENT, leaf(), output_is_well_formed_enough_to_read_back(), text_needing_escapes_round_trips(), write(), write_collection() (+6 more)

### Community 136 - "EquipmentOutcome"
Cohesion: 0.06
Nodes (29): EquipmentCommand, Archive, Get, List, Revision, Save, EquipmentListQuery, EquipmentOutcome (+21 more)

### Community 138 - "Krausen Coding Standards"
Cohesion: 0.13
Nodes (15): Acceptance, Canonical models and projection boundaries, Change delivery and client state, Component Patterns, Database Queries, Design System Adherence, Dynamic Query Builders, Error Handling (+7 more)

### Community 139 - "DEFAULT_TEST_USER"
Cohesion: 0.13
Nodes (6): DEFAULT_TEST_USER, loginUser(), sqlText(), importGeometryExpect, catalogueAction(), catalogueCreateForm()

### Community 140 - "server/src/lib.rs"
Cohesion: 0.18
Nodes (4): bootstrap_owner(), build_router(), claim_legacy_personal_owner(), wrap_service()

### Community 142 - "bootstrap.rs"
Cohesion: 0.12
Nodes (9): exercise(), postgres_operator_bootstrap(), sqlite_operator_bootstrap(), refresh_tokens(), RefreshedTokens, exercise(), exercise_oauth(), postgres_browser_credential_lifecycle() (+1 more)

### Community 143 - "migration_orders.rs"
Cohesion: 0.20
Nodes (11): BREW_EVIDENCE, BREWS, CATALOGUE, CREDENTIALS, EQUIPMENT, exercise(), NATIVE, postgres_empty_and_both_upgrade_orders() (+3 more)

### Community 144 - "RedisKVStore"
Cohesion: 0.12
Nodes (5): RedisKVStore, create_pool(), ping(), RedisPool, test_redis_connects()

### Community 145 - "middleware.rs"
Cohesion: 0.22
Nodes (4): AuthState, AuthUser, CredentialIdRow, extract_token()

### Community 146 - "Recipe"
Cohesion: 0.11
Nodes (18): ScaleApplyOutcome, Applied, Archived, Conflict, ContextChanged, ContextConflict, Forbidden, Invalid (+10 more)

### Community 147 - "4. Architecture"
Cohesion: 0.48
Nodes (4): exercise_sqlite(), exercise_sqlite_constraints(), seed_sqlite(), sqlite_brew_plan_is_immutable()

### Community 148 - "onboarding_service.rs"
Cohesion: 0.16
Nodes (11): recovery_verify_service(), RecoveryVerifyServiceResult, AccountNotVerified, InvalidToken, Success, notify_signup(), send_signup_slack(), accept_terms() (+3 more)

### Community 149 - "kv_store_memory.rs"
Cohesion: 0.24
Nodes (10): del_removes_key(), expire_sets_ttl(), getdel_missing_returns_none(), getdel_returns_and_removes(), incr_creates_and_increments(), missing_key_returns_none(), ping_succeeds(), set_and_get() (+2 more)

### Community 150 - "tests/brew_start.rs"
Cohesion: 0.11
Nodes (20): exercise(), postgres_editor(), postgres_resource_lock_wait_rechecks_both_expiries(), save(), sqlite_editor(), RecipeSaveOutcome, Conflict, ContextChanged (+12 more)

### Community 151 - "EmptyStateVariant"
Cohesion: 0.16
Nodes (8): EmptyState(), EmptyStateVariant, Default, Error, Info, Sidebar, Success, Warning

### Community 152 - "PreviewOutcome"
Cohesion: 0.09
Nodes (25): head_conflict(), lock_workspace(), recipe_archived(), recipe_head(), start(), start_inner(), StartBrewOutcome, ContextChanged (+17 more)

### Community 153 - "Validation"
Cohesion: 0.18
Nodes (10): Diagnostic, Readiness, Incomplete, Invalid, Ready, Validation, diagnostic_message(), readiness_label() (+2 more)

### Community 154 - "Krausen design system"
Cohesion: 0.14
Nodes (14): Compact default density, Component ownership, Confirmed product interactions, Interaction and accessibility contracts, Krausen design system, Living-system workflow, Prototype and its limits, Read this before changing the interface (+6 more)

### Community 155 - "Recipes, revisions and historical brews"
Cohesion: 0.14
Nodes (14): Actual formulation, readings and corrections, Brew start and calculation context, Examples and acceptance scenarios, Immutable `BrewPlanV1`, Lifecycle, archive and retention, Migration and schema evolution, Native recipe contract, Ownership and representation (+6 more)

### Community 156 - "components.js"
Cohesion: 0.27
Nodes (11): estimatePrecision(), finiteNumber(), IngredientRow(), ingredientSwatch(), Metric(), paths, rangeNumber(), StyleRange() (+3 more)

### Community 157 - "Prioritized findings"
Cohesion: 0.14
Nodes (14): B1 — P1: specify one transaction owner and a composable database context, B2 — P2: name the canonical native model and conversion owners, B3 — P2: scope relationships to both workspace and parent, B4 — P2: distinguish selected historical revision from expected current head, B5 — P2: check the complete calculation context at brew start, B6 — P2: give measurements stable quantity semantics, B7 — P2: keep transport delivery out of brewing workflow dependencies, B8 — P3: move executable migration ownership to the composition root (+6 more)

### Community 158 - "shell-source-check.cjs"
Cohesion: 0.16
Nodes (9): layout, ui, assert, layout, library, { readFileSync }, { resolve }, root (+1 more)

### Community 159 - "steps"
Cohesion: 0.24
Nodes (14): steps, GH_TOKEN, IMAGE_DIGEST, SOURCE_SHA, VERSION, steps, env, id (+6 more)

### Community 160 - "BrewLifecycleOutcome"
Cohesion: 0.11
Nodes (32): apply_changes(), canonical_digest(), change(), comparison(), comparison_changes(), comparison_inner(), feedback_preview(), feedback_preview_inner() (+24 more)

### Community 161 - "theme.rs"
Cohesion: 0.21
Nodes (9): apply_to_document(), read_local_storage(), resolve_theme(), save_theme_to_local_storage(), STORAGE_KEY, ThemeProvider(), ThemeState, use_theme() (+1 more)

### Community 162 - "BrewFeedbackPreviewOutcome"
Cohesion: 0.15
Nodes (13): BrewFeedbackPreview, BrewFeedbackPreviewOutcome, Conflict, ContextChanged, Forbidden, InvalidRequest, NoActual, NoChanges (+5 more)

### Community 163 - "switch.rs"
Cohesion: 0.15
Nodes (9): LABEL_DEFAULT_CLASS, LABEL_WRAPPER_CLASS, Switch(), THUMB_BASE, THUMB_CHECKED, THUMB_UNCHECKED, TRACK_BASE, TRACK_CHECKED (+1 more)

### Community 164 - "ImportCommitOutcome"
Cohesion: 0.17
Nodes (12): DuplicateOutcome, Archived, Conflict, ContextChanged, Duplicated, Forbidden, InvalidRequest, NotFound (+4 more)

### Community 165 - "manager"
Cohesion: 0.22
Nodes (9): BreweryNotice(), NoticeAnnouncement, Persistent, Settled, NoticeTone, Error, Info, Success (+1 more)

### Community 166 - "RawElement"
Cohesion: 0.25
Nodes (10): Equipment, invalid_chilling_and_geometry_are_not_replaced_by_defaults(), invalid_field(), no_hops_is_zero_ibu_and_zero_optional_losses_are_valid(), recipe(), rejects_gravity_outside_abv_formula_domain(), rejects_nonfinite_values_before_they_can_poison_results(), validates_each_row_and_hop_exposure() (+2 more)

### Community 167 - "native_recipe.rs"
Cohesion: 0.25
Nodes (8): SwatchRole, Background, Border, Copper, Ink, Sage, Surface, TokenSwatch()

### Community 168 - "select.rs"
Cohesion: 0.18
Nodes (9): CHECK_CLASS, CHEVRON_CLASS, CHEVRON_STYLE, CONTENT_CLASS, DynSelect(), ITEM_CLASS, SELECT_CLASS, StyledSelect() (+1 more)

### Community 169 - "StatusBadgeVariant"
Cohesion: 0.20
Nodes (9): BASE, StatusBadge(), StatusBadgeVariant, Default, Error, Info, Success, Warning (+1 more)

### Community 170 - "SearchInput"
Cohesion: 0.22
Nodes (9): 4.1 The calculation core is a pure function, 4.2 Evaluation order is a DAG, and it matters, 4.3 Metric-native extract, 4.4 Colour uses post-boil volume, 4.5 Whirlpool is an explicit recipe field, 4.6 Brews snapshot recipes, they do not reference them, 4.7 Equipment profile is first-class from day one, 4.8 Import before UI (+1 more)

### Community 171 - "Calculation and interchange contracts"
Cohesion: 0.17
Nodes (12): BeerXML preservation and export, Bitterness, timing and chilling, Calculation and interchange contracts, Calculation context, Calculation policy V1, Fermentables and efficiency, Gravity, attenuation and colour, Ownership and shared execution (+4 more)

### Community 172 - "verify-deployment-app"
Cohesion: 0.17
Nodes (11): jobs, verify-deployment-app, name, permissions, attestations, contents, id-token, packages (+3 more)

### Community 174 - "OriginalOutcome"
Cohesion: 0.28
Nodes (8): original(), original_inner(), OriginalOutcome, ContextChanged, Forbidden, NotFound, Original, Unavailable

### Community 175 - "IngredientSection"
Cohesion: 0.40
Nodes (5): IngredientSection, Fermentable, Hop, Other, Yeast

### Community 177 - "settings_shell.rs"
Cohesion: 0.25
Nodes (6): StyleRange(), StyleState, Above, Awaiting, Below, Within

### Community 178 - "TransferStatus"
Cohesion: 0.07
Nodes (21): WorkspaceContext, InvitationStatus, Accepted, Cancelled, Declined, Expired, Pending, impl_sqlx_varchar_enum! (+13 more)

### Community 179 - "krausen-ui/package.json"
Cohesion: 0.24
Nodes (10): devDependencies, tailwindcss@4.2.2, @tailwindcss/cli@4.2.2, @tailwindcss/typography@0.5.16, name, private, version, tailwindcss (+2 more)

### Community 181 - "App Starter Skeleton — historical plan"
Cohesion: 0.18
Nodes (11): App Starter Skeleton — historical plan, Approach, For each task, Task 1: Add back missing auth service files, Task 2: Strip Kyomi migration to auth-only tables, Task 3: Copy Kyomi UI components library, Task 4: Copy auth pages from Kyomi UI, Task 5: Copy settings pages from Kyomi UI (+3 more)

### Community 182 - "profile-browser.sh"
Cohesion: 0.18
Nodes (9): BASE_URL, DATABASE_URL, ENCRYPTION_KEY, FRONTEND_URL, JWT_SECRET_KEY, KRAUSEN_E2E_DATABASE_PATH, KRAUSEN_MODE, PORT (+1 more)

### Community 183 - "Krausen"
Cohesion: 0.18
Nodes (11): Build and Run, Code boundaries, Deployment contract, Environment Variables, Krausen, License, Prerequisites, Project state (+3 more)

### Community 184 - "colour.rs"
Cohesion: 0.09
Nodes (31): equipment(), main(), recipe(), main(), darker_malt_darkens_the_beer(), EBC_PER_SRM, ebc_to_lovibond(), malt() (+23 more)

### Community 185 - "bootstrap.rs"
Cohesion: 0.23
Nodes (7): connect_delivers_an_immediate_heartbeat(), disconnect_removes_only_the_named_connection(), every_connection_for_a_user_receives_the_message(), manager(), messages_are_not_delivered_to_other_users(), per_user_connection_limit_is_enforced(), send_to_user_reaches_that_users_connection()

### Community 187 - "mode"
Cohesion: 0.20
Nodes (10): mode, default, description, options, type, on, push, workflow_dispatch (+2 more)

### Community 188 - "krausen-ui"
Cohesion: 0.58
Nodes (10): krausen-app, krausen-auth, krausen-beerxml, krausen-brewery, krausen-calc, krausen-core, krausen-design-preview, krausen-server (+2 more)

### Community 190 - "rate_limiter.rs"
Cohesion: 0.22
Nodes (7): refresh_token(), routes(), BucketConfig, check_bucket(), check_rate_limit(), RateLimitResult, UNLIMITED_REMAINING

### Community 191 - "deserialize"
Cohesion: 0.12
Nodes (7): google_login(), GoogleLoginQuery, routes(), routes(), well_known_routes(), AppState, AuthState

### Community 195 - "invitation-expiry.spec.ts"
Cohesion: 0.12
Nodes (16): apiRequest(), AuthConfig, futureISO(), generateInvitationId(), generateTransferId(), generateWorkspaceId(), hashPassword(), isolateClientIp() (+8 more)

### Community 196 - "Krausen recipe and component prototype"
Cohesion: 0.25
Nodes (8): Browser verification, Build and run, File responsibilities, Interaction checks, Krausen recipe and component prototype, Review checklist, Style reference data, What is real and what is demonstrated

### Community 197 - "Krausen agent guidance"
Cohesion: 0.25
Nodes (8): Boundary checks for implementation and review, Checkout and coordination, Delivery coordination, Krausen agent guidance, Mandatory UX/IA review gate, Required reading, Scope and design status, Test cadence

### Community 199 - "Targets"
Cohesion: 0.13
Nodes (21): preview_from_formulation(), record(), CalculationContext, Targets, AcceptedScaleContext, SCALE_POLICY_LINEAR_INGREDIENTS_V1, ScaleChange, ScalePreview (+13 more)

### Community 201 - "DuplicateOutcome"
Cohesion: 0.33
Nodes (6): SaveState, Conflict, Error, Saved, Saving, Unsaved

### Community 204 - "Architecture adoption and implementation contract"
Cohesion: 0.29
Nodes (7): Acceptance scenarios, Architecture adoption and implementation contract, Changed-contract review and verification, Deliberate deferrals, Delivery order and acceptance ownership, Resolution of independent review findings, Scope and authority

### Community 206 - "icon"
Cohesion: 0.14
Nodes (7): Modal(), ModalSize, Full, Lg, Md, Sm, Xl

### Community 208 - "image"
Cohesion: 0.33
Nodes (6): if, outputs, runs-on, timeout-minutes, image, digest

### Community 210 - "NativeMigration"
Cohesion: 0.40
Nodes (3): BreweryDialogReturnFocus, dialog_logical_lifetime_does_not_expose_native_closed_markup(), use_brewery_dialog_return_focus()

### Community 211 - "krausen-app/src/recipe_library.rs"
Cohesion: 0.06
Nodes (47): archive(), archive_inner(), ArchiveRow, Cursor, cursor(), decode(), history(), history_inner() (+39 more)

### Community 212 - "Krausen documentation"
Cohesion: 0.40
Nodes (5): Design adoption, Historical context, Independent reviews, Krausen documentation, Read first

### Community 213 - "deploy-notify"
Cohesion: 0.40
Nodes (5): if, needs, runs-on, timeout-minutes, deploy-notify

### Community 214 - "Krausen release configuration"
Cohesion: 0.50
Nodes (3): Krausen release configuration, Release, Versioning

### Community 217 - "EditorAccess"
Cohesion: 0.50
Nodes (4): AdditionalText, Name, Producer, Unit

### Community 233 - "AdditionalText"
Cohesion: 0.50
Nodes (4): StepNumber, Temperature, Time, Volume

## Knowledge Gaps
- **726 isolated node(s):** `Forbidden`, `ContextChanged`, `EquipmentArchived`, `IngredientArchived`, `BrewArchived` (+721 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 1986 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **33 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `DbPool` connect `user_service.rs` to `src/brew_evidence.rs`, `ActorContext`, `tests/process_summary.rs`, `db.rs`, `krausen-app/src/catalogue.rs`, `server/src/lib.rs`, `auth_service.rs`, `bootstrap.rs`, `migration_orders.rs`, `onboarding_service.rs`, `recover_request`, `tests/brew_start.rs`, `workspace_service.rs`, `PreviewOutcome`, `native_store.rs`, `krausen-app/src/scaling.rs`, `BrewLifecycleOutcome`, `ScaleChange`, `recipe_crud.rs`, `browser_refresh.rs`, `OriginalOutcome`, `mcp_auth.rs`, `ActorContext`, `server_fns/brew.rs`, `manager.rs`, `Validation`, `bootstrap.rs`, `krausen-app/src/interchange.rs`, `KVPool`, `token_service.rs`, `serialize`, `tests/brew_history.rs`, `oauth_service.rs`, `PreviewOutcome`, `tests/brew_evidence.rs`, `krausen-app/src/recipe_library.rs`, `beerxml.rs`, `signup_flow.rs`, `RecipeReadOutcome`, `server_fns/recipes.rs`, `tests/brew_start.rs`, `ProcessSummaryOutcome`, `krausen-app/src/process_summary.rs`?**
  _High betweenness centrality (0.184) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `RecipeDocumentV1` (e.g. with `BrewFeedbackPreview` and `ExportSelection`) actually correct?**
  _`RecipeDocumentV1` has 3 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Forbidden`, `ContextChanged`, `EquipmentArchived` to the rest of the system?**
  _726 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `src/brew_evidence.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.053551912568306013 - nodes in this community are weakly interconnected._
- **Why does `RecipeDocumentV1` connect `RecipeDraft` to `ActorContext`, `tests/process_summary.rs`, `draft_validation.rs`, `brew_evidence_store.rs`, `ButtonVariant`, `RecipeSaveOutcome`, `tests/brew_start.rs`, `native_store.rs`, `krausen-app/src/scaling.rs`, `BrewLifecycleOutcome`, `ScaleChange`, `BrewFeedbackPreviewOutcome`, `RecipeDocumentV1`, `native.rs`, `project_xml`, `Validation`, `recipe_comparison.rs`, `Targets`, `tests/brew_history.rs`, `PreviewOutcome`, `tests/brew_evidence.rs`, `krausen-app/src/recipe_library.rs`, `BrewStatusV1`, `pages/recipe_editor.rs`, `brewery.rs`, `RecipeReadOutcome`, `krausen-types/src/interchange.rs`, `tests/brew_start.rs`, `Target`, `ProcessSummaryOutcome`, `krausen-app/src/process_summary.rs`, `krausen-types/src/process_summary.rs`, `EquipmentSettingsV1`?**
  _High betweenness centrality (0.046) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `OperationIntent` (e.g. with `PreviewRequest` and `ExportRequest`) actually correct?**
  _`OperationIntent` has 3 INFERRED edges - model-reasoned connections that need verification._
- **Should `ActorContext` be split into smaller, more focused modules?**
  _Cohesion score 0.08578088578088579 - nodes in this community are weakly interconnected._