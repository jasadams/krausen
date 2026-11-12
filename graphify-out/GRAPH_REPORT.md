# Graph Report - kra46-guarded-recovery  (2026-11-13)

## Corpus Check
- 392 files · ~380,671 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 30 file(s) not represented in the graph (top: .ttf 16, .css 7, (none) 5)

## Summary
- 5488 nodes · 12951 edges · 232 communities (195 shown, 37 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 449 edges (avg confidence: 0.82)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `f2be7564`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- src/brew_evidence.rs
- DbPool
- extract_context
- mcp.rs
- tests/process_summary.rs
- src/components/mod.rs
- DbTx
- tests/recipe_migration.rs
- printable_recipe.rs
- auth.rs
- prelude
- catalogue_picker.rs
- auth_service.rs
- Error
- EditorGuard
- encryption.rs
- operations.rs
- retry.rs
- E2E Test Plan
- presets.rs
- brew_history_presentation.rs
- EquipmentSettingsV1
- src/brew.rs
- workspace_service.rs
- native_store.rs
- two_factor_auth.rs
- mcp_session_manager.rs
- app.js
- krausen-app/src/scaling.rs
- server_fns/team.rs
- oauth.rs
- app.rs
- Targets
- convert.rs
- mibu.rs
- accept_ownership.rs
- IntoView
- email_service.rs
- super
- native.rs
- krausen-app/src/process_summary.rs
- input_boundaries.rs
- popover.rs
- recipe_crud.rs
- krausen-app/src/catalogue.rs
- RecipeComposition
- mcp_auth.rs
- pages/brew_evidence.rs
- parse.rs
- server_fns/brew.rs
- krausen-calc/src/lib.rs
- BrewLifecycleOutcome
- foundation
- manager.rs
- reference_recipe.rs
- guard_tasks.rs
- RecipeDocumentV1
- @playwright/test
- tests/process_summary.rs
- ExportOutcome
- Interaction and state contracts
- pages/recipe_editor.rs
- evidence_recovery.rs
- EditorGuard
- KVPool
- token_service.rs
- sync_log_service.rs
- DbPool
- BrewEvidenceViewOutcome
- icon
- scripts
- arc
- printable_recipe.rs
- oauth_service.rs
- ibu.rs
- PreviewOutcome
- jwt.rs
- Equipment
- brew-start.spec.ts
- getDefaultWorkspaceId
- McpHandler
- src/brew.rs
- guard_tasks.rs
- BrewPlanV1
- CalcError
- baseline_golden.rs
- utils/webauthn.rs
- brewery.rs
- super
- pages/brew_start.rs
- websocket.rs
- constants.rs
- ImportRequest
- IngredientOutcome
- test-helpers.ts
- middleware/mod.rs
- leptos_frontend.rs
- signup_flow.rs
- utils/webauthn.rs
- guard_history.rs
- krausen-app/src/catalogue.rs
- with
- 4. Architecture
- server_fns/recipes.rs
- google_oauth.rs
- write.rs
- record.rs
- ColourUnit
- Button
- docs/README.md
- settings_shell.rs
- NoteOutcome
- tests/scaling.rs
- Target
- WebSocketManager
- ProcessSummaryOutcome
- Recipe
- passkey_signup_complete.rs
- utils/webauthn.rs
- krausen-types/src/formulation.rs
- recipe_service.rs
- InMemoryKVStore
- layout.rs
- toast.rs
- Shared application architecture
- MessageType
- Field
- krausen-app/src/brew_history.rs
- totp.rs
- Navigation requirements
- krausen-calc/src/formulation.rs
- db.rs
- PasskeyLoginServiceResult
- krausen-types/src/process_summary.rs
- guard_history.rs
- EquipmentOutcome
- FormulationPreview
- Krausen Coding Standards
- loginUser
- server/src/lib.rs
- canonical_json.rs
- credential_lifecycle.rs
- migration_orders.rs
- RedisKVStore
- middleware.rs
- ScaleApplyOutcome
- MeasurementOutcome
- recovery_verify_service
- User
- read_document
- EmptyStateVariant
- GuardTaskKind
- Krausen design system
- Recipes, revisions and historical brews
- components.js
- Prioritized findings
- shell-source-check.cjs
- steps
- BrewFormulationChange
- NativeMigration
- Recipe
- switch.rs
- recipe_service.rs
- canonical_json.rs
- evidence_recovery.rs
- create_user
- select.rs
- StatusBadgeVariant
- pages/equipment_profiles.rs
- Calculation and interchange contracts
- verify-deployment-app
- browser_refresh.rs
- expiry_comparison.rs
- BrewHistorySection
- .begin_write
- EvidenceReceipt
- WorkspaceRole
- krausen-ui/package.json
- recipe_recovery.rs
- App Starter Skeleton — historical plan
- profile-browser.sh
- Krausen
- 4. Architecture
- manager
- RedisPool
- mode
- krausen-ui
- history_actual
- auth_token.rs
- BrewEvidenceViewOutcome
- native_transactions.rs
- round_trip.rs
- request_id.rs
- invitation-expiry.spec.ts
- Krausen recipe and component prototype
- Krausen agent guidance
- fetch_workspace_settings_snapshot
- NoteOutcome
- brew_evidence_rows.rs
- SummaryRow
- full_table_name_expr_prefixed
- Architecture adoption and implementation contract
- settings/team.rs
- image
- WorkspaceStatus
- Krausen documentation
- deploy-notify
- Krausen release configuration
- auth_refresh.rs
- request_id.rs
- krausen-app/README.md
- kra20_baseline_4fc8313/README.md
- build.sh
- fonts/README.md
- AdditionalText
- prelude
- tests/brew_evidence.rs

## God Nodes (most connected - your core abstractions)
1. `DbPool` - 331 edges
2. `ActorContext` - 149 edges
3. `RecipeDocumentV1` - 112 edges
4. `DbTx` - 109 edges
5. `OperationIntent` - 107 edges
6. `EditorGuard` - 54 edges
7. `authorize_workspace()` - 52 edges
8. `KVPool` - 52 edges
9. `RecipeDraft` - 49 edges
10. `@playwright/test` - 48 edges

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
- 2-file cycle: `crates/krausen-ui/src/pages/brew_evidence.rs -> crates/krausen-ui/src/pages/evidence_recovery.rs -> crates/krausen-ui/src/pages/brew_evidence.rs`
- 2-file cycle: `crates/krausen-types/src/brewery.rs -> crates/krausen-types/src/formulation.rs -> crates/krausen-types/src/brewery.rs`

## Communities (232 total, 37 thin omitted)

### Community 0 - "src/brew_evidence.rs"
Cohesion: 0.08
Nodes (46): amend_actual(), archive_inner(), begin(), canonical_document(), correct_measurement(), correct_note(), digest_of(), EvidenceCursor (+38 more)

### Community 1 - "DbPool"
Cohesion: 0.08
Nodes (54): begin(), get(), search(), valid_entry_cursor_id(), digest(), get(), get_revision(), identifiers() (+46 more)

### Community 2 - "extract_context"
Cohesion: 0.05
Nodes (61): AppearanceCard(), ClearLocalDataCard(), CopyButton(), InvitationsCard(), McpConnectionCard(), ProfileInfoCard(), ProfilePage(), add_passkey_flow() (+53 more)

### Community 3 - "mcp.rs"
Cohesion: 0.03
Nodes (32): authenticate(), BeerXmlExport, BeerXmlImport, BeerXmlOriginal, BeerXmlPreview, BrewActualInput, BrewArchiveInput, BrewComparisonInput (+24 more)

### Community 4 - "tests/process_summary.rs"
Cohesion: 0.09
Nodes (17): project(), ResolvedSummary, complete_draft(), create_equipment(), equipment_request(), exercise(), intent(), postgres_preview_rechecks_authority_after_profile_lock_wait() (+9 more)

### Community 5 - "src/components/mod.rs"
Cohesion: 0.06
Nodes (62): BreweryActionGroup(), BreweryAddition(), BreweryBadge(), BreweryBeerPortrait(), BreweryBrand(), BreweryCompactIngredientRow(), BreweryDialog(), BreweryDialogReturnFocus (+54 more)

### Community 6 - "DbTx"
Cohesion: 0.05
Nodes (62): ACTUAL_BASE_SELECT, ACTUAL_SELECT, ActualRow, BrewEvidenceState, BrewStateRow, insert_actual_formulation(), insert_archive_event(), insert_lifecycle_event() (+54 more)

### Community 7 - "tests/recipe_migration.rs"
Cohesion: 0.25
Nodes (7): backfill_legacy_recipes(), MigrationReport, exercise(), postgres_migration_recovery(), postgres_upgrade_backfills_preexisting_rows(), sqlite_migration_recovery(), sqlite_upgrade_backfills_preexisting_rows()

### Community 8 - "printable_recipe.rs"
Cohesion: 0.05
Nodes (42): readiness(), summary_from_plan(), project_step(), ChillingV1, Counterflow, Forced, Natural, IngredientSection (+34 more)

### Community 9 - "auth.rs"
Cohesion: 0.05
Nodes (62): PasskeySignInButton(), CheckEmailView(), CredentialsView(), LOGIN_SUBTITLES, LoginPage(), LoginView, CheckEmail, Credentials (+54 more)

### Community 10 - "prelude"
Cohesion: 0.04
Nodes (51): BreweryPopover(), return_popover_focus(), BASE, Button(), ButtonLink(), ButtonSize, Default, Icon (+43 more)

### Community 11 - "catalogue_picker.rs"
Cohesion: 0.08
Nodes (27): fetch_one(), load_entry(), load_starter(), resolve_source(), CatalogueScope, Starter, Workspace, IngredientEntry (+19 more)

### Community 12 - "auth_service.rs"
Cohesion: 0.06
Nodes (53): build_exclude_ids(), ensure_google_oauth_auth_method(), ensure_user_has_workspace(), establish_verified_account(), EstablishVerifiedAccountParams, google_oauth_callback_service(), GoogleOAuthCallbackParams, GoogleOAuthServiceResult (+45 more)

### Community 13 - "Error"
Cohesion: 0.07
Nodes (31): effective(), engine_source_hash(), policy_source_hash(), POLICY_VERSION, resolve_in_tx(), draft(), exercise(), postgres_profiles() (+23 more)

### Community 14 - "EditorGuard"
Cohesion: 0.09
Nodes (7): EditorGuard, GuardTransition, Cancelled, Committed, Failed, Idle, Pending

### Community 15 - "encryption.rs"
Cohesion: 0.09
Nodes (30): decrypt(), decrypt_json(), decrypt_slack_token(), derive_key(), derive_key_correct_length(), derive_key_invalid_base64_rejected(), derive_key_wrong_length_rejected(), each_encryption_produces_different_ciphertext() (+22 more)

### Community 16 - "operations.rs"
Cohesion: 0.08
Nodes (45): BEERXML_EXPORT, BEERXML_IMPORT, BEERXML_ORIGINAL, BEERXML_PREVIEW, BREW_ACTUAL, BREW_ARCHIVE, BREW_COMPARISON, BREW_EVIDENCE_VIEW (+37 more)

### Community 17 - "retry.rs"
Cohesion: 0.11
Nodes (19): check_editor_access(), ContextCheckGeneration, current_context_matches(), EditorAccess, Checking, Expired, Forbidden, Mismatch (+11 more)

### Community 18 - "E2E Test Plan"
Cohesion: 0.04
Nodes (46): Current acceptance scope, E2E Test Plan, Notes for Test Implementation, TC-001: Signup Flow (Email Verification Path), TC-002: Signup With Invitation, TC-003: Login Flow (Password), TC-004: Login Flow (Passkey), TC-005: Logout (+38 more)

### Community 19 - "presets.rs"
Cohesion: 0.08
Nodes (26): a_later_preset_overrides_an_earlier_one_with_the_same_id(), a_preset_can_be_plugged_in_at_runtime(), a_preset_profile_derives_a_recipe(), adopting_a_preset_records_where_the_profile_came_from(), builtin_presets(), builtins_are_available_and_uniquely_identified(), chilling_choice_survives_a_round_trip_through_a_preset(), EquipmentField (+18 more)

### Community 20 - "brew_history_presentation.rs"
Cohesion: 0.12
Nodes (26): actor_text(), change(), change_actual_text(), change_planned_text(), change_section_text(), comparison_value_text(), diagnostic_text(), format_delta() (+18 more)

### Community 21 - "EquipmentSettingsV1"
Cohesion: 0.06
Nodes (51): recipe_head(), head_conflict(), lock_workspace(), recipe_archived(), recipe_head(), start(), start_inner(), boundary_error() (+43 more)

### Community 22 - "src/brew.rs"
Cohesion: 0.06
Nodes (37): actual(), get_actual(), map_actual_error(), ActualFormulationCommand, Amend, Get, History, ActualFormulationOutcome (+29 more)

### Community 23 - "workspace_service.rs"
Cohesion: 0.10
Nodes (35): accept_invitation(), accept_invitation_for_user(), check_existing_member_by_email(), check_pending_invitation(), complete_ownership_transfer(), count_admins(), count_pending_invitations(), count_workspace_users() (+27 more)

### Community 24 - "native_store.rs"
Cohesion: 0.10
Nodes (38): BackfillError, Encode, Legacy, encode_document(), EncodeError, Canonical, UnsupportedVersion, prepare_backfill() (+30 more)

### Community 25 - "two_factor_auth.rs"
Cohesion: 0.08
Nodes (18): ActionStatus(), Card(), CardContent(), CardDescription(), CardFooter(), CardHeader(), CardTitle(), INPUT_CLASS (+10 more)

### Community 26 - "mcp_session_manager.rs"
Cohesion: 0.16
Nodes (27): cleanup(), clone_shares_state(), create_and_validate_session(), invalidate_empty_workspace_is_noop(), invalidate_workspace_clears_all_workspace_sessions(), mcp_apps_support_default_false(), mcp_apps_support_unknown_session_returns_false(), MCPSessionManager (+19 more)

### Community 27 - "app.js"
Cohesion: 0.09
Nodes (35): beerGlass(), brews, catalogue, changeState(), clone(), closeDialog(), commitNavigation(), continuePending() (+27 more)

### Community 28 - "krausen-app/src/scaling.rs"
Cohesion: 0.12
Nodes (35): absent_values_stay_absent(), accepted_digest(), accepted_digest_binds_revision_target_policy_and_context(), apply(), apply_inner(), build_preview(), capacity_and_fixed_losses_are_explicit(), capacity_warning_checks_pre_boil_fill_too() (+27 more)

### Community 29 - "server_fns/team.rs"
Cohesion: 0.07
Nodes (41): WorkspaceRenameOutcome, Conflict, Forbidden, Invalid, RequestIdentityConflict, ResultExpired, Retryable, Saved (+33 more)

### Community 30 - "oauth.rs"
Cohesion: 0.16
Nodes (23): approve(), auth_failure(), authorization_metadata(), authorize(), client_auth(), continue_authorize(), html_escape(), invalid_client() (+15 more)

### Community 31 - "app.rs"
Cohesion: 0.07
Nodes (8): App(), Shell(), AccountRecoveryPage(), OAuthCompletePage(), PasskeyRecoveryPage(), DesignSystemPage(), OnboardingPage(), SecurityTab()

### Community 32 - "Targets"
Cohesion: 0.10
Nodes (13): document(), exercise(), postgres_scaling(), run(), settings(), sqlite_scaling(), capacity_warning_names_values_and_explicit_geometry_policy(), change_text() (+5 more)

### Community 33 - "convert.rs"
Cohesion: 0.11
Nodes (32): EXAMPLES, main(), a_converted_recipe_derives(), a_hop_use_the_engine_cannot_model_is_reported_not_hidden(), a_missing_value_is_defaulted_and_declared(), a_value_that_is_present_but_unreadable_is_reported_as_such(), an_aroma_addition_becomes_a_whirlpool_addition(), an_imported_equipment_profile_says_what_beerxml_could_not_supply() (+24 more)

### Community 34 - "mibu.rs"
Cohesion: 0.09
Nodes (31): Equipment, a_flameout_addition_still_gains_bitterness(), a_held_hop_stand_extracts_less_than_a_boiling_one(), a_lidded_kettle_cools_slower_than_an_open_one(), a_longer_stand_extracts_more_when_chilling_would_otherwise_have_started(), a_wider_kettle_cools_faster(), AA_DECAY_PER_MIN, additions_are_independent_of_each_other() (+23 more)

### Community 36 - "accept_ownership.rs"
Cohesion: 0.11
Nodes (29): AcceptOwnershipPage(), capability_item(), error_view(), fetch_ownership_transfer(), format_date_script(), icon_alert_circle_large(), icon_alert_circle_sm(), icon_arrow_right_left() (+21 more)

### Community 37 - "IntoView"
Cohesion: 0.11
Nodes (22): AdditionalFields(), AdditionalGroup(), AdditionalIngredients(), AdditionalKind, Other, Yeast, AdditionalNumberField(), AdditionalTextField() (+14 more)

### Community 38 - "email_service.rs"
Cohesion: 0.10
Nodes (11): admin_email_brand_preserves_escaped_sections(), admin_email_template(), assert_email_brand(), email_template(), EmailService, html_escape(), LOGO_BYTES, LOGO_CID (+3 more)

### Community 40 - "native.rs"
Cohesion: 0.13
Nodes (31): beerxml_dry_hop_time_is_interpreted_as_minutes(), edited_export_uses_current_native_values_and_reports_source_omission(), envelope_fragments_are_only_reused_for_unchanged_single_recipe(), explicit_choices_project_colour_and_efficiency_without_changing_source(), explicit_style_identity_survives_native_projection_and_export(), extension_follows_row_identity_after_reordering(), from_xml(), from_xml_at() (+23 more)

### Community 41 - "krausen-app/src/process_summary.rs"
Cohesion: 0.21
Nodes (15): exact_number(), from_legacy(), NativeMigration, note_unmapped_number(), OriginalSource, preserved_extensions(), preserved_fields(), project_xml() (+7 more)

### Community 43 - "popover.rs"
Cohesion: 0.10
Nodes (25): Align, End, Start, bottom_end_aligns_right_edges(), bottom_start_fits_below_no_flip(), bottom_start_flips_when_no_room_below(), caps_max_height_when_neither_side_has_room(), compute_position() (+17 more)

### Community 44 - "recipe_crud.rs"
Cohesion: 0.15
Nodes (23): create_recipe(), get_recipe(), list_recipes(), search_recipes(), a_created_recipe_reads_back(), a_deleted_recipe_disappears_from_reads_and_lists(), a_nameless_recipe_still_gets_a_usable_name(), a_recipe_is_invisible_from_another_workspace() (+15 more)

### Community 45 - "krausen-app/src/catalogue.rs"
Cohesion: 0.12
Nodes (24): digest(), identifiers(), lock_workspace(), normalize(), normalize_query(), project_page(), resolve_draft_sources(), save() (+16 more)

### Community 46 - "RecipeComposition"
Cohesion: 0.09
Nodes (3): EmailBrandSourceTests, RecipeComposition, data_url()

### Community 47 - "mcp_auth.rs"
Cohesion: 0.11
Nodes (10): https_authorization_and_sdk_through_reverse_proxy(), Proxy, brew_test_document(), brew_test_profile(), brew_ui_outcome(), exercise(), postgres_sdk_clients(), run_mcp_exercise() (+2 more)

### Community 48 - "pages/brew_evidence.rs"
Cohesion: 0.07
Nodes (38): CorrectNoteRequest, NoteCommand, Correct, List, Record, Retract, RecordNoteRequest, RetractNoteRequest (+30 more)

### Community 49 - "parse.rs"
Cohesion: 0.11
Nodes (22): ParseError, NoRootElement, Xml, accepts_a_bare_recipe_without_the_recipes_wrapper(), an_empty_element_is_recorded_as_empty_not_absent(), cdata_survives(), collect(), collections_keep_their_order() (+14 more)

### Community 50 - "server_fns/brew.rs"
Cohesion: 0.13
Nodes (28): map_view_error(), target_for(), view(), view_inner(), ArchiveBrewRequest, BrewEvidenceViewOutcome, ContextChanged, Forbidden (+20 more)

### Community 51 - "krausen-calc/src/lib.rs"
Cohesion: 0.12
Nodes (20): Corrections, Derived, FermentablePoints, IbuContribution, Volumes, abv(), compute(), corrections() (+12 more)

### Community 52 - "BrewLifecycleOutcome"
Cohesion: 0.06
Nodes (40): archive(), execute(), map_archive_error(), map_lifecycle_error(), transition(), BrewArchiveOutcome, Archived, Conflict (+32 more)

### Community 53 - "foundation"
Cohesion: 0.06
Nodes (31): concurrency, cancel-in-progress, group, KRAUSEN_TEST_POSTGRES_URL, POSTGRES_DB, POSTGRES_PASSWORD, POSTGRES_USER, env (+23 more)

### Community 54 - "manager.rs"
Cohesion: 0.13
Nodes (8): Inner, MAX_CONNECTIONS_PER_USER, NEXT_CONNECTION_ID, REDIS_CHANNEL_PREFIX, sqlite_memory_pool(), TrackedConnection, WS_CHANNEL_CAPACITY, WsSender

### Community 55 - "reference_recipe.rs"
Cohesion: 0.22
Nodes (30): derive(), ABV_EPS, assert_close(), boil_time_moves_everything_downstream_of_volume(), chilling_sooner_means_less_bitterness(), colour_uses_post_boil_volume_not_batch_volume(), CORRECTION_EPS, EBC_EPS (+22 more)

### Community 56 - "guard_tasks.rs"
Cohesion: 0.10
Nodes (24): actual_native_arrival_policy_rejects_cancelled_newer_or_retired_task_leases(), actual_navigation_keeps_first_destination_and_old_completion_cannot_satisfy_a_new_attempt(), GuardLeaveAttempt, GuardSaveResolution, Failed, NewerEdits, Saved, GuardTask (+16 more)

### Community 57 - "RecipeDocumentV1"
Cohesion: 0.11
Nodes (9): DraftContents, editing_clears_stale_server_diagnostics_and_preview(), explicit_submission_settles_raw_fields_and_old_recovery_remains_readable(), raw_numbers_preserve_empty_invalid_and_real_zero(), RecipeDraft, replacing_clean_saved_document_invalidates_existing_preview_generation(), settled_feedback_tracks_correction_without_content_or_generation_changes_on_blur(), transitional_number_is_retained_and_blocks_snapshot_without_premature_feedback() (+1 more)

### Community 58 - "@playwright/test"
Cohesion: 0.13
Nodes (11): db, gotoAuthenticated(), AUTH_STATE, hasAuthState, navigateToSecurity(), navigateToSecurity(), navigateToSecurity(), disable2FAIfEnabled() (+3 more)

### Community 59 - "tests/process_summary.rs"
Cohesion: 0.13
Nodes (17): append(), archive(), create(), duplicate_key_exists(), encode(), entry_id_in_use(), like_pattern(), execute! (+9 more)

### Community 60 - "ExportOutcome"
Cohesion: 0.12
Nodes (17): ExportOutcome, ContextChanged, Exported, Forbidden, Invalid, LossAcknowledgementRequired, NotFound, Unavailable (+9 more)

### Community 61 - "Interaction and state contracts"
Cohesion: 0.05
Nodes (38): ACC-001: Names, fields and local states, ACC-002: Chosen account/actions popup pattern, ACC-003: True modal and preview boundaries, ACC-004: Add, remove and Undo retain orientation, ACC-005: Announce meaningful settled outcomes, ACC-006: Reachability, enlargement and honest evidence, Accessibility contracts, Approved reference and visual contracts (+30 more)

### Community 62 - "pages/recipe_editor.rs"
Cohesion: 0.20
Nodes (19): changes(), compare(), display(), entered_text(), field_label(), ingredient_and_process_changes_use_names_and_keep_preserved_source_values(), native_row(), native_rows() (+11 more)

### Community 63 - "evidence_recovery.rs"
Cohesion: 0.12
Nodes (23): GuardScope, cleanup_confirmed(), EvidenceRecovery, key_in_scope(), load(), load_selected(), lock_manager(), lock_name() (+15 more)

### Community 64 - "EditorGuard"
Cohesion: 0.13
Nodes (15): ScaleOutcome, Archived, ContextChanged, Forbidden, Invalid, NotFound, Preview, Unavailable (+7 more)

### Community 65 - "KVPool"
Cohesion: 0.11
Nodes (28): delete_recovery_session(), delete_webauthn_challenge(), get_pending_signup(), get_pending_terms(), get_pending_totp(), get_recovery_session(), get_webauthn_challenge(), peek_recovery_session() (+20 more)

### Community 66 - "token_service.rs"
Cohesion: 0.12
Nodes (21): get_sessions(), create_verification_token(), create_verification_token_with_expiry(), get_user_sessions(), hash_refresh_token(), RefreshTokenUserData, RefreshTokenVerifyResult, GracePeriod (+13 more)

### Community 67 - "sync_log_service.rs"
Cohesion: 0.11
Nodes (16): action_type_to_str(), get_entries_since(), get_latest_sync_id(), is_sync_id_available(), normalise_timestamp(), parse_action_type(), prune_old_entries(), SyncLogRow (+8 more)

### Community 68 - "DbPool"
Cohesion: 0.09
Nodes (31): add_passkey_to_user(), admin_update_user(), create_api_token(), create_workspace_for_user(), default_workspace_name(), delete_passkey_from_user(), delete_user(), find_user_by_credential_id() (+23 more)

### Community 69 - "BrewEvidenceViewOutcome"
Cohesion: 0.13
Nodes (15): CONTEXT_CHANGED_KEY, EditorDestination, History, Href, Logout, Workspace, Layout(), Sidebar() (+7 more)

### Community 70 - "icon"
Cohesion: 0.22
Nodes (27): Component families and production mapping, brewsPage(), compactSummary(), composition(), editor(), fmt(), historyPage(), ingredientPanel() (+19 more)

### Community 71 - "scripts"
Cohesion: 0.10
Nodes (21): dependencies, argon2@^0.44.0, devDependencies, otplib@^12.0.1, @playwright/test@^1.49.0, name, private, scripts (+13 more)

### Community 72 - "arc"
Cohesion: 0.12
Nodes (12): notify_signup(), send_signup_slack(), accept_terms(), AcceptTermsOutcome, InvalidToken, Success, Config, KrausenMode (+4 more)

### Community 73 - "printable_recipe.rs"
Cohesion: 0.12
Nodes (14): actual(), correct_request(), document(), evidence_view(), exercise(), fixture(), integrated_acceptance(), intent() (+6 more)

### Community 74 - "oauth_service.rs"
Cohesion: 0.16
Nodes (23): http_client(), approve(), authenticate_client(), begin_authorization(), bind_pending(), Client, Code, Credential (+15 more)

### Community 75 - "ibu.rs"
Cohesion: 0.13
Nodes (24): a_whirlpool_with_no_stand_still_isomerises_while_the_wort_chills(), AA_DECAY_PER_MIN, BIGNESS_BASE, BIGNESS_COEFFICIENT, bigness_factor(), bitterness_scales_linearly_with_hop_weight(), contributions(), dry_hops_contribute_no_bitterness_and_no_row() (+16 more)

### Community 76 - "PreviewOutcome"
Cohesion: 0.12
Nodes (20): project_preview(), PreviewOutcome, ContextChanged, Forbidden, Invalid, Preview, Unavailable, UnresolvedProfile (+12 more)

### Community 77 - "jwt.rs"
Cohesion: 0.15
Nodes (18): BROWSER_AUDIENCE, BROWSER_ISSUER, Claims, create_access_token(), create_access_token_str(), create_refresh_token(), decode_python_shaped_jwt_with_extra_fields(), empty_token_rejected() (+10 more)

### Community 78 - "Equipment"
Cohesion: 0.17
Nodes (12): Equipment, equipment(), main(), recipe(), WithChilling, Chilling, Counterflow, Forced (+4 more)

### Community 79 - "brew-start.spec.ts"
Cohesion: 0.10
Nodes (13): { chromium }, path, SCREENSHOT_DIR, AUTH_STATE_PATH, getKrausenMode(), globalSetup(), sqliteExec(), importRecipe() (+5 more)

### Community 80 - "getDefaultWorkspaceId"
Cohesion: 0.26
Nodes (15): allowSignup(), cleanTestData(), createSecondUser(), generateUserId(), getDefaultUserId(), getDefaultWorkspaceId(), nowISO(), seedUser() (+7 more)

### Community 81 - "McpHandler"
Cohesion: 0.06
Nodes (37): Alert(), AlertDescription(), AlertTitle(), AlertVariant, Default, Error, Info, Success (+29 more)

### Community 82 - "src/brew.rs"
Cohesion: 0.14
Nodes (29): amend_document(), complete_document(), correct_request(), equipment_request(), exercise(), exercise_acceptance_cases(), fixture(), intent() (+21 more)

### Community 83 - "guard_tasks.rs"
Cohesion: 0.15
Nodes (18): ingredient_number(), ingredient_text(), IngredientKind, Fermentable, Hop, IngredientNumber, Amount, ColourOrTime (+10 more)

### Community 84 - "BrewPlanV1"
Cohesion: 0.11
Nodes (22): load_brew(), BREW_PLAN_SCHEMA_VERSION, BrewRow, BrewSummaryRow, encode_plan(), insert(), list_for_recipe(), load() (+14 more)

### Community 85 - "CalcError"
Cohesion: 0.16
Nodes (20): CalcError, AttenuationOutOfRange, BoilOffExceedsPreBoil, EfficiencyOutOfRange, EmptyGrist, InvalidInput, MashWaterExceedsTotal, NegativeBoilOffRate (+12 more)

### Community 86 - "baseline_golden.rs"
Cohesion: 0.11
Nodes (9): exercise(), postgres_interchange(), sqlite_interchange(), BeerXmlExport(), BeerXmlForm(), BeerXmlPage(), hand_off_import_action_focus(), matches_import_context() (+1 more)

### Community 87 - "utils/webauthn.rs"
Cohesion: 0.16
Nodes (7): build_webauthn(), finish_authentication(), finish_discoverable_authentication(), finish_registration(), start_authentication(), start_discoverable_authentication(), start_registration()

### Community 88 - "brewery.rs"
Cohesion: 0.13
Nodes (20): resolve_one(), actual_from_plan(), actual_from_recipe(), actual_projection_retains_boil_duration_and_does_not_mutate_plan(), planned_recipe(), ActualFormulationV1, AdditionStage, FermentableAddition (+12 more)

### Community 89 - "super"
Cohesion: 0.13
Nodes (18): AccountRecoveryCompletePage(), error_view(), PageState, Error, Ready, Submitting, Success, Verifying (+10 more)

### Community 90 - "pages/brew_start.rs"
Cohesion: 0.16
Nodes (14): build_start_request(), diagnostic_text(), HowToStart(), invalidate_selection(), may_prepare(), metric(), prepared_current_selection_keeps_original_head_guard(), prepared_head_guard() (+6 more)

### Community 91 - "websocket.rs"
Cohesion: 0.14
Nodes (10): CLOSE_AUTH_REQUIRED, CLOSE_FORBIDDEN, CLOSE_TOO_MANY_CONNECTIONS, close_with_code(), handle_authenticated_ws(), MAX_MESSAGE_SIZE, PING_INTERVAL_SECS, valid_credential() (+2 more)

### Community 92 - "constants.rs"
Cohesion: 0.16
Nodes (13): Constants, CONSTANTS, CookieConstants, get(), JwtConstants, load_with_fallback(), RateLimitBucket, RateLimitConstants (+5 more)

### Community 93 - "ImportRequest"
Cohesion: 0.14
Nodes (14): ImportCommitOutcome, ChoicesRequired, ContextChanged, Forbidden, Imported, Invalid, InvalidRequest, InvalidSource (+6 more)

### Community 94 - "IngredientOutcome"
Cohesion: 0.09
Nodes (25): project(), project_get(), INGREDIENT_ENTRY_KINDS, IngredientCommand, Archive, Get, Save, Search (+17 more)

### Community 95 - "test-helpers.ts"
Cohesion: 0.20
Nodes (15): ensureLoggedIn(), fetchAuthConfig(), generateEmail(), getCookies(), getKrausenMode(), isPersonalMode(), isSaasWithSmtp(), isSelfHostedMode() (+7 more)

### Community 96 - "middleware/mod.rs"
Cohesion: 0.15
Nodes (7): cors_layer(), security_headers(), clear_recovery_session_cookie(), clear_token_cookies(), get_cookie_value(), set_recovery_session_cookie(), set_token_cookies()

### Community 97 - "leptos_frontend.rs"
Cohesion: 0.15
Nodes (9): ANYTHING, ASSET, BROWSER, dist_dir(), serve(), serve_asset_or_shell(), serve_leptos_shell(), static_files_service() (+1 more)

### Community 98 - "signup_flow.rs"
Cohesion: 0.20
Nodes (19): signup_complete_service(), SignupCompleteServiceResult, Error, InvalidToken, Success, a_known_address_is_answered_the_same_as_an_unknown_one(), a_second_link_does_not_make_a_second_account(), an_unaccepted_token_leaves_nothing_behind() (+11 more)

### Community 99 - "utils/webauthn.rs"
Cohesion: 0.10
Nodes (20): BrewFeedbackApplyOutcome, ActualConflict, Applied, ContextChanged, ContextConflict, Forbidden, HeadConflict, Invalid (+12 more)

### Community 100 - "guard_history.rs"
Cohesion: 0.17
Nodes (15): Bookmark, get(), install(), key(), Phase, Choosing, Committing, Held (+7 more)

### Community 101 - "krausen-app/src/catalogue.rs"
Cohesion: 0.12
Nodes (11): backoff_delay(), INITIAL_INTERVAL_SECS, MAX_INTERVAL, MAX_RETRIES, max_retries_exhausted_returns_last_error(), MULTIPLIER, non_retryable_error_fails_immediately(), retry_with_backoff() (+3 more)

### Community 102 - "with"
Cohesion: 0.10
Nodes (20): with, cache-from, cache-to, client-id, context, file, owner, password (+12 more)

### Community 103 - "4. Architecture"
Cohesion: 0.07
Nodes (27): 1. The thesis, 2. What Krausen is not, 3.1 Hide questions, not capabilities, 3.2 The software has a point of view, 3.3 Learn the environment, never silently change it, 3.4 Flag runs, not misses, 3.5 Say "I don't know yet", 3.6 AI never produces a number (+19 more)

### Community 104 - "server_fns/recipes.rs"
Cohesion: 0.23
Nodes (16): Recipe, create_recipe(), delete_recipe(), DerivedSummary, detail(), get_recipe(), import_beerxml(), list_recipes() (+8 more)

### Community 105 - "google_oauth.rs"
Cohesion: 0.18
Nodes (17): authorization_url_targets_google_with_login_scopes(), build_authorization_url(), build_oauth_data(), exchange_code_for_tokens(), get_user_info(), GOOGLE_AUTH_URI, GOOGLE_TOKEN_URI, GOOGLE_USER_INFO_URI (+9 more)

### Community 106 - "write.rs"
Cohesion: 0.21
Nodes (4): BeerXml, Mash, Recipe, write_recipe_document()

### Community 107 - "record.rs"
Cohesion: 0.14
Nodes (8): a_number_with_a_unit_suffix_parses_and_still_writes_verbatim(), an_empty_element_survives_the_round_trip(), an_unparseable_number_is_kept_rather_than_dropped_or_defaulted(), fields_are_written_in_declaration_order(), record!, only_fields_that_were_set_are_written(), Record, .TAG

### Community 108 - "ColourUnit"
Cohesion: 0.14
Nodes (15): ColourUnit, Lovibond, Srm, EfficiencyChoice, RecipeOverride, SelectedEquipment, ImportPreview, ImportPreviewOutcome (+7 more)

### Community 109 - "Button"
Cohesion: 0.27
Nodes (19): actualsDialog(), allFermentables(), catalogueResults(), createFermentableDialog(), equipmentDialog(), inspectRevision(), openAdd(), openFermentablePicker() (+11 more)

### Community 111 - "settings_shell.rs"
Cohesion: 0.16
Nodes (6): SettingsShell(), SettingsTab, TABS, visible_tabs(), get_user_context(), UserContext

### Community 112 - "NoteOutcome"
Cohesion: 0.10
Nodes (21): map_note_error(), notes(), NoteHistoryPage, NoteOutcome, Archived, Conflict, ContextChanged, Corrected (+13 more)

### Community 113 - "tests/scaling.rs"
Cohesion: 0.07
Nodes (25): baseline_v1_golden_refused_and_v2_matches_targets(), CONTEXT, DIGEST, FIXTURE, STYLE, TARGETS, VALIDATION, complete_document() (+17 more)

### Community 114 - "Target"
Cohesion: 0.07
Nodes (31): ALL, EXTRACT, IBU, Rule, Addition, Finite, Fraction, Nonnegative (+23 more)

### Community 115 - "WebSocketManager"
Cohesion: 0.20
Nodes (11): send_error(), send_member_joined(), send_member_role_changed(), send_ownership_transfer_completed(), send_ownership_transfer_declined(), send_ownership_transfer_offered(), send_sync_action(), send_workspace_invitation() (+3 more)

### Community 116 - "ProcessSummaryOutcome"
Cohesion: 0.14
Nodes (14): ProcessSummaryOutcome, ContextChanged, Forbidden, Invalid, InvalidRequest, NotFound, Summary, Unavailable (+6 more)

### Community 118 - "passkey_signup_complete.rs"
Cohesion: 0.11
Nodes (10): Checkbox(), CHECKBOX_ACTIVE, CHECKBOX_BASE, CHECKBOX_UNCHECKED, Spinner(), WelcomePage(), accept_terms(), AcceptTermsResult (+2 more)

### Community 119 - "utils/webauthn.rs"
Cohesion: 0.24
Nodes (9): array_buffer_to_base64url(), base64url_to_array_buffer(), convert_credential_descriptors(), is_webauthn_available(), js_err(), js_get(), js_set(), start_authentication() (+1 more)

### Community 120 - "krausen-types/src/formulation.rs"
Cohesion: 0.13
Nodes (21): compare(), identities(), identities_resolve_and_bounds_are_inclusive_at_full_precision(), resolve(), merge_defaults(), record_default(), targets_with_defaults(), BrewPlanV1 (+13 more)

### Community 121 - "recipe_service.rs"
Cohesion: 0.10
Nodes (21): map_measurement_error(), MeasurementOutcome, Archived, Conflict, ContextChanged, Corrected, EntryConflict, Forbidden (+13 more)

### Community 122 - "InMemoryKVStore"
Cohesion: 0.13
Nodes (12): del_removes_key(), Entry, expire_sets_ttl(), getdel_missing_returns_none(), getdel_returns_and_removes(), incr_creates_and_increments(), InMemoryKVStore, missing_key_returns_none() (+4 more)

### Community 123 - "layout.rs"
Cohesion: 0.14
Nodes (12): append(), archive(), create(), encode(), list(), load(), load_revision(), execute! (+4 more)

### Community 124 - "toast.rs"
Cohesion: 0.20
Nodes (13): add_toast(), Toast, toast_error(), toast_info(), toast_success(), ToastContainer(), ToastItem(), ToastProvider() (+5 more)

### Community 125 - "Shared application architecture"
Cohesion: 0.12
Nodes (17): Application architecture, Authentication and authorization, Capability matrix, Common execution and persistence contract, Confirmed product contracts, Dependency and implementation acceptance, Existing implementation and gaps, Installation and credential contract (+9 more)

### Community 126 - "MessageType"
Cohesion: 0.14
Nodes (12): MessageType, Error, Heartbeat, MemberJoined, MemberRoleChanged, OwnershipTransferCompleted, OwnershipTransferDeclined, OwnershipTransferOffered (+4 more)

### Community 127 - "Field"
Cohesion: 0.10
Nodes (8): a_value_krausen_writes_itself_round_trips_through_raw(), bool, f64, Field, leading_number(), Scalar<T>, String, u32

### Community 128 - "krausen-app/src/brew_history.rs"
Cohesion: 0.05
Nodes (77): apply_changes(), comparison(), comparison_inner(), feedback_apply(), feedback_apply_inner(), feedback_preview(), feedback_preview_inner(), historical_print() (+69 more)

### Community 129 - "totp.rs"
Cohesion: 0.19
Nodes (14): build_totp(), DIGITS, generate_qr_code(), generate_secret(), generate_secret_returns_valid_base32(), ISSUER, provisioning_uri(), provisioning_uri_contains_issuer() (+6 more)

### Community 130 - "Navigation requirements"
Cohesion: 0.11
Nodes (18): Authority and use, Current-state boundary, IA-001: Recipe-first home and truthful destinations, IA-002: One compact account dropdown, IA-003: Workspace switching is exceptional account context, IA-004: Mobile navigation has one modal owner, IA-005: Internal showcase policy, IA-006: Object-local vocabulary and source context (+10 more)

### Community 131 - "krausen-calc/src/formulation.rs"
Cohesion: 0.11
Nodes (20): calculate(), colour_ebc(), ENGINE_BUILD, ENGINE_SOURCE, ENGINE_VERSION, FermentableInput, FermentableStage, Boil (+12 more)

### Community 132 - "db.rs"
Cohesion: 0.09
Nodes (12): create_pool(), DbQueryResult, db_execute!, db_fetch_all!, db_fetch_one!, db_fetch_optional!, db_fetch_scalar!, db_with_pool! (+4 more)

### Community 133 - "PasskeyLoginServiceResult"
Cohesion: 0.12
Nodes (15): get_passkeys_for_user(), passkey_login_complete_service(), PasskeyLoginServiceResult, AuthFailed, InvalidChallenge, InvalidCredentials, RateLimited, Success (+7 more)

### Community 134 - "krausen-types/src/process_summary.rs"
Cohesion: 0.29
Nodes (13): empty_collections_are_still_written(), indent(), INDENT, leaf(), output_is_well_formed_enough_to_read_back(), text_needing_escapes_round_trips(), write(), write_collection() (+5 more)

### Community 136 - "EquipmentOutcome"
Cohesion: 0.08
Nodes (30): EquipmentRevision, ArchiveEquipment, EquipmentCommand, Archive, Get, List, Revision, Save (+22 more)

### Community 138 - "Krausen Coding Standards"
Cohesion: 0.13
Nodes (15): Acceptance, Canonical models and projection boundaries, Change delivery and client state, Component Patterns, Database Queries, Design System Adherence, Dynamic Query Builders, Error Handling (+7 more)

### Community 139 - "loginUser"
Cohesion: 0.11
Nodes (5): DEFAULT_TEST_USER, sqlText(), importGeometryExpect, catalogueAction(), catalogueCreateForm()

### Community 140 - "server/src/lib.rs"
Cohesion: 0.18
Nodes (4): bootstrap_owner(), build_router(), claim_legacy_personal_owner(), wrap_service()

### Community 142 - "credential_lifecycle.rs"
Cohesion: 0.12
Nodes (9): exercise(), postgres_operator_bootstrap(), sqlite_operator_bootstrap(), refresh_tokens(), RefreshedTokens, exercise(), exercise_oauth(), postgres_browser_credential_lifecycle() (+1 more)

### Community 143 - "migration_orders.rs"
Cohesion: 0.20
Nodes (11): BREW_EVIDENCE, BREWS, CATALOGUE, CREDENTIALS, EQUIPMENT, exercise(), NATIVE, postgres_empty_and_both_upgrade_orders() (+3 more)

### Community 145 - "middleware.rs"
Cohesion: 0.23
Nodes (5): AuthState, AuthUser, CredentialIdRow, extract_token(), WorkspaceContext

### Community 146 - "ScaleApplyOutcome"
Cohesion: 0.07
Nodes (39): DuplicateOutcome, Archived, Conflict, ContextChanged, Duplicated, Forbidden, InvalidRequest, NotFound (+31 more)

### Community 147 - "MeasurementOutcome"
Cohesion: 0.48
Nodes (4): exercise_sqlite(), exercise_sqlite_constraints(), seed_sqlite(), sqlite_brew_plan_is_immutable()

### Community 148 - "recovery_verify_service"
Cohesion: 0.40
Nodes (5): recovery_verify_service(), RecoveryVerifyServiceResult, AccountNotVerified, InvalidToken, Success

### Community 149 - "User"
Cohesion: 0.26
Nodes (12): get_user_workspace_context(), get_workspace(), get_workspace_user(), get_user_workspaces(), get_workspace_users(), ApiToken, OwnershipTransfer, User (+4 more)

### Community 150 - "read_document"
Cohesion: 0.12
Nodes (20): exercise(), postgres_editor(), postgres_resource_lock_wait_rechecks_both_expiries(), save(), sqlite_editor(), RecipeSaveOutcome, Conflict, ContextChanged (+12 more)

### Community 151 - "EmptyStateVariant"
Cohesion: 0.16
Nodes (8): EmptyState(), EmptyStateVariant, Default, Error, Info, Sidebar, Success, Warning

### Community 152 - "GuardTaskKind"
Cohesion: 0.10
Nodes (18): StartBrewOutcome, ContextChanged, ContextConflict, EquipmentProfileArchived, Forbidden, HeadConflict, InvalidRequest, NotFound (+10 more)

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

### Community 160 - "BrewFormulationChange"
Cohesion: 0.20
Nodes (15): canonical_digest(), change(), comparison_changes(), ingredient_changes(), value_for_fermentable(), value_for_hop(), value_for_other(), value_for_step() (+7 more)

### Community 161 - "NativeMigration"
Cohesion: 0.21
Nodes (9): apply_to_document(), read_local_storage(), resolve_theme(), save_theme_to_local_storage(), STORAGE_KEY, ThemeProvider(), ThemeState, use_theme() (+1 more)

### Community 162 - "Recipe"
Cohesion: 0.27
Nodes (7): Recipe, RecipeRow, RecipeSource, BeerXml, Krausen, RecipeSummary, SummaryRow

### Community 163 - "switch.rs"
Cohesion: 0.15
Nodes (9): LABEL_DEFAULT_CLASS, LABEL_WRAPPER_CLASS, Switch(), THUMB_BASE, THUMB_CHECKED, THUMB_UNCHECKED, TRACK_BASE, TRACK_CHECKED (+1 more)

### Community 164 - "recipe_service.rs"
Cohesion: 0.25
Nodes (11): create_recipe(), delete_recipe(), get_legacy_recovery(), index(), Indexed, LegacyRecovery, NativeHead, new_recipe_id() (+3 more)

### Community 165 - "canonical_json.rs"
Cohesion: 0.21
Nodes (8): bytes(), CanonicalError, IntegerOutOfRange, Json, NonFinite, Serialize, from_serializable(), sha256()

### Community 166 - "evidence_recovery.rs"
Cohesion: 0.50
Nodes (7): invalid_chilling_and_geometry_are_not_replaced_by_defaults(), invalid_field(), no_hops_is_zero_ibu_and_zero_optional_losses_are_valid(), recipe(), rejects_gravity_outside_abv_formula_domain(), rejects_nonfinite_values_before_they_can_poison_results(), validates_each_row_and_hop_exposure()

### Community 167 - "create_user"
Cohesion: 0.23
Nodes (8): admin_create_user(), create_user(), generate_user_id(), get_user_by_id(), a_workspace_is_named_after_its_owner(), a_workspace_is_named_even_without_a_display_name(), name_of(), test_db()

### Community 168 - "select.rs"
Cohesion: 0.18
Nodes (9): CHECK_CLASS, CHEVRON_CLASS, CHEVRON_STYLE, CONTENT_CLASS, DynSelect(), ITEM_CLASS, SELECT_CLASS, StyledSelect() (+1 more)

### Community 169 - "StatusBadgeVariant"
Cohesion: 0.20
Nodes (9): BASE, StatusBadge(), StatusBadgeVariant, Default, Error, Info, Success, Warning (+1 more)

### Community 170 - "pages/equipment_profiles.rs"
Cohesion: 0.20
Nodes (4): assign_values(), EquipmentProfiles(), FIELDS, values()

### Community 171 - "Calculation and interchange contracts"
Cohesion: 0.17
Nodes (12): BeerXML preservation and export, Bitterness, timing and chilling, Calculation and interchange contracts, Calculation context, Calculation policy V1, Fermentables and efficiency, Gravity, attenuation and colour, Ownership and shared execution (+4 more)

### Community 172 - "verify-deployment-app"
Cohesion: 0.17
Nodes (11): jobs, verify-deployment-app, name, permissions, attestations, contents, id-token, packages (+3 more)

### Community 174 - "expiry_comparison.rs"
Cohesion: 0.29
Nodes (7): an_expiry_an_hour_ago_is_not_in_the_future(), an_expiry_later_today_is_in_the_future(), evaluate(), expiry_on_other_days_still_works(), SHAPES, stamp(), the_naive_comparison_is_why_this_helper_exists()

### Community 175 - "BrewHistorySection"
Cohesion: 0.18
Nodes (11): BrewHistorySection, Extensions, Fermentable, FermentationStep, Hop, MashStep, Notes, OtherIngredient (+3 more)

### Community 177 - "EvidenceReceipt"
Cohesion: 0.18
Nodes (7): EvidenceReceipt, Archive, Lifecycle, Measurement, Note, original_command_roundtrip_and_scope_are_independent_of_unsent_inputs(), read_commands_and_unknown_versions_are_never_recovered_as_writes()

### Community 178 - "WorkspaceRole"
Cohesion: 0.07
Nodes (20): InvitationStatus, Accepted, Cancelled, Declined, Expired, Pending, impl_sqlx_varchar_enum!, TransferStatus (+12 more)

### Community 179 - "krausen-ui/package.json"
Cohesion: 0.24
Nodes (10): devDependencies, tailwindcss@4.2.2, @tailwindcss/cli@4.2.2, @tailwindcss/typography@0.5.16, name, private, version, tailwindcss (+2 more)

### Community 180 - "recipe_recovery.rs"
Cohesion: 0.31
Nodes (5): PendingRecipeSave, find(), RecoveryRecord, RecoveryScope, write()

### Community 181 - "App Starter Skeleton — historical plan"
Cohesion: 0.18
Nodes (11): App Starter Skeleton — historical plan, Approach, For each task, Task 1: Add back missing auth service files, Task 2: Strip Kyomi migration to auth-only tables, Task 3: Copy Kyomi UI components library, Task 4: Copy auth pages from Kyomi UI, Task 5: Copy settings pages from Kyomi UI (+3 more)

### Community 182 - "profile-browser.sh"
Cohesion: 0.18
Nodes (9): BASE_URL, DATABASE_URL, ENCRYPTION_KEY, FRONTEND_URL, JWT_SECRET_KEY, KRAUSEN_E2E_DATABASE_PATH, KRAUSEN_MODE, PORT (+1 more)

### Community 183 - "Krausen"
Cohesion: 0.18
Nodes (11): Build and Run, Code boundaries, Deployment contract, Environment Variables, Krausen, License, Prerequisites, Project state (+3 more)

### Community 184 - "4. Architecture"
Cohesion: 0.10
Nodes (23): main(), darker_malt_darkens_the_beer(), EBC_PER_SRM, ebc_to_lovibond(), malt(), more_water_lightens_the_beer(), morey(), MOREY_COEFFICIENT (+15 more)

### Community 185 - "manager"
Cohesion: 0.25
Nodes (7): connect_delivers_an_immediate_heartbeat(), disconnect_removes_only_the_named_connection(), every_connection_for_a_user_receives_the_message(), manager(), messages_are_not_delivered_to_other_users(), per_user_connection_limit_is_enforced(), send_to_user_reaches_that_users_connection()

### Community 186 - "RedisPool"
Cohesion: 0.33
Nodes (4): create_pool(), ping(), RedisPool, test_redis_connects()

### Community 187 - "mode"
Cohesion: 0.20
Nodes (10): mode, default, description, options, type, on, push, workflow_dispatch (+2 more)

### Community 188 - "krausen-ui"
Cohesion: 0.58
Nodes (10): krausen-app, krausen-auth, krausen-beerxml, krausen-brewery, krausen-calc, krausen-core, krausen-design-preview, krausen-server (+2 more)

### Community 189 - "history_actual"
Cohesion: 0.33
Nodes (9): decode_cursor(), encode_cursor(), history_actual(), list_measurements(), list_notes(), validate_limit(), ActualFormulationHistoryRequest, MeasurementListRequest (+1 more)

### Community 190 - "auth_token.rs"
Cohesion: 0.22
Nodes (7): refresh_token(), routes(), BucketConfig, check_bucket(), check_rate_limit(), RateLimitResult, UNLIMITED_REMAINING

### Community 191 - "BrewEvidenceViewOutcome"
Cohesion: 0.13
Nodes (7): google_login(), GoogleLoginQuery, routes(), routes(), well_known_routes(), AppState, AuthState

### Community 192 - "native_transactions.rs"
Cohesion: 0.38
Nodes (3): exercise(), postgres_native_transactions(), sqlite_native_transactions()

### Community 195 - "invitation-expiry.spec.ts"
Cohesion: 0.12
Nodes (16): apiRequest(), AuthConfig, futureISO(), generateInvitationId(), generateTransferId(), generateWorkspaceId(), hasCookie(), hashPassword() (+8 more)

### Community 196 - "Krausen recipe and component prototype"
Cohesion: 0.25
Nodes (8): Browser verification, Build and run, File responsibilities, Interaction checks, Krausen recipe and component prototype, Review checklist, Style reference data, What is real and what is demonstrated

### Community 197 - "Krausen agent guidance"
Cohesion: 0.25
Nodes (8): Boundary checks for implementation and review, Checkout and coordination, Delivery coordination, Krausen agent guidance, Mandatory UX/IA review gate, Required reading, Scope and design status, Test cadence

### Community 198 - "fetch_workspace_settings_snapshot"
Cohesion: 0.83
Nodes (3): fetch_workspace_settings_snapshot(), get_workspace_settings_for_sync(), update_workspace_settings()

### Community 199 - "NoteOutcome"
Cohesion: 0.11
Nodes (21): preview_from_formulation(), record(), Targets, AcceptedScaleContext, SCALE_POLICY_LINEAR_INGREDIENTS_V1, ScaleChange, ScalePreview, ScaleSection (+13 more)

### Community 204 - "Architecture adoption and implementation contract"
Cohesion: 0.29
Nodes (7): Acceptance scenarios, Architecture adoption and implementation contract, Changed-contract review and verification, Deliberate deferrals, Delivery order and acceptance ownership, Resolution of independent review findings, Scope and authority

### Community 206 - "settings/team.rs"
Cohesion: 0.06
Nodes (23): Badge(), BadgeVariant, Default, Destructive, Outline, Secondary, Warning, BASE (+15 more)

### Community 208 - "image"
Cohesion: 0.33
Nodes (6): if, outputs, runs-on, timeout-minutes, image, digest

### Community 211 - "WorkspaceStatus"
Cohesion: 0.06
Nodes (50): archive(), ArchiveRow, Cursor, cursor(), decode(), history(), history_inner(), limit() (+42 more)

### Community 212 - "Krausen documentation"
Cohesion: 0.40
Nodes (5): Design adoption, Historical context, Independent reviews, Krausen documentation, Read first

### Community 213 - "deploy-notify"
Cohesion: 0.40
Nodes (5): if, needs, runs-on, timeout-minutes, deploy-notify

### Community 214 - "Krausen release configuration"
Cohesion: 0.50
Nodes (3): Krausen release configuration, Release, Versioning

### Community 217 - "request_id.rs"
Cohesion: 0.50
Nodes (4): AdditionalText, Name, Producer, Unit

### Community 233 - "AdditionalText"
Cohesion: 0.50
Nodes (4): StepNumber, Temperature, Time, Volume

## Knowledge Gaps
- **750 isolated node(s):** `Forbidden`, `ContextChanged`, `EquipmentArchived`, `IngredientArchived`, `BrewArchived` (+745 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 2032 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **37 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `DbPool` connect `DbPool` to `src/brew_evidence.rs`, `krausen-app/src/brew_history.rs`, `tests/process_summary.rs`, `PasskeyLoginServiceResult`, `db.rs`, `tests/recipe_migration.rs`, `server/src/lib.rs`, `Error`, `credential_lifecycle.rs`, `migration_orders.rs`, `auth_service.rs`, `recovery_verify_service`, `EquipmentSettingsV1`, `src/brew.rs`, `read_document`, `User`, `workspace_service.rs`, `krausen-app/src/scaling.rs`, `Targets`, `recipe_service.rs`, `create_user`, `recipe_crud.rs`, `browser_refresh.rs`, `krausen-app/src/catalogue.rs`, `mcp_auth.rs`, `server_fns/brew.rs`, `BrewLifecycleOutcome`, `manager.rs`, `manager`, `history_actual`, `native_transactions.rs`, `KVPool`, `token_service.rs`, `sync_log_service.rs`, `DbPool`, `fetch_workspace_settings_snapshot`, `arc`, `printable_recipe.rs`, `oauth_service.rs`, `src/brew.rs`, `WorkspaceStatus`, `baseline_golden.rs`, `signup_flow.rs`, `server_fns/recipes.rs`, `NoteOutcome`, `tests/scaling.rs`?**
  _High betweenness centrality (0.168) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `RecipeDocumentV1` (e.g. with `BrewFeedbackPreview` and `ExportSelection`) actually correct?**
  _`RecipeDocumentV1` has 3 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Forbidden`, `ContextChanged`, `EquipmentArchived` to the rest of the system?**
  _750 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `src/brew_evidence.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.08158508158508158 - nodes in this community are weakly interconnected._
- **Why does `RecipeDocumentV1` connect `krausen-app/src/scaling.rs` to `krausen-app/src/brew_history.rs`, `DbPool`, `krausen-calc/src/formulation.rs`, `tests/process_summary.rs`, `src/components/mod.rs`, `DbTx`, `printable_recipe.rs`, `EquipmentOutcome`, `Error`, `retry.rs`, `EquipmentSettingsV1`, `read_document`, `native_store.rs`, `Targets`, `IntoView`, `super`, `native.rs`, `krausen-app/src/process_summary.rs`, `krausen-app/src/catalogue.rs`, `recipe_recovery.rs`, `RecipeDocumentV1`, `pages/recipe_editor.rs`, `NoteOutcome`, `printable_recipe.rs`, `src/brew.rs`, `WorkspaceStatus`, `guard_tasks.rs`, `brewery.rs`, `ColourUnit`, `tests/scaling.rs`, `Target`, `krausen-types/src/formulation.rs`?**
  _High betweenness centrality (0.048) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `OperationIntent` (e.g. with `PreviewRequest` and `ExportRequest`) actually correct?**
  _`OperationIntent` has 3 INFERRED edges - model-reasoned connections that need verification._
- **Should `DbPool` be split into smaller, more focused modules?**
  _Cohesion score 0.08088531187122737 - nodes in this community are weakly interconnected._