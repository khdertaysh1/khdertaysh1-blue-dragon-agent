# Install and update Blue Dragon

## Termux: regular users

```bash
pkg install curl jq -y &&
curl -fsSL https://raw.githubusercontent.com/khdertaysh1/khdertaysh1-blue-dragon-agent/main/scripts/install.sh | bash
```

This downloads a published Android ARM64 executable into Termux's private `$PREFIX/bin`. No Rust, Cargo, Git clone, root, checkout or lockfile is involved. Do not install it in shared Downloads/SD-card storage, which may not allow execution. Source-build problems do not affect this path.

```bash
blue-dragon --version
cd /path/to/your/project
blue-dragon --provider free tui
```

The project directory is the workspace. Existing model/provider configuration is preserved; old conflicting `BLUE_DRAGON_BASE_URL` or `BLUE_DRAGON_MODEL` exports should be unset if you want to change provider. Kilo free models require no API keys; data handling is described in [MODELS.md](MODELS.md).

## Update / pinned release / rollback

Run the same installer to update. It selects the newest compatible **published** release, including prereleases; draft/incomplete releases are excluded. All current builds are alpha, not a stable channel. Exact version installation is available:

```bash
curl -fsSL https://raw.githubusercontent.com/khdertaysh1/khdertaysh1-blue-dragon-agent/main/scripts/install.sh |
  bash -s -- --version v0.5.0-alpha.1
```

The installer detects OS/CPU, downloads a raw executable plus license notices, checks SHA256, and runs `--version` before replacing an existing binary. The final move stays on the same filesystem. A failed network request, missing release, checksum/version mismatch, unsupported CPU or non-executable storage leaves the installed binary intact. This is integrity verification using GitHub HTTPS/checksums, not an independent code-signing system.

The old executable remains as `blue-dragon.previous` next to the new one. Reinstalling identical bytes preserves that rollback copy. To try it without removing the current version:

```bash
"$PREFIX/bin/blue-dragon.previous" --version
"$PREFIX/bin/blue-dragon.previous" tui
```

The installer does not modify workspaces, Git branches, shell profiles, sessions or provider keys. License notices are saved beside the executable. It prints the exact installation path and, when needed outside Termux, a PATH command for the current shell. `--bin-dir DIR` selects another executable private directory. It refuses a destination symlink instead of replacing it.

## Supported binary targets

| Environment | Target | Notes |
|---|---|---|
| Termux / Android | ARM64, API26+ | Cross-built with NDK; physical phone verification still needed |
| Linux | x86_64 | Current Ubuntu build; older glibc distributions may not work |
| macOS | Apple Silicon ARM64 | Unix installer needs curl/jq and SHA256 utility |
| Windows | x64 | Download the `.exe` and notices/checksums from Releases; this Bash installer is not a Windows installer |

Unsupported CPU/OS combinations stop explicitly; a Linux executable is never substituted for Android. There is no APK. Release downloads persist on GitHub; CI artifacts are temporary and are not the regular install path.

## Developer / old source checkout

`Cargo.lock` is committed for this binary application. CI verifies a clean checkout with `cargo build --locked --release`; users should not remove `--locked` or regenerate dependencies to install a release. Source builds need the required Rust/toolchain dependencies; current CI uses stable Rust.

For a clean developer clone:

```bash
git clone https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent.git
cd khdertaysh1-blue-dragon-agent
cargo build --locked --release
./target/release/blue-dragon --version
```

For an older checkout where Cargo generated an untracked lockfile, use the source updater from the repository's current main version:

```bash
curl -fsSL https://raw.githubusercontent.com/khdertaysh1/khdertaysh1-blue-dragon-agent/main/scripts/update-source.sh | bash
```

It verifies the configured repository, refuses tracked local edits, fetches main, preserves an untracked Cargo.lock in a unique application-data backup, switches/fast-forwards without forcing, and builds with `--locked`. It never resets, cleans, stashes or deletes source changes. Unrelated untracked conflicts or divergent local commits stop rather than overwrite user work. Regular users can instead install the binary and leave their old checkout intact.

## Release safeguards

GitHub Actions publishes only after Windows/Linux/macOS tests/builds, Android ARM64 cross-build, installer/legacy-upgrade regressions, and Linux real-browser tests pass. A release starts as a draft; all binaries, unsigned developer browser-extension archives, SHA256 checksums, source commit metadata and combined license notices are uploaded before publication. Published assets are not replaced by later main builds with the same version; bump Cargo version to publish an update. A failed draft upload can resume only for the same source commit.

Browser control needs an installed browser and explicit connection. The extension archives are not signed/store-approved. See [BROWSER.md](BROWSER.md) before installing a bridge, especially on Android.
