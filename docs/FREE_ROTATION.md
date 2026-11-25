# Blue Free gateway routing and quotas

Select `/model blue-free` to start with `kilo-auto/free`. Blue Dragon uses the gateway’s limits by default. You can set a stricter local limit with `BLUE_DRAGON_RPM` or `BLUE_DRAGON_TPM`. Provider cooldowns are shared across local sessions and apply to the scope reported by the gateway.

On a pre-stream HTTP 429, cloud chat tries another eligible provider first. The default order is Kilo, JarvisClaw, then OpenRouter Free when its key is configured. The new provider receives the conversation and existing tool results. The current task continues there, and completed tools are not replayed. Each service is tried at most once on this path. No retry time is inferred when the upstream omits it.

Set `BLUE_DRAGON_FALLBACK_PROVIDERS=kilo,jarvis,openrouter,groq,gemini` to set the destination order explicitly. Groq and Gemini require their own keys and billing depends on your account. OpenAI and Anthropic are available as explicit destinations too. Providers missing a key or in a recorded cooldown are skipped. Use `--no-rotation` to keep the current endpoint and model. Local/custom endpoints and sessions pinned to a vision destination do not forward their history automatically.

Use `/rotation` to inspect the configured destination order and switches. A switch changes the active session rather than the saved startup preference. If there is no independent destination, Kilo can still recover from an explicitly model-scoped limit using its validated free catalog.

The estimated input target and maximum output per request are independent of rate limits. See [the provider capacity review](provider-capacity-review.md) for historical measurements.
