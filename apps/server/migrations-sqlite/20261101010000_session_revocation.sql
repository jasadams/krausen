-- Account-scoped session revocation audit and durable request tombstones.
CREATE TABLE account_session_revocations (
    account_id TEXT NOT NULL REFERENCES users(user_id),
    request_id TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    expected_token_id TEXT NOT NULL,
    family_id TEXT NOT NULL,
    revoked_current BOOLEAN NOT NULL,
    recorded_at BIGINT NOT NULL,
    result_expires_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, request_id)
);
