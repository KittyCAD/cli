#!/bin/bash
set -e
set -o pipefail

# Install our deps.
sudo apt update -y && sudo apt install -y \
	ca-certificates \
	clang \
	cmake \
	curl \
	g++ \
	gcc \
	gcc-mingw-w64-i686 \
	gcc-mingw-w64 \
	jq \
	libmpc-dev \
	libmpfr-dev \
	libgmp-dev \
	libssl-dev \
	libxml2-dev \
	mingw-w64 \
	wget \
	zlib1g-dev

# CI prepares the Cargo tools once before fanning out to target builds.
# Keep installing them by default for local builds and the release workflow.
if [[ "${1:-}" != "--system-only" ]]; then
	# We need this for the version.
	cargo install toml-cli

	# Install cross.
	cargo install cross
fi
