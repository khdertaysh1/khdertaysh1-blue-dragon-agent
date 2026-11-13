# Blue Dragon v0.1 architecture

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
- `agent.rs`: model-agnostic loop capped at 12 tool rounds; persisted after each step.
- `tools.rs`: constrained file and subprocess tool boundary with human approvals.
- `session.rs`: resumable, JSON-backed local conversations.
- `config.rs`: provider selection, key lookup from environment, workspace settings.
- `main.rs`: subcommands and interactive REPL.

`run_command` is deliberately not auto-approved. This is a coding assistant for trusted workspaces, not a hardened sandbox.

## Future design (not yet implemented)

Independent policy daemon, remote MCP transports, browser instrumentation, distributed workers, project indexing, permission-scoped resource graph, and evidence-gated multi-agent execution.

## Workbench modules
TUI rendering and input run separately from the model worker. A bounded ordered event queue carries deltas, tools, plans and task completion; approval replies use a separate one-shot response. Cooperative cancellation is shared with command polling, SSE parsing and MCP RPC waits. Session plan/report fields are serde-defaulted to preserve old session compatibility. stdio MCP is explicit opt-in and never auto-launches repository config. See WORKBENCH.md for transport and cancellation limits.
