Blue Dragon 0.4.0-alpha.4 adds a binary installer and verified public release assets.

- Termux ARM64 users can install/update without Git, Cargo or compiling dependencies.
- The installer detects the target platform, checks SHA256 and executes a version preflight before replacing the current binary. A failed download/check leaves the installed binary intact. A previous binary is retained for rollback.
- Sessions, API settings, workspaces and source checkouts are preserved. The legacy source updater backs up generated untracked Cargo.lock files and refuses tracked local changes.
- Releases publish only after desktop tests/builds, Android ARM64 cross-build and installer/upgrade regression checks pass. Assets include checksums, version metadata and third-party licenses.
- Existing features include the keyless Kilo free-model picker, approved workspace tools, streamed responses, plans, sessions and opt-in stdio MCP.

This is still an alpha release. Android binaries target ARM64/API26+ and have not been tested on a physical phone. Linux binaries are built on Ubuntu's current runner and may need a recent glibc. macOS is Apple Silicon; Windows is x64. Free model access is subject to service availability/data handling documented in docs/MODELS.md. Browser automation, smart context compaction and multi-agent worktrees are not implemented.
