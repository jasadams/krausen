// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dual-backend acceptance for the shared ingredient catalogue operations.
use krausen_app::{ActorContext, Error, catalogue, recipe_editor};
use krausen_core::DbPool;
use krausen_types::{
    brewery::*,
    catalogue::{
        ArchiveIngredientEntry, CatalogueFilter, CatalogueScope, IngredientEntry,
        IngredientEntryDocumentV1, SaveIngredientEntry, SearchCatalogue,
    },
    recipe_editor::RecipeSaveOutcome,
    request_context::OperationIntent,
};

fn doc(name: &str, producer: Option<&str>, yield_fraction: f64, colour: f64) -> IngredientEntryDocumentV1 {
    IngredientEntryDocumentV1::fermentable(
        "Base malt".into(),
        FermentableSpecification {
            name: Some(name.into()),
            producer: producer.map(str::to_owned),
            kind: Some("grain".into()),
            yield_fraction: Some(yield_fraction),
            colour_ebc: Some(colour),
        },
    )
}

fn save_request(
    intent: &OperationIntent,
    entry_id: &str,
    document: IngredientEntryDocumentV1,
    expected_revision: i64,
    expected_metadata_revision: i64,
) -> SaveIngredientEntry {
    SaveIngredientEntry {
        intent: intent.clone(),
        entry_id: entry_id.to_owned(),
        document,
        expected_revision,
        expected_metadata_revision,
        request_id: uuid::Uuid::new_v4().to_string(),
    }
}

fn search(intent: &OperationIntent, query: &str, filter: CatalogueFilter) -> SearchCatalogue {
    SearchCatalogue {
        intent: intent.clone(),
        query: query.to_owned(),
        filter,
        cursor: None,
        limit: 100,
        include_archived: false,
    }
}

fn recipe_with(source: Option<IngredientSource>) -> RecipeDocumentV1 {
    let mut recipe = RecipeDocumentV1::default();
    recipe.title = "Catalogue recipe".into();
    recipe.batch_size_l = Some(20.0);
    recipe.boil_minutes = Some(60.0);
    recipe.process.mash_water_method = Some("manual".into());
    recipe.process.mash_water_l = Some(12.0);
    recipe.process.apparent_attenuation_fraction = Some(0.8);
    recipe.fermentables.push(FermentableAddition {
        row_id: "grain-1".into(),
        amount_kg: Some(4.0),
        addition: AdditionStage {
            stage: "mash".into(),
            time_minutes: None,
        },
        specification: FermentableSpecification {
            name: Some("Copied malt".into()),
            producer: None,
            kind: Some("grain".into()),
            yield_fraction: Some(0.8),
            colour_ebc: Some(5.0),
        },
        source,
    });
    recipe
}

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4();
    let user = format!("u-{suffix}");
    let workspace_a = format!("a-{suffix}");
    let workspace_b = format!("b-{suffix}");
    let cred = format!("c-{suffix}");
    macro_rules! run {
        ($sql:expr $(,$arg:expr)*) => {
            match &db {
                DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
                DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
            }
        };
        // Expect a database error (immutability triggers).
    }
    macro_rules! run_err {
        ($sql:expr $(,$arg:expr)*) => {{
            let result: Result<(), sqlx::Error> = match &db {
                DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.map(|_| ()),
                DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.map(|_| ()),
            };
            assert!(result.is_err(), "immutable history must reject writes");
        }};
    }
    macro_rules! scalar {
        ($sql:expr $(,$arg:expr)*) => {
            match &db {
                DbPool::Sqlite(p) => sqlx::query_scalar::<_, i64>($sql)$(.bind($arg))*.fetch_one(p).await.unwrap(),
                DbPool::Postgres(p) => sqlx::query_scalar::<_, i64>($sql)$(.bind($arg))*.fetch_one(p).await.unwrap(),
            }
        };
    }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &user,
        format!("{suffix}@example.test")
    );
    for workspace in [&workspace_a, &workspace_b] {
        run!(
            "INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)",
            workspace,
            &user
        );
        run!(
            "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'workspace_user')",
            workspace,
            &user
        );
    }
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
        &cred,
        &user,
        4102444800_i64
    );
    let actor_a = ActorContext {
        actor_id: user.clone(),
        workspace_id: workspace_a.clone(),
        credential_id: cred.clone(),
        access_expires_at: 4102444800,
    };
    let actor_b = ActorContext {
        actor_id: user.clone(),
        workspace_id: workspace_b.clone(),
        credential_id: cred.clone(),
        access_expires_at: 4102444800,
    };
    let intent_a = OperationIntent {
        account_id: user.clone(),
        workspace_id: workspace_a.clone(),
    };
    let intent_b = OperationIntent {
        account_id: user.clone(),
        workspace_id: workspace_b.clone(),
    };

    // --- Public starter-read exception (identical from both workspaces) ---
    let page = catalogue::search(&db, &actor_a, &search(&intent_a, "", CatalogueFilter::All))
        .await
        .unwrap();
    assert_eq!(page.items.len(), 4, "starter seed must be searchable");
    assert!(
        page.items
            .iter()
            .all(|entry| entry.scope == CatalogueScope::Starter && entry.workspace_id.is_none())
    );
    let vienna = page
        .items
        .iter()
        .find(|entry| entry.entry_id == "starter-vienna")
        .expect("starter seed entry");
    assert!(vienna.provenance.source == "krausen-starter-catalogue-v1");
    assert!(vienna.provenance.license.is_some());
    assert!(!vienna.provenance.notes.is_empty());
    assert_eq!(vienna.revision, 1);
    let b_starter = catalogue::search(&db, &actor_b, &search(&intent_b, "", CatalogueFilter::All))
        .await
        .unwrap();
    let a_ids: Vec<_> = page.items.iter().map(|e| e.entry_id.clone()).collect();
    let b_ids: Vec<_> = b_starter.items.iter().map(|e| e.entry_id.clone()).collect();
    assert_eq!(a_ids, b_ids, "starter reads are workspace-independent");
    assert!(
        catalogue::get(&db, &actor_b, &intent_b, "starter-vienna")
            .await
            .unwrap()
            .is_some()
    );

    // Starter entries are immutable through workspace operations.
    assert!(matches!(
        catalogue::save(
            &db,
            &actor_a,
            save_request(&intent_a, "starter-vienna", doc("Renamed", None, 0.8, 5.0), 0, 0)
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        catalogue::set_archived(
            &db,
            &actor_a,
            ArchiveIngredientEntry {
                intent: intent_a.clone(),
                entry_id: "starter-vienna".into(),
                archived: true,
                expected_metadata_revision: 1,
                request_id: uuid::Uuid::new_v4().to_string(),
            }
        )
        .await,
        Err(Error::Invalid(_))
    ));

    // --- Create, validation and duplicate handling ---
    let entry_id = uuid::Uuid::new_v4().to_string();
    let created = catalogue::save(
        &db,
        &actor_a,
        save_request(
            &intent_a,
            &entry_id,
            doc("Local pale malt", Some("Maltster"), 0.8, 4.2),
            0,
            0,
        ),
    )
    .await
    .unwrap();
    assert_eq!((created.revision, created.metadata_revision), (1, 1));
    assert_eq!(created.scope, CatalogueScope::Workspace);
    assert_eq!(created.workspace_id.as_deref(), Some(workspace_a.as_str()));
    assert_eq!(created.provenance.source, "workspace-custom");
    assert!(created.provenance.license.is_none());
    assert!(created.saved_by.as_deref() == Some(user.as_str()));
    let source = created.ingredient_source();
    assert_eq!(source.scope, "workspace");
    assert_eq!(source.version, Some(1));

    // Case-insensitive duplicate name+producer.
    let duplicate = catalogue::save(
        &db,
        &actor_a,
        save_request(
            &intent_a,
            &uuid::Uuid::new_v4().to_string(),
            doc("  local pale malt  ", Some("MALTSTER"), 0.81, 4.0),
            0,
            0,
        ),
    )
    .await;
    assert!(matches!(duplicate, Err(Error::DuplicateIngredient)));

    // Missing and out-of-range numerical specifications are rejected.
    let mut missing_yield = doc("No yield", None, 0.8, 4.0);
    missing_yield.specification.yield_fraction = None;
    assert!(matches!(
        catalogue::save(
            &db,
            &actor_a,
            save_request(&intent_a, &uuid::Uuid::new_v4().to_string(), missing_yield, 0, 0)
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        catalogue::save(
            &db,
            &actor_a,
            save_request(
                &intent_a,
                &uuid::Uuid::new_v4().to_string(),
                doc("Negative colour", None, 0.8, -1.0),
                0,
                0
            )
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        catalogue::save(
            &db,
            &actor_a,
            save_request(
                &intent_a,
                &uuid::Uuid::new_v4().to_string(),
                doc("Impossible yield", None, 1.5, 4.0),
                0,
                0
            )
        )
        .await,
        Err(Error::Invalid(_))
    ));

    // --- Request identity: identical retry returns the original result ---
    let request_id = uuid::Uuid::new_v4().to_string();
    let identity_request = SaveIngredientEntry {
        intent: intent_a.clone(),
        entry_id: uuid::Uuid::new_v4().to_string(),
        document: doc("Retry malt", None, 0.75, 6.0),
        expected_revision: 0,
        expected_metadata_revision: 0,
        request_id: request_id.clone(),
    };
    let first = catalogue::save(&db, &actor_a, identity_request.clone())
        .await
        .unwrap();
    let repeat = catalogue::save(&db, &actor_a, identity_request.clone())
        .await
        .unwrap();
    assert_eq!(first, repeat, "identical retry returns the first result");
    let mut conflicting = identity_request.clone();
    conflicting.document = doc("Different payload", None, 0.75, 6.0);
    assert!(matches!(
        catalogue::save(&db, &actor_a, conflicting).await,
        Err(Error::RequestIdentityConflict)
    ));
    run!(
        "UPDATE app_requests SET result_expires_at=0 WHERE request_id=$1",
        &request_id
    );
    assert!(matches!(
        catalogue::save(&db, &actor_a, identity_request.clone()).await,
        Err(Error::ResultExpired { .. })
    ));

    // --- Guarded edit with immutable versions ---
    let edited = catalogue::save(
        &db,
        &actor_a,
        save_request(
            &intent_a,
            &entry_id,
            doc("Local pale malt v2", Some("Maltster"), 0.82, 4.5),
            1,
            1,
        ),
    )
    .await
    .unwrap();
    assert_eq!((edited.revision, edited.metadata_revision), (2, 2));
    assert!(matches!(
        catalogue::save(
            &db,
            &actor_a,
            save_request(
                &intent_a,
                &entry_id,
                doc("Local pale malt v3", Some("Maltster"), 0.82, 4.5),
                1,
                2,
            )
        )
        .await,
        Err(Error::Conflict { expected: 1, actual: 2 })
    ));
    assert!(matches!(
        catalogue::save(
            &db,
            &actor_a,
            save_request(
                &intent_a,
                &entry_id,
                doc("Local pale malt v3", Some("Maltster"), 0.82, 4.5),
                2,
                1,
            )
        )
        .await,
        Err(Error::Conflict {
            expected: 1,
            actual: 2
        })
    ));
    // An identical save keeps the observed revisions.
    let unchanged = catalogue::save(
        &db,
        &actor_a,
        save_request(
            &intent_a,
            &entry_id,
            doc("  Local pale malt v2  ", Some("Maltster"), 0.82, 4.5),
            2,
            2,
        ),
    )
    .await
    .unwrap();
    assert_eq!((unchanged.revision, unchanged.metadata_revision), (2, 2));
    let revisions: i64 = scalar!("SELECT COUNT(*) FROM ingredient_revisions WHERE entry_id=$1", &entry_id);
    assert_eq!(revisions, 2, "append-only content history");

    // --- Guarded archive/unarchive with retained events ---
    let archived = catalogue::set_archived(
        &db,
        &actor_a,
        ArchiveIngredientEntry {
            intent: intent_a.clone(),
            entry_id: entry_id.clone(),
            archived: true,
            expected_metadata_revision: 2,
            request_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await
    .unwrap();
    assert!(archived.archived);
    assert_eq!(archived.metadata_revision, 3);
    assert_eq!(
        scalar!(
            "SELECT COUNT(*) FROM ingredient_archive_events WHERE entry_id=$1",
            &entry_id
        ),
        1
    );
    // Archived entries disappear from search but stay readable by ID.
    let after_archive =
        catalogue::search(&db, &actor_a, &search(&intent_a, "Local pale", CatalogueFilter::All))
            .await
            .unwrap();
    assert!(after_archive.items.iter().all(|e| e.entry_id != entry_id));
    let loaded = catalogue::get(&db, &actor_a, &intent_a, &entry_id)
        .await
        .unwrap()
        .expect("archived entry remains readable");
    assert!(loaded.archived);
    // Archived entries cannot be edited or re-archived with stale metadata.
    assert!(matches!(
        catalogue::save(
            &db,
            &actor_a,
            save_request(
                &intent_a,
                &entry_id,
                doc("Local pale malt v3", Some("Maltster"), 0.82, 4.5),
                2,
                3,
            )
        )
        .await,
        Err(Error::IngredientArchived)
    ));
    assert!(matches!(
        catalogue::set_archived(
            &db,
            &actor_a,
            ArchiveIngredientEntry {
                intent: intent_a.clone(),
                entry_id: entry_id.clone(),
                archived: true,
                expected_metadata_revision: 2,
                request_id: uuid::Uuid::new_v4().to_string(),
            }
        )
        .await,
        Err(Error::Conflict {
            expected: 2,
            actual: 3
        })
    ));
    let restored = catalogue::set_archived(
        &db,
        &actor_a,
        ArchiveIngredientEntry {
            intent: intent_a.clone(),
            entry_id: entry_id.clone(),
            archived: false,
            expected_metadata_revision: 3,
            request_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await
    .unwrap();
    assert!(!restored.archived);
    assert_eq!(
        scalar!(
            "SELECT COUNT(*) FROM ingredient_archive_events WHERE entry_id=$1",
            &entry_id
        ),
        2
    );
    run_err!(
        "UPDATE ingredient_archive_events SET archived=FALSE WHERE entry_id=$1",
        &entry_id
    );
    run_err!(
        "UPDATE ingredient_revisions SET name='tampered' WHERE entry_id=$1 AND revision=1",
        &entry_id
    );

    // --- Change records for the guarded writes ---
    assert!(
        scalar!(
            "SELECT COUNT(*) FROM app_changes WHERE operation='ingredient.save.v1' AND resource_id=$1",
            &entry_id
        ) >= 1
    );
    assert_eq!(
        scalar!(
            "SELECT COUNT(*) FROM app_changes WHERE operation='ingredient.archive.v1' AND resource_id=$1",
            &entry_id
        ),
        2
    );

    // --- Tenant scoping: same actor belongs to both workspaces ---
    let foreign = catalogue::get(&db, &actor_b, &intent_b, &entry_id)
        .await
        .unwrap();
    assert!(
        foreign.is_none(),
        "a member of both workspaces must not read the other workspace's entry"
    );
    let b_page =
        catalogue::search(&db, &actor_b, &search(&intent_b, "Local pale", CatalogueFilter::All))
            .await
            .unwrap();
    assert!(
        b_page.items.iter().all(|e| e.entry_id != entry_id),
        "workspace A entries must not appear in workspace B search"
    );
    // Global IDs cannot be recreated in another workspace.
    assert!(matches!(
        catalogue::save(
            &db,
            &actor_b,
            save_request(
                &intent_b,
                &entry_id,
                doc("Foreign clone", None, 0.8, 4.0),
                0,
                0
            )
        )
        .await,
        Err(Error::Invalid(_))
    ));

    // --- Recipe source resolution and history preservation ---
    // Workspace source resolves only in its own workspace.
    let mut source_a = source.clone();
    source_a.version = Some(2);
    let recipe_in_b = recipe_with(Some(source_a.clone()));
    assert!(matches!(
        recipe_editor::save(&db, &actor_b, &intent_b, None, None, &recipe_in_b, uuid::Uuid::new_v4()).await,
        RecipeSaveOutcome::UnresolvedIngredientSource
    ));
    // Starter sources are the explicit global-read exception.
    let starter_source = IngredientSource {
        scope: "starter".into(),
        catalogue_id: Some("krausen-starter-catalogue-v1".into()),
        entry_id: "starter-vienna".into(),
        version: Some(1),
    };
    let recipe_starter = recipe_with(Some(starter_source));
    let RecipeSaveOutcome::Saved {
        recipe_id: starter_recipe,
        ..
    } = recipe_editor::save(
        &db,
        &actor_b,
        &intent_b,
        None,
        None,
        &recipe_starter,
        uuid::Uuid::new_v4(),
    )
    .await
    else {
        panic!("starter source must resolve in any workspace");
    };
    // Workspace source resolves in its own workspace.
    let recipe_in_a = recipe_with(Some(source_a.clone()));
    let RecipeSaveOutcome::Saved {
        recipe_id: recipe_a,
        revision: first_revision,
    } = recipe_editor::save(
        &db,
        &actor_a,
        &intent_a,
        None,
        None,
        &recipe_in_a,
        uuid::Uuid::new_v4(),
    )
    .await
    else {
        panic!("workspace source must resolve in its own workspace");
    };
    // Unknown scope and unknown IDs never resolve.
    let mut unknown_scope = recipe_in_a.clone();
    unknown_scope.fermentables[0].row_id = "grain-2".into();
    unknown_scope.fermentables[0].source = Some(IngredientSource {
        scope: "made-up".into(),
        catalogue_id: None,
        entry_id: entry_id.clone(),
        version: Some(2),
    });
    assert!(matches!(
        recipe_editor::save(
            &db,
            &actor_a,
            &intent_a,
            Some(&recipe_a),
            Some(first_revision),
            &unknown_scope,
            uuid::Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::UnresolvedIngredientSource
    ));
    let mut missing_entry = recipe_in_a.clone();
    missing_entry.fermentables[0].row_id = "grain-3".into();
    missing_entry.fermentables[0].source = Some(IngredientSource {
        scope: "workspace".into(),
        catalogue_id: None,
        entry_id: uuid::Uuid::new_v4().to_string(),
        version: Some(1),
    });
    assert!(matches!(
        recipe_editor::save(
            &db,
            &actor_a,
            &intent_a,
            Some(&recipe_a),
            Some(first_revision),
            &missing_entry,
            uuid::Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::UnresolvedIngredientSource
    ));

    // Editing or archiving the source later never invalidates saved copies.
    let edited_again = catalogue::save(
        &db,
        &actor_a,
        save_request(
            &intent_a,
            &entry_id,
            doc("Local pale malt v3", Some("Maltster"), 0.83, 4.6),
            2,
            4,
        ),
    )
    .await
    .unwrap();
    assert_eq!(edited_again.revision, 3);
    let mut later_save = recipe_in_a.clone();
    later_save.title = "Saved after a catalogue edit".into();
    let RecipeSaveOutcome::Saved {
        revision: second_revision,
        ..
    } = recipe_editor::save(
        &db,
        &actor_a,
        &intent_a,
        Some(&recipe_a),
        Some(first_revision),
        &later_save,
        uuid::Uuid::new_v4(),
    )
    .await
    else {
        panic!("saved recipe must survive a catalogue edit");
    };
    catalogue::set_archived(
        &db,
        &actor_a,
        ArchiveIngredientEntry {
            intent: intent_a.clone(),
            entry_id: entry_id.clone(),
            archived: true,
            expected_metadata_revision: 5,
            request_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await
    .unwrap();
    let mut after_archive_save = recipe_in_a.clone();
    after_archive_save.title = "Saved after archiving the source".into();
    let RecipeSaveOutcome::Saved {
        revision: third_revision,
        ..
    } = recipe_editor::save(
        &db,
        &actor_a,
        &intent_a,
        Some(&recipe_a),
        Some(second_revision),
        &after_archive_save,
        uuid::Uuid::new_v4(),
    )
    .await
    else {
        panic!("saved recipe must survive archiving its source");
    };
    // A NEW row referencing an archived entry is rejected.
    let mut new_row_on_archived = recipe_in_a.clone();
    new_row_on_archived.fermentables[0].row_id = "grain-4".into();
    new_row_on_archived.fermentables[0].source = Some(IngredientSource {
        scope: "workspace".into(),
        catalogue_id: None,
        entry_id: entry_id.clone(),
        version: Some(3),
    });
    assert!(matches!(
        recipe_editor::save(
            &db,
            &actor_a,
            &intent_a,
            Some(&recipe_a),
            Some(third_revision),
            &new_row_on_archived,
            uuid::Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::UnresolvedIngredientSource
    ));
    // A version newer than the entry never resolves.
    let mut future_version = recipe_in_a.clone();
    future_version.fermentables[0].row_id = "grain-5".into();
    future_version.fermentables[0].source = Some(IngredientSource {
        scope: "starter".into(),
        catalogue_id: None,
        entry_id: "starter-vienna".into(),
        version: Some(99),
    });
    assert!(matches!(
        recipe_editor::save(
            &db,
            &actor_a,
            &intent_a,
            Some(&recipe_a),
            Some(third_revision),
            &future_version,
            uuid::Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::UnresolvedIngredientSource
    ));
    // F3-style identity strictness for new/changed sources: the declared
    // catalogue identity and a real version must match the scope, while
    // recipe-specific specification edits are never part of validation.
    let mut wrong_starter_catalogue = recipe_in_a.clone();
    wrong_starter_catalogue.fermentables[0].row_id = "grain-6".into();
    wrong_starter_catalogue.fermentables[0].source = Some(IngredientSource {
        scope: "starter".into(),
        catalogue_id: None,
        entry_id: "starter-vienna".into(),
        version: Some(1),
    });
    assert!(matches!(
        recipe_editor::save(
            &db,
            &actor_a,
            &intent_a,
            Some(&recipe_a),
            Some(third_revision),
            &wrong_starter_catalogue,
            uuid::Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::UnresolvedIngredientSource
    ));
    let mut starter_wrong_id = recipe_in_a.clone();
    starter_wrong_id.fermentables[0].row_id = "grain-7".into();
    starter_wrong_id.fermentables[0].source = Some(IngredientSource {
        scope: "starter".into(),
        catalogue_id: Some("someone-elses-catalogue".into()),
        entry_id: "starter-vienna".into(),
        version: Some(1),
    });
    assert!(matches!(
        recipe_editor::save(
            &db,
            &actor_a,
            &intent_a,
            Some(&recipe_a),
            Some(third_revision),
            &starter_wrong_id,
            uuid::Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::UnresolvedIngredientSource
    ));
    let mut workspace_with_catalogue_id = recipe_in_a.clone();
    workspace_with_catalogue_id.fermentables[0].row_id = "grain-8".into();
    workspace_with_catalogue_id.fermentables[0].source = Some(IngredientSource {
        scope: "workspace".into(),
        catalogue_id: Some("krausen-starter-catalogue-v1".into()),
        entry_id: entry_id.clone(),
        version: Some(3),
    });
    assert!(matches!(
        recipe_editor::save(
            &db,
            &actor_a,
            &intent_a,
            Some(&recipe_a),
            Some(third_revision),
            &workspace_with_catalogue_id,
            uuid::Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::UnresolvedIngredientSource
    ));
    let mut version_zero = recipe_in_a.clone();
    version_zero.fermentables[0].row_id = "grain-9".into();
    version_zero.fermentables[0].source = Some(IngredientSource {
        scope: "starter".into(),
        catalogue_id: Some("krausen-starter-catalogue-v1".into()),
        entry_id: "starter-vienna".into(),
        version: Some(0),
    });
    assert!(matches!(
        recipe_editor::save(
            &db,
            &actor_a,
            &intent_a,
            Some(&recipe_a),
            Some(third_revision),
            &version_zero,
            uuid::Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::UnresolvedIngredientSource
    ));
    let mut missing_version = recipe_in_a.clone();
    missing_version.fermentables[0].row_id = "grain-10".into();
    missing_version.fermentables[0].source = Some(IngredientSource {
        scope: "starter".into(),
        catalogue_id: Some("krausen-starter-catalogue-v1".into()),
        entry_id: "starter-vienna".into(),
        version: None,
    });
    assert!(matches!(
        recipe_editor::save(
            &db,
            &actor_a,
            &intent_a,
            Some(&recipe_a),
            Some(third_revision),
            &missing_version,
            uuid::Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::UnresolvedIngredientSource
    ));
    // Versions outside the signed storage range must be rejected rather than
    // passing the comparison because the conversion failed.
    for (row_id, version) in [
        ("grain-11", i64::MAX as u64 + 1),
        ("grain-12", u64::MAX),
    ] {
        let mut overflow = recipe_in_a.clone();
        overflow.fermentables[0].row_id = row_id.into();
        overflow.fermentables[0].source = Some(IngredientSource {
            scope: "workspace".into(),
            catalogue_id: None,
            entry_id: entry_id.clone(),
            version: Some(version),
        });
        assert!(matches!(
            recipe_editor::save(
                &db,
                &actor_a,
                &intent_a,
                Some(&recipe_a),
                Some(third_revision),
                &overflow,
                uuid::Uuid::new_v4()
            )
            .await,
            RecipeSaveOutcome::UnresolvedIngredientSource
        ));
    }
    // Failed source resolution must commit nothing: every rejected attempt
    // above left the head revision untouched (atomic rollback of the save).
    let krausen_types::recipe_editor::RecipeReadOutcome::Found(snapshot) =
        recipe_editor::read(&db, &actor_a, &intent_a, &recipe_a).await
    else {
        panic!("recipe must remain readable after rejected saves");
    };
    assert_eq!(
        snapshot.revision, third_revision,
        "rejected saves must not advance the recipe head"
    );

    // A recipe-specific specification edit with an unchanged source must
    // still save: catalogue validation never compares copied specifications.
    let mut recipe_specific_edit = recipe_in_a.clone();
    recipe_specific_edit.fermentables[0].specification.name = Some("Recipe-local rename".into());
    recipe_specific_edit.fermentables[0].specification.yield_fraction = Some(0.77);
    let RecipeSaveOutcome::Saved { .. } = recipe_editor::save(
        &db,
        &actor_a,
        &intent_a,
        Some(&recipe_a),
        Some(third_revision),
        &recipe_specific_edit,
        uuid::Uuid::new_v4(),
    )
    .await
    else {
        panic!("recipe-specific specification edits must survive source validation");
    };

    // The starter recipe saved from workspace B stays readable.
    assert!(
        matches!(
            recipe_editor::read(&db, &actor_b, &intent_b, &starter_recipe).await,
            krausen_types::recipe_editor::RecipeReadOutcome::Found(_)
        )
    );

    // --- Bounded search, cursor binding and stable ordering ---
    let mut created_entries: Vec<IngredientEntry> = Vec::new();
    for index in 0..102 {
        let created = catalogue::save(
            &db,
            &actor_a,
            save_request(
                &intent_a,
                &uuid::Uuid::new_v4().to_string(),
                doc(&format!("Bulk malt {index:03}"), None, 0.8, 4.0),
                0,
                0,
            ),
        )
        .await
        .unwrap();
        created_entries.push(created);
    }
    assert!(matches!(
        catalogue::search(&db, &actor_a, &SearchCatalogue { limit: 101, ..search(&intent_a, "", CatalogueFilter::All) }).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        catalogue::search(&db, &actor_a, &SearchCatalogue { limit: 0, ..search(&intent_a, "", CatalogueFilter::All) }).await,
        Err(Error::Invalid(_))
    ));
    let mut all_items = Vec::new();
    let mut cursor = None;
    loop {
        let page = catalogue::search(
            &db,
            &actor_a,
            &SearchCatalogue {
                cursor: cursor.clone(),
                ..search(&intent_a, "bulk", CatalogueFilter::Workspace)
            },
        )
        .await
        .unwrap();
        all_items.extend(page.items);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(all_items.len(), 102, "cursor pagination must not cut off");
    assert!(
        all_items.windows(2).all(|pair| {
            (pair[0].created_at, &pair[0].entry_id) < (pair[1].created_at, &pair[1].entry_id)
        }),
        "stable (created_at, id) ordering"
    );
    let first_page =
        catalogue::search(&db, &actor_a, &search(&intent_a, "bulk", CatalogueFilter::Workspace))
            .await
            .unwrap();
    assert_eq!(first_page.items.len(), 100);
    assert!(first_page.next_cursor.is_some());
    let cursor_value = first_page.next_cursor.clone().expect("cursor");
    // The cursor is bound to workspace, filter, query and order.
    assert!(matches!(
        catalogue::search(
            &db,
            &actor_b,
            &SearchCatalogue {
                cursor: Some(cursor_value.clone()),
                ..search(&intent_b, "bulk", CatalogueFilter::Workspace)
            },
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        catalogue::search(
            &db,
            &actor_a,
            &SearchCatalogue {
                cursor: Some(cursor_value.clone()),
                ..search(&intent_a, "bulk", CatalogueFilter::All)
            },
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        catalogue::search(
            &db,
            &actor_a,
            &SearchCatalogue {
                cursor: Some(cursor_value.clone()),
                ..search(&intent_a, "other query", CatalogueFilter::Workspace)
            },
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        catalogue::search(
            &db,
            &actor_a,
            &SearchCatalogue {
                cursor: Some("not base64 json".into()),
                ..search(&intent_a, "", CatalogueFilter::Workspace)
            },
        )
        .await,
        Err(Error::Invalid(_))
    ));
    // Archived entries are hidden while their history stays retained.
    let bulk_id = created_entries[0].entry_id.clone();
    catalogue::set_archived(
        &db,
        &actor_a,
        ArchiveIngredientEntry {
            intent: intent_a.clone(),
            entry_id: bulk_id.clone(),
            archived: true,
            expected_metadata_revision: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await
    .unwrap();
    let filtered =
        catalogue::search(&db, &actor_a, &search(&intent_a, "Bulk malt 000", CatalogueFilter::Workspace))
            .await
            .unwrap();
    assert!(filtered.items.iter().all(|e| e.entry_id != bulk_id));
    assert_eq!(
        scalar!(
            "SELECT COUNT(*) FROM ingredient_revisions WHERE entry_id=$1",
            &bulk_id
        ),
        1,
        "archiving never prunes content history"
    );
    // Search terms are literal: wildcards do not widen the match.
    let literal =
        catalogue::search(&db, &actor_a, &search(&intent_a, "%", CatalogueFilter::Workspace))
            .await
            .unwrap();
    assert!(literal.items.is_empty());

    // Archived entries are hidden by default, discoverable with the explicit
    // workspace-scoped flag, and that flag is bound into the cursor.
    let mut discover = search(&intent_a, "Bulk malt 000", CatalogueFilter::Workspace);
    discover.include_archived = true;
    let discovery = catalogue::search(&db, &actor_a, &discover).await.unwrap();
    assert!(
        discovery
            .items
            .iter()
            .any(|e| e.entry_id == bulk_id && e.archived),
        "archived entries must be discoverable for restore"
    );
    let hidden =
        catalogue::search(&db, &actor_a, &search(&intent_a, "Bulk malt 000", CatalogueFilter::Workspace))
            .await
            .unwrap();
    assert!(hidden.items.iter().all(|e| e.entry_id != bulk_id));
    let mut archived_page = search(&intent_a, "", CatalogueFilter::Workspace);
    archived_page.include_archived = true;
    let first_archived_page =
        catalogue::search(&db, &actor_a, &archived_page).await.unwrap();
    if let Some(cursor_value) = first_archived_page.next_cursor.clone() {
        let mut replay_hidden = search(&intent_a, "", CatalogueFilter::Workspace);
        replay_hidden.cursor = Some(cursor_value.clone());
        assert!(
            matches!(
                catalogue::search(&db, &actor_a, &replay_hidden).await,
                Err(Error::Invalid(_))
            ),
            "cursor must bind include_archived"
        );
        let mut replay_shown = search(&intent_a, "", CatalogueFilter::Workspace);
        replay_shown.cursor = Some(cursor_value);
        replay_shown.include_archived = true;
        assert!(catalogue::search(&db, &actor_a, &replay_shown).await.is_ok());
    }

    // --- Context intent and credential revocation fail closed ---
    assert!(matches!(
        catalogue::search(&db, &actor_a, &search(&intent_b, "", CatalogueFilter::All)).await,
        Err(Error::ContextChanged)
    ));
    run!(
        "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2",
        chrono::Utc::now().timestamp(),
        &actor_a.credential_id
    );
    assert!(matches!(
        catalogue::search(&db, &actor_a, &search(&intent_a, "", CatalogueFilter::All)).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        catalogue::save(
            &db,
            &actor_a,
            save_request(&intent_a, &uuid::Uuid::new_v4().to_string(), doc("Blocked", None, 0.8, 4.0), 0, 0)
        )
        .await,
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
async fn sqlite_ingredient_catalogue() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Sqlite(pool)).await;
}

#[tokio::test]
async fn postgres_ingredient_catalogue() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
