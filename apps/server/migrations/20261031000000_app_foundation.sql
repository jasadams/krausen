ALTER TABLE workspaces ADD COLUMN revision BIGINT NOT NULL DEFAULT 0;
CREATE TABLE app_requests (
 actor_id TEXT NOT NULL, workspace_id TEXT NOT NULL, operation TEXT NOT NULL,
 request_id TEXT NOT NULL, payload_digest TEXT NOT NULL, result_json TEXT,
 result_expires_at BIGINT NOT NULL, resource_id TEXT,
 PRIMARY KEY (actor_id, workspace_id, operation, request_id)
);
CREATE TABLE app_changes (
 change_id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL, actor_id TEXT NOT NULL,
 operation TEXT NOT NULL, resource_id TEXT NOT NULL, revision BIGINT NOT NULL,
 created_at BIGINT NOT NULL
);
CREATE TABLE app_outbox (
 outbox_id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL, operation TEXT NOT NULL,
 payload_json TEXT NOT NULL, created_at BIGINT NOT NULL, delivered_at BIGINT
);
