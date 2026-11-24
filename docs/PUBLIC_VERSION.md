# Public version and Cargo mapping

The public CLI and release tag use the four numbers in `VERSION`. The first three
also identify the Cargo package. For example, public release **1.1.14.1** uses
Cargo package **1.1.14**. The fourth number distinguishes maintenance revisions;
all four components are unsigned decimal numbers with no leading zeroes.

`build.rs` rejects missing/malformed VERSION and mismatched Cargo metadata. It
sets BLUE_DRAGON_PUBLIC_VERSION for Clap --version, diagnostic display and MCP
client identification. `scripts/release_version.py` validates the same mapping
for release and trial-kit manifests. Cargo.lock must match Cargo.toml. No new
runtime dependency or credential is introduced by this mapping.

The tag is `v1.1.14.1`, the binary reports `blue-dragon 1.1.14.1`, and release.json
records version, tag, package_version and version_scheme. The manifest-aware installer in `scripts/install.sh` requires
and checksum-verifies release.json for four-part tags, checks its mapping, then
requires exact CLI/tag equality before replacing an existing binary. Explicit
requested tags must match the API response. Legacy three-part alpha tags retain
their original verification path and require no rewritten assets. Use the new
installer for four-part releases; an old installer may reject the new syntax.

Offline installer fixtures cover upgrading an alpha.6 executable, preserving
session/settings bytes and a source checkout, rollback preservation, repeated
updates and rejection of checksum/version/manifest/tag disagreement. These are
Linux-hosted Termux-shaped fixtures, not physical Android validation. Release
payload tests check manifest/tag/CLI mapping separately from platform cross-builds.

Publication is pending review and final CI. The release workflow refuses to
replace an already published same-tag release; tag/release availability must be
checked immediately before publication. Existing
alpha.6 and earlier tags/assets stay immutable. The number does not certify
stability, provider capacity, AlphaCode parity or a successful phone trial.
