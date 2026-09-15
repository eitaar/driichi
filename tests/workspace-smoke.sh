#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
metadata="$(cargo metadata --manifest-path "$root/Cargo.toml" --no-deps --format-version 1)"

for package in \
  double_riichi_core \
  double_riichi_server \
  double_riichi_mjai \
  double_riichi_mcp \
  double_riichi_replay; do
  grep -Fq "\"name\":\"$package\"" <<<"$metadata"
done

grep -Fq '"name":"driichi"' <<<"$metadata"
grep -Fq '"name":"driichi-mcp"' <<<"$metadata"

test -f "$root/frontend/package.json"
test -f "$root/frontend/package-lock.json"
test -f "$root/rust-toolchain.toml"
test -f "$root/justfile"
test -f "$root/config.toml.example"
test -f "$root/.env.example"
grep -Fq 'ADMIN_USERNAME=' "$root/.env.example"
grep -Fq 'ADMIN_PASSWORD_HASH=' "$root/.env.example"
! grep -Fq 'DRIICHI_ADMIN_PASSWORD_HASH=' "$root/.env.example"
