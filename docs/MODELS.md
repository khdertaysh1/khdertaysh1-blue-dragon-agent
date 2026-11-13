# Ready-to-use free models — v0.4.0-alpha.3

## Start and choose without keys

```sh
blue-dragon --provider free tui
```

`free` aliases the **Kilo anonymous free preset**. Type `/models` (or `/model`) for the terminal picker, type to filter, use Up/Down to choose, and Enter to select/save. Esc closes without changing anything. Plain `chat` lists IDs for selection with `/model kilo MODEL_ID`.

`/models refresh` updates metadata. CLI equivalents: `blue-dragon models`, `blue-dragon models --refresh`, `blue-dragon model kilo MODEL_ID`. Listing is metadata discovery, not an availability test. No prompt/workspace content is sent when listing or selecting. Tasks send context and tool results to the chosen service. No account, API key, embedded secret, wallet or payment is configured for Kilo free access. Unrelated keys are never forwarded.

## What AlphaCode uses

AlphaCode reference snapshot `7b325dfdb73066eaea372b7b39bf125effa2ae72` uses Kilo Gateway for Alphax Free. Its legacy base is `https://api.kilo.ai/api/gateway`, with `kilo-auto/free` and discovered `:free` pool members; its picker supports concrete pool members. Blue Dragon independently implements this using the same publicly documented service at `https://ai-gateway.kilo.ai/api/v1`. Both catalogs returned the same free IDs during verification. No additional AlphaCode source, branding or credentials are copied.

## Live catalog snapshot: 2026-11-13

These zero-priced tool-capable rows were returned by Kilo:

| Model | Exact ID |
|---|---|
| Auto Free (gateway chooses) | `kilo-auto/free` |
| Baidu ERNIE 5.5 Flash Preview | `baidu/ernie-5.5-flash-preview:free` |
| Cohere North Mini Code | `cohere/north-mini-code:free` |
| Dots3 Note Preview | `dots-studio/dots-3-note-preview:free` |
| inclusionAI Ling 3.2 Flash | `inclusionai/ling-3.2-flash:free` |
| LiquidAI LFM3 1.7B | `liquid/lfm-3-1.7b:free` |
| Nemotron 3 Nano Omni Reasoning | `nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free` |
| Nemotron 3 Ultra | `nvidia/nemotron-3-ultra-550b-a55b:free` |
| Nemotron 3.5 Lightning | `nvidia/nemotron-3.5-lightning:free` |
| Nemotron 3.5 Super VL | `nvidia/nemotron-3.5-super-vl:free` |
| Particle 1 | `particle-ai/particle-1:free` |

This is a historical snapshot, not a permanent lineup. Runtime filtering requires eligible free IDs, advertised tool support, zero prompt/completion prices, and zero for every other advertised price component. Missing/unknown/negative/exponential prices are rejected; no floating-point epsilon admits tiny nonzero prices. `kilo-auto/frontier`, `efficient` and `small` are excluded. Individual selections must exist in the validated local catalog. The documented Auto Free alias is the only offline floor.

## Cache, selection and compatibility

Metadata is cached as `kilo-models.json` in application data for one hour, with bounds and validation. If refresh fails, cached choices are explicitly labeled; without a cache only Auto Free appears. Listing does not prove a model is currently serving requests. HTTP 429/402 stop a task; choose another model manually. No automatic retry, paid fallback, purchase or cross-provider switch occurs.

Selections are stored as provider/model names in `model.json`, never from repository config. Fresh installs use Kilo unless explicit legacy endpoint/model/key settings exist. Previously saved `{"provider":"free","model":"auto/free"}` migrates to **JarvisClaw** without silently changing its destination; `/model free` explicitly switches to Kilo.

| Preset | Models | Key |
|---|---|---|
| `free` / `kilo` | Auto Free or discovered eligible `:free` IDs | None |
| `jarvis` | `auto/free` | None |
| `openrouter` | `openrouter/free` or `:free` IDs | `OPENROUTER_API_KEY` |
| `ollama` | Installed model with tools | None; local server required |
| `openai` | Explicit ID or default | `OPENAI_API_KEY`; may incur charges |
| `anthropic` | Explicit ID or default | `ANTHROPIC_API_KEY`; may incur charges |

`/models openrouter` lists separate OpenRouter metadata, without claiming keyless inference. Existing custom `--base-url`, `--model`, `BLUE_DRAGON_API_KEY` configurations remain supported. Unset conflicting old `BLUE_DRAGON_BASE_URL` / `BLUE_DRAGON_MODEL` exports before a different preset. `--provider` cannot be combined with `--base-url`.

## Data handling and validation

Kilo Auto Free and NVIDIA free endpoints may log prompts, tool results and outputs and use them for service improvement. NVIDIA routes are trial use; do not submit personal/confidential data. The picker displays this notice. Blue Dragon does not operate the service or guarantee unlimited usage, retention, model identity or availability. TLS verification stays enabled with native and bundled trusted roots. No keys are extracted from AlphaCode or distributed here.

Anonymous SSE calls to Auto Free, North Mini Code and LFM3 returned valid `ping` tool calls with gateway-reported cost zero. The opt-in `python scripts/check_free_models.py` exercises release-agent `read_file` roundtrips in temporary synthetic workspaces with provider keys removed. CI never makes live model calls. Tests cover catalog filters/cache/migration, key isolation, narrow/wide rendering, offline CLI selection and real Unix PTY picker/filter/select/save/exit. Physical Android needs device testing. Catalog membership is not a coding/security benchmark.

Primary documentation: [Kilo anonymous authentication](https://kilo.ai/docs/gateway/authentication), [models](https://kilo.ai/docs/gateway/models-and-providers), [free usage/data handling](https://kilo.ai/docs/getting-started/using-kilo-for-free).
