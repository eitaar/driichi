set shell := ["bash", "-cu"]

# Deterministic local checks; external credentials are never required.
check: fmt-check test-rust test-frontend test-spec test-contract test-smoke test-release-scripts

test-all: fmt-check test-rust test-frontend test-spec test-contract test-smoke test-release-scripts test-e2e

fmt-check:
    cargo fmt --all -- --check

test-rust:
    cargo test --workspace

test-frontend:
    npm ci --prefix frontend
    npm run typecheck --prefix frontend
    npm test --prefix frontend -- --run

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

test-release-scripts:
    python scripts/release/test_release.py
    python scripts/release/test_live.py --help

test-e2e:
    npm ci --prefix frontend
    npm run test:browser --prefix frontend

# Build order is intentionally npm, frontend production build, then Cargo.
# package.py creates all generated Starter media in a temporary directory.
build-release:
    python scripts/release/package.py build --platform "$${RELEASE_PLATFORM:-auto}" --output "$${RELEASE_OUTPUT:-target/release-artifacts}"

release-smoke:
    test -n "$${RELEASE_ARCHIVE:-}" || { echo 'RELEASE_ARCHIVE is required' >&2; exit 2; }
    test -n "$${RELEASE_PLATFORM:-}" || { echo 'RELEASE_PLATFORM is required' >&2; exit 2; }
    test -n "$${RELEASE_VERSION:-}" || { echo 'RELEASE_VERSION is required' >&2; exit 2; }
    test -n "$${RELEASE_COMMIT:-}" || { echo 'RELEASE_COMMIT is required' >&2; exit 2; }
    python scripts/release/smoke.py "$${RELEASE_ARCHIVE}" --platform "$${RELEASE_PLATFORM}" --version "$${RELEASE_VERSION}" --commit "$${RELEASE_COMMIT}"

release-smoke-dry-run:
    test -n "$${RELEASE_ARCHIVE:-}" || { echo 'RELEASE_ARCHIVE is required' >&2; exit 2; }
    test -n "$${RELEASE_PLATFORM:-}" || { echo 'RELEASE_PLATFORM is required' >&2; exit 2; }
    test -n "$${RELEASE_VERSION:-}" || { echo 'RELEASE_VERSION is required' >&2; exit 2; }
    test -n "$${RELEASE_COMMIT:-}" || { echo 'RELEASE_COMMIT is required' >&2; exit 2; }
    python scripts/release/smoke.py "$${RELEASE_ARCHIVE}" --platform "$${RELEASE_PLATFORM}" --version "$${RELEASE_VERSION}" --commit "$${RELEASE_COMMIT}" --dry-run

# This gate is intentionally opt-in and is invoked only by the scheduled CI job.
test-live:
    test "$${RUN_LIVE_TESTS:-0}" = 1 || { echo 'test-live is scheduled-only; set RUN_LIVE_TESTS=1 in the scheduled job' >&2; exit 2; }
    python scripts/release/test_live.py
