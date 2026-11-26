<p align="center"><img src="assets/abyss-banner.svg" alt="Blue Dragon" width="100%"></p>

# Blue Dragon

Blue Dragon is a Rust coding agent for the terminal. Open it in your project to trace a function, review a change, or investigate what a page does in the browser. The terminal shows the proposed diff before you approve a file edit.

[![Release](https://img.shields.io/github/v/release/khdertaysh1/khdertaysh1-blue-dragon-agent)](https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/releases/latest) [![CI](https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/actions/workflows/ci.yml) [![MIT license](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

## Quick Start

Open a terminal in the project folder you want Blue Dragon to work in. These commands install and start **Blue Dragon 1.1.14.3** and verify the executable's SHA-256 and reported version.

**Linux x64, macOS on Apple Silicon, or Android ARM64 in Termux** (Bash and curl)

```bash
curl -fsSL https://raw.githubusercontent.com/khdertaysh1/khdertaysh1-blue-dragon-agent/v1.1.14.3/scripts/start.sh | bash
```

**Windows x64** (PowerShell)

```powershell
& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing 'https://raw.githubusercontent.com/khdertaysh1/khdertaysh1-blue-dragon-agent/v1.1.14.3/scripts/start.ps1').Content))
```

The release includes platform binaries, SHA-256 checksums and browser extensions. For a first session on your phone, follow the Termux walkthrough. [Downloads](https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/releases/tag/v1.1.14.3) · [Termux walkthrough](docs/FIRST_SESSION.md) · [Install and update](docs/INSTALL.md)

## Graphical workspace (development build)

Run `blue-dragon gui` in your project directory on Windows, Linux or macOS. On Android, run `blue-dragon tui` in Termux. The Android `gui` alias also opens the terminal. Type `/steps` there to expand or collapse tool activity. On desktop this opens a native Blue Dragon window. Choose **Open project** to switch workspaces, or **Sessions** to resume a conversation. The next launch restores the last desktop session. Open a relative file reference such as `src/main.rs:16` in a reply to see the highlighted source line. Use `gui --no-open` for the private web client. The Linux Debian package adds an Applications entry and the macOS package installs Blue Dragon.app. On Windows, run `.\blue-dragon.exe gui` or register the executable with `.\scripts\register-desktop.ps1 -Binary .\blue-dragon.exe` from a source checkout.

Use **Sessions** to choose a workspace or continue a saved conversation. Messages, tool results and approval requests use the same agent and session store as the terminal. The global `--root`, `--resume` and `--read-only` options work with `gui`. The server listens only on IPv4 loopback and the launch URL grants access to that running instance.

## What's new in 1.1.14.3

- Local file search reports verified line numbers and exact matching lines when available, including in compact context.
- The terminal can approve a local form interaction inline and remember consent on that page.
- Browser precondition failures distinguish an action that never ran from an uncertain outcome.

[Full release notes](docs/RELEASE_NOTES.md) · [Documentation](docs/README.md)

## Capabilities

| Area | What is currently available |
|:--|:--|
| **AI providers** | Direct keyless Kilo and JarvisClaw gateways, user-selected quota fallback, local Ollama |
| **Workspace intelligence** | Tree-sitter symbols/imports/syntactic references for Rust, Python, JS and TS/TSX; bounded ranked local code search and exact file reads |
| **Code changes** | Targeted exact-match edit and full text-file write, both behind diff approval |
| **Git inspection** | Working-tree status and unstaged diff |
| **Execution** | Commands with explicit approval, timeout and bounded output |
| **Memory & context** | Original sessions, budgeted request projection, exact recall, scoped historical project notes and evidence-linked findings |
| **Skills** | Opt-in local Markdown workflows, without running bundled scripts |
| **Safety controls** | `--read-only`, protected-path checks and deny-by-default noninteractive approvals |
| **Terminal** | Full-screen `tui`, streamed responses, approval overlay, cancellation, plan pane; CLI `chat`, `run` and diagnostics |
| **MCP** | Connect a stdio or Streamable HTTP server, approve tool calls, read text resources, and preview server prompts. [MCP guide](docs/MCP_RESOURCE_CONTEXT.md) |
| **Planning & evidence** | Saved task steps, bounded tool-action report, resume, exact native-result acceptance checks with freshness tracking |
| **Network & files** | Approved text web fetch, directory creation, file metadata |
| **Real browser** | Connect via CDP or a paired Firefox/Chromium extension to inspect page DOM, click/fill, JavaScript, network metadata, console, bounded source and local screenshots |
| **Parallel code review** | `/swarm PATH \| TASK` runs implementation and test reviewers concurrently and collects their evidence. |
| **JavaScript review** | Search saved source files, follow imports and named calls across cached modules, and inspect assignments within a function. Findings cite source locations and identify unresolved paths. |
| **Recorded HTTP requests** | `/httpflow EVIDENCE_ID [OFFSET]` pages through network observations already captured by the browser. |
| **Web search and desktop editor** | `/websearch WORDS` uses a configured JSON-enabled SearXNG instance. `/desktop open PATH` opens a workspace text file in an installed editor. |
| **Long reviews** | `/compact` creates a local session recap. `/resume-review` continues an interrupted file review. |
| **Project and automation interfaces** | Workspace-root `AGENTS.md` supplies project conventions. Use `run --json TASK` for a final result or `run --ndjson TASK` for an event stream. |
| **Blue Free** | Keyless gateway automatic routing; automatic alias and concrete model choices stay pinned in the client |

## Working in a project

The default Blue Free route uses a hosted model. For a local model or a different API provider, see [model setup](docs/MODELS.md). Files and tool output included in a request can reach the selected provider. Approved commands run with your account's permissions. See the [security model](SECURITY.md).

Use `/help` in a session to see commands. From the shell, `blue-dragon map` lists the files and languages in the current project without calling a model. For a working example with a JavaScript file and a reviewable edit, try [your first session](docs/FIRST_SESSION.md).

## Development

Build from a checkout using the stable Rust toolchain:

```sh
cargo build --locked --release
cargo test --locked --all-targets
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
```

Start a source build with `./target/release/blue-dragon tui`. See the [architecture](docs/ARCHITECTURE.md), [documentation index](docs/README.md), [release history](docs/RELEASE_NOTES.md), and [third-party notices](third_party/NOTICE.md).
