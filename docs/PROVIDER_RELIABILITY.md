# Provider reliability: development work, not a release claim

Base: Blue Dragon `ad9ba2149fa122d09f88f054c7a86231b6e0c78a` (`0.9.0-alpha.2`, including the approved Abyss identity).
Reference inspected: AlphaCode `d49f883` (1.1.8), particularly
`alphacode_provider_core/retry.rs` and `alphacode_base/provider/dispatch.rs`.
No Alpha source was copied into this change; existing notices/licenses remain.
Historical Alpha executable comparisons remain historical, not measurements of
this new reference or this development executable.

## Changes

- Validate outbound history structurally before transmission: supported message
  roles/content containers, unique tool names, parameter-schema objects, complete
  JSON-object call arguments, unique call identities, exactly paired results and
  no outstanding tool calls. Rejection preserves the original history. This is
  not a complete JSON Schema validator or proof of every model's capabilities.
- Validate a complete incoming tool batch before executing it. Truncated JSON,
  duplicate identities, response-length limits, content filtering and explicit
  refusals cannot become executable calls. Unadvertised but structurally valid
  names still reach the existing deny-by-dispatch boundary, preserving native
  denial evidence and child permission tests.
- Retain bounded incomplete assistant text in the private session with an explicit
  historical/incomplete label. Reports retain category and size, not a copy of the
  partial text. Original completed native evidence remains available after an
  interrupted turn. The existing single summary-only request after an empty
  post-tool response is preserved; it does not rerun tools.
- Retain HTTP status, selected error code/message and an allowlisted request-ID
  header. Redact the configured credential, echoed request strings/tokens and
  common credential assignments. Raw bodies, authorization headers and complete
  requests are not logged. Arbitrary unknown secrets embedded in prose cannot be
  guaranteed detectable; error text remains bounded, untrusted diagnostic data.
- Distinguish provider-header, provider-body and program-default cooldowns. Parse
  numeric and HTTP-date Retry-After. Do not shorten large delays to a local cap.
  HTTP 400 is an invalid request, not evidence of a shared rate limit. Only an
  explicit context-size error can trigger the existing single *changed request*
  recovery attempt on the same model.
- Share durable admission across parent, child and concurrent sessions using the
  same application home, endpoint origin and credential identity. State contains
  hashed identifiers/counters, never the credential or conversation. OS file locks
  serialize updates; requests are charged before transmission and never refunded
  after an uncertain send. The program's separate local ceiling is **60 requests
  per minute**, not a claim about the provider's quota. Existing per-team lifetime
  and two-child concurrency budgets still apply.
- Persist shared/unknown-scope 429 and service cooldowns across model changes.
  Explicit model-scoped failures retain model-specific cooldowns. Generic 502/503
  no longer establish model-specific scope by themselves. Requests are not
  automatically retried after transport interruption. No new automatic POST retry
  loop was added. A busy/corrupt budget store fails closed. A backwards clock
  refuses a budget reset.
- Record definition bytes, estimated tokens, instance request counts, actual
  model and allowlisted usage counters when reported. Complete streamed usage
  following finish_reason is collected before the terminal marker. Estimates are
  not billing data; unavailable usage is not reported as zero. A newly created
  provider instance has a new counter; this is not an aggregate account invoice.
- `discover_tools` lists tools in bounded pages and enables up to eight optional
  schemas for the current phase plus core file/recall/planning tools. If full
  schemas prevent context projection, a visible notice activates the core set
  and preserves discoverability. This does not grant permissions, delete user
  constraints or replay actions. A long-session HTTP 400 recovery test verifies
  a changed request, preserved original history and one native read only.
- New children inherit the active routed model, while the user's auto/pinned
  selection remains separate. No shared API key or unrestricted-model claim was
  introduced.

## Boundaries still requiring work

Different application homes or different credentials cannot be assumed to share
an account, even if a service maps them to one. Admission covers completion/image POSTs, not every catalog/browser/MCP request. The admission file is not a global
service/IP limiter, an OS sandbox or a tamper-proof accounting system. Model
cooldowns and local admission do not replace provider enforcement. Window limits
are local defaults, not user-configurable quota profiles yet. The scheduler limits
request admission; it does not impose a new cross-team concurrent-process cap.

Automatic semantic task-phase classification is not implemented. Model-specific schema dialects,
full capability/catalog validation across alternate providers, semantic handling
of superseded long-session instructions and an aggregate usage ledger remain.
Usage on interrupted streams may be unavailable even if earlier usage fragments
were received. No live-provider quality comparison or physical Android run was
performed for this work. Cross-platform file-lock behavior and the Android build
must pass CI on the final candidate before release.

The bounded multi-file child-proposal path is now integrated on the supervised-edits
development branch; see SUPERVISED_EDIT_PROPOSALS.md. This is not a release claim.
