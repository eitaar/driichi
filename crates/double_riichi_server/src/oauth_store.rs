use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use thiserror::Error;

use crate::{ChatgptOAuthConfig, Storage};

const ACCESS_TOKEN_LIFETIME_SECONDS: i64 = 10 * 60;
const AUTHORIZATION_CODE_LIFETIME_SECONDS: i64 = 5 * 60;
const REFRESH_FAMILY_LIFETIME_SECONDS: i64 = 30 * 24 * 60 * 60;
const OAUTH_SCOPE: &str = "driichi:play";
const ADMIN_SUBJECT: &str = "admin";

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum OAuthError {
    #[error("invalid OAuth grant")]
    InvalidGrant,
    #[error("OAuth access token has insufficient scope")]
    InsufficientScope,
    #[error("OAuth storage is unavailable")]
    Storage,
}

pub(crate) struct CodeExchange {
    pub(crate) client_id: String,
    pub(crate) redirect_uri: String,
    pub(crate) code: String,
    pub(crate) verifier: String,
    pub(crate) resource: String,
}

pub(crate) struct RefreshExchange {
    pub(crate) client_id: String,
    pub(crate) refresh_token: String,
    pub(crate) resource: String,
}

#[derive(Serialize)]
pub(crate) struct TokenPair {
    pub(crate) access_token: String,
    pub(crate) token_type: &'static str,
    pub(crate) expires_in: u64,
    pub(crate) refresh_token: String,
    pub(crate) scope: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AccessGrant {
    pub(crate) client_id: String,
    pub(crate) subject: String,
    pub(crate) resource: String,
    pub(crate) scope: String,
}

#[derive(Clone)]
pub(crate) struct OAuthService {
    store: OAuthStore,
    client_id: String,
    redirect_uri: String,
    resource: String,
}

impl OAuthService {
    pub(crate) fn new(config: ChatgptOAuthConfig, storage: Arc<Storage>) -> Self {
        Self {
            store: OAuthStore::new(storage.pool().clone()),
            client_id: config.client_id.to_string(),
            redirect_uri: config.redirect_uri.to_string(),
            resource: config.resource.to_string(),
        }
    }

    pub(crate) async fn issue_authorization_code(
        &self,
        client_id: &str,
        redirect_uri: &str,
        resource: &str,
        scope: &str,
        pkce_challenge: &str,
    ) -> Result<String, OAuthError> {
        if client_id != self.client_id
            || redirect_uri != self.redirect_uri
            || resource != self.resource
            || scope != OAUTH_SCOPE
            || !valid_s256_challenge(pkce_challenge)
        {
            return Err(OAuthError::InvalidGrant);
        }

        let code = new_secret("code");
        let issued_at = now_unix_seconds();
        self.store
            .insert_code(
                &code,
                client_id,
                redirect_uri,
                resource,
                scope,
                pkce_challenge,
                issued_at,
                issued_at + AUTHORIZATION_CODE_LIFETIME_SECONDS,
            )
            .await?;
        Ok(code)
    }

    pub(crate) async fn exchange_code(
        &self,
        exchange: CodeExchange,
    ) -> Result<TokenPair, OAuthError> {
        if exchange.client_id != self.client_id
            || exchange.redirect_uri != self.redirect_uri
            || exchange.resource != self.resource
            || !valid_code_verifier(&exchange.verifier)
        {
            return Err(OAuthError::InvalidGrant);
        }
        self.store.exchange_code(exchange).await
    }

    pub(crate) async fn rotate_refresh(
        &self,
        exchange: RefreshExchange,
    ) -> Result<TokenPair, OAuthError> {
        if exchange.client_id != self.client_id || exchange.resource != self.resource {
            return Err(OAuthError::InvalidGrant);
        }
        self.store.rotate_refresh(exchange).await
    }

    pub(crate) async fn validate_access(
        &self,
        token: &str,
        resource: &str,
        scope: &str,
    ) -> Result<AccessGrant, OAuthError> {
        if resource != self.resource {
            return Err(OAuthError::InvalidGrant);
        }
        let grant = self.store.validate_access(token, resource).await?;
        if scope != OAUTH_SCOPE || grant.scope != scope {
            return Err(OAuthError::InsufficientScope);
        }
        Ok(grant)
    }
}

#[derive(Clone)]
struct OAuthStore {
    pool: SqlitePool,
}

impl OAuthStore {
    fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn insert_code(
        &self,
        code: &str,
        client_id: &str,
        redirect_uri: &str,
        resource: &str,
        scope: &str,
        pkce_challenge: &str,
        issued_at: i64,
        expires_at: i64,
    ) -> Result<(), OAuthError> {
        sqlx::query(
            "INSERT INTO oauth_codes \
             (code_hash, client_id, redirect_uri, resource, scope, subject, pkce_challenge, issued_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(hash_secret(code))
        .bind(client_id)
        .bind(redirect_uri)
        .bind(resource)
        .bind(scope)
        .bind(ADMIN_SUBJECT)
        .bind(pkce_challenge)
        .bind(issued_at)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|_| OAuthError::Storage)?;
        Ok(())
    }

    async fn exchange_code(&self, exchange: CodeExchange) -> Result<TokenPair, OAuthError> {
        let now = now_unix_seconds();
        let mut transaction = self.pool.begin().await.map_err(|_| OAuthError::Storage)?;
        let row = sqlx::query(
            "UPDATE oauth_codes SET consumed_at = ? \
             WHERE code_hash = ? AND client_id = ? AND redirect_uri = ? AND resource = ? \
               AND subject = ? AND consumed_at IS NULL AND expires_at > ? \
             RETURNING scope, pkce_challenge",
        )
        .bind(now)
        .bind(hash_secret(&exchange.code))
        .bind(&exchange.client_id)
        .bind(&exchange.redirect_uri)
        .bind(&exchange.resource)
        .bind(ADMIN_SUBJECT)
        .bind(now)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| OAuthError::Storage)?;

        let Some(row) = row else {
            transaction
                .rollback()
                .await
                .map_err(|_| OAuthError::Storage)?;
            return Err(OAuthError::InvalidGrant);
        };
        let scope: String = row.try_get("scope").map_err(|_| OAuthError::Storage)?;
        let challenge: String = row
            .try_get("pkce_challenge")
            .map_err(|_| OAuthError::Storage)?;
        if scope != OAUTH_SCOPE || s256_challenge(&exchange.verifier) != challenge {
            transaction
                .rollback()
                .await
                .map_err(|_| OAuthError::Storage)?;
            return Err(OAuthError::InvalidGrant);
        }

        let family_id = new_family_id();
        let family_expires_at = now + REFRESH_FAMILY_LIFETIME_SECONDS;
        let pair = new_token_pair(now, family_expires_at);
        sqlx::query(
            "INSERT INTO oauth_refresh_families \
             (family_id, client_id, subject, resource, scope, issued_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&family_id)
        .bind(&exchange.client_id)
        .bind(ADMIN_SUBJECT)
        .bind(&exchange.resource)
        .bind(OAUTH_SCOPE)
        .bind(now)
        .bind(family_expires_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| OAuthError::Storage)?;
        insert_token_rows(
            &mut transaction,
            &family_id,
            &pair,
            &exchange.resource,
            now,
            family_expires_at,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|_| OAuthError::Storage)?;
        Ok(pair)
    }

    async fn rotate_refresh(&self, exchange: RefreshExchange) -> Result<TokenPair, OAuthError> {
        let now = now_unix_seconds();
        let mut transaction = self.pool.begin().await.map_err(|_| OAuthError::Storage)?;
        let token_hash = hash_secret(&exchange.refresh_token);

        // Make the conditional update the transaction's first statement. In
        // SQLite this reserves the writer before reading, so a concurrent
        // redemption waits and then observes the consumed token as a replay.
        let consumed = sqlx::query(
            "UPDATE oauth_refresh_tokens SET consumed_at = ? \
             WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ? \
               AND EXISTS ( \
                 SELECT 1 FROM oauth_refresh_families f \
                 WHERE f.family_id = oauth_refresh_tokens.family_id \
                   AND f.client_id = ? AND f.resource = ? AND f.scope = ? \
                   AND f.revoked_at IS NULL AND f.expires_at > ? \
               ) \
             RETURNING family_id",
        )
        .bind(now)
        .bind(&token_hash)
        .bind(now)
        .bind(&exchange.client_id)
        .bind(&exchange.resource)
        .bind(OAUTH_SCOPE)
        .bind(now)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| OAuthError::Storage)?;

        let Some(consumed) = consumed else {
            let family = sqlx::query(
                "SELECT f.family_id, f.client_id, f.resource, f.scope, f.expires_at, f.revoked_at, \
                        r.consumed_at, r.expires_at AS token_expires_at \
                 FROM oauth_refresh_tokens r \
                 JOIN oauth_refresh_families f ON f.family_id = r.family_id \
                 WHERE r.token_hash = ?",
            )
            .bind(&token_hash)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| OAuthError::Storage)?;
            let Some(family) = family else {
                transaction
                    .rollback()
                    .await
                    .map_err(|_| OAuthError::Storage)?;
                return Err(OAuthError::InvalidGrant);
            };

            let family_id: String = family
                .try_get("family_id")
                .map_err(|_| OAuthError::Storage)?;
            let client_id: String = family
                .try_get("client_id")
                .map_err(|_| OAuthError::Storage)?;
            let resource: String = family
                .try_get("resource")
                .map_err(|_| OAuthError::Storage)?;
            let scope: String = family.try_get("scope").map_err(|_| OAuthError::Storage)?;
            let family_expires_at: i64 = family
                .try_get("expires_at")
                .map_err(|_| OAuthError::Storage)?;
            let revoked_at: Option<i64> = family
                .try_get("revoked_at")
                .map_err(|_| OAuthError::Storage)?;
            let consumed_at: Option<i64> = family
                .try_get("consumed_at")
                .map_err(|_| OAuthError::Storage)?;
            let token_expires_at: i64 = family
                .try_get("token_expires_at")
                .map_err(|_| OAuthError::Storage)?;

            if client_id != exchange.client_id
                || resource != exchange.resource
                || scope != OAUTH_SCOPE
            {
                transaction
                    .rollback()
                    .await
                    .map_err(|_| OAuthError::Storage)?;
                return Err(OAuthError::InvalidGrant);
            }
            if consumed_at.is_some() {
                sqlx::query(
                    "UPDATE oauth_refresh_families SET revoked_at = COALESCE(revoked_at, ?) WHERE family_id = ?",
                )
                .bind(now)
                .bind(&family_id)
                .execute(&mut *transaction)
                .await
                .map_err(|_| OAuthError::Storage)?;
                transaction
                    .commit()
                    .await
                    .map_err(|_| OAuthError::Storage)?;
                return Err(OAuthError::InvalidGrant);
            }
            if revoked_at.is_some() || family_expires_at <= now || token_expires_at <= now {
                transaction
                    .rollback()
                    .await
                    .map_err(|_| OAuthError::Storage)?;
                return Err(OAuthError::InvalidGrant);
            }

            transaction
                .rollback()
                .await
                .map_err(|_| OAuthError::Storage)?;
            return Err(OAuthError::InvalidGrant);
        };

        let family_id: String = consumed
            .try_get("family_id")
            .map_err(|_| OAuthError::Storage)?;
        let family =
            sqlx::query("SELECT expires_at FROM oauth_refresh_families WHERE family_id = ?")
                .bind(&family_id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|_| OAuthError::Storage)?;
        let Some(family) = family else {
            transaction
                .rollback()
                .await
                .map_err(|_| OAuthError::Storage)?;
            return Err(OAuthError::InvalidGrant);
        };
        let family_expires_at: i64 = family
            .try_get("expires_at")
            .map_err(|_| OAuthError::Storage)?;

        let pair = new_token_pair(now, family_expires_at);
        insert_token_rows(
            &mut transaction,
            &family_id,
            &pair,
            &exchange.resource,
            now,
            family_expires_at,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|_| OAuthError::Storage)?;
        Ok(pair)
    }

    async fn validate_access(
        &self,
        token: &str,
        resource: &str,
    ) -> Result<AccessGrant, OAuthError> {
        let now = now_unix_seconds();
        let row = sqlx::query(
            "SELECT f.client_id, f.subject, f.scope AS family_scope, a.resource, a.scope \
             FROM oauth_access_tokens a \
             JOIN oauth_refresh_families f ON f.family_id = a.family_id \
             WHERE a.token_hash = ? AND a.resource = ? \
               AND a.expires_at > ? AND f.expires_at > ? AND f.revoked_at IS NULL",
        )
        .bind(hash_secret(token))
        .bind(resource)
        .bind(now)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| OAuthError::Storage)?;
        let Some(row) = row else {
            return Err(OAuthError::InvalidGrant);
        };
        let scope: String = row.try_get("scope").map_err(|_| OAuthError::Storage)?;
        let family_scope: String = row.try_get("family_scope").map_err(|_| OAuthError::Storage)?;
        if scope != family_scope {
            return Err(OAuthError::InvalidGrant);
        }
        Ok(AccessGrant {
            client_id: row.try_get("client_id").map_err(|_| OAuthError::Storage)?,
            subject: row.try_get("subject").map_err(|_| OAuthError::Storage)?,
            resource: row.try_get("resource").map_err(|_| OAuthError::Storage)?,
            scope,
        })
    }
}

async fn insert_token_rows(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    family_id: &str,
    pair: &TokenPair,
    resource: &str,
    issued_at: i64,
    refresh_expires_at: i64,
) -> Result<(), OAuthError> {
    sqlx::query(
        "INSERT INTO oauth_refresh_tokens (token_hash, family_id, issued_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(hash_secret(&pair.refresh_token))
    .bind(family_id)
    .bind(issued_at)
    .bind(refresh_expires_at)
    .execute(&mut **transaction)
    .await
    .map_err(|_| OAuthError::Storage)?;
    sqlx::query(
        "INSERT INTO oauth_access_tokens (token_hash, family_id, resource, scope, issued_at, expires_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(hash_secret(&pair.access_token))
    .bind(family_id)
    .bind(resource)
    .bind(OAUTH_SCOPE)
    .bind(issued_at)
    .bind(access_expires_at(issued_at, refresh_expires_at))
    .execute(&mut **transaction)
    .await
    .map_err(|_| OAuthError::Storage)?;
    Ok(())
}

fn new_token_pair(issued_at: i64, family_expires_at: i64) -> TokenPair {
    TokenPair {
        access_token: new_secret("access"),
        token_type: "Bearer",
        expires_in: (access_expires_at(issued_at, family_expires_at) - issued_at) as u64,
        refresh_token: new_secret("refresh"),
        scope: OAUTH_SCOPE,
    }
}

fn access_expires_at(issued_at: i64, family_expires_at: i64) -> i64 {
    (issued_at + ACCESS_TOKEN_LIFETIME_SECONDS).min(family_expires_at)
}

fn new_secret(kind: &str) -> String {
    let random = rand::random::<[u8; 32]>();
    format!("driichi_{kind}_{}", URL_SAFE_NO_PAD.encode(random))
}

fn new_family_id() -> String {
    URL_SAFE_NO_PAD.encode(rand::random::<[u8; 16]>())
}

fn hash_secret(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

fn s256_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn valid_s256_challenge(challenge: &str) -> bool {
    challenge.len() == 43
        && challenge
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_code_verifier(verifier: &str) -> bool {
    (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
