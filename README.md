<p align="center">
  <img src="assets/blue-dragon-hero.webp" alt="Blue Dragon — an intact electric-blue dragon with cyber lighting" width="100%" />
</p>

<h1 align="center">BLUE DRAGON</h1>

<p align="center"><strong>An open-source, cross-platform AI coding agent built with Rust.</strong></p>

<p align="center"><em>Inspect the project. Plan with context. Review every change.</em></p>

<p align="center">
  <a href="Cargo.toml"><img alt="Rust" src="https://img.shields.io/badge/Rust-2021-0c1c31?logo=rust&amp;logoColor=38d6f5"></a>
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/License-MIT-0c1c31?labelColor=09213a"></a>
  <a href="https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/actions/workflows/ci.yml"><img alt="Build status" src="https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <img alt="Developer preview" src="https://img.shields.io/badge/Status-developer%20preview-2374bd">
</p>

<p align="center">
  <a href="#quick-start">Get started</a> ·
  <a href="#the-dragon-mind">Explore the agent</a> ·
  <a href="#terminal-preview">Animated preview</a> ·
  <a href="docs/SHOWCASE.md">Visual gallery</a> ·
  <a href="#capabilities">Capabilities</a> ·
  <a href="#human-in-control">Safety</a> ·
  <a href="#roadmap">Roadmap</a>
</p>

> [!IMPORTANT]
> **Developer preview: `v0.4.0-alpha.1`.** Blue Dragon has a working, testable agent core, but is **not** feature-equivalent to AlphaCode, Codex CLI or Claude Code. This workbench revision adds a full-screen TUI, streamed model responses, task plans, action reports and opt-in stdio MCP tools. Browser/desktop automation and multi-agent orchestration are not implemented yet. See [workbench setup and limits](docs/WORKBENCH.md).

## The Dragon Mind

<div align="center">
  <img src="assets/dragon-mind.svg" alt="Illustrated neural workflow: goal, project map, tools, user approval, verification and evidence" width="100%" />
  <sub>Animated neural workflow: travelling signals, rotating orbit and neural pulses. Motion is decorative; the agent runs in Rust.</sub>
</div>

Blue Dragon is designed around a simple engineering loop: **understand → act with approval → verify → explain**. Unlike an opaque "autopilot", writes and subprocess commands are presented for explicit user approval. The project is independently developed; a small adapted MIT protocol component is credited in [third-party notices](third_party/NOTICE.md).

## Capabilities

| Area | What is currently available |
|:--|:--|
| **AI providers** | OpenAI-compatible tool calling, plus a native Anthropic Messages adapter (unit-tested) |
| **Workspace intelligence** | Bounded project map, file listing, file reading, line-range reads and string search |
| **Code changes** | Targeted exact-match edit and full text-file write, both behind diff approval |
| **Git inspection** | Working-tree status and unstaged diff |
| **Execution** | Commands with explicit approval, timeout and bounded output |
| **Memory** | Locally saved JSON sessions, resume by ID or latest |
| **Skills** | Opt-in local Markdown workflows, without running bundled scripts |
| **Safety controls** | `--read-only`, protected-path checks and deny-by-default noninteractive approvals |
| **Terminal** | Full-screen `tui`, streamed responses, approval overlay, cancellation, plan pane; CLI `chat`, `run` and diagnostics |
| **MCP** | Opt-in stdio servers, approved launch/discovery/tool calls; no auto-execution of repository config |
| **Planning & evidence** | Saved task steps, bounded tool-action report, resume |
| **Network & files** | Approved text web fetch, directory creation, file metadata |

<details>
<summary><b>See all 14 built-in agent tools</b></summary>

```text
list_files     read_file      read_lines    search_files
project_map    git_status     git_diff      edit_file
write_file     run_command    update_plan   create_directory
file_info      web_fetch
```

Every write or command requires a per-action approval. Tool availability depends on the local environment.

</details>

## Terminal Preview

<div align="center">
  <a href="assets/neural-console-preview.gif">
    <img src="assets/neural-console-preview.gif" alt="Animated illustrative walkthrough of the Blue Dragon terminal: project map, tool steps, and an explicit approve-or-deny dialog" width="100%" />
  </a>
  <br />
  <sub><strong>Animated product concept</strong> — not a recorded live session or a claim of current runtime functionality. Click to open the animation.</sub>
</div>

Run the actual terminal with `blue-dragon tui`. It includes a conversation, live output, tool progress and approval modal. The artwork above remains an illustration; it is not a captured runtime session. See [workbench documentation](docs/WORKBENCH.md).

<details>
<summary><strong>Explore the static terminal mockup and the design notes</strong></summary>

<br />
<p align="center">
  <img src="assets/tui-preview.svg" alt="Static illustrated mockup of the experimental terminal UI, with a proposed change approval" width="100%" />
</p>

The demo is deliberately illustrative. Commands and results are sample text; they are **not** captured execution evidence. The agent's real, tested capabilities are listed in the table above. See the [visual identity and motion notes](docs/VISUAL_IDENTITY.md) for the editable asset inventory.

</details>

## Quick Start

Requires a **Rust toolchain** and a model endpoint supporting tool/function calling. Git is recommended.

```bash
git clone https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent.git
cd khdertaysh1-blue-dragon-agent
cargo build --locked --release
./target/release/blue-dragon doctor
```

Start the agent with a hosted, OpenAI-compatible endpoint:

```bash
export BLUE_DRAGON_API_KEY="YOUR_API_KEY"
export BLUE_DRAGON_MODEL="YOUR_TOOL_CALLING_MODEL"
./target/release/blue-dragon --root . tui
```

Prefer to try inspection before connecting AI? The project map and diagnostics run locally:

```bash
./target/release/blue-dragon map
./target/release/blue-dragon skills
./target/release/blue-dragon doctor
```

<details>
<summary><b>Windows PowerShell setup</b></summary>

```powershell
$env:BLUE_DRAGON_API_KEY = "YOUR_API_KEY"
$env:BLUE_DRAGON_MODEL = "YOUR_TOOL_CALLING_MODEL"
.\target\release\blue-dragon.exe doctor
.\target\release\blue-dragon.exe chat
```

</details>

<details>
<summary><b>Anthropic Claude native API</b></summary>

```bash
export BLUE_DRAGON_BASE_URL="https://api.anthropic.com/v1"
export BLUE_DRAGON_MODEL="YOUR_CLAUDE_MODEL_ID"
export ANTHROPIC_API_KEY="YOUR_ANTHROPIC_API_KEY"
./target/release/blue-dragon chat
```

The protocol adapter is unit-tested, but live credentials have not been exercised in CI.

</details>

<details>
<summary><b>Local, OpenAI-compatible model endpoint</b></summary>

```bash
BLUE_DRAGON_BASE_URL=http://127.0.0.1:11434/v1 \
BLUE_DRAGON_MODEL=YOUR_TOOL_CALLING_MODEL \
./target/release/blue-dragon chat
```

Choose a model with compatible function calling. **No free hosted AI provider is bundled.**

</details>

<details>
<summary><b>Termux / Android (not yet verified in CI)</b></summary>

```bash
pkg install git rust clang pkg-config
git clone https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent.git
cd khdertaysh1-blue-dragon-agent
cargo build --locked --release
./target/release/blue-dragon doctor
```

Termux builds are device and package dependent; Linux CI success does not establish Android compatibility.

</details>

## How It Works

```mermaid
flowchart LR
    U["Developer task"] --> M["Project context"]
    M --> A["Blue Dragon agent"]
    A --> R{"Read-only tool?"}
    R -->|Yes| T["Read / inspect"]
    R -->|No| P{"User approves?"}
    P -->|Yes| X["Edit or execute"]
    P -->|No| D["Deny action"]
    T --> V["Review evidence"]
    X --> V
    D --> V
    V --> S["Save session & report"]
```

For module boundaries, provider integration and tool definitions, see [Architecture](docs/ARCHITECTURE.md).

## Human in Control

Blue Dragon is **not a sandbox**. Approved commands run on your machine; inspect each proposal before typing `y`. Files and tool transcripts can be sent to the configured model service. Secrets, excluded paths and size limits have safety checks, but cannot guarantee isolation from a hostile local workspace.

```bash
./target/release/blue-dragon --root . --read-only run "Review the project structure"
./target/release/blue-dragon --root . --resume latest chat
```

[Read the security model](SECURITY.md) · [Review architecture](docs/ARCHITECTURE.md)

## Roadmap

<details>
<summary><b>Delivered — Agent foundation</b></summary>

- [x] Rust CLI and OpenAI-compatible tool calling
- [x] Native Anthropic API conversion adapter
- [x] Ten workspace tools, diff review and explicit approvals
- [x] Project mapping, skills and session resume
- [x] Cross-platform CI on Windows, Linux and macOS

</details>

<details>
<summary><b>In progress — A more capable developer experience</b></summary>

- [x] Integrated full-screen TUI, streaming, task plans and opt-in stdio MCP
- [ ] Physical-device terminal testing and wider MCP compatibility
- [ ] Better patch-oriented edits, context compaction and richer code navigation
- [ ] Reliable release packaging and Termux-specific verification

</details>

<details>
<summary><b>Planned — Beyond the terminal</b></summary>

- [ ] Browser/desktop automation, remote MCP transports and isolated worktrees
- [ ] Parallel agents, evidence-driven verification and security analysis modules
- [ ] Optional native desktop interface and reproducible performance benchmarks

</details>

## Development & Contributing

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --locked --release
```

Read the [design notes](docs/VISUAL_IDENTITY.md), [feature-parity tracker](docs/ALPHACODE_PARITY.md), or open an issue with a reproducible bug. Contributions should preserve explicit approval, clear evidence and truthful documentation.

<div align="center">
  <img src="assets/dragon-mark.svg" width="80" alt="Blue Dragon logo"/>
  <br/>
  <sub>BLUE DRAGON · Rust · MIT License · Independent open-source project</sub>
</div>
