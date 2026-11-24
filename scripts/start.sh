#!/usr/bin/env bash
# Install the pinned release and launch its interactive terminal.
set -euo pipefail
tag=v1.1.14.1
assets="https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/releases/download/$tag"
if [[ ${PREFIX:-} == */com.termux/files/usr ]]; then
  [[ $(uname -m) == aarch64 ]] || { echo 'Android ARM64 required' >&2; exit 1; }
  target=android-arm64
  bindir="$PREFIX/bin"
else
  case "$(uname -s)/$(uname -m)" in
    Linux/x86_64) target=linux-x64 ;;
    Darwin/arm64) target=macos-arm64 ;;
    *) echo 'Supported desktop targets are Linux x64 and macOS Apple Silicon' >&2; exit 1 ;;
  esac
  bindir="$HOME/.local/bin"
fi
mkdir -p "$bindir"
stage=$(mktemp -d "$bindir/.blue-dragon.XXXXXXXX")
trap 'rm -rf -- "$stage"' EXIT
name="blue-dragon-$target"
fetch() { curl --proto '=https' --proto-redir '=https' -fsSL --retry 3 "$assets/$1" -o "$stage/$1"; }
fetch SHA256SUMS.txt
for item in "$name" LICENSES.txt; do
  fetch "$item"
  expected=$(awk -v name="$item" '$2 == name {print $1}' "$stage/SHA256SUMS.txt")
  [[ $expected =~ ^[a-f0-9]{64}$ ]] || { echo "Missing checksum for $item" >&2; exit 1; }
  if command -v sha256sum >/dev/null; then actual=$(sha256sum "$stage/$item"); else actual=$(shasum -a 256 "$stage/$item"); fi
  [[ ${actual%% *} == "$expected" ]] || { echo "Checksum mismatch for $item" >&2; exit 1; }
done
chmod 755 "$stage/$name"
[[ $("$stage/$name" --version) == "blue-dragon ${tag#v}" ]]
mv -f "$stage/LICENSES.txt" "$bindir/blue-dragon.LICENSES.txt"
mv -f "$stage/$name" "$bindir/blue-dragon"
rm -rf -- "$stage"
trap - EXIT
if [[ $# -gt 0 ]]; then exec "$bindir/blue-dragon" "$@"; fi
exec "$bindir/blue-dragon" --provider free tui </dev/tty
