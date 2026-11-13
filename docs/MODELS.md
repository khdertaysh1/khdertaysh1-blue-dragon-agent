# Free access and model selection — v0.4.0-alpha.2

## Start without an account or API key

```sh
blue-dragon --provider free tui
```

The `free` preset calls `https://api.jarvisclaw.ai/v1/chat/completions` using the documented anonymous `auto/free` route. No authorization header, wallet, payment flow or bundled secret is used. The gateway chooses the underlying model; these model weights are not bundled with Blue Dragon. No paid model fallback or automatic rate-limit retry occurs.

A fresh installation with no saved selection, explicit model, endpoint or OpenAI key starts on this preset. Existing key/model and custom endpoint configurations keep their old behavior. If old `BLUE_DRAGON_MODEL` or `BLUE_DRAGON_BASE_URL` exports conflict with a new preset, unset them first; `--provider` and `--base-url` cannot be combined.

Prompts, previous conversation context and tool results are sent to JarvisClaw and its chosen upstream when you submit a task. The active endpoint is displayed. Changing provider changes the destination for subsequent context. Free access is subject to the provider's quotas and availability; it is not unlimited or a service hosted by Blue Dragon. HTTP 429 and 402 stop the request with a clear error. Nothing automatically purchases capacity or switches services.

## Choose and remember a model

In either the CLI chat or TUI:

```text
/models
/model free
/model openrouter openrouter/free
/model ollama YOUR_INSTALLED_MODEL_ID
/models refresh
```

Use `blue-dragon models` outside chat, `blue-dragon models --refresh` for discovery, or `blue-dragon model free` to save without a model request. A launch-only override is `--provider free`. Saved selection lives in application data as `model.json`, containing only provider/model names. It is never loaded from the inspected project and contains no keys. CLI flags/environment overrides take precedence as described above. Changing a model in chat updates both the active client and the session's model field; the same conversation is retained.

| Preset | Default / required model | Authentication |
|---|---|---|
| `free` | `auto/free` only | None; unrelated exported keys are never forwarded |
| `openrouter` | `openrouter/free`, or explicit `:free` ID | `OPENROUTER_API_KEY` |
| `ollama` | Explicit installed model supporting tools | Local `127.0.0.1:11434/v1`; none |
| `openai` | `gpt-4o-mini` or explicit ID | `OPENAI_API_KEY`; may incur charges |
| `anthropic` | `claude-sonnet-4-5` or explicit ID | `ANTHROPIC_API_KEY`; may incur charges |

Custom OpenAI-compatible endpoints remain supported with `--base-url`, `--model`, and `BLUE_DRAGON_API_KEY`. The legacy global key is preserved for existing OpenAI setups, but is not reused across provider presets. Non-free models are rejected by the `free` and `openrouter` presets. Use a custom endpoint explicitly for other OpenRouter models.

`/models refresh` performs a public metadata GET of OpenRouter's model catalog, without credentials or conversation data. It includes only IDs ending in `:free` (or `openrouter/free`), tool support, zero prompt/completion prices, and no nonzero/unknown advertised price components. Missing prices are not treated as free. This is a catalog snapshot, not a live test or guarantee of future price, model quality or availability. The anonymous JarvisClaw catalog endpoint returned 401 during verification, so Blue Dragon lists only its documented free routing alias rather than inventing a selectable pool.

TLS uses trusted bundled and OS certificates, honoring the native certificate-store configuration. It never disables certificate verification. Server requests time out and redirects are disabled. Native trust-store differences on physical Android still need device testing.

## Validation (2026-11-13)

- Real anonymous HTTP request to JarvisClaw: 200 with a function tool call.
- Real anonymous SSE request: 200, fragmented tool arguments, finish reason and DONE marker.
- Real release binary in a temporary synthetic workspace, with provider keys removed: streamed turn called `read_file`, obtained `BLUE_DRAGON_SYNTHETIC_42`, returned that content and persisted a successful action report. No private project contents were used.
- Unit tests cover preset restrictions, keyless credential isolation, bounded/validated selection storage and zero-price/tool-capability filtering. Offline CLI and real Unix PTY tests cover switching and saved selections.
- Other candidate gateways did not pass verification at that time (503/502) and are not automatic fallbacks.

These checks demonstrate one real tool roundtrip, not a comparison benchmark or a promise that every coding task succeeds. Third-party upstream identity is gateway-reported. Physical Android and authenticated OpenRouter/Ollama calls were not tested in this revision.

Primary service documentation: [JarvisClaw anonymous free API](https://jarvisclaw.ai/en/free-llm-api/), [OpenRouter free router](https://openrouter.ai/docs/guides/routing/routers/free-router).
