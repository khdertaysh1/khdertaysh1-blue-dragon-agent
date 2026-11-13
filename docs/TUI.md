# Integrated terminal — 0.4 alpha

Run:
    cargo build --locked --release
    ./target/release/blue-dragon tui

With an interactive terminal, launching blue-dragon without a subcommand also opens the TUI. Chat/run remain available for plain-terminal workflows. The same model/provider environment settings apply to all modes.

| Control | Action |
|---|---|
| Enter | Submit task or slash command |
| Left/Right/Home/End, Backspace/Delete | Edit the current prompt |
| Bracketed paste | Paste without interpreting text as approval keystrokes |
| PageUp/PageDown | Scroll the transcript |
| Y/N or Escape during approval | Approve/deny exactly the proposed action |
| Up/Down during approval | Scroll approval detail |
| Ctrl+C during work | Request cancellation |
| Ctrl+C while idle, Escape, /exit | Exit and deny pending actions |
| /plan, /report | Show saved task steps or observed tool-action summary |
| /mcp, /connect NAME | List configured MCP servers or approve a connection |

Wide terminals display a plan pane; narrow terminals use a single conversation. Responses stream as provider deltas arrive. An approval modal protects writes, commands and external actions; it is not an OS sandbox. Long approval text beyond 8 KiB is refused, rather than hidden.

Cancellation is cooperative and may wait for blocked provider I/O (request deadline 120 seconds). Direct tool processes are killed on cancellation; process descendants have no cross-platform isolation guarantee. MCP input/request limits and supported features are detailed in [WORKBENCH.md](WORKBENCH.md).

Validation includes small/wide TestBackend rendering and a real Unix PTY launch/help/exit/mode-restoration smoke test. No physical Android or Windows Terminal input-latency benchmark has been performed.

Use `/models` or `/model` for the free-model picker: type to filter, Up/Down to choose, Enter to select/save, Esc to close. `/models refresh` updates the Kilo free catalog; `/model kilo MODEL_ID` also works directly. `/models openrouter` lists the separate OpenRouter catalog (requires a key to use). The selected model/endpoint updates in the status bar. See [MODELS.md](MODELS.md).
