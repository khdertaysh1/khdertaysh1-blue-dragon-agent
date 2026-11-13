# BLUE DRAGON 🐉

**Cross-platform AI coding agent — built in Rust, designed for explicit consent and verifiable changes.**

> **Status: v0.1.0 developer preview.** This is a functioning initial agent core, **not** feature parity with AlphaCode, Codex CLI, or Claude Code. A full-screen TUI, browser automation, multi-agent orchestration, MCP, and security scanner integrations are roadmap items, not current features.

## Why Blue Dragon?

Blue Dragon takes an evidence-first approach to autonomous code work: inspect the project, ask before making changes, execute tests with approval, and preserve an auditable local conversation. It is an independently implemented project; it is **not** a fork or source-code copy of AlphaCode.

### Available today

- OpenAI-compatible `chat/completions` tool-calling API (cloud endpoints and local Ollama/LM Studio adapters where compatible).
- Six tools: `list_files`, `read_file`, `search_files`, `git_diff`, `write_file`, `run_command`.
- Mandatory interactive approval for **every** write or command. `--read-only` blocks both; noninteractive stdin defaults to **deny**.
- Path traversal and protected-secret path blocking; file size limits, output caps, command timeout, and 12 tool-call rounds per turn.
- Local JSON session persistence, `--resume <id>` or `--resume latest`, session listing.
- `doctor`, interactive `chat`, and noninteractive `run "task"` (mutations still need explicit approval).
- CI on Linux, Windows, macOS; single executable produced by `cargo build --release`.

## Quickstart

Requirements: stable Rust toolchain, Git optional, and a tool-calling model endpoint. On Windows, use PowerShell or Windows Terminal; on Linux/macOS, a terminal; on Termux, a Rust toolchain compatible with your device is required.

```bash
git clone https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent.git
cd khdertaysh1-blue-dragon-agent
cargo build --release
```

**Hosted model:**

```bash
export BLUE_DRAGON_API_KEY="<your-api-key>"
export BLUE_DRAGON_MODEL="gpt-4o-mini"
./target/release/blue-dragon doctor
./target/release/blue-dragon --root . chat
```

Windows PowerShell:

```powershell
$env:BLUE_DRAGON_API_KEY = '<your-api-key>'
$env:BLUE_DRAGON_MODEL = 'gpt-4o-mini'
.\target\release\blue-dragon.exe doctor
.\target\release\blue-dragon.exe --root . chat
```

**Local OpenAI-compatible server (no API key when the server permits it):**

```bash
BLUE_DRAGON_BASE_URL=http://127.0.0.1:11434/v1 \
BLUE_DRAGON_MODEL=your-tool-calling-model \
./target/release/blue-dragon --root /path/to/your/project chat
```

Use a model that actually supports OpenAI-style function/tool calling; some local models do not. Provider credentials are supplied via environment variables, **not** committed to git. Custom remote endpoints must use HTTPS.

## CLI

```text
blue-dragon --help
blue-dragon doctor
blue-dragon sessions
blue-dragon --root /path/to/project chat
blue-dragon --root /path/to/project run "Explain the code structure"
blue-dragon --root /path/to/project --read-only run "Review the repository"
blue-dragon --root /path/to/project --resume latest chat
```

Inside chat: `/help`, `/diff`, `/session`, `/doctor`, `/exit`.

## Security and limitations

- **Not a sandbox.** Approved commands execute on your operating system. Review the exact command before typing `y`. Read [SECURITY.md](SECURITY.md).
- No automatic deletion, deployment, or publishing. Every write/command requires explicit approval.
- Do not use untrusted projects as a security boundary; symlink checks are best-effort and cannot protect against concurrent adversarial filesystem mutation.
- File contents and tool transcripts may be sent to the chosen model provider. Check privacy requirements before running on private code.
- Session JSON under your OS application data directory contains conversation content and tool results. Protect this folder; Unix session files are written with `0600` permissions.
- Currently writes text files inside **existing** project directories. The API does not make nested folders automatically. `git_diff` displays unstaged changes only.
- Model provider availability, cost, compatibility, and authentication depend on the chosen service. No built-in free AI provider is included.

## Roadmap

1. **v0.2**: streaming terminal UI (`ratatui`), patch-oriented editing, contextual project map, richer diff review.
2. **v0.3**: browser via Playwright/CDP, MCP clients, parallel isolated worktrees, test evidence engine.
3. **v0.4**: security modules (SAST, configuration, dependency scans), policy engine, reproducible benchmarks.
4. **v1.0**: optional native desktop GUI plus release installers and verified cross-platform performance.

## Development

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

MIT licensed. This project is independently authored and does not claim affiliation with AlphaCode.
