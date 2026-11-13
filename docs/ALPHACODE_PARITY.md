# Blue Dragon / AlphaCode: capability tracking

Blue Dragon is independently written. This is an implementation audit, not a claim of feature parity.

| Capability | Status in v0.4 alpha |
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
| Full-screen TUI / streaming UI | Implemented; OpenAI-compatible and Anthropic SSE, local fixture-tested |
| Chromium/Firefox browser automation | Not implemented |
| Desktop control | Not implemented |
| MCP client | stdio tools implemented, explicitly connected/approved; other transports/features absent |
| Multi-agent swarm and isolated worktrees | Not implemented |
| Model router / free built-in provider | Not implemented |
| Native desktop GUI | Not implemented |

Next milestones: wider MCP and device verification, AST-oriented indexing, isolated patch worktrees, browser and MCP integration, evidence-first test reports, crash recovery, native installers.

Acceptance gate: complete CI on Windows/Linux/macOS, functional model tests with opted-in credentials, repeated benchmark tasks on identical model settings, no unverified success claims.

Task plans, action summaries, cancellation, directory creation and approved bounded web fetch are implemented. No model-quality parity percentage is claimed. See WORKBENCH.md and third_party/NOTICE.md for limits and attribution.
