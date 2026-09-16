use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use axum::{
    Router,
    body::Body,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
    routing::get,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const CHARACTER_ROOT_NAME: &str = "character-packs";
const MANIFEST_LIMIT: u64 = 64 * 1024;
const LICENSE_LIMIT: u64 = 1024 * 1024;
const ICON_LIMIT: u64 = 2 * 1024 * 1024;
const PORTRAIT_LIMIT: u64 = 8 * 1024 * 1024;
const VOICE_LIMIT: u64 = 8 * 1024 * 1024;
const STARTER_VERSION: &str = "1.0.0";
const REQUIRED_VOICES: [VoiceLine; 6] = [
    VoiceLine::Chi,
    VoiceLine::Pon,
    VoiceLine::Kan,
    VoiceLine::Riichi,
    VoiceLine::Ron,
    VoiceLine::Tsumo,
];

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CharacterRegistryError {
    #[error("character registry could not be read")]
    Io,
    #[error("character pack is invalid")]
    InvalidPack,
    #[error("character pack path is unsafe")]
    UnsafePath,
    #[error("duplicate character ID")]
    DuplicateId,
    #[error("duplicate character asset path")]
    DuplicatePath,
    #[error("no valid Human Character Pack is configured")]
    MissingHumanPack,
    #[error("a required Character Pack is missing")]
    MissingRequiredPack,
    #[error("a required Character Pack has the wrong usage")]
    UsageMismatch,
    #[error("Character Pack configuration is invalid")]
    InvalidRequirement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CharacterUsage {
    Human,
    Mjai,
    Mcp,
    Builtin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum VoiceLine {
    Chi,
    Pon,
    Kan,
    Riichi,
    Ron,
    Tsumo,
}

impl VoiceLine {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chi => "chi",
            Self::Pon => "pon",
            Self::Kan => "kan",
            Self::Riichi => "riichi",
            Self::Ron => "ron",
            Self::Tsumo => "tsumo",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "chi" => Self::Chi,
            "pon" => Self::Pon,
            "kan" => Self::Kan,
            "riichi" => Self::Riichi,
            "ron" => Self::Ron,
            "tsumo" => Self::Tsumo,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum CharacterAsset {
    Portrait,
    Icon,
    Voice(VoiceLine),
}

impl CharacterAsset {
    pub const fn relative_path(self) -> &'static str {
        match self {
            Self::Portrait => "portrait.webp",
            Self::Icon => "icon.webp",
            Self::Voice(VoiceLine::Chi) => "voices/chi.ogg",
            Self::Voice(VoiceLine::Pon) => "voices/pon.ogg",
            Self::Voice(VoiceLine::Kan) => "voices/kan.ogg",
            Self::Voice(VoiceLine::Riichi) => "voices/riichi.ogg",
            Self::Voice(VoiceLine::Ron) => "voices/ron.ogg",
            Self::Voice(VoiceLine::Tsumo) => "voices/tsumo.ogg",
        }
    }

    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Portrait | Self::Icon => "image/webp",
            Self::Voice(_) => "audio/ogg",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CharacterSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct CharacterPack {
    id: String,
    name: String,
    usage: CharacterUsage,
    assets: BTreeMap<CharacterAsset, CharacterAssetFile>,
}

impl CharacterPack {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn usage(&self) -> CharacterUsage {
        self.usage
    }

    pub fn summary(&self) -> CharacterSummary {
        CharacterSummary {
            id: self.id.clone(),
            name: self.name.clone(),
        }
    }

    pub fn asset(&self, asset: CharacterAsset) -> Option<&CharacterAssetFile> {
        self.assets.get(&asset)
    }
}

#[derive(Debug, Clone)]
pub struct CharacterAssetFile {
    relative_path: PathBuf,
    content_type: &'static str,
    bytes: Arc<[u8]>,
    etag: String,
}

impl CharacterAssetFile {
    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }

    pub fn content_type(&self) -> &'static str {
        self.content_type
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn etag(&self) -> &str {
        &self.etag
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterRequirements {
    pub mjai: String,
    pub builtin: String,
    pub mcp: String,
    pub mcp_providers: BTreeMap<String, String>,
}

impl CharacterRequirements {
    pub fn new<I, K, V>(
        mjai: &str,
        builtin: &str,
        mcp: &str,
        mcp_providers: I,
    ) -> Result<Self, CharacterRegistryError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mcp_providers = mcp_providers
            .into_iter()
            .map(|(provider, pack)| (provider.as_ref().to_owned(), pack.as_ref().to_owned()))
            .collect::<BTreeMap<_, _>>();
        let requirements = Self {
            mjai: mjai.to_owned(),
            builtin: builtin.to_owned(),
            mcp: mcp.to_owned(),
            mcp_providers,
        };
        requirements.validate()?;
        Ok(requirements)
    }

    pub fn starter() -> Self {
        Self {
            mjai: "mjai-bot".to_owned(),
            builtin: "tsumogiri-bot".to_owned(),
            mcp: "mcp-agent".to_owned(),
            mcp_providers: BTreeMap::new(),
        }
    }

    fn validate(&self) -> Result<(), CharacterRegistryError> {
        for id in [&self.mjai, &self.builtin, &self.mcp] {
            if !is_safe_id(id) {
                return Err(CharacterRegistryError::InvalidRequirement);
            }
        }
        for (provider, id) in &self.mcp_providers {
            if !is_provider(provider) || !is_safe_id(id) {
                return Err(CharacterRegistryError::InvalidRequirement);
            }
        }
        Ok(())
    }

    fn expected(&self) -> impl Iterator<Item = (&str, CharacterUsage)> {
        std::iter::once((self.mjai.as_str(), CharacterUsage::Mjai))
            .chain(std::iter::once((
                self.builtin.as_str(),
                CharacterUsage::Builtin,
            )))
            .chain(std::iter::once((self.mcp.as_str(), CharacterUsage::Mcp)))
            .chain(
                self.mcp_providers
                    .values()
                    .map(|id| (id.as_str(), CharacterUsage::Mcp)),
            )
    }
}

#[derive(Debug, Clone)]
pub struct CharacterRegistry {
    root: PathBuf,
    packs: BTreeMap<String, CharacterPack>,
}

impl CharacterRegistry {
    pub fn scan(root: &Path) -> Result<Self, CharacterRegistryError> {
        Self::scan_with_requirements(root, None)
    }

    pub fn load(
        root: &Path,
        requirements: &CharacterRequirements,
    ) -> Result<Self, CharacterRegistryError> {
        requirements.validate()?;
        Self::scan_with_requirements(root, Some(requirements))
    }

    pub fn load_from_data_root(
        data_root: &Path,
        requirements: &CharacterRequirements,
    ) -> Result<Self, CharacterRegistryError> {
        Self::load(&data_root.join(CHARACTER_ROOT_NAME), requirements)
    }

    pub fn load_default_from_data_root(data_root: &Path) -> Result<Self, CharacterRegistryError> {
        Self::load_from_data_root(data_root, &CharacterRequirements::starter())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn character(&self, id: &str) -> Option<&CharacterPack> {
        if !is_safe_id(id) {
            return None;
        }
        self.packs.get(id)
    }

    pub fn characters(&self) -> impl Iterator<Item = &CharacterPack> {
        self.packs.values()
    }

    pub fn human_characters(&self) -> Vec<CharacterSummary> {
        self.packs
            .values()
            .filter(|pack| pack.usage == CharacterUsage::Human)
            .map(CharacterPack::summary)
            .collect()
    }

    pub fn asset(&self, id: &str, asset: CharacterAsset) -> Option<&CharacterAssetFile> {
        self.character(id).and_then(|pack| pack.asset(asset))
    }

    fn scan_with_requirements(
        root: &Path,
        requirements: Option<&CharacterRequirements>,
    ) -> Result<Self, CharacterRegistryError> {
        let root = canonical_directory(root)?;
        let mut folders = fs::read_dir(&root)
            .map_err(|_| CharacterRegistryError::Io)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| CharacterRegistryError::Io)?;
        folders.sort_by(|left, right| left.file_name().cmp(&right.file_name()));

        let mut packs = BTreeMap::new();
        let mut ids = HashSet::new();
        let mut paths = HashSet::new();
        let mut case_folded_paths = HashMap::new();
        let mut file_identities = HashSet::new();

        for entry in folders {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|_| CharacterRegistryError::Io)?;
            if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
                return Err(CharacterRegistryError::UnsafePath);
            }
            if !metadata.is_dir() {
                continue;
            }

            let manifest = match read_manifest(&path) {
                Ok(manifest) => manifest,
                Err(error) if error == CharacterRegistryError::UnsafePath => return Err(error),
                Err(_) => {
                    tracing::warn!("ignored invalid unreferenced Character Pack");
                    continue;
                }
            };
            if !ids.insert(manifest.id.clone()) {
                return Err(CharacterRegistryError::DuplicateId);
            }

            match validate_pack(
                &root,
                &path,
                manifest,
                &mut paths,
                &mut case_folded_paths,
                &mut file_identities,
            ) {
                Ok(pack) => {
                    if packs.insert(pack.id.clone(), pack).is_some() {
                        return Err(CharacterRegistryError::DuplicateId);
                    }
                }
                Err(error) if error == CharacterRegistryError::UnsafePath => return Err(error),
                Err(error) if error == CharacterRegistryError::DuplicatePath => return Err(error),
                Err(_) => tracing::warn!("ignored invalid unreferenced Character Pack"),
            }
        }

        let registry = Self { root, packs };
        if registry
            .packs
            .values()
            .all(|pack| pack.usage != CharacterUsage::Human)
        {
            return Err(CharacterRegistryError::MissingHumanPack);
        }
        if let Some(requirements) = requirements {
            for (id, usage) in requirements.expected() {
                let Some(pack) = registry.packs.get(id) else {
                    return Err(CharacterRegistryError::MissingRequiredPack);
                };
                if pack.usage != usage {
                    return Err(CharacterRegistryError::UsageMismatch);
                }
            }
        }
        Ok(registry)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    id: String,
    name: String,
    usage: CharacterUsage,
}

fn read_manifest(pack_path: &Path) -> Result<ManifestFile, CharacterRegistryError> {
    let path = pack_path.join("manifest.json");
    let bytes = read_bounded_file(pack_path, &path, MANIFEST_LIMIT)?;
    serde_json::from_slice(&bytes).map_err(|_| CharacterRegistryError::InvalidPack)
}

fn validate_pack(
    root: &Path,
    pack_path: &Path,
    manifest: ManifestFile,
    paths: &mut HashSet<PathBuf>,
    case_folded_paths: &mut HashMap<String, PathBuf>,
    file_identities: &mut HashSet<(u64, u64)>,
) -> Result<CharacterPack, CharacterRegistryError> {
    let folder = pack_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(CharacterRegistryError::InvalidPack)?;
    if !is_safe_id(&manifest.id) || manifest.id != folder {
        return Err(CharacterRegistryError::InvalidPack);
    }
    let name = normalize_display_name(&manifest.name).ok_or(CharacterRegistryError::InvalidPack)?;
    let mut assets = BTreeMap::new();

    let manifest_path = canonical_asset_path(root, &pack_path.join("manifest.json"))?;
    register_path(manifest_path, paths, case_folded_paths, file_identities)?;

    let license_path = canonical_asset_path(root, &pack_path.join("LICENSE"))?;
    let license_bytes = read_bounded_canonical_file(&license_path, LICENSE_LIMIT)?;
    if license_bytes.is_empty() || std::str::from_utf8(&license_bytes).is_err() {
        return Err(CharacterRegistryError::InvalidPack);
    }
    register_path(license_path, paths, case_folded_paths, file_identities)?;

    for asset in [CharacterAsset::Portrait, CharacterAsset::Icon] {
        let path = canonical_asset_path(root, &pack_path.join(asset.relative_path()))?;
        let limit = match asset {
            CharacterAsset::Portrait => PORTRAIT_LIMIT,
            CharacterAsset::Icon => ICON_LIMIT,
            CharacterAsset::Voice(_) => unreachable!(),
        };
        let bytes = read_bounded_canonical_file(&path, limit)?;
        if !is_webp(&bytes) {
            return Err(CharacterRegistryError::InvalidPack);
        }
        register_path(path.clone(), paths, case_folded_paths, file_identities)?;
        assets.insert(asset, asset_file(root, path, bytes));
    }

    for voice in REQUIRED_VOICES {
        let asset = CharacterAsset::Voice(voice);
        let path = canonical_asset_path(root, &pack_path.join(asset.relative_path()))?;
        let bytes = read_bounded_canonical_file(&path, VOICE_LIMIT)?;
        if !is_ogg(&bytes) {
            return Err(CharacterRegistryError::InvalidPack);
        }
        register_path(path.clone(), paths, case_folded_paths, file_identities)?;
        assets.insert(asset, asset_file(root, path, bytes));
    }

    Ok(CharacterPack {
        id: manifest.id,
        name,
        usage: manifest.usage,
        assets,
    })
}

fn asset_file(root: &Path, path: PathBuf, bytes: Vec<u8>) -> CharacterAssetFile {
    let relative_path = path
        .strip_prefix(root)
        .expect("canonical asset path is rooted")
        .to_path_buf();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = hasher.finalize();
    let mut etag = String::with_capacity(66);
    etag.push('"');
    for byte in hash {
        etag.push_str(&format!("{byte:02x}"));
    }
    etag.push('"');
    CharacterAssetFile {
        relative_path,
        content_type: if path.extension().and_then(|extension| extension.to_str()) == Some("webp") {
            "image/webp"
        } else {
            "audio/ogg"
        },
        bytes: Arc::from(bytes),
        etag,
    }
}

fn read_bounded_file(
    root: &Path,
    path: &Path,
    limit: u64,
) -> Result<Vec<u8>, CharacterRegistryError> {
    let canonical = canonical_asset_path(root, path)?;
    read_bounded_canonical_file(&canonical, limit)
}

fn read_bounded_canonical_file(path: &Path, limit: u64) -> Result<Vec<u8>, CharacterRegistryError> {
    let metadata = fs::metadata(path).map_err(|_| CharacterRegistryError::InvalidPack)?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(CharacterRegistryError::InvalidPack);
    }
    let file = fs::File::open(path).map_err(|_| CharacterRegistryError::InvalidPack)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| CharacterRegistryError::InvalidPack)?;
    if bytes.len() as u64 > limit {
        return Err(CharacterRegistryError::InvalidPack);
    }
    Ok(bytes)
}

fn canonical_directory(path: &Path) -> Result<PathBuf, CharacterRegistryError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| CharacterRegistryError::Io)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return Err(CharacterRegistryError::UnsafePath);
    }
    fs::canonicalize(path).map_err(|_| CharacterRegistryError::Io)
}

fn canonical_asset_path(root: &Path, path: &Path) -> Result<PathBuf, CharacterRegistryError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| CharacterRegistryError::UnsafePath)?;
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(CharacterRegistryError::UnsafePath);
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(CharacterRegistryError::UnsafePath);
        };
        current.push(part);
        let metadata =
            fs::symlink_metadata(&current).map_err(|_| CharacterRegistryError::InvalidPack)?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(CharacterRegistryError::UnsafePath);
        }
    }
    let canonical = fs::canonicalize(path).map_err(|_| CharacterRegistryError::InvalidPack)?;
    if !canonical.starts_with(root) {
        return Err(CharacterRegistryError::UnsafePath);
    }
    Ok(canonical)
}

fn register_path(
    path: PathBuf,
    paths: &mut HashSet<PathBuf>,
    case_folded_paths: &mut HashMap<String, PathBuf>,
    file_identities: &mut HashSet<(u64, u64)>,
) -> Result<(), CharacterRegistryError> {
    if !paths.insert(path.clone()) {
        return Err(CharacterRegistryError::DuplicatePath);
    }
    if let Some(identity) = file_identity(&path)
        && !file_identities.insert(identity)
    {
        return Err(CharacterRegistryError::DuplicatePath);
    }
    let key = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_lowercase())
        .collect::<Vec<_>>()
        .join("/");
    if case_folded_paths.insert(key, path).is_some() {
        return Err(CharacterRegistryError::DuplicatePath);
    }
    Ok(())
}

#[cfg(unix)]
fn file_identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn file_identity(_path: &Path) -> Option<(u64, u64)> {
    // Stable Rust does not expose Windows file handles' volume/index identity.
    // Canonical component checks and rejection of symlink/reparse points remain
    // the traversal boundary; case-folded canonical paths catch path aliases.
    None
}

#[cfg(not(any(unix, windows)))]
fn file_identity(_path: &Path) -> Option<(u64, u64)> {
    None
}

fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

fn is_ogg(bytes: &[u8]) -> bool {
    // The startup contract intentionally validates only the fixed container
    // header. Full media decoding belongs to the client and release tooling.
    bytes.starts_with(b"OggS")
}

fn normalize_display_name(value: &str) -> Option<String> {
    let value = value.trim_matches(char::is_whitespace);
    if !(1..=64).contains(&value.chars().count()) || value.chars().any(char::is_control) {
        return None;
    }
    Some(value.to_owned())
}

pub fn is_safe_character_id(value: &str) -> bool {
    is_safe_id(value)
}

fn is_safe_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn is_provider(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

pub fn character_router(registry: Arc<CharacterRegistry>) -> Router {
    Router::new()
        .route("/api/v1/characters/human", get(list_human_characters))
        .route("/assets/characters/{id}/portrait.webp", get(portrait_asset))
        .route("/assets/characters/{id}/icon.webp", get(icon_asset))
        .route("/assets/characters/{id}/voices/{voice}", get(voice_asset))
        .with_state(registry)
}

async fn list_human_characters(State(registry): State<Arc<CharacterRegistry>>) -> Response {
    let body = match serde_json::to_vec(&registry.human_characters()) {
        Ok(body) => body,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        }
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "public, no-cache")
        .header("x-content-type-options", "nosniff")
        .body(Body::from(body))
        .unwrap()
}

async fn portrait_asset(
    State(registry): State<Arc<CharacterRegistry>>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    serve_asset(&registry, &id, CharacterAsset::Portrait, &headers)
}

async fn icon_asset(
    State(registry): State<Arc<CharacterRegistry>>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    serve_asset(&registry, &id, CharacterAsset::Icon, &headers)
}

async fn voice_asset(
    State(registry): State<Arc<CharacterRegistry>>,
    AxumPath((id, voice)): AxumPath<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let Some(voice) = voice.strip_suffix(".ogg").and_then(VoiceLine::parse) else {
        return not_found();
    };
    serve_asset(&registry, &id, CharacterAsset::Voice(voice), &headers)
}

fn serve_asset(
    registry: &CharacterRegistry,
    id: &str,
    asset: CharacterAsset,
    headers: &HeaderMap,
) -> Response {
    let Some(asset) = registry.asset(id, asset) else {
        return not_found();
    };
    let not_modified = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| etag_matches(value, asset.etag()));
    let mut builder = Response::builder()
        .status(if not_modified {
            StatusCode::NOT_MODIFIED
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, asset.content_type())
        .header(header::CACHE_CONTROL, "public, no-cache")
        .header(header::ETAG, asset.etag())
        .header("x-content-type-options", "nosniff");
    if !not_modified {
        builder = builder.header(header::CONTENT_LENGTH, asset.bytes().len());
    }
    builder
        .body(if not_modified {
            Body::empty()
        } else {
            Body::from(asset.bytes().to_vec())
        })
        .unwrap()
}

fn etag_matches(value: &str, etag: &str) -> bool {
    value.trim() == "*"
        || value
            .split(',')
            .map(str::trim)
            .any(|candidate| candidate == etag || candidate == format!("W/{etag}"))
}

fn not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header("x-content-type-options", "nosniff")
        .body(Body::empty())
        .unwrap()
}

pub fn starter_version() -> &'static str {
    STARTER_VERSION
}
