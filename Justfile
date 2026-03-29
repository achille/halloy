# Halloy development commands

# Run cargo check
check:
    cargo check

# Build debug
build:
    cargo build

# Build release
release:
    cargo build --release

# Run all tests
test:
    cargo test

# Run tests for a specific crate
test-data:
    cargo test -p data

# Run cargo fmt check
lint:
    cargo fmt -- --check
    cargo clippy

# Auto-fix formatting
format:
    cargo fmt

# Build macOS .app bundle (release)
app: release
    #!/usr/bin/env bash
    set -euo pipefail
    rm -rf result/Halloy.app
    mkdir -p result/Halloy.app/Contents/MacOS
    mkdir -p result/Halloy.app/Contents/Resources
    cp assets/macos/Halloy.app/Contents/Resources/halloy.icns result/Halloy.app/Contents/Resources/
    VERSION=$(cat VERSION)
    sed -e "s/{{ VERSION }}/$VERSION/" -e "s/{{ BUILD }}/1/" \
        assets/macos/Halloy.app/Contents/Info.plist \
        > result/Halloy.app/Contents/Info.plist
    cp target/release/halloy result/Halloy.app/Contents/MacOS/halloy
    @echo "Built result/Halloy.app"

# Open the built .app
run: app
    open result/Halloy.app

# Nix build
nix-build:
    nix build

# Nix check (cargo check in sandbox)
nix-check:
    nix build .#checks.{{arch()}}-{{os()}}.cargo-check
