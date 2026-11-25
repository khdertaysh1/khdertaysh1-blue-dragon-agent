# Blue Free gateway routing and quotas

Select `/model blue-free` to start with `kilo-auto/free`. Blue Dragon uses the gateway’s limits by default. You can set a stricter local limit with `BLUE_DRAGON_RPM` or `BLUE_DRAGON_TPM`. Provider cooldowns are shared across local sessions and apply to the scope reported by the gateway.

With rotation enabled, a pre-stream HTTP 429 explicitly scoped to one model can switch the active task to another free model on the same gateway. Candidates must appear in a recent validated catalog, support tools and fit the current context. Rotation tries up to three alternative routes per turn. Previous tool results remain in the conversation. Use `/rotation` to inspect switches or `--no-rotation` to pin your selection.

An account, IP, gateway-wide or unspecified quota keeps the current route. A promotion-limit response without explicit model scope is reported as a promotion limit. If the gateway omits its retry time, Blue Dragon leaves it unspecified. In synchronous terminal chat, a successful switch returns control to the prompt. The next instruction goes to the selected fallback.

The estimated input target and maximum output per request are independent of rate limits. See [the provider capacity review](provider-capacity-review.md) for historical measurements.
