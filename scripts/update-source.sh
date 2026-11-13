#!/usr/bin/env bash
# Developer/legacy checkout updater. Ordinary users should use install.sh.
set -euo pipefail
die() { printf 'Blue Dragon source update: %s\n' "$*" >&2; exit 1; }
root=$(git rev-parse --show-toplevel) || die 'Run inside the Blue Dragon checkout'
cd -- "$root"
origin=$(git config --get remote.origin.url) || die 'No origin configured'
case "$origin" in
  https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent|https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent.git|git@github.com:khdertaysh1/khdertaysh1-blue-dragon-agent.git) ;;
  *) die 'Origin is not the expected Blue Dragon repository; nothing changed' ;;
esac
[[ -z $(git status --porcelain --untracked-files=no) ]] || die 'Tracked local changes found; commit or save them before updating. Nothing was overwritten.'
git fetch origin main
if [[ -e Cargo.lock ]] && ! git ls-files --error-unmatch Cargo.lock >/dev/null 2>&1; then
  backups="${XDG_DATA_HOME:-$HOME/.local/share}/blue-dragon/source-backups"
  mkdir -p -- "$backups"
  saved=$(mktemp -d "$backups/lock.XXXXXX")
  mv -- Cargo.lock "$saved/Cargo.lock"
  printf 'Preserved generated Cargo.lock at %s/Cargo.lock\n' "$saved"
fi
git switch main
git merge --ff-only origin/main
git ls-files --error-unmatch Cargo.lock >/dev/null || die 'This source version has no tracked lockfile; refusing an unlocked build'
command -v cargo >/dev/null || die 'Rust is needed for source builds; use the binary installer instead'
cargo build --locked --release
./target/release/blue-dragon --version
