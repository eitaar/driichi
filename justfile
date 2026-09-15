set shell := ["bash", "-cu"]

# Verify the bootstrap workspace without requiring a production frontend build.
check: fmt-check test-rust test-frontend test-spec test-smoke

fmt-check:
    cargo fmt --all -- --check

test-rust:
    cargo test --workspace

test-frontend:
    npm ci --prefix frontend
    npm run typecheck --prefix frontend

test-spec:
    grep -Fq 'Basic accessibility is a v1 requirement.' spec/implementation-v1.md
    grep -Fq 'Reduced-motion fallbacks are a v1 requirement.' spec/implementation-v1.md
    grep -Fq 'Mobile gameplay layout remains deferred.' spec/implementation-v1.md
    grep -Fq 'Full Pixi keyboard and screen-reader narration remains deferred.' spec/implementation-v1.md

test-smoke:
    bash tests/workspace-smoke.sh

build-release:
    npm ci --prefix frontend
    npm run build --prefix frontend
    cargo build --release
