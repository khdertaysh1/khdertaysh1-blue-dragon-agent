# Blue Dragon workbench — 0.4 alpha

This revision integrates the formerly experimental terminal into the current core while preserving the visual assets. It is a practical capability upgrade, not AlphaCode parity.

## Run
Build with the current stable Rust toolchain (locally validated with rustc 1.100.0):
    cargo build --locked --release
    ./target/release/blue-dragon tui

Set BLUE_DRAGON_API_KEY, BLUE_DRAGON_MODEL and optionally BLUE_DRAGON_BASE_URL for your provider. Native OpenAI-compatible and Anthropic streaming are supported. The built-in `free`/`kilo` preset uses Kilo Gateway's free catalog without credentials; the old JarvisClaw selection remains available as `jarvis`; no paid credentials are bundled. Provider/model selection is documented in [MODELS.md](MODELS.md). A server that does not implement SSE can use --no-stream.

Commands: /help, /map, /status, /diff, /session, /skills, /skill NAME, /plan, /report, /mcp, /connect NAME, /clear, /exit.
The wide terminal includes a task-plan pane. Small screens keep a single conversation pane. Input supports left/right/home/end, delete/backspace, Unicode code points and bracketed paste. Text shaping/grapheme-aware editing is not a full terminal text editor.

Ctrl+C requests cancellation while working; when idle it exits. Esc exits. Active direct subprocesses are killed on cancellation. Provider cancellation is cooperative: blocked network I/O can wait for the request timeout (120 seconds). MCP requests have a 30-second wait budget and an input-queue limit. Process descendants are not an OS sandbox and do not have a cross-platform process-tree guarantee.

## Plans and evidence
The model can use update_plan to maintain at most 20 steps and one in-progress item. Plans survive session resume and are restored to the terminal pane. Plan completion is a model claim, not verification evidence.

Each task records an action summary in the session: tool names, return success/failure, steps and completion/cancellation. /report shows it. Tool result success means the tool returned successfully; it does not certify a vulnerability or prove the project correct. Results still need inspection and relevant tests.

Tool-call batches are validated before entering the saved history. Denied/cancelled tools receive paired tool results. If a process ended before saving a result, resume inserts an explicitly unknown-status envelope; it never reruns the action or invents success. The user/model must inspect the workspace before proposing a retry. A 1 MB transcript budget stops growth rather than silently deleting context. This revision does not implement intelligent context compaction.

## MCP (stdio, explicit opt-in)
Copy examples/mcp.example.json to a private file and configure a trusted local process. The file is loaded only through --mcp-config or BLUE_DRAGON_MCP_CONFIG. Repository files are not auto-executed.

    blue-dragon --mcp-config /path/to/servers.json tui

/mcp lists configured servers without launching them.
/connect docs asks permission before starting the process, performs initialize → initialized → tools/list, and registers names such as mcp__docs__search. Every remote tool call needs a separate approval, regardless of an advertised read-only hint.

Configuration holds environment variable names, not secret values. Only explicitly named values and basic OS variables are inherited. The server runs with your OS permissions and can modify files or use the network; approval is not isolation.

Implemented: JSON-RPC stdio, initialization/version checking, paginated tool discovery, bounded messages/queues, timeout/cancellation, call results, default denial for unsupported server requests. Not included: Streamable HTTP, OAuth, roots/resources/prompts, sampling, elicitation, server reconnection or a browser extension. Test coverage uses a real local subprocess fixture; third-party MCP servers still require compatibility testing.

## Added tools
Alongside the existing ten workspace tools:
- update_plan: maintain the session task plan.
- create_directory: create one directory inside the workspace after approval.
- file_info: inspect file/directory metadata.
- web_fetch: explicitly approved GET of HTTPS or loopback HTTP, no redirect/cookies/scripts, bounded text. This is not browser automation.

## Android / Termux
CI cross-builds an Android ARM64 executable using the Android NDK (API 26+). This is a Termux command-line binary, not an APK. It still needs physical-device testing.

Extract the Android artifact into Termux private storage, then:
    chmod +x blue-dragon
    ./blue-dragon doctor
    ./blue-dragon tui

Or build in Termux:
    pkg install git rust clang pkg-config
    cargo build --locked --release
    ./target/release/blue-dragon tui

Linux x86_64 binaries do not run on ARM64 Android. MCP servers/dependencies must be installed separately in Termux.

## Reference and validation
AlphaCode's module structure and stdio protocol were consulted at commit 7b325dfdb73066eaea372b7b39bf125effa2ae72. The adapted JSON-RPC envelope preserves MIT attribution in third_party/. No AlphaCode API endpoints, account flows or branding are copied.

Validation includes both SSE protocols, fragmented tool arguments/Unicode, truncated streams, a local model-tool roundtrip with denied writes, persisted reports, a real stdio MCP handshake/tool call, response-ID rejection, and small/wide terminal rendering with approval overlays. No live provider account or physical Android device was used.
