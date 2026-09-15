use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=../../frontend/dist");
    println!("cargo:rustc-check-cfg=cfg(frontend_dist)");

    if std::env::var("PROFILE").as_deref() != Ok("release") {
        return;
    }

    let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../frontend/dist");
    if !dist.is_dir() {
        panic!(
            "frontend/dist is required for production server embedding; run `npm ci --prefix frontend && npm run build --prefix frontend` before `cargo build --release`"
        );
    }

    println!("cargo:rustc-cfg=frontend_dist");
}
