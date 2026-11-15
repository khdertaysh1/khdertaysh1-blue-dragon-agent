# Blue Free gateway routing and quota policy

Select `/model blue-free` (also `/model free` or `/model kilo`) in chat/TUI, or `blue-dragon model free` before launch. The selected model is `kilo-auto/free`. The gateway chooses its upstream; Blue Dragon preserves the alias. Concrete selections remain pinned. No paid fallback, account/key cycling, embedded credential or service-quota bypass is provided.

The current source candidate no longer replaces the automatic alias with a concrete pool member after an error, including an explicitly model-scoped 429. `--no-rotation` remains accepted for explicit configuration. `/rotation` includes `gateway_auto_alias_preserved`; the legacy internal bounded pool machinery is not used to replace this alias. The gateway's own routing remains outside client control.

An unknown 429 remains unknown/shared scope. `tpm_rate_limit_exceeded` identifies throughput, not a context-window error or a numerical quota. Provider Retry-After is distinguished from the existing program default; no same-request retry or automatic sleeping loop is added. Durable cooldowns and admission reservations coordinate processes sharing application state. Interrupted outcomes retain reservations and never authorize replay of a tool action.

The candidate defaults free requests to an 8192 estimated input target and 2048 output maximum, with discoverable tool schemas and non-destructive projection. Optional explicit RPM/TPM controls and a local free 200/hour ceiling apply before dispatch. Local counters cannot see all clients behind the same IP, and changing models does not create capacity. See [the exact implementation, measurements and limits](provider-capacity-review.md).

Context and tool results go to the selected service; free upstreams may log or use that data. Live trials use only synthetic fixtures. The original transcript remains local for bounded recall; context projection does not turn historical evidence into fresh observations. Pending tool calls are not hidden to make a request fit.

Offline regression tests verify that a completed tool is not repeated after a quota error, that the alias remains selected, and that shared rolling limits/cooldowns persist. Real-service observations are explicitly separated from loopback tests in the capacity review. No new published release is implied by this source candidate.
