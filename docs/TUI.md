# Blue Dragon terminal UI (v0.3 alpha)

Blue Dragon now includes a cross-platform full-screen terminal interface built using Ratatui and Crossterm. It shares the same Rust agent and tools as the plain CLI.

## Run

~~~bash
cargo build --release
export BLUE_DRAGON_API_KEY="YOUR_KEY"
./target/release/blue-dragon --root /path/to/your/project tui
~~~

Windows: use target/release/blue-dragon.exe from PowerShell, with provider variables set using PowerShell syntax. To use a local tool-calling model or the native Anthropic API, follow the root README.

## Keyboard

| Input | Purpose |
|---|---|
| Enter | Send task or slash command |
| /map | Project structure |
| /diff or /status | Git changes |
| /skills or /skill NAME | Local Markdown skills |
| /session or /doctor | Session and model |
| /clear | Clear displayed history, not persisted session |
| PageUp or PageDown | Scroll the transcript |
| Y or N while approval is open | Approve or reject precisely one action |
| Up or Down while approval is open | Scroll command or diff details |
| Escape or Ctrl+C or /exit | Exit; pending approval is denied |

Use --read-only to prevent commands and file writes.

## Security and status

Every write or command still requires approval, shown in the foreground terminal while the model agent runs on a worker thread. If the terminal closes, pending approval defaults to denial. Approval details larger than 8 KiB are refused rather than silently truncated. This is not a sandbox: approved commands execute on the host OS.

The TUI shows tool-step events and final model responses, but not individual token streaming. Browser automation, MCP and multi-agent orchestration are not yet available. GitHub CI checks Windows, Linux and macOS. Termux support is not independently verified.
