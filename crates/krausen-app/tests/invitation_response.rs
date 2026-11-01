// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::invitation::{AccountActor, respond};
use krausen_core::DbPool;
use krausen_types::invitation::{InvitationAction as Action, InvitationResponseOutcome as Outcome};

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let owner = format!("o-{suffix}");
    let recipient = format!("r-{suffix}");
    let other = format!("x-{suffix}");
    let workspace = format!("w-{suffix}");
    let email = format!("{suffix}@example.test");
    let credential = format!("c-{suffix}");
    let other_credential = format!("d-{suffix}");
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    for (id, address) in [
        (&owner, format!("owner-{email}")),
        (&recipient, email.clone()),
        (&other, format!("other-{email}")),
    ] {
        run!(
            "INSERT INTO users (user_id,email,active) VALUES ($1,$2,TRUE)",
            id,
            address
        );
    }
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id,status) VALUES ($1,$2,'active')",
        &workspace,
        &owner
    );
    for (id, user) in [(&credential, &recipient), (&other_credential, &other)] {
        run!(
            "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
            id,
            user,
            chrono::Utc::now().timestamp() + 3600
        );
    }
    let actor = AccountActor {
        actor_id: recipient.clone(),
        credential_id: credential,
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    let stranger = AccountActor {
        actor_id: other,
        credential_id: other_credential,
        access_expires_at: actor.access_expires_at,
    };
    let invite = format!("i-{suffix}");
    let expires = chrono::Utc::now() + chrono::Duration::days(1);
    run!(
        "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,$5)",
        &invite,
        &workspace,
        email.to_uppercase(),
        &owner,
        expires
    );
    assert_eq!(
        respond(
            &db,
            &stranger,
            &invite,
            Action::Decline,
            uuid::Uuid::new_v4()
        )
        .await
        .unwrap(),
        Outcome::NotFound
    );
    let key = uuid::Uuid::new_v4();
    assert_eq!(
        respond(&db, &actor, &invite, Action::Accept, key)
            .await
            .unwrap(),
        Outcome::Accepted
    );
    assert_eq!(
        respond(&db, &actor, &invite, Action::Accept, key)
            .await
            .unwrap(),
        Outcome::Accepted
    );
    assert_eq!(
        respond(&db, &actor, &invite, Action::Decline, uuid::Uuid::new_v4())
            .await
            .unwrap(),
        Outcome::Conflict
    );
    let role = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar::<_, String>(
            "SELECT role FROM workspace_users WHERE workspace_id=$1 AND user_id=$2",
        )
        .bind(&workspace)
        .bind(&recipient)
        .fetch_one(p)
        .await
        .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar::<_, String>(
            "SELECT role FROM workspace_users WHERE workspace_id=$1 AND user_id=$2",
        )
        .bind(&workspace)
        .bind(&recipient)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(role, "workspace_user");
    // Accepting an invitation must never lower an existing admin's role.
    run!(
        "UPDATE workspace_users SET role='workspace_admin' WHERE workspace_id=$1 AND user_id=$2",
        &workspace,
        &recipient
    );
    let existing_id = format!("m-{suffix}");
    run!(
        "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,$5)",
        &existing_id,
        &workspace,
        &email,
        &owner,
        expires
    );
    assert_eq!(
        respond(
            &db,
            &actor,
            &existing_id,
            Action::Accept,
            uuid::Uuid::new_v4()
        )
        .await
        .unwrap(),
        Outcome::Accepted
    );
    let retained_role = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar::<_, String>(
            "SELECT role FROM workspace_users WHERE workspace_id=$1 AND user_id=$2",
        )
        .bind(&workspace)
        .bind(&recipient)
        .fetch_one(p)
        .await
        .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar::<_, String>(
            "SELECT role FROM workspace_users WHERE workspace_id=$1 AND user_id=$2",
        )
        .bind(&workspace)
        .bind(&recipient)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(retained_role, "workspace_admin");
    let expired_id = format!("e-{suffix}");
    let expired = chrono::Utc::now() - chrono::Duration::days(1);
    run!(
        "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,$5)",
        &expired_id,
        &workspace,
        &email,
        &owner,
        expired
    );
    assert_eq!(
        respond(
            &db,
            &actor,
            &expired_id,
            Action::Decline,
            uuid::Uuid::new_v4()
        )
        .await
        .unwrap(),
        Outcome::Conflict
    );
    // A failing change record must roll back the invitation transition and retry record.
    let rollback_id = format!("b-{suffix}");
    run!(
        "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,$5)",
        &rollback_id,
        &workspace,
        format!("other-{email}"),
        &owner,
        expires
    );
    if db.is_postgres() {
        run!(
            "CREATE OR REPLACE FUNCTION kra11_fail_invitation_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'forced rollback'; END $$"
        );
        run!(
            "CREATE TRIGGER kra11_fail_invitation_change BEFORE INSERT ON app_changes FOR EACH ROW EXECUTE FUNCTION kra11_fail_invitation_change()"
        );
    } else {
        run!(
            "CREATE TRIGGER kra11_fail_invitation_change BEFORE INSERT ON app_changes BEGIN SELECT RAISE(ABORT, 'forced rollback'); END"
        );
    }
    let rollback_key = uuid::Uuid::new_v4();
    assert!(
        respond(&db, &stranger, &rollback_id, Action::Accept, rollback_key)
            .await
            .is_err()
    );
    if db.is_postgres() {
        run!("DROP TRIGGER kra11_fail_invitation_change ON app_changes");
        run!("DROP FUNCTION kra11_fail_invitation_change()");
    } else {
        run!("DROP TRIGGER kra11_fail_invitation_change");
    }
    let membership_count = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workspace_users WHERE workspace_id=$1 AND user_id=$2",
        )
        .bind(&workspace)
        .bind(&stranger.actor_id)
        .fetch_one(p)
        .await
        .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workspace_users WHERE workspace_id=$1 AND user_id=$2",
        )
        .bind(&workspace)
        .bind(&stranger.actor_id)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(membership_count, 0);
    assert_eq!(
        respond(&db, &stranger, &rollback_id, Action::Accept, rollback_key)
            .await
            .unwrap(),
        Outcome::Accepted
    );
    let race_id = format!("q-{suffix}");
    run!(
        "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,$5)",
        &race_id,
        &workspace,
        &email,
        &owner,
        expires
    );
    let (accept, decline) = tokio::join!(
        respond(&db, &actor, &race_id, Action::Accept, uuid::Uuid::new_v4()),
        respond(&db, &actor, &race_id, Action::Decline, uuid::Uuid::new_v4())
    );
    let (accept, decline) = (accept.unwrap(), decline.unwrap());
    assert!(matches!(
        (accept, decline),
        (Outcome::Accepted, Outcome::Conflict) | (Outcome::Conflict, Outcome::Declined)
    ));
    run!(
        "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2",
        chrono::Utc::now().timestamp(),
        &actor.credential_id
    );
    assert!(matches!(
        respond(&db, &actor, &race_id, Action::Accept, uuid::Uuid::new_v4()).await,
        Err(krausen_app::Error::Forbidden)
    ));
}

#[tokio::test]
async fn sqlite_invitation_response() {
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
async fn postgres_invitation_response() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
