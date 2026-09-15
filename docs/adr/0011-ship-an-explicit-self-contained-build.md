# Ship an explicit self-contained build

Runtime data is rooted at the selected config file, Admin secrets come from its required `.env`, and production builds explicitly run npm before Cargo rather than invoking npm from a build script. Platform archives include both binaries, notices, examples, bundled SQLite/rustls behavior, vendored tile art, and generated CC0 Starter Packs, require no CDN or system database, and publish SHA-256 checksums.
