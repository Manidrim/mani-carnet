#!/usr/bin/env bash
# Raccourci des commandes cargo du README. Usage : ./install.sh
set -euo pipefail
cd "$(dirname "$0")"
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
command -v cargo >/dev/null || { echo "Installe Rust d'abord : https://rustup.rs"; exit 1; }

cargo bundle --version >/dev/null 2>&1 || cargo install cargo-bundle   # une seule fois
cargo bundle --release
pkill -x carnet 2>/dev/null || true
ditto target/release/bundle/osx/Carnet.app "$HOME/Applications/Carnet.app"
open "$HOME/Applications/Carnet.app"
echo "✔ Carnet installé dans ~/Applications (⌘Espace → Carnet)."
