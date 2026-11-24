# Install and update Blue Dragon

## Install and start

Use the [one-line command for your platform](../README.md#quick-start) from the project folder you want to work in. It downloads the executable for release `v1.1.14.1`, checks its SHA-256 and reported version, and opens the terminal interface. On a fresh Termux installation, run `pkg install curl` first if curl is missing.

## Start again

Open your project folder and use the command for your system. These commands use the installed executable directly.

| System | Start a session |
|---|---|
| Android ARM64 in Termux (API 26+) | `blue-dragon --provider free tui` |
| Linux x64 or Apple Silicon macOS | `"$HOME/.local/bin/blue-dragon" --provider free tui` |
| Windows x64 in PowerShell | `& "$env:LOCALAPPDATA\BlueDragon\bin\blue-dragon.exe" --provider free tui` |

To check which version is installed, replace `--provider free tui` with `--version`. The Android download is a Termux command-line executable. The Linux build targets Ubuntu-compatible systems and may require a newer glibc than older distributions provide.

## Install or update without opening a session

On Termux, Linux and macOS, the [Unix installer](../scripts/install.sh) supports a specific release tag, a custom directory and a rollback copy. It requires Bash, curl, jq and a SHA-256 utility. To install this release:

```bash
curl -fsSL https://raw.githubusercontent.com/khdertaysh1/khdertaysh1-blue-dragon-agent/v1.1.14.1/scripts/install.sh | bash -s -- --version v1.1.14.1
```

Omitting `--version` selects the first compatible published release in GitHub's release list, which can include prereleases. The pinned quick-start command always selects the version in its URL.

On Windows, the [PowerShell installer](../scripts/install.ps1) downloads the x64 executable and prints its version. Run this in PowerShell:

```powershell
& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing 'https://raw.githubusercontent.com/khdertaysh1/khdertaysh1-blue-dragon-agent/v1.1.14.1/scripts/install.ps1').Content))
```

## Use an earlier version

When `install.sh` replaces an existing Unix installation with different bytes, it saves the old executable as `blue-dragon.previous` alongside the new one. In Termux, use `"$PREFIX/bin/blue-dragon.previous" --version` to check that copy. For Windows or an installation made with the Unix quick-start script `start.sh`, use the desired release download to roll back. You can also select a specific version from [published releases](https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/releases).

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

GitHub Actions publishes only after Windows/Linux/macOS tests/builds, Android ARM64 cross-build, installer/legacy-upgrade regressions, and Linux real-browser tests pass. A release starts as a draft; all binaries, unsigned developer browser-extension archives, SHA256 checksums, source commit metadata and combined license notices are uploaded before publication. Published assets are not replaced by later main builds with the same version; choose a new public version for each update following the [version mapping](PUBLIC_VERSION.md). A failed draft upload can resume only for the same source commit.

Browser control needs an installed browser and explicit connection. The extension archives are not signed/store-approved. See [BROWSER.md](BROWSER.md) before installing a bridge, especially on Android.

To inspect a URL directly: `blue-dragon 'https://example.com'`. Termux first use prepares official Chromium automatically; see [URL_INSPECTION.md](URL_INSPECTION.md). This is a separate browser package download, not a Rust build.
