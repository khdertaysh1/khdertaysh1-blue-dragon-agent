# Blue Dragon core architecture

```text
CLI (clap) --> Agent loop --> OpenAI-compatible provider (reqwest)
                  |                    |
                  |              assistant tool calls
                  v                    |
           ToolBox dispatcher <--------+
                  |
        read / search / git diff
        write + human approval
        subprocess + human approval
                  |
           local workspace

Session (JSON, app-data directory) records dialogue and tool results after each operation.
```

## Interfaces

- `provider.rs`: blocking HTTP transport with explicit timeouts and no redirects.
- `agent.rs`: model-agnostic loop capped by --max-steps (default 24); persisted after each step.
- `tools.rs`: constrained file and subprocess tool boundary with human approvals.
- `session.rs`: resumable, JSON-backed local conversations.
- `config.rs`: provider selection, key lookup from environment, workspace settings.
- `main.rs`: subcommands and interactive REPL.

`run_command` is deliberately not auto-approved. This is a coding assistant for trusted workspaces, not a hardened sandbox.

## Future design (not yet implemented)

Independent policy daemon, remote MCP transports, richer browser instrumentation, distributed workers, resolved semantic project graph, permission-scoped resource graph, and evidence-gated multi-agent execution.

## Workbench modules
TUI rendering and input run separately from the model worker. A bounded ordered event queue carries deltas, tools, plans and task completion; approval replies use a separate one-shot response. Cooperative cancellation is shared with command polling, SSE parsing and MCP RPC waits. Session plan/report fields are serde-defaulted to preserve old session compatibility. stdio MCP is explicit opt-in and never auto-launches repository config. See WORKBENCH.md for transport and cancellation limits.

## Browser and model routing

`browser.rs` owns explicit browser setup and approval. `browser/cdp.rs` speaks local CDP on a newly created tab; `browser/bridge.rs` authenticates the independently written WebExtension over localhost WebSocket. Page evaluation runs in MAIN world, isolated from extension APIs. Results are bounded and marked untrusted. Pairing tokens never enter the model tool. `rotation.rs` classifies pre-stream provider errors and selects only validated Kilo free candidates under a per-turn budget and per-process cooldowns. Retries reuse current history without replaying tools. See BROWSER.md and FREE_ROTATION.md for boundaries.

## Request-time task control

`reasoning/context.rs` constructs a bounded provider view without altering the original transcript, keeping complete recent tool envelopes and exact recall indexes. `reasoning/loops.rs` tracks canonical calls and unchanged/denied results without serving a cache as fresh observation. `reasoning/mod.rs` supplies fixed task-specific guidance and validates evidence-linked finding records. Engine observation IDs refer to actual persisted transcript positions; supported citations are not semantic verification. See AGENT_REASONING.md.

## Local project intelligence
`intelligence/code.rs` walks eligible source/text with ignore rules, hashes contents on refresh and reuses parsed metadata only for unchanged hashes. Tree-sitter supplies syntax structures for five language variants; lexical search covers other eligible text. Resource/depth/parser limits are returned explicitly; references are syntactic candidates.

`intelligence/memory.rs` persists bounded per-workspace historical notes under app data, with target-origin scoping, source session IDs, a bounded cross-process file lock and atomic replacement. Notes are untrusted model-authored data and never fresh verification. `intelligence/checks.rs` records declared exact built-in tool/input/native-field contracts; the agent invalidates evidence on observed source mutation and checks a fresh bounded workspace fingerprint before finalization. Tool approvals remain in ToolBox. Missing/stale checks get one review and then an explicit unverified task status; neither criteria nor fingerprints establish semantic correctness. See PROJECT_INTELLIGENCE.md.
