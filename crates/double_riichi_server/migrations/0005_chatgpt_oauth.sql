CREATE TABLE oauth_codes (
    code_hash BLOB PRIMARY KEY CHECK(length(code_hash) = 32),
    client_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    resource TEXT NOT NULL,
    scope TEXT NOT NULL CHECK(scope = 'driichi:play'),
    subject TEXT NOT NULL CHECK(subject = 'admin'),
    pkce_challenge TEXT NOT NULL,
    issued_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER
);

CREATE INDEX oauth_codes_expiry ON oauth_codes(expires_at);

CREATE TABLE oauth_refresh_families (
    family_id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL,
    subject TEXT NOT NULL CHECK(subject = 'admin'),
    resource TEXT NOT NULL,
    scope TEXT NOT NULL CHECK(scope = 'driichi:play'),
    issued_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked_at INTEGER
);

CREATE TABLE oauth_refresh_tokens (
    token_hash BLOB PRIMARY KEY CHECK(length(token_hash) = 32),
    family_id TEXT NOT NULL REFERENCES oauth_refresh_families(family_id),
    issued_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER
);

CREATE INDEX oauth_refresh_tokens_family_id ON oauth_refresh_tokens(family_id);
CREATE INDEX oauth_refresh_tokens_expiry ON oauth_refresh_tokens(expires_at);

CREATE TABLE oauth_access_tokens (
    token_hash BLOB PRIMARY KEY CHECK(length(token_hash) = 32),
    family_id TEXT NOT NULL REFERENCES oauth_refresh_families(family_id),
    resource TEXT NOT NULL,
    scope TEXT NOT NULL CHECK(scope = 'driichi:play'),
    issued_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX oauth_access_tokens_family_id ON oauth_access_tokens(family_id);
CREATE INDEX oauth_access_tokens_expiry ON oauth_access_tokens(expires_at);
