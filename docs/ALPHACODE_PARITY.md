# Blue Dragon / AlphaCode: capability tracking

Blue Dragon is independently written. This is an implementation audit, not a claim of feature parity.

| Capability | Status on this branch |
|---|---|
| Rust CLI and cross-platform CI | Implemented |
| OpenAI-compatible tool calling | Implemented |
| Native Anthropic API | Adapter + unit tests; live access not validated |
| Source search/read and targeted edit | Implemented |
| Project indexing | Bounded content-hashed index; Tree-sitter declarations/imports/syntactic references in Rust, Python, JavaScript and TypeScript/TSX; lexical ranking, not resolved cross-file semantic graph |
| Persistent project memory | Workspace/target-scoped historical notes with qualified source-session IDs, atomic locked storage, lexical retrieval; no embedding/semantic graph |
| Verification contracts | Exact native tool/input/result metadata with bounded snapshot freshness, stale-proof invalidation and one completion review; model criteria may be insufficient |
| Git diff and Git status | Implemented |
| Explicit approval for writes and commands | Implemented |
| Local user skills | Manual Markdown skills plus turn-local automatic workspace-read guides. Manual selection wins; unique templates route locally and paraphrases can use the model-facing catalog. |
| Persistent sessions and resume | Atomic checkpoints, process-exit lock release and interrupted tool-history recovery. Child checkpoints record interrupted work. See [session recovery](RESUME_AND_REPRODUCTION.md). |
| Context and task control | Budgeted non-destructive request projection, exact recall, adaptive task guidance, stalled-loop handling and bounded empty/context recovery; no semantic memory graph |
| Full-screen TUI / streaming UI | Implemented; OpenAI-compatible and Anthropic SSE, local fixture-tested |
| Chromium/Firefox browser automation | URL-only automatic preparation/initial inspection, official Termux Chromium setup, own CDP client and authenticated extension bridge; managed tab DOM/click/fill/JS/network/console/source/screenshots. Linux real-browser tests; Android physical validation and Firefox signing pending |
| Desktop control | The operator can open a workspace text file in a supported editor with `/desktop open` and use the clipboard on supported systems. General mouse, keyboard and window automation remains open. |
| MCP client | Explicit stdio and Streamable HTTP connections with tools, paginated text resources and user-selected prompts. See [MCP resource context](MCP_RESOURCE_CONTEXT.md). |
| Multi-agent swarm and isolated worktrees | [Concurrent child agents](DELEGATED_AGENTS.md) have scoped reads and can submit [patch proposals](SUPERVISED_EDIT_PROPOSALS.md) for parent review. Isolated writable Git worktrees remain open. |
| Model selection / free gateway | Searchable picker of keyless Kilo free models, Blue Free bounded automatic rotation, pinned concrete choices, one-hour catalog cache, saved selections. JarvisClaw retained; OpenRouter requires own key |
| Native installer candidates | CI packaging for Debian/Ubuntu amd64 and macOS arm64 (unsigned pkg). Candidate artifacts are separate from the published one-command installers. |
| Native desktop GUI | Not implemented |

Next milestones: wider MCP and device verification, resolved cross-file indexing, isolated patch worktrees, signed extension distribution, richer structured test reports, crash recovery, native installers.

Acceptance gate: complete CI on Windows/Linux/macOS, functional model tests with opted-in credentials, repeated benchmark tasks on identical model settings, no unverified success claims.

Task plans, action summaries, cancellation, directory creation and approved bounded web fetch are implemented. No model-quality parity percentage is claimed. See WORKBENCH.md and third_party/NOTICE.md for limits and attribution.

See [the control/context comparison audit](AGENT_REASONING.md) for the inspected AlphaCode commit, independent implementation, evidence checks and remaining gaps.

The 0.7 comparison inspected AlphaCode commit `2e1db4018f65c12429c8d113aa469617836e5873`. See [project intelligence](PROJECT_INTELLIGENCE.md) for the specific reference modules, independent code and remaining gaps.
