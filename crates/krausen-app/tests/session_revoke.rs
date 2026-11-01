// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{Error, invitation::AccountActor, session_revoke};
use krausen_core::DbPool;
use krausen_types::session_revoke::{SessionRevokeCommand, SessionRevokeOutcome as Outcome};

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("user-{suffix}");
    let other = format!("other-{suffix}");
    let caller = format!("caller-{suffix}");
    let target = format!("target-{suffix}");
    let foreign = format!("foreign-{suffix}");
    let token = format!("token-{suffix}");
    let expiry = chrono::Utc::now() + chrono::Duration::days(1);
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2),($3,$4)",
        &user,
        format!("{user}@example.test"),
        &other,
        format!("{other}@example.test")
    );
    for (id, owner) in [(&caller, &user), (&target, &user), (&foreign, &other)] {
        run!(
            "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',4102444800)",
            id,
            owner
        );
    }
    for (id, owner, family) in [
        (&token, &user, &target),
        (&format!("foreign-token-{suffix}"), &other, &foreign),
    ] {
        run!(
            "INSERT INTO refresh_tokens (token_id,user_id,token_hash,family_id,expires_at,is_active) VALUES ($1,$2,$3,$4,$5,TRUE)",
            id,
            owner,
            format!("hash-{id}"),
            family,
            expiry
        );
    }
    let actor = AccountActor {
        actor_id: user.clone(),
        credential_id: caller.clone(),
        access_expires_at: 4102444800,
    };
    let cmd = SessionRevokeCommand {
        expected_current_token_id: token.clone(),
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let inaccessible = SessionRevokeCommand {
        expected_current_token_id: format!("foreign-token-{suffix}"),
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    assert!(matches!(
        session_revoke::revoke(&db, &actor, &inaccessible)
            .await
            .unwrap(),
        Outcome::NotFound
    ));
    run!(
        "UPDATE refresh_tokens SET replaced_at=CURRENT_TIMESTAMP WHERE token_id=$1",
        &token
    );
    let new_token = format!("new-{suffix}");
    run!(
        "INSERT INTO refresh_tokens (token_id,user_id,token_hash,family_id,expires_at,is_active) VALUES ($1,$2,$3,$4,$5,TRUE)",
        &new_token,
        &user,
        format!("hash-{new_token}"),
        &target,
        expiry
    );
    assert!(matches!(
        session_revoke::revoke(&db, &actor, &cmd).await.unwrap(),
        Outcome::Conflict
    ));
    let current = SessionRevokeCommand {
        expected_current_token_id: new_token.clone(),
        request_id: cmd.request_id.clone(),
    };
    let duplicate = format!("duplicate-{suffix}");
    run!(
        "INSERT INTO refresh_tokens (token_id,user_id,token_hash,family_id,expires_at,is_active) VALUES ($1,$2,$3,$4,$5,TRUE)",
        &duplicate,
        &user,
        format!("hash-{duplicate}"),
        &target,
        expiry
    );
    assert!(matches!(
        session_revoke::revoke(&db, &actor, &current).await.unwrap(),
        Outcome::Conflict
    ));
    run!(
        "UPDATE refresh_tokens SET is_active=FALSE WHERE token_id=$1",
        &duplicate
    );
    // Fail the final ledger insert, after both revocation updates, to prove rollback.
    match &db {
        DbPool::Postgres(p) => {
            sqlx::query("CREATE FUNCTION kra11_fail_session_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected audit failure'; END $$").execute(p).await.unwrap();
            sqlx::query("CREATE TRIGGER kra11_fail_session_audit BEFORE INSERT ON account_session_revocations FOR EACH ROW EXECUTE FUNCTION kra11_fail_session_audit()").execute(p).await.unwrap();
        }
        DbPool::Sqlite(p) => {
            sqlx::query("CREATE TRIGGER kra11_fail_session_audit BEFORE INSERT ON account_session_revocations BEGIN SELECT RAISE(ABORT, 'injected audit failure'); END").execute(p).await.unwrap();
        }
    }
    assert!(session_revoke::revoke(&db, &actor, &current).await.is_err());
    let active: i64 = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM refresh_tokens WHERE token_id=$1 AND is_active=TRUE",
        )
        .bind(&new_token)
        .fetch_one(p)
        .await
        .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM refresh_tokens WHERE token_id=$1 AND is_active=TRUE",
        )
        .bind(&new_token)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(active, 1, "failed audit must roll back family revocation");
    let valid_credential: i64 = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM auth_credentials WHERE credential_id=$1 AND revoked_at IS NULL",
        )
        .bind(&target)
        .fetch_one(p)
        .await
        .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM auth_credentials WHERE credential_id=$1 AND revoked_at IS NULL",
        )
        .bind(&target)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(
        valid_credential, 1,
        "failed audit must roll back credential revocation"
    );
    match &db {
        DbPool::Postgres(p) => {
            sqlx::query("DROP TRIGGER kra11_fail_session_audit ON account_session_revocations")
                .execute(p)
                .await
                .unwrap();
            sqlx::query("DROP FUNCTION kra11_fail_session_audit()")
                .execute(p)
                .await
                .unwrap();
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DROP TRIGGER kra11_fail_session_audit")
                .execute(p)
                .await
                .unwrap();
        }
    }
    assert!(matches!(
        session_revoke::revoke(&db, &actor, &current).await.unwrap(),
        Outcome::Revoked {
            revoked_current: false,
            ..
        }
    ));
    assert!(matches!(
        session_revoke::revoke(&db, &actor, &current).await.unwrap(),
        Outcome::Revoked {
            revoked_current: false,
            ..
        }
    ));
    assert!(matches!(
        session_revoke::revoke(&db, &actor, &cmd).await,
        Err(Error::RequestIdentityConflict)
    ));
    run!(
        "UPDATE account_session_revocations SET result_expires_at=0 WHERE account_id=$1 AND request_id=$2",
        &user,
        &current.request_id
    );
    assert!(matches!(
        session_revoke::revoke(&db, &actor, &current).await,
        Err(Error::ResultExpired { .. })
    ));
    let race_family = format!("race-family-{suffix}");
    let race_token = format!("race-token-{suffix}");
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',4102444800)",
        &race_family,
        &user
    );
    run!(
        "INSERT INTO refresh_tokens (token_id,user_id,token_hash,family_id,expires_at,is_active) VALUES ($1,$2,$3,$4,$5,TRUE)",
        &race_token,
        &user,
        format!("hash-{race_token}"),
        &race_family,
        expiry
    );
    let first = SessionRevokeCommand {
        expected_current_token_id: race_token.clone(),
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let second = SessionRevokeCommand {
        expected_current_token_id: race_token,
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let (a, b) = tokio::join!(
        session_revoke::revoke(&db, &actor, &first),
        session_revoke::revoke(&db, &actor, &second)
    );
    let outcomes = [a.unwrap(), b.unwrap()];
    assert_eq!(
        outcomes
            .iter()
            .filter(|x| matches!(x, Outcome::Revoked { .. }))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|x| matches!(x, Outcome::Conflict))
            .count(),
        1
    );
    let self_token = format!("self-{suffix}");
    run!(
        "INSERT INTO refresh_tokens (token_id,user_id,token_hash,family_id,expires_at,is_active) VALUES ($1,$2,$3,$4,$5,TRUE)",
        &self_token,
        &user,
        format!("hash-{self_token}"),
        &caller,
        expiry
    );
    let self_cmd = SessionRevokeCommand {
        expected_current_token_id: self_token,
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    assert!(matches!(
        session_revoke::revoke(&db, &actor, &self_cmd)
            .await
            .unwrap(),
        Outcome::Revoked {
            revoked_current: true,
            ..
        }
    ));
    assert!(matches!(
        session_revoke::revoke(&db, &actor, &self_cmd).await,
        Err(Error::Forbidden)
    ));
    let replacement = format!("replacement-{suffix}");
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'mcp','test','test',4102444800)",
        &replacement,
        &user
    );
    let other_caller = AccountActor {
        actor_id: user,
        credential_id: replacement,
        access_expires_at: 4102444800,
    };
    assert!(matches!(
        session_revoke::revoke(&db, &other_caller, &self_cmd)
            .await
            .unwrap(),
        Outcome::Revoked {
            revoked_current: true,
            ..
        }
    ));
}

#[tokio::test]
async fn sqlite_session_revoke() {
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
async fn postgres_session_revoke() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
