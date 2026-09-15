//! HTTP server crate for double-riichi.

#[cfg(frontend_dist)]
use rust_embed::RustEmbed;

#[cfg(frontend_dist)]
#[derive(RustEmbed)]
#[folder = "../../frontend/dist/"]
pub struct FrontendAssets;
