set shell := ["bash", "-cu"]

# Verify the bootstrap workspace without requiring a production frontend build.
check: fmt-check test-rust test-frontend test-spec test-contract test-smoke

fmt-check:
    cargo fmt --all -- --check

test-rust:
    cargo test --workspace

test-frontend:
    npm ci --prefix frontend
    npm run typecheck --prefix frontend

contracts-install:
    python -m pip install --disable-pip-version-check --no-input --requirement scripts/requirements-contracts.txt

test-contract: contracts-install
    python scripts/validate_contracts.py
    cargo test -p double_riichi_server --test task16_contracts -- --test-threads=1

test-spec:
    grep -Fq 'Basic accessibility is a v1 requirement.' spec/implementation-v1.md
    grep -Fq 'Reduced-motion fallbacks are a v1 requirement.' spec/implementation-v1.md
    grep -Fq 'Mobile gameplay layout remains deferred.' spec/implementation-v1.md
    grep -Fq 'Full Pixi keyboard and screen-reader narration remains deferred.' spec/implementation-v1.md

test-smoke:
    bash tests/workspace-smoke.sh

generate-starter-packs:
    python scripts/generate_starter_packs.py --output character-packs --zip character-packs-1.0.0.zip

build-release: generate-starter-packs
    npm ci --prefix frontend
    npm run build --prefix frontend
    cargo build --release
