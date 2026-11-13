# Blue Dragon / AlphaCode: capability tracking

Blue Dragon is independently written. This is an implementation audit, not a claim of feature parity.

| Capability | Status in v0.2 alpha |
|---|---|
| Rust CLI and cross-platform CI | Implemented |
| OpenAI-compatible tool calling | Implemented |
| Native Anthropic API | Adapter + unit tests; live access not validated |
| Source search/read and targeted edit | Implemented |
| Project indexing | Heuristic file overview, not an AST graph |
| Git diff and Git status | Implemented |
| Explicit approval for writes and commands | Implemented |
| Local user skills | Markdown files; user must activate them |
| Persistent sessions and resume | Implemented |
| Full-screen TUI / streaming UI | Not implemented |
| Chromium/Firefox browser automation | Not implemented |
| Desktop control | Not implemented |
| MCP client | Not implemented |
| Multi-agent swarm and isolated worktrees | Not implemented |
| Model router / free built-in provider | Not implemented |
| Native desktop GUI | Not implemented |

Next milestones: streaming ratatui UI, AST-oriented indexing, isolated patch worktrees, browser and MCP integration, evidence-first test reports, crash recovery, native installers.

Acceptance gate: complete CI on Windows/Linux/macOS, functional model tests with opted-in credentials, repeated benchmark tasks on identical model settings, no unverified success claims.
