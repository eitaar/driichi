use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{body::Body, http::Request};
use double_riichi_server::{
    CharacterRegistry, CharacterRequirements, CharacterUsage, RuntimeConfig, character_router,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const VOICES: [&str; 6] = ["chi", "pon", "kan", "riichi", "ron", "tsumo"];

fn temp_root(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("double-riichi-task7-{name}-{suffix}"));
    fs::create_dir_all(&root).unwrap();
    root
}

fn webp() -> Vec<u8> {
    let mut bytes = b"RIFF\0\0\0\0WEBP".to_vec();
    bytes.extend_from_slice(b"starter");
    bytes
}

fn ogg() -> Vec<u8> {
    b"OggS\0starter".to_vec()
}

fn write_pack(root: &Path, id: &str, usage: &str, name: &str) {
    let pack = root.join(id);
    fs::create_dir_all(pack.join("voices")).unwrap();
    fs::write(
        pack.join("manifest.json"),
        format!(r#"{{"id":"{id}","name":"{name}","usage":"{usage}"}}"#),
    )
    .unwrap();
    fs::write(pack.join("LICENSE"), "CC0 1.0 Universal\n").unwrap();
    fs::write(pack.join("portrait.webp"), webp()).unwrap();
    fs::write(pack.join("icon.webp"), webp()).unwrap();
    for voice in VOICES {
        fs::write(pack.join("voices").join(format!("{voice}.ogg")), ogg()).unwrap();
    }
}

fn write_all_starter_packs(root: &Path) {
    write_pack(root, "player-red", "human", "Player Red");
    write_pack(root, "player-blue", "human", "Player Blue");
    write_pack(root, "mjai-bot", "mjai", "MJAI Bot");
    write_pack(root, "tsumogiri-bot", "builtin", "Tsumogiri Bot");
    write_pack(root, "mcp-agent", "mcp", "MCP Agent");
}

async fn body(response: axum::response::Response) -> Vec<u8> {
    axum::body::to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn registry_rejects_unknown_manifest_fields_and_bad_ids() {
    let root = temp_root("strict-manifest");
    write_pack(&root, "safe-pack", "human", "Safe");
    let manifest = root.join("safe-pack/manifest.json");
    fs::write(
        &manifest,
        r#"{"id":"safe-pack","name":"Safe","usage":"human","extra":true}"#,
    )
    .unwrap();
    assert!(CharacterRegistry::scan(&root).is_err());

    fs::write(
        &manifest,
        r#"{"id":"safe-pack","name":"Safe","name":"Other","usage":"human"}"#,
    )
    .unwrap();
    assert!(CharacterRegistry::scan(&root).is_err());

    fs::remove_dir_all(&root).unwrap();
    fs::create_dir_all(&root).unwrap();
    write_pack(&root, "Bad", "human", "Bad");
    assert!(CharacterRegistry::scan(&root).is_err());
    fs::remove_dir_all(root).unwrap();

    let root = temp_root("duplicate-id");
    write_pack(&root, "one", "human", "One");
    write_pack(&root, "two", "human", "Two");
    fs::write(
        root.join("two/manifest.json"),
        r#"{"id":"one","name":"Two","usage":"human"}"#,
    )
    .unwrap();
    assert!(CharacterRegistry::scan(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_requires_complete_packs_and_validates_content_headers_and_limits() {
    let root = temp_root("limits");
    write_pack(&root, "safe-pack", "human", "Safe");
    fs::remove_file(root.join("safe-pack/voices/ron.ogg")).unwrap();
    assert!(CharacterRegistry::scan(&root).is_err());

    fs::write(root.join("safe-pack/voices/ron.ogg"), b"not-ogg").unwrap();
    assert!(CharacterRegistry::scan(&root).is_err());

    fs::write(root.join("safe-pack/voices/ron.ogg"), ogg()).unwrap();
    fs::write(root.join("safe-pack/icon.webp"), b"not-webp").unwrap();
    assert!(CharacterRegistry::scan(&root).is_err());

    fs::write(root.join("safe-pack/icon.webp"), webp()).unwrap();
    fs::write(root.join("safe-pack/LICENSE"), vec![b'x'; 1_048_577]).unwrap();
    assert!(CharacterRegistry::scan(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_accepts_header_only_assets_without_decoding_dimensions_or_duration() {
    let root = temp_root("headers-only");
    write_pack(&root, "safe-pack", "human", "Safe");
    let registry = CharacterRegistry::scan(&root).unwrap();
    assert!(
        registry
            .asset("safe-pack", double_riichi_server::CharacterAsset::Icon)
            .is_some()
    );
    assert!(
        registry
            .asset(
                "safe-pack",
                double_riichi_server::CharacterAsset::Voice(
                    double_riichi_server::VoiceLine::Riichi
                )
            )
            .is_some()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_ignores_broken_unreferenced_packs_but_requires_valid_humans() {
    let root = temp_root("unreferenced");
    write_pack(&root, "safe-pack", "human", "Safe");
    fs::create_dir_all(root.join("broken")).unwrap();
    fs::write(
        root.join("broken/manifest.json"),
        r#"{"id":"broken","name":"Broken","usage":"human","unknown":true}"#,
    )
    .unwrap();
    let registry = CharacterRegistry::scan(&root).unwrap();
    assert_eq!(registry.human_characters().len(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn runtime_config_loads_character_registry_from_its_parent_data_root() {
    let root = temp_root("config-root");
    let config_path = root.join("nested/config.toml");
    fs::create_dir_all(config_path.parent().unwrap().join("character-packs")).unwrap();
    write_all_starter_packs(&config_path.parent().unwrap().join("character-packs"));
    fs::write(
        &config_path,
        "public_origin = \"http://127.0.0.1:3000\"\n[characters]\nmjai = \"mjai-bot\"\nbuiltin = \"tsumogiri-bot\"\nmcp = \"mcp-agent\"\n",
    )
    .unwrap();
    let config = RuntimeConfig::from_path(&config_path).unwrap();
    assert_eq!(
        config.character_pack_root(),
        config_path.parent().unwrap().join("character-packs")
    );
    assert_eq!(
        config
            .load_character_registry()
            .unwrap()
            .human_characters()
            .len(),
        2
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_enforces_required_usage_and_does_not_disclose_filesystem_paths() {
    let root = temp_root("requirements");
    write_all_starter_packs(&root);
    let requirements = CharacterRequirements::new(
        "mjai-bot",
        "tsumogiri-bot",
        "mcp-agent",
        [("provider", "mcp-agent")],
    )
    .unwrap();
    let registry = CharacterRegistry::load(&root, &requirements).unwrap();
    assert_eq!(registry.human_characters().len(), 2);
    assert_eq!(
        registry.character("player-red").unwrap().usage(),
        CharacterUsage::Human
    );

    let bad = CharacterRequirements::new(
        "player-red",
        "tsumogiri-bot",
        "mcp-agent",
        std::iter::empty::<(&str, &str)>(),
    )
    .unwrap();
    let error = CharacterRegistry::load(&root, &bad).unwrap_err();
    assert!(!error.to_string().contains(root.to_string_lossy().as_ref()));
    assert!(
        CharacterRequirements::new(
            "mjai-bot",
            "tsumogiri-bot",
            "mcp-agent",
            [(" provider ", "mcp-agent")],
        )
        .is_err()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn config_rejects_unknown_character_fields_and_invalid_character_mappings() {
    let root = temp_root("config-validation");
    let unknown = root.join("unknown.toml");
    fs::write(
        &unknown,
        "public_origin = \"http://127.0.0.1:3000\"\n[characters]\nextra = \"pack\"\n",
    )
    .unwrap();
    assert!(RuntimeConfig::from_path(&unknown).is_err());

    let invalid_id = root.join("invalid-id.toml");
    fs::write(
        &invalid_id,
        "public_origin = \"http://127.0.0.1:3000\"\n[characters]\nmjai = \"../outside\"\n",
    )
    .unwrap();
    assert!(RuntimeConfig::from_path(&invalid_id).is_err());

    let invalid_provider = root.join("invalid-provider.toml");
    fs::write(
        &invalid_provider,
        "public_origin = \"http://127.0.0.1:3000\"\n[characters.mcp_providers]\n\"Provider\" = \"mcp-agent\"\n",
    )
    .unwrap();
    assert!(RuntimeConfig::from_path(&invalid_provider).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(windows)]
fn registry_rejects_windows_hardlink_aliases() {
    let root = temp_root("windows-hardlink-aliases");
    write_pack(&root, "one", "human", "One");
    write_pack(&root, "two", "human", "Two");
    fs::remove_file(root.join("two/portrait.webp")).unwrap();
    fs::hard_link(
        root.join("one/portrait.webp"),
        root.join("two/portrait.webp"),
    )
    .unwrap();
    assert!(CharacterRegistry::scan(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(unix)]
fn registry_rejects_canonical_path_aliases_and_symlink_escape() {
    use std::os::unix::fs::symlink;

    let root = temp_root("paths");
    write_pack(&root, "safe-pack", "human", "Safe");
    let outside = temp_root("outside");
    fs::write(outside.join("portrait.webp"), webp()).unwrap();
    fs::remove_file(root.join("safe-pack/portrait.webp")).unwrap();
    symlink(
        outside.join("portrait.webp"),
        root.join("safe-pack/portrait.webp"),
    )
    .unwrap();
    assert!(CharacterRegistry::scan(&root).is_err());
    fs::remove_dir_all(&root).unwrap();
    fs::remove_dir_all(outside).unwrap();

    let root = temp_root("aliases");
    write_pack(&root, "one", "human", "One");
    write_pack(&root, "two", "human", "Two");
    fs::remove_file(root.join("two/portrait.webp")).unwrap();
    symlink(
        root.join("one/portrait.webp"),
        root.join("two/portrait.webp"),
    )
    .unwrap();
    assert!(CharacterRegistry::scan(&root).is_err());
    fs::remove_dir_all(root).unwrap();

    let root = temp_root("hardlink-aliases");
    write_pack(&root, "one", "human", "One");
    write_pack(&root, "two", "human", "Two");
    fs::remove_file(root.join("two/portrait.webp")).unwrap();
    fs::hard_link(
        root.join("one/portrait.webp"),
        root.join("two/portrait.webp"),
    )
    .unwrap();
    assert!(CharacterRegistry::scan(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn character_routes_only_serve_allowlisted_assets_with_etags_and_listing_metadata() {
    let root = temp_root("routes");
    write_pack(&root, "safe-pack", "human", "Safe");
    let registry = Arc::new(CharacterRegistry::scan(&root).unwrap());
    let app = character_router(registry);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/characters/human")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "application/json");
    let listing: Value = serde_json::from_slice(&body(response).await).unwrap();
    assert_eq!(
        listing,
        serde_json::json!([{"id":"safe-pack","name":"Safe"}])
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/assets/characters/safe-pack/icon.webp")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "image/webp");
    assert_eq!(response.headers()["cache-control"], "public, no-cache");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    let etag = response.headers()["etag"].to_str().unwrap().to_owned();
    assert!(etag.starts_with('"') && etag.ends_with('"'));
    assert_eq!(body(response).await, webp());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/assets/characters/safe-pack/icon.webp")
                .header("if-none-match", &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 304);
    assert!(body(response).await.is_empty());

    for uri in [
        "/assets/characters/safe-pack/LICENSE",
        "/assets/characters/safe-pack/manifest.json",
        "/assets/characters/safe-pack/voices/nope.ogg",
        "/assets/characters/safe-pack/%2e%2e/LICENSE",
        "/assets/characters/safe-pack/voices/%2e%2e/LICENSE",
        "/assets/characters/safe-pack/icon.webp%2f..%2fLICENSE",
        "/assets/characters/unknown/icon.webp",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 404, "{uri}");
    }
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn character_voice_routes_have_ogg_type_and_unknown_ids_never_touch_disk() {
    let root = temp_root("voice-routes");
    write_pack(&root, "safe-pack", "human", "Safe");
    let registry = Arc::new(CharacterRegistry::scan(&root).unwrap());
    let app = character_router(registry);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/assets/characters/safe-pack/voices/riichi.ogg")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "audio/ogg");
    assert_eq!(body(response).await, ogg());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn starter_generator_is_deterministic_and_covers_every_required_pack_asset() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = repo.join("scripts/generate_starter_packs.py");
    assert!(script.is_file());
    let first = temp_root("generated-first");
    let second = temp_root("generated-second");
    let first_zip = first.with_extension("zip");
    let second_zip = second.with_extension("zip");
    for (output, archive) in [(&first, &first_zip), (&second, &second_zip)] {
        let status = std::process::Command::new("python")
            .arg(&script)
            .arg("--output")
            .arg(output)
            .arg("--zip")
            .arg(archive)
            .status()
            .expect("python and ffmpeg are required for the generator test");
        assert!(status.success());
        assert!(archive.is_file());
        assert_eq!(
            fs::read_to_string(output.join("STARTER_VERSION")).unwrap(),
            "1.0.0\n"
        );
        for (id, usage) in [
            ("player-red", "human"),
            ("player-blue", "human"),
            ("mjai-bot", "mjai"),
            ("tsumogiri-bot", "builtin"),
            ("mcp-agent", "mcp"),
        ] {
            let pack = output.join(id);
            let manifest: Value =
                serde_json::from_slice(&fs::read(pack.join("manifest.json")).unwrap()).unwrap();
            assert_eq!(manifest["usage"], usage);
            let source_license = fs::read_to_string(repo.join("scripts/CC0-1.0.txt"))
                .unwrap()
                .replace("\r\n", "\n");
            assert_eq!(
                fs::read_to_string(pack.join("LICENSE")).unwrap(),
                source_license
            );
            for file in ["portrait.webp", "icon.webp"] {
                assert_eq!(&fs::read(pack.join(file)).unwrap()[..4], b"RIFF");
            }
            for voice in VOICES {
                assert_eq!(
                    &fs::read(pack.join("voices").join(format!("{voice}.ogg"))).unwrap()[..4],
                    b"OggS"
                );
            }
        }
        let registry = CharacterRegistry::load(output, &CharacterRequirements::starter()).unwrap();
        assert_eq!(registry.human_characters().len(), 2);
        let notice = fs::read_to_string(output.join("CC0-NOTICE.txt")).unwrap();
        for (id, _) in [
            ("player-red", "human"),
            ("player-blue", "human"),
            ("mjai-bot", "mjai"),
            ("tsumogiri-bot", "builtin"),
            ("mcp-agent", "mcp"),
        ] {
            for asset in ["portrait.webp", "icon.webp"] {
                assert!(notice.contains(&format!("{id}/{asset}")));
                assert!(!notice.contains(&format!("character-packs/{id}/{asset}")));
            }
            for voice in VOICES {
                assert!(notice.contains(&format!("{id}/voices/{voice}.ogg")));
                assert!(!notice.contains(&format!("character-packs/{id}/voices/{voice}.ogg")));
            }
        }
        for line in fs::read_to_string(output.join("SHA256SUMS"))
            .unwrap()
            .lines()
        {
            let (expected, relative) = line.split_once("  ").unwrap();
            assert_eq!(
                expected,
                sha256_hex(&fs::read(output.join(relative)).unwrap())
            );
        }
    }
    assert_eq!(
        fs::read(first.join("SHA256SUMS")).unwrap(),
        fs::read(second.join("SHA256SUMS")).unwrap()
    );
    assert_eq!(
        fs::read(&first_zip).unwrap(),
        fs::read(&second_zip).unwrap()
    );
    let first_archive_checksum =
        fs::read_to_string(first_zip.with_extension("zip.sha256")).unwrap();
    assert_eq!(
        first_archive_checksum,
        format!(
            "{}  {}\n",
            sha256_hex(&fs::read(&first_zip).unwrap()),
            first_zip.file_name().unwrap().to_string_lossy()
        )
    );
    let archive_listing = std::process::Command::new("python")
        .args(["-c", "import sys, zipfile; names = set(zipfile.ZipFile(sys.argv[1]).namelist()); required = {'STARTER_VERSION', 'CC0-NOTICE.txt', 'SHA256SUMS', 'player-red/LICENSE', 'player-red/voices/riichi.ogg'}; assert required <= names" ])
        .arg(&first_zip)
        .status()
        .unwrap();
    assert!(archive_listing.success());
    fs::remove_dir_all(first).unwrap();
    fs::remove_dir_all(second).unwrap();
    fs::remove_file(first_zip.with_extension("zip.sha256")).unwrap();
    fs::remove_file(second_zip.with_extension("zip.sha256")).unwrap();
    fs::remove_file(first_zip).unwrap();
    fs::remove_file(second_zip).unwrap();
}
