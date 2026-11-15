# Public version and Cargo mapping

The candidate public version is **1.1.9.0**, exactly. `VERSION` is its sole
source. Cargo's internal package SemVer is **1.1.9**. A future public version **1.2.1.0** would map to Cargo **1.2.1**; it is not
selected or published by this candidate.
The currently supported mapping is canonical `A.B.C.0` to `A.B.C`. A nonzero fourth
component requires an explicit policy change rather than silently reusing a
package identity. This is public product numbering, not four-part SemVer.

`build.rs` rejects missing/malformed VERSION and mismatched Cargo metadata. It
sets BLUE_DRAGON_PUBLIC_VERSION for Clap --version, diagnostic display and MCP
client identification. `scripts/release_version.py` validates the same mapping
for release and trial-kit manifests. Cargo.lock must match Cargo.toml. No new
runtime dependency or credential is introduced by this mapping.

The tag is `v1.1.9.0`, the binary reports `blue-dragon 1.1.9.0`, and release.json
records version, tag, package_version and version_scheme. The installer requires
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
