#!/usr/bin/env bash
set -euo pipefail

# Exercise the real release recipe across containers with different host libcs.
# A single-target matrix job cannot catch reuse of incompatible host build scripts.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
fixture_dir=$(mktemp -d "${TMPDIR:-/tmp}/zoo-release-builds.XXXXXX")
trap 'rm -rf "$fixture_dir"' EXIT

mkdir -p "$fixture_dir/src" "$fixture_dir/scripts"
cp "$repo_root/Makefile" "$fixture_dir/Makefile"
cp "$repo_root/rust-toolchain.toml" "$fixture_dir/rust-toolchain.toml"
cp "$repo_root/scripts/release-install-instructions.sh" "$fixture_dir/scripts/"

cat > "$fixture_dir/Cargo.toml" <<'EOF'
[package]
name = "zoo"
version = "0.0.0"
edition = "2024"

[dependencies]
# Its host build script exposed the cross-container glibc mismatch in releases.
rustix = "=1.1.4"
EOF
echo 'fn main() {}' > "$fixture_dir/src/main.rs"

make -C "$fixture_dir" release \
    CROSS_TARGETS="x86_64-unknown-linux-musl aarch64-unknown-linux-musl" \
    VERSION=0.0.0 GITCOMMIT=test GITUNTRACKEDCHANGES=

for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
    test -x "$fixture_dir/cross/zoo-$target"
    sha256sum -c "$fixture_dir/cross/zoo-$target.sha256"
    grep -q "^### $target$" "$fixture_dir/cross/README.md"
done
