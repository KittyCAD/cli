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

# We need this for the version.
cargo install toml-cli

# Install cross.
cargo install cross
