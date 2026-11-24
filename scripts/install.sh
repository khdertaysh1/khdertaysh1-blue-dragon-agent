#!/usr/bin/env bash
# Install/update a published binary. Never checks out or builds a repository.
set -euo pipefail

repo='khdertaysh1/khdertaysh1-blue-dragon-agent'
api="https://api.github.com/repos/$repo"
requested=''
bin_dir=''
stage=''
fail() { printf 'Blue Dragon: %s\n' "$*" >&2; exit 1; }
valid_tag() { [[ "$1" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(\.(0|[1-9][0-9]*)|-[A-Za-z0-9][A-Za-z0-9.-]*)?$ ]]; }
cleanup() { if [[ -n "$stage" ]]; then rm -rf -- "$stage"; fi; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

while (($#)); do
  case "$1" in
    --version) (($# >= 2)) || fail '--version needs a release tag'; requested="$2"; shift 2 ;;
    --bin-dir) (($# >= 2)) || fail '--bin-dir needs a directory'; bin_dir="$2"; shift 2 ;;
    --help|-h)
      printf 'Usage: bash install.sh [--version vA.B.C.D or legacy vX.Y.Z[-prerelease]] [--bin-dir DIR]\nRun again to update. No Git, Rust, root or API key required.\n'
      exit 0 ;;
    *) fail "Unknown argument: $1" ;;
  esac
done
for cmd in curl jq uname mktemp chmod mv cp; do
  command -v "$cmd" >/dev/null || fail "Missing $cmd. In Termux: pkg install curl jq coreutils"
done
if ! command -v sha256sum >/dev/null && ! command -v shasum >/dev/null; then
  fail 'SHA256 utility missing; install coreutils'
fi

os=$(uname -s)
arch=$(uname -m)
android=false
if [[ $(uname -o 2>/dev/null || true) == Android ]]; then android=true; fi
case "${PREFIX:-}" in /data/data/com.termux/files/usr|/data/user/0/com.termux/files/usr) android=true ;; esac
if $android; then
  case "$arch" in aarch64|arm64) platform='android-arm64' ;; *) fail "Unsupported Android architecture: $arch (ARM64 required)" ;; esac
  if [[ -z "$bin_dir" ]]; then
    [[ -n "${PREFIX:-}" ]] || fail 'Run inside Termux, or pass --bin-dir in private executable storage'
    bin_dir="$PREFIX/bin"
  fi
else
  case "$os/$arch" in
    Linux/x86_64|Linux/amd64) platform='linux-x64' ;;
    Darwin/arm64|Darwin/aarch64) platform='macos-arm64' ;;
    *) fail "Unsupported platform: $os/$arch. No incompatible binary will be installed." ;;
  esac
  bin_dir="${bin_dir:-$HOME/.local/bin}"
fi
[[ -z "$requested" ]] || valid_tag "$requested" || fail 'Invalid version tag'
asset="blue-dragon-$platform"
mkdir -p -- "$bin_dir"
bin_dir=$(cd -- "$bin_dir" && pwd -P)
destination="$bin_dir/blue-dragon"
[[ ! -L "$destination" ]] || fail "Refusing to replace a symlink: $destination"
[[ ! -e "$destination" || -f "$destination" ]] || fail "Destination is not a regular file: $destination"
stage=$(mktemp -d "$bin_dir/.blue-dragon-install.XXXXXX")
download() {
  local url="$1" output="$2"
  [[ "$url" == "https://api.github.com/repos/$repo/"* || "$url" == "https://github.com/$repo/releases/download/"* ]] || fail 'Unexpected download origin'
  local label="${output##*/}"
  printf 'Blue Dragon: downloading %s (bounded retries; stalled transfers stop after 20s)...\n' "$label" >&2
  local progress=(--silent)
  if [[ -t 2 ]]; then progress=(--progress-bar); fi
  if ! curl --proto '=https' --proto-redir '=https' --tlsv1.2 --fail "${progress[@]}" --show-error --location \
    --connect-timeout 10 --max-time 120 --speed-limit 1024 --speed-time 20 \
    --retry 1 --retry-delay 2 --retry-max-time 150 "$url" -o "$output"; then
    fail "Download failed for $label; existing installation was not changed. Check connectivity and rerun the installer."
  fi
  printf 'Blue Dragon: downloaded %s\n' "$label" >&2
}

printf 'Blue Dragon: checking published releases for %s...\n' "$platform"
if [[ -n "$requested" ]]; then
  download "$api/releases/tags/$requested" "$stage/api-release.json"
  jq -e 'type == "object" and .draft == false' "$stage/api-release.json" >/dev/null || fail 'Release is not published'
else
  # /releases/latest excludes prereleases; include compatible historical prereleases as well.
  download "$api/releases?per_page=30" "$stage/releases.json"
  jq -e --arg asset "$asset" '[.[] | select(.draft == false) |
    select(any(.assets[]; .name == $asset)) |
    select(any(.assets[]; .name == "SHA256SUMS.txt")) |
    select(any(.assets[]; .name == "LICENSES.txt"))][0] // error("No compatible published release")' \
    "$stage/releases.json" > "$stage/api-release.json" || fail 'No compatible release is ready; existing installation was not changed'
fi
tag=$(jq -er '.tag_name' "$stage/api-release.json")
valid_tag "$tag" || fail 'Release contains an invalid version tag'
[[ -z "$requested" || "$tag" == "$requested" ]] || fail 'Returned release tag does not match the requested version'
verify_names=("$asset" LICENSES.txt)
download_names=("$asset" SHA256SUMS.txt LICENSES.txt)
if [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  verify_names+=(release.json)
  download_names+=(release.json)
fi
for name in "${download_names[@]}"; do
  url=$(jq -er --arg name "$name" '.assets[] | select(.name == $name) | .browser_download_url' "$stage/api-release.json") || fail "Release asset missing: $name"
  [[ "$url" == "https://github.com/$repo/releases/download/$tag/$name" ]] || fail 'Asset does not belong to the selected release'
  download "$url" "$stage/$name"
done

hash_of() {
  if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi
}
printf 'Blue Dragon: verifying SHA256 checksums...\n' >&2
for name in "${verify_names[@]}"; do
  expected=''
  while read -r digest filename remainder; do
    if [[ "$filename" == "$name" ]]; then expected+="$digest"; fi
  done < "$stage/SHA256SUMS.txt"
  [[ "$expected" =~ ^[0-9a-f]{64}$ ]] || fail "Missing or ambiguous SHA256 for $name"
  actual=$(hash_of "$stage/$name"); actual=${actual%% *}
  [[ "$actual" == "$expected" ]] || fail "SHA256 mismatch for $name; installation not changed"
done
if [[ -f "$stage/release.json" ]]; then
  public="${tag#v}"
  package="${public%.*}"
  jq -e --arg version "$public" --arg package "$package" --arg tag "$tag" \
    '.version == $version and .package_version == $package and .tag == $tag and .version_scheme == "four-part-public"' \
    "$stage/release.json" >/dev/null || fail 'Release manifest version mapping mismatch; installation not changed'
fi
chmod 755 "$stage/$asset"
printf 'Blue Dragon: checking binary compatibility...\n' >&2
reported=$("$stage/$asset" --version) || fail 'Downloaded binary cannot run on this device; installation not changed'
[[ "$reported" == "blue-dragon ${tag#v}" ]] || fail "Binary version mismatch: expected ${tag#v}; installation not changed"

# Verification and execution happen before touching the current binary.
if [[ -f "$destination" ]]; then
  installed_hash=$(hash_of "$destination"); installed_hash=${installed_hash%% *}
  candidate_hash=$(hash_of "$stage/$asset"); candidate_hash=${candidate_hash%% *}
  if [[ "$installed_hash" == "$candidate_hash" ]]; then
    mv -f -- "$stage/LICENSES.txt" "$bin_dir/blue-dragon.LICENSES.txt"
    printf '%s is already installed; existing rollback copy preserved.\n' "$reported"
    exit 0
  fi
  cp -p -- "$destination" "$stage/previous"
  mv -f -- "$stage/previous" "$bin_dir/blue-dragon.previous"
fi
mv -f -- "$stage/LICENSES.txt" "$bin_dir/blue-dragon.LICENSES.txt"
mv -f -- "$stage/$asset" "$destination"
printf 'Installed %s to %s\n' "$reported" "$destination"
printf 'Start: %q --provider free tui\n' "$destination"
case ":${PATH:-}:" in *":$bin_dir:"*) ;; *) printf 'For this shell: export PATH=%q:"$PATH"\n' "$bin_dir" ;; esac
printf 'Run this installer again to update. Sessions and source checkouts are untouched.\n'
