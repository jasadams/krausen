# Graph Report - password-reset-submit  (2026-11-02)

## Corpus Check
- 385 files · ~336,759 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 30 file(s) not represented in the graph (top: .ttf 16, .css 7, (none) 5)

## Summary
- 5179 nodes · 12277 edges · 248 communities (208 shown, 40 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 437 edges (avg confidence: 0.82)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `934c6983`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- src/brew_evidence.rs
- ActorContext
- extract_context
- mcp.rs
- tests/brew_history.rs
- src/components/mod.rs
- brew_evidence_store.rs
- ActionStatus
- printable_recipe.rs
- auth.rs
- ButtonVariant
- DbPool
- auth_service.rs
- Error
- krausen-app/src/recipe_library.rs
- encryption.rs
- operations.rs
- tests/brew_start.rs
- E2E Test Plan
- presets.rs
- brew_history_presentation.rs
- passkey_recovery_complete.rs
- krausen-app/src/brew_history.rs
- workspace_service.rs
- server_fns/brew_history.rs
- prelude
- mcp_session_manager.rs
- app.js
- krausen-app/src/scaling.rs
- server_fns/team.rs
- oauth.rs
- app.rs
- recipe_scaling.rs
- convert.rs
- mibu.rs
- accept_ownership.rs
- IntoView
- email_service.rs
- DbTx
- native.rs
- tests/process_summary.rs
- Recipe
- popover.rs
- recipe_crud.rs
- recipe_revision_history.rs
- RecipeComposition
- mcp_auth.rs
- pages/brew_evidence.rs
- parse.rs
- ingredient_catalogue.rs
- krausen-calc/src/lib.rs
- server_fns/scaling.rs
- foundation
- manager.rs
- reference_recipe.rs
- Preview
- RecipeDocumentV1
- @playwright/test
- catalogue_store.rs
- krausen-types/src/brew_history.rs
- session.rs
- pages/recipe_editor.rs
- BadgeVariant
- catalogue_picker.rs
- KVPool
- token_service.rs
- sync_log_service.rs
- user_service.rs
- layout.rs
- icon
- scripts
- onboarding_service.rs
- Equipment
- oauth_service.rs
- ibu.rs
- PreviewOutcome
- jwt.rs
- super
- brew-start.spec.ts
- getDefaultWorkspaceId
- retry.rs
- OperationIntent
- EquipmentSettingsV1
- BrewPlanV1
- CalcError
- beerxml.rs
- src/webauthn.rs
- brewery.rs
- account_recovery_complete.rs
- pages/brew_start.rs
- websocket.rs
- constants.rs
- krausen-types/src/interchange.rs
- WorkspaceRenameOutcome
- test-helpers.ts
- middleware/mod.rs
- leptos_frontend.rs
- signup_flow.rs
- BrewFeedbackApplyOutcome
- StartBrewOutcome
- IngredientOutcome
- with
- 4. Architecture
- server_fns/recipes.rs
- google_oauth.rs
- Recipe
- record.rs
- ImportChoices
- Button
- docs/README.md
- ServerContext
- McpHandler
- utils/webauthn.rs
- Target
- WebSocketManager
- write.rs
- MeasurementOutcome
- ProcessSummaryOutcome
- ScaleApplyOutcome
- pages/equipment_profiles.rs
- draft_validation.rs
- kv_store_memory.rs
- NoteOutcome
- toast.rs
- Shared application architecture
- MessageType
- Field
- ActualFormulationOutcome
- BrewLifecycleOutcome
- models.rs
- krausen-calc/src/formulation.rs
- db.rs
- BrewEvidenceViewOutcome
- ScaleOutcome
- ModalSize
- EquipmentOutcome
- FormulationPreview
- Krausen Coding Standards
- loginUser
- server/src/lib.rs
- AppState
- bootstrap.rs
- migration_orders.rs
- RedisKVStore
- middleware.rs
- Recipe
- native_recipe.rs
- recipe_service.rs
- BrewArchiveOutcome
- ImportCommitOutcome
- EmptyStateVariant
- theme.rs
- server_fns/brew.rs
- Krausen design system
- Recipes, revisions and historical brews
- components.js
- Prioritized findings
- shell-source-check.cjs
- steps
- workspace_naming.rs
- canonical_json.rs
- server_fns/profile.rs
- switch.rs
- signup_complete.rs
- NativeMigration
- read_document
- KVStore
- select.rs
- StatusBadgeVariant
- IngredientKind
- Calculation and interchange contracts
- verify-deployment-app
- browser_refresh.rs
- expiry_comparison.rs
- credential_lifecycle.rs
- Validation
- Targets
- TransferStatus
- krausen-ui/package.json
- recipe_recovery.rs
- App Starter Skeleton — historical plan
- profile-browser.sh
- Krausen
- auth_google_oauth.rs
- manager
- IngredientEntry
- mode
- krausen-ui
- DuplicateOutcome
- auth_token.rs
- RedisPool
- Scalar<T>
- round_trip.rs
- passkey_signup_complete.rs
- AdditionalText
- Krausen recipe and component prototype
- Krausen agent guidance
- TransferOwnershipModal
- brew_schema.rs
- native_transactions.rs
- ExportOutcome
- brew_evidence_rows.rs
- AdditionalKind
- Architecture adoption and implementation contract
- 3. Principles
- DeviceIconKind
- Result
- image
- SessionRow
- recovery_verify_service
- NativeImportError
- Krausen documentation
- deploy-notify
- Krausen release configuration
- PickerView
- auth_refresh.rs
- SummaryRow
- CataloguePicker
- krausen-app/README.md
- kra20_baseline_4fc8313/README.md
- full_table_name_expr_prefixed
- build.sh
- fonts/README.md
- request_id.rs
- ScaleWarning
- SearchInput
- Tooltip
- schema
- CatalogueScope
- NavigationProgress
- GoogleSignInButton
- PasskeySignInButton
- StepNumber
- PasskeyRow
- security-passkeys.spec.ts
- SecurityTab
- PasswordField
- playwright.config.ts

## God Nodes (most connected - your core abstractions)
1. `DbPool` - 331 edges
2. `ActorContext` - 149 edges
3. `RecipeDocumentV1` - 111 edges
4. `DbTx` - 109 edges
5. `OperationIntent` - 105 edges
6. `authorize_workspace()` - 52 edges
7. `KVPool` - 52 edges
8. `@playwright/test` - 48 edges
9. `extract_context()` - 46 edges
10. `E2E Test Plan` - 46 edges

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

## Communities (248 total, 40 thin omitted)

### Community 0 - "src/brew_evidence.rs"
Cohesion: 0.06
Nodes (71): actual(), amend_actual(), archive(), archive_inner(), begin(), canonical_document(), correct_measurement(), correct_note() (+63 more)

### Community 1 - "ActorContext"
Cohesion: 0.05
Nodes (96): begin(), feedback_apply(), feedback_apply_inner(), digest(), get(), identifiers(), lock_workspace(), normalize() (+88 more)

### Community 2 - "extract_context"
Cohesion: 0.17
Nodes (29): add_passkey_flow(), check_webauthn_support(), PasskeyManager(), PasswordManager(), SessionManagement(), TwoFactorAuth(), get_user_context(), UserContext (+21 more)

### Community 3 - "mcp.rs"
Cohesion: 0.03
Nodes (32): authenticate(), BeerXmlExport, BeerXmlImport, BeerXmlOriginal, BeerXmlPreview, BrewActualInput, BrewArchiveInput, BrewComparisonInput (+24 more)

### Community 4 - "tests/brew_history.rs"
Cohesion: 0.08
Nodes (9): actual(), document(), exercise(), postgres_brew_history(), postgres_integrated_acceptance(), sqlite_brew_history(), sqlite_integrated_acceptance(), actor_text() (+1 more)

### Community 5 - "src/components/mod.rs"
Cohesion: 0.07
Nodes (44): BreweryAddition(), BreweryBadge(), BreweryBeerPortrait(), BreweryBrand(), BreweryCompactIngredientRow(), BreweryDialog(), BreweryDisclosure(), BreweryDraftArea() (+36 more)

### Community 6 - "brew_evidence_store.rs"
Cohesion: 0.05
Nodes (62): ACTUAL_BASE_SELECT, actual_projection_retains_boil_duration_and_does_not_mutate_plan(), ACTUAL_SELECT, ActualRow, BrewEvidenceState, BrewStateRow, insert_actual_formulation(), insert_archive_event() (+54 more)

### Community 8 - "printable_recipe.rs"
Cohesion: 0.06
Nodes (36): readiness(), summary_from_plan(), project_step(), ChillingV1, Counterflow, Forced, Natural, IngredientSection (+28 more)

### Community 9 - "auth.rs"
Cohesion: 0.06
Nodes (51): AuthDivider(), CheckEmailView(), CredentialsView(), LOGIN_SUBTITLES, LoginPage(), LoginView, CheckEmail, Credentials (+43 more)

### Community 10 - "ButtonVariant"
Cohesion: 0.06
Nodes (36): Button(), ButtonLink(), ButtonSize, Default, Icon, IconSm, IconXs, Lg (+28 more)

### Community 11 - "DbPool"
Cohesion: 0.06
Nodes (28): backfill_legacy_recipes(), MigrationReport, correct_request(), evidence_view(), fixture(), integrated_acceptance(), intent(), measurement() (+20 more)

### Community 12 - "auth_service.rs"
Cohesion: 0.08
Nodes (41): build_exclude_ids(), ensure_google_oauth_auth_method(), ensure_user_has_workspace(), establish_verified_account(), EstablishVerifiedAccountParams, get_passkeys_for_user(), google_oauth_callback_service(), GoogleOAuthCallbackParams (+33 more)

### Community 13 - "Error"
Cohesion: 0.06
Nodes (29): preview_request(), effective(), engine_source_hash(), policy_source_hash(), POLICY_VERSION, resolve_in_tx(), list_styles(), preview() (+21 more)

### Community 14 - "krausen-app/src/recipe_library.rs"
Cohesion: 0.09
Nodes (38): archive(), ArchiveRow, Cursor, cursor(), decode(), history(), history_inner(), limit() (+30 more)

### Community 15 - "encryption.rs"
Cohesion: 0.09
Nodes (30): decrypt(), decrypt_json(), decrypt_slack_token(), derive_key(), derive_key_correct_length(), derive_key_invalid_base64_rejected(), derive_key_wrong_length_rejected(), each_encryption_produces_different_ciphertext() (+22 more)

### Community 16 - "operations.rs"
Cohesion: 0.06
Nodes (59): build_totp(), DIGITS, generate_qr_code(), generate_secret(), generate_secret_returns_valid_base32(), ISSUER, provisioning_uri(), provisioning_uri_contains_issuer() (+51 more)

### Community 17 - "tests/brew_start.rs"
Cohesion: 0.05
Nodes (42): CONTEXT, DIGEST, FIXTURE, STYLE, TARGETS, VALIDATION, complete_document(), create_profile() (+34 more)

### Community 18 - "E2E Test Plan"
Cohesion: 0.04
Nodes (46): Current acceptance scope, E2E Test Plan, Notes for Test Implementation, TC-001: Signup Flow (Email Verification Path), TC-002: Signup With Invitation, TC-003: Login Flow (Password), TC-004: Login Flow (Passkey), TC-005: Logout (+38 more)

### Community 19 - "presets.rs"
Cohesion: 0.08
Nodes (26): a_later_preset_overrides_an_earlier_one_with_the_same_id(), a_preset_can_be_plugged_in_at_runtime(), a_preset_profile_derives_a_recipe(), adopting_a_preset_records_where_the_profile_came_from(), builtin_presets(), builtins_are_available_and_uniquely_identified(), chilling_choice_survives_a_round_trip_through_a_preset(), EquipmentField (+18 more)

### Community 20 - "brew_history_presentation.rs"
Cohesion: 0.07
Nodes (48): canonical_digest(), change(), comparison_changes(), ingredient_changes(), value_for_fermentable(), value_for_hop(), value_for_other(), BrewChangeKind (+40 more)

### Community 21 - "passkey_recovery_complete.rs"
Cohesion: 0.14
Nodes (18): creating_view(), error_view(), map_webauthn_error(), PageState, Creating, Error, Ready, Success (+10 more)

### Community 22 - "krausen-app/src/brew_history.rs"
Cohesion: 0.13
Nodes (18): apply_changes(), feedback_preview(), feedback_preview_inner(), historical_print(), historical_print_inner(), history(), history_inner(), map_preview() (+10 more)

### Community 23 - "workspace_service.rs"
Cohesion: 0.09
Nodes (39): accept_invitation(), accept_invitation_for_user(), check_existing_member_by_email(), check_pending_invitation(), complete_ownership_transfer(), count_admins(), count_pending_invitations(), count_workspace_users() (+31 more)

### Community 24 - "server_fns/brew_history.rs"
Cohesion: 0.11
Nodes (27): BrewPrintOutcome, ContextChanged, Forbidden, InvalidRequest, NotFound, Print, Unavailable, RecipeBrewHistoryOutcome (+19 more)

### Community 25 - "prelude"
Cohesion: 0.07
Nodes (30): Alert(), AlertDescription(), AlertTitle(), AlertVariant, Default, Error, Info, Success (+22 more)

### Community 26 - "mcp_session_manager.rs"
Cohesion: 0.16
Nodes (27): cleanup(), clone_shares_state(), create_and_validate_session(), invalidate_empty_workspace_is_noop(), invalidate_workspace_clears_all_workspace_sessions(), mcp_apps_support_default_false(), mcp_apps_support_unknown_session_returns_false(), MCPSessionManager (+19 more)

### Community 27 - "app.js"
Cohesion: 0.09
Nodes (35): beerGlass(), brews, catalogue, changeState(), clone(), closeDialog(), commitNavigation(), continuePending() (+27 more)

### Community 28 - "krausen-app/src/scaling.rs"
Cohesion: 0.13
Nodes (30): absent_values_stay_absent(), accepted_digest(), accepted_digest_binds_revision_target_policy_and_context(), apply(), apply_inner(), build_preview(), capacity_and_fixed_losses_are_explicit(), capacity_warning_checks_pre_boil_fill_too() (+22 more)

### Community 29 - "server_fns/team.rs"
Cohesion: 0.19
Nodes (21): PendingAction, CancelInvitation, CancelTransfer, RemoveMember, TeamPageInner(), cancel_invitation(), cancel_ownership_transfer(), generate_invitation_id() (+13 more)

### Community 30 - "oauth.rs"
Cohesion: 0.16
Nodes (23): approve(), auth_failure(), authorization_metadata(), authorize(), client_auth(), continue_authorize(), html_escape(), invalid_client() (+15 more)

### Community 31 - "app.rs"
Cohesion: 0.05
Nodes (17): App(), Shell(), AccountRecoveryPage(), FormView(), RecoveryKind, Account, Passkey, RecoveryRequestCard() (+9 more)

### Community 32 - "recipe_scaling.rs"
Cohesion: 0.11
Nodes (13): document(), exercise(), postgres_scaling(), run(), settings(), sqlite_scaling(), capacity_warning_names_values_and_explicit_geometry_policy(), change_text() (+5 more)

### Community 33 - "convert.rs"
Cohesion: 0.11
Nodes (32): EXAMPLES, main(), a_converted_recipe_derives(), a_hop_use_the_engine_cannot_model_is_reported_not_hidden(), a_missing_value_is_defaulted_and_declared(), a_value_that_is_present_but_unreadable_is_reported_as_such(), an_aroma_addition_becomes_a_whirlpool_addition(), an_imported_equipment_profile_says_what_beerxml_could_not_supply() (+24 more)

### Community 34 - "mibu.rs"
Cohesion: 0.10
Nodes (30): a_flameout_addition_still_gains_bitterness(), a_held_hop_stand_extracts_less_than_a_boiling_one(), a_lidded_kettle_cools_slower_than_an_open_one(), a_longer_stand_extracts_more_when_chilling_would_otherwise_have_started(), a_wider_kettle_cools_faster(), AA_DECAY_PER_MIN, additions_are_independent_of_each_other(), an_addition_already_spent_in_the_boil_gains_little() (+22 more)

### Community 36 - "accept_ownership.rs"
Cohesion: 0.11
Nodes (29): AcceptOwnershipPage(), capability_item(), error_view(), fetch_ownership_transfer(), format_date_script(), icon_alert_circle_large(), icon_alert_circle_sm(), icon_arrow_right_left() (+21 more)

### Community 37 - "IntoView"
Cohesion: 0.19
Nodes (15): AdditionalFields(), AdditionalGroup(), AdditionalIngredients(), AdditionalNumberField(), AdditionalTextField(), NumberField(), PreviewDetails(), PreviewMetrics() (+7 more)

### Community 38 - "email_service.rs"
Cohesion: 0.10
Nodes (11): admin_email_brand_preserves_escaped_sections(), admin_email_template(), assert_email_brand(), email_template(), EmailService, html_escape(), LOGO_BYTES, LOGO_CID (+3 more)

### Community 39 - "DbTx"
Cohesion: 0.13
Nodes (25): load_brew(), head_conflict(), lock_workspace(), recipe_archived(), recipe_head(), start(), start_inner(), backfill() (+17 more)

### Community 40 - "native.rs"
Cohesion: 0.16
Nodes (25): beerxml_dry_hop_time_is_interpreted_as_minutes(), edited_export_uses_current_native_values_and_reports_source_omission(), envelope_fragments_are_only_reused_for_unchanged_single_recipe(), explicit_choices_project_colour_and_efficiency_without_changing_source(), explicit_style_identity_survives_native_projection_and_export(), extension_follows_row_identity_after_reordering(), from_xml(), from_xml_with_choices() (+17 more)

### Community 41 - "tests/process_summary.rs"
Cohesion: 0.10
Nodes (23): base_revision_exists(), load_saved(), project(), read(), read_inner(), SavedRead, Found, Missing (+15 more)

### Community 42 - "Recipe"
Cohesion: 0.14
Nodes (19): darker_malt_darkens_the_beer(), EBC_PER_SRM, ebc_to_lovibond(), malt(), more_water_lightens_the_beer(), morey(), MOREY_COEFFICIENT, MOREY_EXPONENT (+11 more)

### Community 43 - "popover.rs"
Cohesion: 0.10
Nodes (25): Align, End, Start, bottom_end_aligns_right_edges(), bottom_start_fits_below_no_flip(), bottom_start_flips_when_no_room_below(), caps_max_height_when_neither_side_has_room(), compute_position() (+17 more)

### Community 44 - "recipe_crud.rs"
Cohesion: 0.15
Nodes (23): create_recipe(), get_recipe(), list_recipes(), search_recipes(), a_created_recipe_reads_back(), a_deleted_recipe_disappears_from_reads_and_lists(), a_nameless_recipe_still_gets_a_usable_name(), a_recipe_is_invisible_from_another_workspace() (+15 more)

### Community 45 - "recipe_revision_history.rs"
Cohesion: 0.11
Nodes (14): LibraryCommand, Archive, History, List, Recover, Revision, authority_failed(), HistoricalRecipe() (+6 more)

### Community 46 - "RecipeComposition"
Cohesion: 0.07
Nodes (4): main(), EmailBrandSourceTests, RecipeComposition, data_url()

### Community 47 - "mcp_auth.rs"
Cohesion: 0.11
Nodes (10): https_authorization_and_sdk_through_reverse_proxy(), Proxy, brew_test_document(), brew_test_profile(), brew_ui_outcome(), exercise(), postgres_sdk_clients(), run_mcp_exercise() (+2 more)

### Community 48 - "pages/brew_evidence.rs"
Cohesion: 0.10
Nodes (17): BrewEvidencePage(), BrewEvidencePanel(), continuation_owned(), lifecycle_message(), measurement_is_uncertain(), measurement_message(), note_is_uncertain(), note_message() (+9 more)

### Community 49 - "parse.rs"
Cohesion: 0.11
Nodes (22): ParseError, NoRootElement, Xml, accepts_a_bare_recipe_without_the_recipes_wrapper(), an_empty_element_is_recorded_as_empty_not_absent(), cdata_survives(), collect(), collections_keep_their_order() (+14 more)

### Community 50 - "ingredient_catalogue.rs"
Cohesion: 0.16
Nodes (13): doc(), exercise(), postgres_ingredient_catalogue(), recipe_with(), save_request(), search(), sqlite_ingredient_catalogue(), FermentableSpecification (+5 more)

### Community 51 - "krausen-calc/src/lib.rs"
Cohesion: 0.12
Nodes (20): Corrections, Derived, FermentablePoints, IbuContribution, Volumes, abv(), compute(), corrections() (+12 more)

### Community 52 - "server_fns/scaling.rs"
Cohesion: 0.25
Nodes (10): DuplicateDialog(), duplicate_recipe_shared(), execute_apply(), execute_duplicate(), execute_duplicate_recipe(), execute_preview(), execute_scale_apply(), execute_scale_preview() (+2 more)

### Community 53 - "foundation"
Cohesion: 0.06
Nodes (31): concurrency, cancel-in-progress, group, KRAUSEN_TEST_POSTGRES_URL, POSTGRES_DB, POSTGRES_PASSWORD, POSTGRES_USER, env (+23 more)

### Community 54 - "manager.rs"
Cohesion: 0.13
Nodes (8): Inner, MAX_CONNECTIONS_PER_USER, NEXT_CONNECTION_ID, REDIS_CHANNEL_PREFIX, sqlite_memory_pool(), TrackedConnection, WS_CHANNEL_CAPACITY, WsSender

### Community 55 - "reference_recipe.rs"
Cohesion: 0.22
Nodes (30): derive(), ABV_EPS, assert_close(), boil_time_moves_everything_downstream_of_volume(), chilling_sooner_means_less_bitterness(), colour_uses_post_boil_volume_not_batch_volume(), CORRECTION_EPS, EBC_EPS (+22 more)

### Community 56 - "Preview"
Cohesion: 0.14
Nodes (16): compare(), identities(), identities_resolve_and_bounds_are_inclusive_at_full_precision(), resolve(), StyleReference, CalculationContext, checked_request_json(), Preview (+8 more)

### Community 57 - "RecipeDocumentV1"
Cohesion: 0.15
Nodes (7): RecipeDocumentV1, DraftContents, editing_clears_stale_server_diagnostics_and_preview(), raw_numbers_preserve_empty_invalid_and_real_zero(), RecipeDraft, replacing_clean_saved_document_invalidates_existing_preview_generation(), undo_and_recovery_keep_unfinished_numbers()

### Community 58 - "@playwright/test"
Cohesion: 0.20
Nodes (13): cleanTestData(), createSecondUser(), db, generateEmail(), gotoAuthenticated(), isSelfHostedMode(), waitForWasm(), walCheckpoint() (+5 more)

### Community 59 - "catalogue_store.rs"
Cohesion: 0.20
Nodes (13): append(), archive(), create(), duplicate_key_exists(), encode(), entry_id_in_use(), like_pattern(), execute! (+5 more)

### Community 60 - "krausen-types/src/brew_history.rs"
Cohesion: 0.08
Nodes (29): comparison(), comparison_inner(), measurement_comparison(), BrewComparison, BrewComparisonOutcome, Comparison, ContextChanged, Forbidden (+21 more)

### Community 61 - "session.rs"
Cohesion: 0.07
Nodes (26): GoogleOAuthServiceResult, PendingTerms, RateLimited, Success, LoginServiceResult, InvalidCredentials, RateLimited, Success (+18 more)

### Community 62 - "pages/recipe_editor.rs"
Cohesion: 0.11
Nodes (20): RecipeReadOutcome, ContextChanged, Forbidden, Found, NotFound, Unavailable, RecipeSnapshot, BeerXmlExport() (+12 more)

### Community 63 - "BadgeVariant"
Cohesion: 0.20
Nodes (9): Badge(), BadgeVariant, Default, Destructive, Outline, Secondary, Warning, BASE (+1 more)

### Community 64 - "catalogue_picker.rs"
Cohesion: 0.15
Nodes (10): archived_badge_and_terminal_notices_are_distinct(), button_is_disabled(), button_open_tag(), CATEGORIES, composed_picker_renders_real_management_actions_by_scope_and_archive_state(), picker_details_format_specs_and_selection_copy(), render_selection(), starter_entry() (+2 more)

### Community 65 - "KVPool"
Cohesion: 0.20
Nodes (21): delete_recovery_session(), delete_webauthn_challenge(), get_pending_signup(), get_pending_terms(), get_pending_totp(), get_recovery_session(), get_webauthn_challenge(), peek_recovery_session() (+13 more)

### Community 66 - "token_service.rs"
Cohesion: 0.13
Nodes (21): get_sessions(), create_verification_token(), create_verification_token_with_expiry(), get_user_sessions(), hash_refresh_token(), RefreshTokenUserData, RefreshTokenVerifyResult, GracePeriod (+13 more)

### Community 67 - "sync_log_service.rs"
Cohesion: 0.11
Nodes (16): action_type_to_str(), get_entries_since(), get_latest_sync_id(), is_sync_id_available(), normalise_timestamp(), parse_action_type(), prune_old_entries(), SyncLogRow (+8 more)

### Community 68 - "user_service.rs"
Cohesion: 0.08
Nodes (36): add_passkey_to_user(), admin_create_user(), admin_update_user(), create_api_token(), create_user(), create_workspace_for_user(), default_workspace_name(), delete_passkey_from_user() (+28 more)

### Community 69 - "layout.rs"
Cohesion: 0.13
Nodes (14): CONTEXT_CHANGED_KEY, EditorDestination, Href, Logout, Workspace, EditorGuard, Layout(), Sidebar() (+6 more)

### Community 70 - "icon"
Cohesion: 0.22
Nodes (27): Component families and production mapping, brewsPage(), compactSummary(), composition(), editor(), fmt(), historyPage(), ingredientPanel() (+19 more)

### Community 71 - "scripts"
Cohesion: 0.08
Nodes (25): dependencies, argon2@^0.44.0, devDependencies, otplib@^12.0.1, @playwright/test@^1.49.0, name, private, scripts (+17 more)

### Community 72 - "onboarding_service.rs"
Cohesion: 0.10
Nodes (14): main(), serve(), notify_signup(), send_signup_slack(), accept_terms(), AcceptTermsOutcome, InvalidToken, Success (+6 more)

### Community 73 - "Equipment"
Cohesion: 0.12
Nodes (22): equipment(), main(), recipe(), main(), Equipment, hop(), Hop, HopUse (+14 more)

### Community 74 - "oauth_service.rs"
Cohesion: 0.16
Nodes (23): http_client(), approve(), authenticate_client(), begin_authorization(), bind_pending(), Client, Code, Credential (+15 more)

### Community 75 - "ibu.rs"
Cohesion: 0.19
Nodes (17): a_whirlpool_with_no_stand_still_isomerises_while_the_wort_chills(), AA_DECAY_PER_MIN, BIGNESS_BASE, BIGNESS_COEFFICIENT, bigness_factor(), bitterness_scales_linearly_with_hop_weight(), contributions(), dry_hops_contribute_no_bitterness_and_no_row() (+9 more)

### Community 76 - "PreviewOutcome"
Cohesion: 0.12
Nodes (20): project_preview(), PreviewOutcome, ContextChanged, Forbidden, Invalid, Preview, Unavailable, UnresolvedProfile (+12 more)

### Community 77 - "jwt.rs"
Cohesion: 0.15
Nodes (18): BROWSER_AUDIENCE, BROWSER_ISSUER, Claims, create_access_token(), create_access_token_str(), create_refresh_token(), decode_python_shaped_jwt_with_extra_fields(), empty_token_rejected() (+10 more)

### Community 78 - "super"
Cohesion: 0.10
Nodes (11): Equipment, WithChilling, Chilling, Counterflow, Forced, None, chilling_rates_fall_as_volume_rises(), immersion_and_ice_bath_differ_in_the_expected_direction() (+3 more)

### Community 79 - "brew-start.spec.ts"
Cohesion: 0.10
Nodes (11): { chromium }, path, SCREENSHOT_DIR, AUTH_STATE_PATH, getKrausenMode(), globalSetup(), sqliteExec(), importRecipe() (+3 more)

### Community 80 - "getDefaultWorkspaceId"
Cohesion: 0.19
Nodes (19): allowSignup(), futureISO(), generateInvitationId(), generateTransferId(), generateWorkspaceId(), getDefaultUserId(), getDefaultWorkspaceId(), nowISO() (+11 more)

### Community 81 - "retry.rs"
Cohesion: 0.11
Nodes (11): backoff_delay(), INITIAL_INTERVAL_SECS, MAX_INTERVAL, MAX_RETRIES, max_retries_exhausted_returns_last_error(), MULTIPLIER, non_retryable_error_fails_immediately(), retry_with_backoff() (+3 more)

### Community 82 - "OperationIntent"
Cohesion: 0.07
Nodes (58): amend_document(), complete_document(), correct_request(), equipment_request(), exercise(), exercise_acceptance_cases(), fixture(), intent() (+50 more)

### Community 83 - "EquipmentSettingsV1"
Cohesion: 0.12
Nodes (26): equipment_request(), append(), archive(), create(), encode(), list(), load(), load_revision() (+18 more)

### Community 84 - "BrewPlanV1"
Cohesion: 0.12
Nodes (24): BREW_PLAN_SCHEMA_VERSION, BrewRow, BrewSummaryRow, encode_plan(), insert(), list_for_recipe(), load(), read_plan() (+16 more)

### Community 85 - "CalcError"
Cohesion: 0.16
Nodes (20): CalcError, AttenuationOutOfRange, BoilOffExceedsPreBoil, EfficiencyOutOfRange, EmptyGrist, InvalidInput, MashWaterExceedsTotal, NegativeBoilOffRate (+12 more)

### Community 86 - "beerxml.rs"
Cohesion: 0.14
Nodes (4): exercise(), postgres_interchange(), sqlite_interchange(), BeerXmlPage()

### Community 87 - "src/webauthn.rs"
Cohesion: 0.16
Nodes (7): build_webauthn(), finish_authentication(), finish_discoverable_authentication(), finish_registration(), start_authentication(), start_discoverable_authentication(), start_registration()

### Community 88 - "brewery.rs"
Cohesion: 0.14
Nodes (17): value_for_step(), actual_from_plan(), actual_from_recipe(), ActualFormulationV1, AdditionStage, FermentableAddition, HopAddition, HopSpecification (+9 more)

### Community 89 - "account_recovery_complete.rs"
Cohesion: 0.13
Nodes (18): AccountRecoveryCompletePage(), error_view(), PageState, Error, Ready, Submitting, Success, Verifying (+10 more)

### Community 90 - "pages/brew_start.rs"
Cohesion: 0.17
Nodes (14): StartBrewRequest, build_start_request(), HowToStart(), invalidate_selection(), may_prepare(), metric(), prepared_current_selection_keeps_original_head_guard(), prepared_head_guard() (+6 more)

### Community 91 - "websocket.rs"
Cohesion: 0.14
Nodes (10): CLOSE_AUTH_REQUIRED, CLOSE_FORBIDDEN, CLOSE_TOO_MANY_CONNECTIONS, close_with_code(), handle_authenticated_ws(), MAX_MESSAGE_SIZE, PING_INTERVAL_SECS, valid_credential() (+2 more)

### Community 92 - "constants.rs"
Cohesion: 0.16
Nodes (13): Constants, CONSTANTS, CookieConstants, get(), JwtConstants, load_with_fallback(), RateLimitBucket, RateLimitConstants (+5 more)

### Community 93 - "krausen-types/src/interchange.rs"
Cohesion: 0.10
Nodes (25): ExportSelection, Draft, Saved, ImportPreview, ImportPreviewOutcome, ContextChanged, Forbidden, InvalidSource (+17 more)

### Community 94 - "WorkspaceRenameOutcome"
Cohesion: 0.13
Nodes (15): WorkspaceRenameOutcome, Conflict, Forbidden, Invalid, RequestIdentityConflict, ResultExpired, Retryable, Saved (+7 more)

### Community 95 - "test-helpers.ts"
Cohesion: 0.13
Nodes (18): apiRequest(), AuthConfig, ensureLoggedIn(), fetchAuthConfig(), generateUserId(), getCookies(), getKrausenMode(), hasCookie() (+10 more)

### Community 96 - "middleware/mod.rs"
Cohesion: 0.15
Nodes (7): cors_layer(), security_headers(), clear_recovery_session_cookie(), clear_token_cookies(), get_cookie_value(), set_recovery_session_cookie(), set_token_cookies()

### Community 97 - "leptos_frontend.rs"
Cohesion: 0.15
Nodes (9): ANYTHING, ASSET, BROWSER, dist_dir(), serve(), serve_asset_or_shell(), serve_leptos_shell(), static_files_service() (+1 more)

### Community 98 - "signup_flow.rs"
Cohesion: 0.27
Nodes (15): signup_complete_service(), a_known_address_is_answered_the_same_as_an_unknown_one(), a_second_link_does_not_make_a_second_account(), an_unaccepted_token_leaves_nothing_behind(), count_users(), device(), emailed_token(), JWT_SECRET (+7 more)

### Community 99 - "BrewFeedbackApplyOutcome"
Cohesion: 0.10
Nodes (20): BrewFeedbackApplyOutcome, ActualConflict, Applied, ContextChanged, ContextConflict, Forbidden, HeadConflict, Invalid (+12 more)

### Community 100 - "StartBrewOutcome"
Cohesion: 0.10
Nodes (18): StartBrewOutcome, ContextChanged, ContextConflict, EquipmentProfileArchived, Forbidden, HeadConflict, InvalidRequest, NotFound (+10 more)

### Community 101 - "IngredientOutcome"
Cohesion: 0.11
Nodes (21): IngredientCommand, Archive, Get, Save, Search, IngredientOutcome, Archived, Conflict (+13 more)

### Community 102 - "with"
Cohesion: 0.10
Nodes (20): with, cache-from, cache-to, client-id, context, file, owner, password (+12 more)

### Community 103 - "4. Architecture"
Cohesion: 0.10
Nodes (20): 1. The thesis, 2. What Krausen is not, 4.1 The calculation core is a pure function, 4.2 Evaluation order is a DAG, and it matters, 4.3 Metric-native extract, 4.4 Colour uses post-boil volume, 4.5 Whirlpool is an explicit recipe field, 4.6 Brews snapshot recipes, they do not reference them (+12 more)

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

### Community 108 - "ImportChoices"
Cohesion: 0.29
Nodes (7): ColourUnit, Lovibond, Srm, EfficiencyChoice, RecipeOverride, SelectedEquipment, ImportChoices

### Community 109 - "Button"
Cohesion: 0.27
Nodes (19): actualsDialog(), allFermentables(), catalogueResults(), createFermentableDialog(), equipmentDialog(), inspectRevision(), openAdd(), openFermentablePicker() (+11 more)

### Community 111 - "ServerContext"
Cohesion: 0.13
Nodes (7): AuthenticatedContext, AuthFlowContext, IntoServerFnError, Result<T, E>, ServerContext, verified_actor(), workspace_id()

### Community 113 - "utils/webauthn.rs"
Cohesion: 0.24
Nodes (9): array_buffer_to_base64url(), base64url_to_array_buffer(), convert_credential_descriptors(), is_webauthn_available(), js_err(), js_get(), js_set(), start_authentication() (+1 more)

### Community 114 - "Target"
Cohesion: 0.08
Nodes (25): ALL, EXTRACT, IBU, Rule, Addition, Finite, Fraction, Nonnegative (+17 more)

### Community 115 - "WebSocketManager"
Cohesion: 0.20
Nodes (11): send_error(), send_member_joined(), send_member_role_changed(), send_ownership_transfer_completed(), send_ownership_transfer_declined(), send_ownership_transfer_offered(), send_sync_action(), send_workspace_invitation() (+3 more)

### Community 116 - "write.rs"
Cohesion: 0.27
Nodes (14): empty_collections_are_still_written(), indent(), INDENT, leaf(), output_is_well_formed_enough_to_read_back(), text_needing_escapes_round_trips(), write(), write_collection() (+6 more)

### Community 117 - "MeasurementOutcome"
Cohesion: 0.11
Nodes (18): MeasurementOutcome, Archived, Conflict, ContextChanged, Corrected, EntryConflict, Forbidden, Invalid (+10 more)

### Community 118 - "ProcessSummaryOutcome"
Cohesion: 0.14
Nodes (14): ProcessSummaryOutcome, ContextChanged, Forbidden, Invalid, InvalidRequest, NotFound, Summary, Unavailable (+6 more)

### Community 119 - "ScaleApplyOutcome"
Cohesion: 0.11
Nodes (18): ScaleApplyOutcome, Applied, Archived, Conflict, ContextChanged, ContextConflict, Forbidden, Invalid (+10 more)

### Community 120 - "pages/equipment_profiles.rs"
Cohesion: 0.13
Nodes (9): draft(), exercise(), postgres_profiles(), settings(), sqlite_profiles(), assign_values(), EquipmentProfiles(), FIELDS (+1 more)

### Community 121 - "draft_validation.rs"
Cohesion: 0.26
Nodes (12): validate_draft(), attenuation_is_explicit_and_never_inferred_from_yeast(), colour_and_yield_have_distinct_target_dependencies(), complete(), extract_only_needs_no_mash_inputs(), missing_hop_input_does_not_hide_water_and_no_hops_is_valid(), nonfinite_and_conflicting_schedule_are_invalid(), profile_thickness_and_top_up_must_fit_water_budget() (+4 more)

### Community 122 - "kv_store_memory.rs"
Cohesion: 0.13
Nodes (12): del_removes_key(), Entry, expire_sets_ttl(), getdel_missing_returns_none(), getdel_returns_and_removes(), incr_creates_and_increments(), InMemoryKVStore, missing_key_returns_none() (+4 more)

### Community 123 - "NoteOutcome"
Cohesion: 0.12
Nodes (17): NoteOutcome, Archived, Conflict, ContextChanged, Corrected, Forbidden, History, Invalid (+9 more)

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
Cohesion: 0.15
Nodes (6): bool, f64, Field, leading_number(), String, u32

### Community 128 - "ActualFormulationOutcome"
Cohesion: 0.12
Nodes (16): ActualFormulationOutcome, Amended, Archived, Conflict, ContextChanged, Forbidden, History, Invalid (+8 more)

### Community 129 - "BrewLifecycleOutcome"
Cohesion: 0.13
Nodes (15): BrewLifecycleOutcome, Archived, Conflict, ContextChanged, Forbidden, Invalid, InvalidTransition, NotFound (+7 more)

### Community 130 - "models.rs"
Cohesion: 0.31
Nodes (10): get_user_workspace_context(), get_workspace(), get_workspace_user(), get_user_workspaces(), get_workspace_users(), ApiToken, UserAuthMethod, VerificationToken (+2 more)

### Community 131 - "krausen-calc/src/formulation.rs"
Cohesion: 0.17
Nodes (20): calculate(), colour_ebc(), ENGINE_BUILD, ENGINE_SOURCE, ENGINE_VERSION, FermentableInput, FermentableStage, Boil (+12 more)

### Community 132 - "db.rs"
Cohesion: 0.10
Nodes (11): DbQueryResult, db_execute!, db_fetch_all!, db_fetch_one!, db_fetch_optional!, db_fetch_scalar!, db_with_pool!, dbtx_execute! (+3 more)

### Community 133 - "BrewEvidenceViewOutcome"
Cohesion: 0.13
Nodes (15): BrewEvidenceOutcome, Actual, Archive, Lifecycle, Measurement, Note, View, BrewEvidenceViewOutcome (+7 more)

### Community 134 - "ScaleOutcome"
Cohesion: 0.13
Nodes (15): ScaleOutcome, Archived, ContextChanged, Forbidden, Invalid, NotFound, Preview, Unavailable (+7 more)

### Community 135 - "ModalSize"
Cohesion: 0.15
Nodes (7): Modal(), ModalSize, Full, Lg, Md, Sm, Xl

### Community 136 - "EquipmentOutcome"
Cohesion: 0.13
Nodes (16): EquipmentOutcome, Archived, Conflict, ContextChanged, Forbidden, Invalid, NotFound, Profile (+8 more)

### Community 137 - "FormulationPreview"
Cohesion: 0.22
Nodes (3): FormulationPreview(), old_or_different_context_results_cannot_replace_current_preview(), PreviewGate

### Community 138 - "Krausen Coding Standards"
Cohesion: 0.13
Nodes (15): Acceptance, Canonical models and projection boundaries, Change delivery and client state, Component Patterns, Database Queries, Design System Adherence, Dynamic Query Builders, Error Handling (+7 more)

### Community 140 - "server/src/lib.rs"
Cohesion: 0.18
Nodes (4): bootstrap_owner(), build_router(), claim_legacy_personal_owner(), wrap_service()

### Community 141 - "AppState"
Cohesion: 0.19
Nodes (4): routes(), well_known_routes(), AppState, AuthState

### Community 142 - "bootstrap.rs"
Cohesion: 0.16
Nodes (8): exercise(), postgres_operator_bootstrap(), sqlite_operator_bootstrap(), SignupStartServiceResult, AccountCreated, Error, RateLimited, VerificationRequired

### Community 143 - "migration_orders.rs"
Cohesion: 0.20
Nodes (11): BREW_EVIDENCE, BREWS, CATALOGUE, CREDENTIALS, EQUIPMENT, exercise(), NATIVE, postgres_empty_and_both_upgrade_orders() (+3 more)

### Community 145 - "middleware.rs"
Cohesion: 0.21
Nodes (5): AuthState, AuthUser, CredentialIdRow, extract_token(), WorkspaceContext

### Community 146 - "Recipe"
Cohesion: 0.27
Nodes (7): Recipe, RecipeRow, RecipeSource, BeerXml, Krausen, RecipeSummary, SummaryRow

### Community 147 - "native_recipe.rs"
Cohesion: 0.22
Nodes (11): BackfillError, Encode, Legacy, encode_document(), EncodeError, Canonical, UnsupportedVersion, prepare_backfill() (+3 more)

### Community 148 - "recipe_service.rs"
Cohesion: 0.25
Nodes (11): create_recipe(), delete_recipe(), get_legacy_recovery(), index(), Indexed, LegacyRecovery, NativeHead, new_recipe_id() (+3 more)

### Community 149 - "BrewArchiveOutcome"
Cohesion: 0.14
Nodes (14): BrewArchiveOutcome, Archived, Conflict, ContextChanged, Forbidden, Invalid, NotFound, RequestIdentityConflict (+6 more)

### Community 150 - "ImportCommitOutcome"
Cohesion: 0.14
Nodes (14): ImportCommitOutcome, ChoicesRequired, ContextChanged, Forbidden, Imported, Invalid, InvalidRequest, InvalidSource (+6 more)

### Community 151 - "EmptyStateVariant"
Cohesion: 0.16
Nodes (8): EmptyState(), EmptyStateVariant, Default, Error, Info, Sidebar, Success, Warning

### Community 152 - "theme.rs"
Cohesion: 0.21
Nodes (9): apply_to_document(), read_local_storage(), resolve_theme(), save_theme_to_local_storage(), STORAGE_KEY, ThemeProvider(), ThemeState, use_theme() (+1 more)

### Community 153 - "server_fns/brew.rs"
Cohesion: 0.29
Nodes (14): brew_actual(), brew_actual_shared(), brew_archive(), brew_archive_shared(), brew_evidence_view(), brew_evidence_view_shared(), brew_lifecycle(), brew_lifecycle_shared() (+6 more)

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

### Community 160 - "workspace_naming.rs"
Cohesion: 0.32
Nodes (4): a_workspace_is_named_after_its_owner(), a_workspace_is_named_even_without_a_display_name(), name_of(), test_db()

### Community 161 - "canonical_json.rs"
Cohesion: 0.23
Nodes (8): bytes(), CanonicalError, IntegerOutOfRange, Json, NonFinite, Serialize, from_serializable(), sha256()

### Community 162 - "server_fns/profile.rs"
Cohesion: 0.21
Nodes (15): AppearanceCard(), ClearLocalDataCard(), CopyButton(), InvitationsCard(), McpConnectionCard(), ProfileInfoCard(), ProfilePage(), accept_invitation() (+7 more)

### Community 163 - "switch.rs"
Cohesion: 0.15
Nodes (9): LABEL_DEFAULT_CLASS, LABEL_WRAPPER_CLASS, Switch(), THUMB_BASE, THUMB_CHECKED, THUMB_UNCHECKED, TRACK_BASE, TRACK_CHECKED (+1 more)

### Community 164 - "signup_complete.rs"
Cohesion: 0.15
Nodes (11): Label(), LABEL_CLASS, creating_view(), error_view(), PageState, Creating, Error, Form (+3 more)

### Community 165 - "NativeMigration"
Cohesion: 0.24
Nodes (13): exact_number(), from_legacy(), NativeMigration, note_unmapped_number(), OriginalSource, preserved_extensions(), preserved_fields(), project_xml() (+5 more)

### Community 166 - "read_document"
Cohesion: 0.24
Nodes (12): read_document(), ReadError, InvalidDocument, InvalidJson, MissingVersion, UnsupportedEquipmentVersion, UnsupportedVersion, load_document() (+4 more)

### Community 168 - "select.rs"
Cohesion: 0.18
Nodes (9): CHECK_CLASS, CHEVRON_CLASS, CHEVRON_STYLE, CONTENT_CLASS, DynSelect(), ITEM_CLASS, SELECT_CLASS, StyledSelect() (+1 more)

### Community 169 - "StatusBadgeVariant"
Cohesion: 0.20
Nodes (9): BASE, StatusBadge(), StatusBadgeVariant, Default, Error, Info, Success, Warning (+1 more)

### Community 170 - "IngredientKind"
Cohesion: 0.15
Nodes (19): ingredient_number(), ingredient_text(), IngredientFields(), IngredientKind, Fermentable, Hop, IngredientNumber, Amount (+11 more)

### Community 171 - "Calculation and interchange contracts"
Cohesion: 0.17
Nodes (12): BeerXML preservation and export, Bitterness, timing and chilling, Calculation and interchange contracts, Calculation context, Calculation policy V1, Fermentables and efficiency, Gravity, attenuation and colour, Ownership and shared execution (+4 more)

### Community 172 - "verify-deployment-app"
Cohesion: 0.17
Nodes (11): jobs, verify-deployment-app, name, permissions, attestations, contents, id-token, packages (+3 more)

### Community 174 - "expiry_comparison.rs"
Cohesion: 0.29
Nodes (7): an_expiry_an_hour_ago_is_not_in_the_future(), an_expiry_later_today_is_in_the_future(), evaluate(), expiry_on_other_days_still_works(), SHAPES, stamp(), the_naive_comparison_is_why_this_helper_exists()

### Community 175 - "credential_lifecycle.rs"
Cohesion: 0.25
Nodes (6): refresh_tokens(), RefreshedTokens, exercise(), exercise_oauth(), postgres_browser_credential_lifecycle(), sqlite_browser_credential_lifecycle()

### Community 176 - "Validation"
Cohesion: 0.13
Nodes (7): ResolvedSummary, merge_defaults(), record_default(), targets(), targets_with_defaults(), AppliedDefault, Validation

### Community 177 - "Targets"
Cohesion: 0.19
Nodes (13): Targets, AcceptedScaleContext, SCALE_POLICY_LINEAR_INGREDIENTS_V1, ScaleChange, ScalePreview, ScaleSection, Fermentable, Hop (+5 more)

### Community 178 - "TransferStatus"
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

### Community 184 - "auth_google_oauth.rs"
Cohesion: 0.31
Nodes (3): google_login(), GoogleLoginQuery, routes()

### Community 185 - "manager"
Cohesion: 0.25
Nodes (7): connect_delivers_an_immediate_heartbeat(), disconnect_removes_only_the_named_connection(), every_connection_for_a_user_receives_the_message(), manager(), messages_are_not_delivered_to_other_users(), per_user_connection_limit_is_enforced(), send_to_user_reaches_that_users_connection()

### Community 186 - "IngredientEntry"
Cohesion: 0.23
Nodes (8): fetch_one(), load_entry(), load_starter(), resolve_source(), Row, IngredientEntry, entry_details(), entry_source_label()

### Community 187 - "mode"
Cohesion: 0.20
Nodes (10): mode, default, description, options, type, on, push, workflow_dispatch (+2 more)

### Community 188 - "krausen-ui"
Cohesion: 0.58
Nodes (10): krausen-app, krausen-auth, krausen-beerxml, krausen-brewery, krausen-calc, krausen-core, krausen-design-preview, krausen-server (+2 more)

### Community 189 - "DuplicateOutcome"
Cohesion: 0.17
Nodes (12): DuplicateOutcome, Archived, Conflict, ContextChanged, Duplicated, Forbidden, InvalidRequest, NotFound (+4 more)

### Community 190 - "auth_token.rs"
Cohesion: 0.22
Nodes (7): refresh_token(), routes(), BucketConfig, check_bucket(), check_rate_limit(), RateLimitResult, UNLIMITED_REMAINING

### Community 191 - "RedisPool"
Cohesion: 0.48
Nodes (4): create_pool(), ping(), RedisPool, test_redis_connects()

### Community 194 - "passkey_signup_complete.rs"
Cohesion: 0.08
Nodes (21): Checkbox(), CHECKBOX_ACTIVE, CHECKBOX_BASE, CHECKBOX_UNCHECKED, creating_view(), error_view(), map_webauthn_error(), PageState (+13 more)

### Community 195 - "AdditionalText"
Cohesion: 0.50
Nodes (4): AdditionalText, Name, Producer, Unit

### Community 196 - "Krausen recipe and component prototype"
Cohesion: 0.25
Nodes (8): Browser verification, Build and run, File responsibilities, Interaction checks, Krausen recipe and component prototype, Review checklist, Style reference data, What is real and what is demonstrated

### Community 197 - "Krausen agent guidance"
Cohesion: 0.29
Nodes (7): Boundary checks for implementation and review, Checkout and coordination, Delivery coordination, Krausen agent guidance, Required reading, Scope and design status, Test cadence

### Community 198 - "TransferOwnershipModal"
Cohesion: 0.29
Nodes (5): format_date(), InvitationRow(), MemberRow(), TransferOwnershipModal(), TransferRow()

### Community 199 - "brew_schema.rs"
Cohesion: 0.48
Nodes (4): exercise_sqlite(), exercise_sqlite_constraints(), seed_sqlite(), sqlite_brew_plan_is_immutable()

### Community 200 - "native_transactions.rs"
Cohesion: 0.38
Nodes (3): exercise(), postgres_native_transactions(), sqlite_native_transactions()

### Community 201 - "ExportOutcome"
Cohesion: 0.25
Nodes (8): ExportOutcome, ContextChanged, Exported, Forbidden, Invalid, LossAcknowledgementRequired, NotFound, Unavailable

### Community 203 - "AdditionalKind"
Cohesion: 0.33
Nodes (3): AdditionalKind, Other, Yeast

### Community 204 - "Architecture adoption and implementation contract"
Cohesion: 0.29
Nodes (7): Acceptance scenarios, Architecture adoption and implementation contract, Changed-contract review and verification, Deliberate deferrals, Delivery order and acceptance ownership, Resolution of independent review findings, Scope and authority

### Community 205 - "3. Principles"
Cohesion: 0.29
Nodes (7): 3.1 Hide questions, not capabilities, 3.2 The software has a point of view, 3.3 Learn the environment, never silently change it, 3.4 Flag runs, not misses, 3.5 Say "I don't know yet", 3.6 AI never produces a number, 3. Principles

### Community 206 - "DeviceIconKind"
Cohesion: 0.21
Nodes (8): detect_device_icon(), DeviceIconKind, Key, Monitor, Smartphone, View, Setup, Status

### Community 208 - "image"
Cohesion: 0.33
Nodes (6): if, outputs, runs-on, timeout-minutes, image, digest

### Community 209 - "SessionRow"
Cohesion: 0.32
Nodes (5): format_date(), parse_user_agent(), session_display_name(), SessionRow(), SessionEntry

### Community 210 - "recovery_verify_service"
Cohesion: 0.40
Nodes (5): recovery_verify_service(), RecoveryVerifyServiceResult, AccountNotVerified, InvalidToken, Success

### Community 211 - "NativeImportError"
Cohesion: 0.29
Nodes (7): from_xml_at(), NativeImportError, InvalidCharacters, Json, Parse, UnsupportedEncoding, selected_multi_recipe_import_preserves_full_original_and_selected_path()

### Community 212 - "Krausen documentation"
Cohesion: 0.40
Nodes (5): Design adoption, Historical context, Independent reviews, Krausen documentation, Read first

### Community 213 - "deploy-notify"
Cohesion: 0.40
Nodes (5): if, needs, runs-on, timeout-minutes, deploy-notify

### Community 214 - "Krausen release configuration"
Cohesion: 0.50
Nodes (3): Krausen release configuration, Release, Versioning

### Community 215 - "PickerView"
Cohesion: 0.50
Nodes (4): PickerView, Catalogue, Create, Edit

### Community 219 - "CataloguePicker"
Cohesion: 0.38
Nodes (3): amount_kg(), CataloguePicker(), CataloguePickerSelection()

### Community 234 - "ScaleWarning"
Cohesion: 0.33
Nodes (6): ScaleWarning, CapacityUnknown, DerivedMashWater, FixedLossesRetained, KettleCapacityExceeded, UnscaledIngredients

### Community 238 - "CatalogueScope"
Cohesion: 0.50
Nodes (3): CatalogueScope, Starter, Workspace

### Community 242 - "StepNumber"
Cohesion: 0.50
Nodes (4): StepNumber, Temperature, Time, Volume

## Knowledge Gaps
- **709 isolated node(s):** `Forbidden`, `ContextChanged`, `EquipmentArchived`, `IngredientArchived`, `BrewArchived` (+704 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 1906 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **40 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `DbPool` connect `DbPool` to `src/brew_evidence.rs`, `ActorContext`, `models.rs`, `tests/brew_history.rs`, `db.rs`, `server/src/lib.rs`, `Error`, `bootstrap.rs`, `migration_orders.rs`, `krausen-app/src/recipe_library.rs`, `tests/brew_start.rs`, `auth_service.rs`, `recipe_service.rs`, `krausen-app/src/brew_history.rs`, `workspace_service.rs`, `krausen-app/src/scaling.rs`, `recipe_scaling.rs`, `workspace_naming.rs`, `DbTx`, `tests/process_summary.rs`, `recipe_crud.rs`, `browser_refresh.rs`, `mcp_auth.rs`, `credential_lifecycle.rs`, `ingredient_catalogue.rs`, `manager.rs`, `manager`, `krausen-types/src/brew_history.rs`, `session.rs`, `KVPool`, `token_service.rs`, `sync_log_service.rs`, `user_service.rs`, `onboarding_service.rs`, `native_transactions.rs`, `oauth_service.rs`, `OperationIntent`, `recovery_verify_service`, `beerxml.rs`, `signup_flow.rs`, `server_fns/recipes.rs`, `pages/equipment_profiles.rs`?**
  _High betweenness centrality (0.188) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `RecipeDocumentV1` (e.g. with `BrewFeedbackPreview` and `ExportSelection`) actually correct?**
  _`RecipeDocumentV1` has 3 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Forbidden`, `ContextChanged`, `EquipmentArchived` to the rest of the system?**
  _709 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `src/brew_evidence.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.06267166042446941 - nodes in this community are weakly interconnected._
- **Why does `RecipeDocumentV1` connect `RecipeDocumentV1` to `ActorContext`, `tests/brew_history.rs`, `brew_evidence_store.rs`, `printable_recipe.rs`, `DbPool`, `Error`, `krausen-app/src/recipe_library.rs`, `tests/brew_start.rs`, `native_recipe.rs`, `krausen-app/src/brew_history.rs`, `krausen-app/src/scaling.rs`, `recipe_scaling.rs`, `NativeMigration`, `read_document`, `DbTx`, `native.rs`, `tests/process_summary.rs`, `IngredientKind`, `IntoView`, `recipe_revision_history.rs`, `Validation`, `Targets`, `recipe_recovery.rs`, `Preview`, `krausen-types/src/brew_history.rs`, `pages/recipe_editor.rs`, `AdditionalKind`, `super`, `OperationIntent`, `EquipmentSettingsV1`, `BrewPlanV1`, `brewery.rs`, `krausen-types/src/interchange.rs`, `Target`, `draft_validation.rs`?**
  _High betweenness centrality (0.052) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `OperationIntent` (e.g. with `PreviewRequest` and `ExportRequest`) actually correct?**
  _`OperationIntent` has 3 INFERRED edges - model-reasoned connections that need verification._
- **Should `ActorContext` be split into smaller, more focused modules?**
  _Cohesion score 0.0494279176201373 - nodes in this community are weakly interconnected._