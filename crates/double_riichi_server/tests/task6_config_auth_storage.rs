use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use double_riichi_server::{
    AdminAuthenticator, AdminSecrets, BotTokenAuthority, BotTokenService, ConfigError,
    CredentialError, RuntimeConfig, Storage, TokenState, hash_password, hash_password_for_cli,
    verify_password,
};
use serde_json::json;
use sqlx::Row;

fn temp_root(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("double-riichi-task6-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

fn write_config(root: &Path, body: &str) -> PathBuf {
    let path = root.join("nested/config.toml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, body).unwrap();
    path
}

#[test]
fn config_uses_config_parent_as_data_root_and_rejects_unknown_or_duplicate_fields() {
    let root = temp_root("config");
    let path = write_config(
        &root,
        r#"
            public_origin = "http://127.0.0.1:3000"
            [time_controls.casual]
            turn_seconds = 30
            response_seconds = 10
        "#,
    );
    let config = RuntimeConfig::from_path(&path).unwrap();
    assert_eq!(config.data_root(), path.parent().unwrap());
    assert_eq!(
        config.database_path(),
        path.parent().unwrap().join("double-riichi.db")
    );
    assert_eq!(config.replay_root(), path.parent().unwrap().join("replays"));
    assert_eq!(config.bind, "127.0.0.1:3000");

    let relative_name = format!("double-riichi-task6-relative-{}.toml", std::process::id());
    fs::write(
        &relative_name,
        "public_origin = \"http://127.0.0.1:3000\"\n",
    )
    .unwrap();
    let relative = RuntimeConfig::from_path(Path::new(&relative_name)).unwrap();
    assert_eq!(relative.data_root(), Path::new("."));
    let _ = fs::remove_file(&relative_name);

    let unknown = write_config(
        &root,
        "public_origin = \"http://127.0.0.1:3000\"\nnot_a_setting = true\n",
    );
    assert!(matches!(
        RuntimeConfig::from_path(&unknown),
        Err(ConfigError::Toml(_))
    ));

    let duplicate = write_config(
        &root,
        "public_origin = \"http://127.0.0.1:3000\"\npublic_origin = \"http://localhost:3000\"\n",
    );
    assert!(RuntimeConfig::from_path(&duplicate).is_err());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn config_enforces_integer_duration_bounds_and_strict_origin() {
    let root = temp_root("config-bounds");
    for (field, value) in [
        ("shutdown_seconds", "0"),
        ("unlimited_watchdog_seconds", "9"),
        ("empty_room_cleanup_seconds", "86401"),
    ] {
        let path = write_config(
            &root,
            &format!("public_origin = \"http://127.0.0.1:3000\"\n{field} = {value}\n"),
        );
        assert!(RuntimeConfig::from_path(&path).is_err(), "{field}={value}");
    }
    let fractional = write_config(
        &root,
        "public_origin = \"http://127.0.0.1:3000\"\nshutdown_seconds = 1.5\n",
    );
    assert!(RuntimeConfig::from_path(&fractional).is_err());
    let bad_origin = write_config(&root, "public_origin = \"not-an-origin\"\n");
    assert!(RuntimeConfig::from_path(&bad_origin).is_err());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn env_is_required_rooted_and_does_not_use_ambient_values_or_duplicate_keys() {
    let root = temp_root("env");
    let hash = hash_password("correct horse battery staple").unwrap();
    fs::write(
        root.join(".env"),
        format!("ADMIN_USERNAME=admin\nADMIN_PASSWORD_HASH={hash}\n"),
    )
    .unwrap();
    let secrets = AdminSecrets::load(&root).unwrap();
    assert_eq!(secrets.username(), "admin");
    assert!(
        secrets.password_hash().starts_with("$argon2id$")
            || secrets.password_hash().starts_with("$argon2")
    );
    assert!(!format!("{secrets:?}").contains(&hash));

    fs::write(
        root.join(".env"),
        format!("ADMIN_USERNAME=admin\nADMIN_USERNAME=other\nADMIN_PASSWORD_HASH={hash}\n"),
    )
    .unwrap();
    assert!(AdminSecrets::load(&root).is_err());

    fs::write(
        root.join(".env"),
        format!("ADMIN_USERNAME=\nADMIN_PASSWORD_HASH={hash}\n"),
    )
    .unwrap();
    assert!(AdminSecrets::load(&root).is_err());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn argon2id_passwords_use_required_policy_and_generic_verification_failures() {
    let hash = hash_password("correct horse battery staple").unwrap();
    assert!(hash.starts_with("$argon2id$v=19$m=65536,t=3,p=1$"));
    assert!(verify_password("correct horse battery staple", &hash).unwrap());
    assert!(!verify_password("wrong password", &hash).unwrap());
    assert!(hash_password("too-short").is_err());
    assert!(
        hash_password_for_cli("correct horse battery staple", "different")
            .unwrap()
            .is_none()
    );
    assert!(verify_password("anything", "not a PHC string").is_err());
}

#[test]
fn admin_sessions_are_hashed_fixed_lifetime_and_independently_revocable() {
    let hash = hash_password("correct horse battery staple").unwrap();
    let auth = AdminAuthenticator::new("admin", hash).unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(1_000);
    let first = auth
        .login("admin", "correct horse battery staple", now)
        .unwrap();
    let second = auth
        .login("admin", "correct horse battery staple", now)
        .unwrap();
    assert_ne!(
        first.credential().as_bytes(),
        second.credential().as_bytes()
    );
    assert!(
        auth.sessions()
            .validate(first.credential(), now + Duration::from_secs(60))
    );
    assert!(
        !auth
            .sessions()
            .validate(first.credential(), now + Duration::from_secs(12 * 60 * 60))
    );
    assert!(
        auth.sessions()
            .validate(second.credential(), now + Duration::from_secs(60))
    );
    auth.sessions().revoke(first.credential());
    assert!(
        !auth
            .sessions()
            .validate(first.credential(), now + Duration::from_secs(60))
    );
    assert!(
        auth.sessions()
            .validate(second.credential(), now + Duration::from_secs(60))
    );
    assert!(
        !auth
            .login("other", "correct horse battery staple", now)
            .is_ok()
    );
    assert!(matches!(
        auth.login("admin", "wrong password", now),
        Err(CredentialError::InvalidCredentials)
    ));
    assert!(!format!("{first:?}").contains(String::from_utf8_lossy(first.credential()).as_ref()));
}

#[tokio::test]
async fn sqlite_storage_applies_exact_pragmas_migrations_and_replay_paths() {
    let root = temp_root("storage");
    let storage = Storage::connect(&root).await.unwrap();
    assert_eq!(storage.max_connections(), 4);
    assert_eq!(
        storage
            .scalar_text("PRAGMA journal_mode")
            .await
            .unwrap()
            .to_lowercase(),
        "wal"
    );
    assert_eq!(storage.scalar_i64("PRAGMA foreign_keys").await.unwrap(), 1);
    assert_eq!(storage.scalar_i64("PRAGMA synchronous").await.unwrap(), 1);
    assert_eq!(
        storage.scalar_i64("PRAGMA busy_timeout").await.unwrap(),
        5_000
    );
    assert!(root.join("double-riichi.db").exists());
    for table in [
        "bot_tokens",
        "matches",
        "match_players",
        "replay_auxiliary_events",
        "audit_logs",
    ] {
        let exists: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(table)
        .fetch_one(storage.pool())
        .await
        .unwrap();
        assert_eq!(exists, 1, "missing {table}");
    }
    for source in ["validate", "compat"] {
        assert!(sqlx::query(
            "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, status) VALUES (?, ?, NULL, '4p-red-east', 1, 'writing')",
        )
        .bind(format!("NON-PERSISTENT-{source}"))
        .bind(source)
        .execute(storage.pool())
        .await
        .is_err());
    }
    let path = storage.resolve_replay_path("4p/replay.mjson").unwrap();
    assert!(path.starts_with(storage.replay_root()));
    assert!(storage.resolve_replay_path("../outside.mjson").is_err());
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn startup_cleanup_removes_unfinished_rows_and_files_but_keeps_completed_rows() {
    let root = temp_root("startup-cleanup");
    let storage = Storage::connect(&root).await.unwrap();
    sqlx::query(
        "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, status) VALUES (?, 'room', NULL, '4p-red-east', 1, 'writing')",
    )
    .bind("UNFINISHED")
    .execute(storage.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO matches (match_id, source, room_name, game_mode, started_at, completed_at, status, replay_path, file_size) VALUES (?, 'room', NULL, '4p-red-east', 1, 2, 'completed', '4p/done.mjson', 2)",
    )
    .bind("COMPLETE")
    .execute(storage.pool())
    .await
    .unwrap();
    fs::write(
        storage
            .replay_root()
            .join("4p/20260915T000000Z_4p-red-east_UNFINISHED.mjson"),
        "partial",
    )
    .unwrap();
    fs::write(storage.replay_root().join("4p/done.mjson"), "done").unwrap();
    fs::write(
        storage
            .replay_root()
            .join(".incomplete/UNFINISHED.mjson.part"),
        "partial",
    )
    .unwrap();
    storage.startup_cleanup().await.unwrap();
    let unfinished: i64 =
        sqlx::query_scalar("SELECT count(*) FROM matches WHERE match_id = 'UNFINISHED'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(unfinished, 0);
    assert!(
        !storage
            .replay_root()
            .join("4p/20260915T000000Z_4p-red-east_UNFINISHED.mjson")
            .exists()
    );
    assert!(
        !storage
            .replay_root()
            .join(".incomplete/UNFINISHED.mjson.part")
            .exists()
    );
    assert!(storage.replay_root().join("4p/done.mjson").exists());
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn bot_tokens_are_presented_once_hashed_cached_and_globally_revoked_after_commit() {
    let root = temp_root("tokens");
    let storage = std::sync::Arc::new(Storage::connect(&root).await.unwrap());
    let authority = std::sync::Arc::new(BotTokenAuthority::empty());
    let service = BotTokenService::new(storage.clone(), authority.clone());
    let normalized = service
        .create(" \u{2003}driichi_runner\u{2003} ", 50, "req-normalize")
        .await
        .unwrap();
    assert_eq!(normalized.record().name(), "driichi_runner");
    let created = service.create("runner", 100, "req-create").await.unwrap();
    let raw = created.secret().expose().to_owned();
    assert!(raw.starts_with("driichi_"));
    assert_eq!(raw.trim_start_matches("driichi_").len(), 43);
    assert!(format!("{created:?}").contains("[REDACTED]"));
    assert!(!format!("{created:?}").contains(&raw));
    assert!(authority.authenticate(&raw).is_ok());
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT token_hash FROM bot_tokens WHERE token_id = ?")
            .bind(created.record().token_id())
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(
        stored.as_slice(),
        double_riichi_server::hash_token(&raw).as_slice()
    );
    assert!(!format!("{:?}", created.record()).contains(&raw));
    let mut revocations = authority.subscribe_revocations();
    service
        .revoke(created.record().token_id(), 200, "req-revoke")
        .await
        .unwrap();
    assert!(matches!(
        authority.authenticate(&raw),
        Err(CredentialError::InvalidCredentials)
    ));
    assert_eq!(
        revocations.recv().await.unwrap().token_id(),
        created.record().token_id()
    );
    assert!(matches!(
        service
            .revoke(created.record().token_id(), 300, "req-revoke-again")
            .await,
        Err(CredentialError::AlreadyRevoked)
    ));
    let request_id: String = sqlx::query_scalar(
        "SELECT request_id FROM audit_logs WHERE action = 'token_create' AND target_id = ?",
    )
    .bind(created.record().token_id())
    .fetch_one(storage.pool())
    .await
    .unwrap();
    assert_eq!(request_id, "req-create");
    let request_id: String = sqlx::query_scalar(
        "SELECT request_id FROM audit_logs WHERE action = 'token_revoke' AND target_id = ?",
    )
    .bind(created.record().token_id())
    .fetch_one(storage.pool())
    .await
    .unwrap();
    assert_eq!(request_id, "req-revoke");
    let state: String = sqlx::query_scalar("SELECT state FROM bot_tokens WHERE token_id = ?")
        .bind(created.record().token_id())
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(state, "revoked");
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn bot_token_audit_insert_failures_are_atomic_and_reopen_counts_once() {
    let root = temp_root("token-audit-failures");
    let storage = std::sync::Arc::new(Storage::connect(&root).await.unwrap());
    let authority = std::sync::Arc::new(BotTokenAuthority::empty());
    let service = BotTokenService::new(storage.clone(), authority.clone());
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    sqlx::query(
        "CREATE TRIGGER task6_fail_token_create_audit BEFORE INSERT ON audit_logs WHEN NEW.action = 'token_create' BEGIN SELECT RAISE(ABORT, 'forced token create audit failure'); END",
    )
    .execute(storage.pool())
    .await
    .unwrap();
    assert!(matches!(
        service
            .create("create-fails", now, "req-create-fails")
            .await,
        Err(CredentialError::Storage)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM bot_tokens")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'token_create'"
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER task6_fail_token_create_audit")
        .execute(storage.pool())
        .await
        .unwrap();

    let created = service
        .create("runner", now + 1, "req-create")
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER task6_fail_token_revoke_audit BEFORE INSERT ON audit_logs WHEN NEW.action = 'token_revoke' BEGIN SELECT RAISE(ABORT, 'forced token revoke audit failure'); END",
    )
    .execute(storage.pool())
    .await
    .unwrap();
    assert!(matches!(
        service
            .revoke(created.record().token_id(), now + 2, "req-revoke-fails")
            .await,
        Err(CredentialError::Storage)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM bot_tokens WHERE token_id = ?")
            .bind(created.record().token_id())
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        "active"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'token_revoke'"
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER task6_fail_token_revoke_audit")
        .execute(storage.pool())
        .await
        .unwrap();

    service
        .revoke(created.record().token_id(), now + 3, "req-revoke")
        .await
        .unwrap();
    storage.close().await;
    drop(service);
    drop(authority);
    drop(storage);

    let reopened = Storage::connect(&root).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'token_create'"
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'token_revoke'"
        )
        .fetch_one(reopened.pool())
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_logs")
            .fetch_one(reopened.pool())
            .await
            .unwrap(),
        2
    );
    reopened.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn audit_summaries_are_allowlisted_and_ninety_day_cleanup_is_retryable() {
    let root = temp_root("audit");
    let storage = Storage::connect(&root).await.unwrap();
    storage
        .insert_audit(
            1,
            "req-1",
            "token_create",
            "bot_token",
            "TOKEN-ID",
            json!({"name": "driichi_runner"}),
        )
        .await
        .unwrap();
    storage
        .insert_audit(
            90 * 24 * 60 * 60 + 1,
            "req-2",
            "room_create",
            "room",
            "ROOM-ID",
            json!({"room_name": "Lobby"}),
        )
        .await
        .unwrap();
    assert!(
        storage
            .insert_audit(
                2,
                "req-3",
                "token_create",
                "bot_token",
                "TOKEN-ID",
                json!({"raw_token": "driichi_secret"}),
            )
            .await
            .is_err()
    );
    assert!(
        storage
            .insert_audit(
                3,
                "req-4",
                "room_configure",
                "room",
                "ROOM-ID",
                json!({"changed_fields": {"Authorization": "Bearer driichi_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}}),
            )
            .await
            .is_err()
    );
    let removed = storage.cleanup_audit(90 * 24 * 60 * 60 + 1).await.unwrap();
    assert_eq!(removed, 1);
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_logs")
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(remaining, 1);
    let detail: String = sqlx::query("SELECT summary_json FROM audit_logs")
        .fetch_one(storage.pool())
        .await
        .unwrap()
        .get(0);
    assert!(!detail.contains("token"));
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn bot_token_authority_reload_preserves_revoked_state_and_emits_seat_signal() {
    let root = temp_root("token-reload");
    let storage = std::sync::Arc::new(Storage::connect(&root).await.unwrap());
    let authority = std::sync::Arc::new(BotTokenAuthority::empty());
    let service = BotTokenService::new(storage.clone(), authority.clone());
    let created = service.create("runner", 100, "req-create").await.unwrap();
    let mut revocations = authority.subscribe_revocations();
    service
        .revoke(created.record().token_id(), 200, "req-revoke")
        .await
        .unwrap();
    let event = revocations.recv().await.unwrap();
    assert_eq!(event.token_id(), created.record().token_id());
    assert_eq!(event.state(), TokenState::Revoked);
    let loaded = storage.load_bot_tokens().await.unwrap();
    let reloaded = BotTokenAuthority::from_records(loaded);
    assert!(matches!(
        reloaded.authenticate(created.secret().expose()),
        Err(CredentialError::InvalidCredentials)
    ));
    storage.close().await;
    let _ = fs::remove_dir_all(root);
}

#[test]
fn chatgpt_oauth_is_opt_in_and_requires_trusted_https_configuration() {
    let root = temp_root("chatgpt-oauth-config");
    let disabled_path = write_config(&root, "public_origin = \"http://127.0.0.1:3000\"\n");
    let disabled = RuntimeConfig::from_path(&disabled_path).unwrap();
    assert!(disabled.chatgpt_oauth.is_none());

    let valid_oauth = r#"
        enabled = true
        client_id = "https://chatgpt.com/oauth/client.json"
        redirect_uri = "https://chatgpt.com/connector_platform_oauth_redirect"
        allowed_origins = ["https://chatgpt.com"]
    "#;
    let enabled_path = write_config(
        &root,
        &format!("public_origin = \"https://driichi.com\"\n[chatgpt_oauth]\n{valid_oauth}"),
    );
    let enabled = RuntimeConfig::from_path(&enabled_path)
        .unwrap()
        .chatgpt_oauth
        .unwrap();
    assert_eq!(enabled.resource.as_str(), "https://driichi.com/chatgpt/mcp");
    assert_eq!(enabled.issuer_identifier(), "https://driichi.com");
    assert_eq!(
        enabled.client_id.as_str(),
        "https://chatgpt.com/oauth/client.json"
    );
    assert_eq!(
        enabled.redirect_uri.as_str(),
        "https://chatgpt.com/connector_platform_oauth_redirect"
    );
    assert_eq!(enabled.allowed_origins.len(), 1);

    let canonical_oauth = valid_oauth.replace(
        "allowed_origins = [\"https://chatgpt.com\"]",
        "allowed_origins = [\"https://chatgpt.com/\"]",
    );
    let canonical_path = write_config(
        &root,
        &format!("public_origin = \"https://driichi.com\"\n[chatgpt_oauth]\n{canonical_oauth}"),
    );
    assert!(
        RuntimeConfig::from_path(&canonical_path)
            .unwrap()
            .chatgpt_oauth
            .is_some()
    );

    for public_origin in [
        "http://driichi.com",
        "https://localhost",
        "https://driichi.localhost",
        "https://127.0.0.1",
        "https://10.2.3.4",
        "https://192.168.1.10",
        "https://[::1]",
        "https://[::ffff:127.0.0.1]",
    ] {
        let path = write_config(
            &root,
            &format!("public_origin = \"{public_origin}\"\n[chatgpt_oauth]\n{valid_oauth}"),
        );
        assert!(RuntimeConfig::from_path(&path).is_err(), "{public_origin}");
    }

    for invalid in [
        valid_oauth.replace(
            "https://chatgpt.com/oauth/client.json",
            "https://localhost/oauth/client.json",
        ),
        valid_oauth.replace(
            "https://chatgpt.com/oauth/client.json",
            "https://127.0.0.1/oauth/client.json",
        ),
        valid_oauth.replace(
            "https://chatgpt.com/oauth/client.json",
            "https://untrusted.example/oauth/client.json",
        ),
        valid_oauth.replace(
            "https://chatgpt.com/connector_platform_oauth_redirect",
            "http://chatgpt.com/connector_platform_oauth_redirect",
        ),
        valid_oauth.replace(
            "https://chatgpt.com/connector_platform_oauth_redirect",
            "https://chatgpt.com/connector_platform_oauth_redirect#fragment",
        ),
        valid_oauth.replace(
            "https://chatgpt.com/oauth/client.json",
            "https://user@chatgpt.com/oauth/client.json",
        ),
        valid_oauth.replace(
            "https://chatgpt.com/oauth/client.json",
            "https://chatgpt.com.evil.example/oauth/client.json",
        ),
        valid_oauth.replace(
            "allowed_origins = [\"https://chatgpt.com\"]",
            "allowed_origins = [\"http://chatgpt.com\"]",
        ),
    ] {
        let path = write_config(
            &root,
            &format!("public_origin = \"https://driichi.com\"\n[chatgpt_oauth]\n{invalid}"),
        );
        assert!(RuntimeConfig::from_path(&path).is_err(), "{invalid}");
    }

    let _ = fs::remove_dir_all(root);
}
