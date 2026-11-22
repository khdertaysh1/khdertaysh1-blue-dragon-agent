# Provider capacity investigation (2026-11-15)

## Scope and provenance

Blue Dragon baseline: `a35626b9e806efb22f000ae7c1bb9487a5d7b145` (published 0.9.0-alpha.3 source). AlphaCode reference: `d49f8830dbf1b79f82ad94aee8045c475a3752a6` (1.1.8). The Blue tree has no AGENTS.md; the Alpha root AGENTS.md was read. This change is a local candidate, not a new published release. Existing license/attribution files remain unchanged. No Alpha source or credentials were copied. Public route identifiers were inspected; no embedded credential was extracted or used.

## Code comparison

| Concern | AlphaCode current implementation | Blue baseline and this change |
|---|---|---|
| Anonymous free route | `src/alphacode_provider_metadata/catalog.rs::ALPHAX_FREE_PROFILE`, `route_cloak.rs` route accessors: base `https://api.kilo.ai/api/gateway`, model `kilo-auto/free`, no required key | `src/models.rs::{KILO_BASE,KILO_MODEL,Selection::new}` already used **automatic** `kilo-auto/free`, with documented `https://ai-gateway.kilo.ai/api/v1`. No evidence that replacing the documented endpoint with Alpha's route increases quota. Endpoint retained. |
| 1.1.4 automatic routing fix | `src/alphacode_app_core/agent/free_pool_rotation.rs::{should_implicitly_rotate_for_provider,is_automatic_free_route}` excludes the automatic alias from implicit pool rotation | Previously an explicit upstream-model 429 could replace the alias with a concrete free model. `src/agent.rs::turn` and `src/rotation.rs::Rotation::next` now preserve it, without catalog fetch/replay. Pinned choices remain pinned. |
| Catalog traffic, 1.1.7+ | `src/alphacode_base/provider/catalog_scheduler.rs` central scheduler; `src/alphacode_provider_openrouter_runtime/mod.rs::{begin_profile_catalog_refresh,begin_background_model_catalog_refresh,maybe_schedule_openai_compatible_profile_catalog_refresh}` shared in-flight/freshness/backoff guards. `catalog_refresh_does_not_probe_unused_models` verifies refresh does not fan out over unused models. | `models.rs::kilo_catalog` already had a one-hour disk cache, but forced/concurrent calls could duplicate. New `catalog_at` locks across processes, rechecks cache after locking, coalesces forced refreshes for 30s and failures for 60s. `refresh` shares OpenRouter success/failure cache in process. OpenRouter cache is **not** cross-process. No background model sweep added. |
| Unnecessary key probes | `src/alphacode_base/usage/api_keys.rs::{configured_key_status,fetch_openai_api_key_report,fetch_anthropic_api_key_report,fetch_compatible_profile_report}` reports configured/not checked; real balance/admin spend APIs remain. `usage/provider_fetch.rs::fetch_gemini_usage_report` likewise. `src/cli/provider_init.rs::init_provider_for_validation` disables login/bootstrap. | `models.rs::Selection::require_key` checks configuration locally. No equivalent model-list key validation request was found to remove. No dummy completion introduced. |
| Dispatch audit | `src/alphacode_provider_core/request_audit.rs::{record,record_operation,AuditedSend}` and `scripts/provider_traffic.py`, scoped turn propagation and private correlation | New `provider.rs::{audit_start,request,send_image_request}`, `models.rs::catalog_get`, `provider_traffic.rs` and `scripts/provider_traffic.py`: bounded metadata-only start/completion records, aggregate rolling-minute requests and estimated/reserved vs provider-reported tokens separately. |
| Context/capabilities | `provider_metadata/free_pool.rs::{remember_free_model_context_limits,free_model_context_limit}` remembers discovered limits; `alphacode_base/provider/dispatch.rs::estimate_request_input` includes schemas/system | Existing `models.rs::{kilo_choices,cached_context_limit}` filters free/tool-capable concrete entries and caches limits. Automatic route capabilities are provider-controlled, not guaranteed by its name. New 8192 estimated input target is a **traffic budget**, not an advertised context window. Original transcript retained. |
| Retry policy | `alphacode_provider_core/retry.rs` centralized bounded policy; non-idempotent POST retries opt-in | No new completion retry. 400 is not repeated unchanged. Existing Retry-After/default distinction retained. Shared admission records requests before dispatch; no refund for uncertain outcomes. |

History examined includes Alpha 1.1.4 reliability commits `bcc68d0`/`9ced65a`, 1.1.7 `5ca0146`, scheduler isolation `92a4eb8`, and subsequent audit privacy `abd60ad`/`c490b23`. The comparison above follows current code, not release notes alone. Alpha free-pool comments about model-specific limits are not evidence that this user's TPM limit has that scope.

## What the supplied failure establishes

`HTTP 429 / tpm_rate_limit_exceeded` establishes a token-throughput rejection. It does **not** identify the quota owner, numerical TPM, or whether input, output reservation, cached tokens, account, model, IP or gateway pool triggered it. `source=program_default` means 300s was selected by the client, not sent as Retry-After. We did not reclassify unknown scope as model-specific. TPM is not context-window capacity.

Current [Kilo usage documentation](https://kilo.ai/docs/gateway/usage-and-billing) publishes a free ceiling of **200 requests/hour/IP**, shared by anonymous and authenticated free access. A numerical free TPM is not published there. Model limits and additional traffic on the same public IP can reduce available capacity. [Model documentation](https://kilo.ai/docs/gateway/models-and-providers) describes the automatic free route; [API documentation](https://kilo.ai/docs/gateway/api-reference) supports the endpoint retained here. Free upstreams may log inputs; live tests used synthetic code only.

## Implemented traffic controls

- Free requests use at most 2048 output tokens by default instead of 4096, and an 8192 estimated input target. `BLUE_DRAGON_MAX_OUTPUT_TOKENS` and `BLUE_DRAGON_INPUT_BUDGET` explicitly override these. Capacity mode starts with eight discoverable core schemas, not 41; whole-file reads remain discoverable, permissions are unchanged. Model switches recompute projection using the selected destination. Original user constraints are not silently removed to fit; an unfit request stops before dispatch.
- Existing bounded source cache/window/continuation tools remain available. Compact results now preserve an explicit source-text excerpt, byte offsets and window hash rather than spending the excerpt on JSON metadata. This fixes a live-observed reread loop without increasing the step cap. Existing deterministic history projection is used earlier; completed tool payloads become historical references/excerpts while originals remain on disk. No model-generated summarization requests were introduced. This reduces retransmission; it does not promise zero repeated bytes or lossless reasoning from excerpts.
- Completed provider `reasoning`, `reasoning_content` and `reasoning_details` fields are archived from pressured projections with an explicit historical marker; the original envelope is retained and recoverable. Pending tool envelopes retain their metadata and cannot be archived to hide missing results. Older complete rounds can become original-message index ranges. A live test exposed large duplicated reasoning metadata that the initial scripted fixture lacked.
- `reasoning/loops.rs::Guard` blocks repeated recall pages even when interleaved with other calls, and caps successful recall pages per turn at eight. No evidence is deleted.
- `provider_budget.rs::{Coordinator::admit_tokens,admit_window}` reserves input estimate plus maximum output before sending, with rolling-minute RPM/optional TPM and a local free 200/hour ceiling. Durable locks coordinate roots, children and processes sharing the same app home/credential identity. Known Kilo host aliases share identity. Unknown completion outcomes retain reservations. No automatic waiting loop, model cycling or retry amplification was added.
- `BLUE_DRAGON_TPM` must be set from a known applicable limit; **60000 is only a test fixture**, not a claimed service quota. The reservation is conservative local admission, not billed usage and not a universal provider accounting rule. Local state cannot observe other applications, devices or all traffic behind the same IP; switching app homes/credentials is not a quota solution.
- Audit records contain origin/model, kind, timestamp, duration, attempt count, schema/history/tool-result byte counts, token estimates, requested output maximum, actual model/usage when returned, and allowlisted numerical rate headers/parsed Retry-After. No bodies, tool arguments, messages, authorization headers or raw arbitrary rate-header text are logged. Audit caps each process file at 16 MB and warns when incomplete. Start/finish dispatch IDs deduplicate; a crash leaves an unknown start. Missing usage remains unknown. Headers with unsupported reset formats are omitted rather than guessed. Raw gateway-internal retries/cache hits are not visible.
- Session metrics now finalize the outcome after response completion; an in-memory `dispatch_started` marker is not left as the successful request's final status.

## Reproduction and observations

Artifacts under `docs/measurements/provider-capacity/` identify binary hashes. Run:

```sh
python3 scripts/test_provider_traffic.py
python3 scripts/benchmark_provider_capacity.py --before /path/to/baseline --after /path/to/candidate
python3 scripts/benchmark_provider_capacity.py --before /path/to/baseline --after /path/to/candidate --cap 1000000
python3 scripts/live_provider_capacity.py --binary /path/to/candidate --output /new/isolated/directory --allow-network --rounds 4
```

The controlled workload uses the actual CLI, identical original history (eight large completed tool rounds), three interactive requests, a 120KB source, an independent source-window/hash oracle and unchanged-file/constraint checks. Model decisions are scripted. The synthetic quota counts estimated input in a sub-minute run; it is not a native tokenizer, billing model or real service.

| Controlled result | Before | After |
|---|---:|---:|
| Same completed workload: useful verified rounds | 3 | 3 |
| Requests for that workload | 6 | 6 |
| Estimated input tokens sent | 158822 | 40300 |
| 429 with generous synthetic allowance | 0 | 0 |
| Useful rounds with synthetic 60000 input TPM | 1 | 3 |
| Requests with synthetic 60000 input TPM | 3 | 6 |
| 429 with synthetic 60000 input TPM | 1 | 0 |

Estimated input decreased **74.6%** for the same completed work. There is no request-count reduction in this workload; the constrained baseline sends fewer requests because it fails early. Loopback runtimes around 0.05–0.11s do not measure model speed. The failed baseline observes the existing 300s cooldown, and the harness does not wait or retry it away.

The first live harness run accidentally removed the environment's required proxy settings and failed before an HTTP response. This is recorded separately, not counted as provider capacity. A fresh session preserves required proxy/trust settings without forwarding API credentials. Intermediate live failures are retained: the initial candidate stopped at round four on the local context target; indexing old rounds alone still failed on provider reasoning metadata; after archiving completed reasoning envelopes, six rounds finished but three omitted requested fresh reads and the first read the whole file. Window-first schemas removed that full read, but a subsequent sample hit the unchanged six-step cap while rereading windows. Source-aware excerpts address the observed missing-content cause. Final results are appended below. Baseline and candidate live samples are observational, not a controlled performance comparison: automatic routing, load and some overlapping wall time can differ.

## Recommended operation and capacity calculation

Use the **existing automatic free route**, not a guessed concrete model or a paid fallback:

```sh
BLUE_DRAGON_INPUT_BUDGET=8192 \
BLUE_DRAGON_MAX_OUTPUT_TOKENS=2048 \
BLUE_DRAGON_PROVIDER_AUDIT_DIR="$HOME/blue-dragon-traffic" \
./target/release/blue-dragon --provider free --no-rotation chat
# In another shell, from the source checkout:
python3 scripts/provider_traffic.py "$HOME/blue-dragon-traffic"
```

Do not set an invented service TPM. If the account provides a documented applicable TPM/RPM, set `BLUE_DRAGON_TPM`/`BLUE_DRAGON_RPM` accordingly. The input target is heuristic; inspect real usage. Smaller output ceilings can truncate long reasoning; incomplete-stop guards still prevent executing truncated tool calls.

For six calls per analysis cycle, 200/hour permits at most **33 complete cycles/hour** if no other caller uses the IP quota. One such cycle every minute needs 360 requests/hour: a deficit of **160/hour** against that ceiling, irrespective of model names. With this controlled candidate workload, six calls require approximately **40300 input tokens plus actual output**; conservative max-output reservations add 12288. The numerical live TPM deficit remains **unknown**, because available TPM is unknown. Required capacity for a chosen cadence is requests/minute × measured mean input/output; use the reported tokenizer usage from real sessions when available.

An explicitly opted-in paid alternative supported by the existing native adapter is `--provider anthropic --model claude-haiku-4-5-20251001` with a personal `ANTHROPIC_API_KEY`. This is currently **active legacy**, not the latest model, with 200K context and published $1/M input, $5/M output; verify availability before use ([model reference](https://platform.claude.com/docs/en/models/haiku-4-5/overview), [pricing](https://platform.claude.com/docs/en/about-claude/pricing)). For a hypothetical 40300 **actual** input + 3000 output tokens, cost is about $0.0553/cycle before caching/taxes; our heuristic estimate is not an invoice. Account RPM/ITPM/OTPM must meet the workload independently; [Anthropic rate-limit documentation](https://platform.claude.com/docs/en/api/rate-limits) explains those dimensions. No paid call was made, no key bundled and no automatic paid switch exists. Its quality/capacity for this user's investigation remains untested; this is a configurable candidate, not a guaranteed substitute.

## Delivery boundary

The candidate is local and unpushed, following the existing coordination instruction. `git pull` or the currently published installer will not install these changes yet. No published tag or binary was replaced. After the reviewed commit is transferred to GitHub, the existing source-build path on Termux is `pkg install git rust clang pkg-config`, clone/fetch that exact commit, `cargo build --locked --release`, then the command above. For immediate local transfer, use the accompanying source archive with a fresh directory; never reset an existing workspace or overwrite user edits. Linux checks do not establish Windows/macOS/Android device behavior. No Termux device was available.


## Final validation and real-service result

Implementation commit: `4a38b63` (with preceding candidate fixes). Linux executable SHA256: `6ef3d4d3b218649824993dc865a03c8e362c15e2c4a0985060521d6054e5b183`. Rust **328 tests passed**, Clippy all targets with warnings denied passed, release build passed. The exact binary passed **33 integrated qualification groups** against pinned local lab `c396c90f2e7ed080f71368ce9edb5cd630098b09`. Python traffic aggregation has two tests; live native-result verification also rejects denied, unknown and forged-hash controls. Original constraints and source bytes were checked independently.

Final live sample: **10 completion requests, zero observed 429**, requested `kilo-auto/free`, reported upstream `nvidia/nemotron-3-ultra-550b-a55b:free`. Provider-reported totals: **47,706 input + 2,488 output tokens**. Peak rolling 60s: six requests and 28,908 reported tokens. Request time totaled 105.582 seconds, plus three deliberate 20-second inter-round intervals. No rate-limit or Retry-After headers were returned. These figures are observations from this host/time, not quota guarantees or a model tokenizer cross-check.

All four CLI rounds finished, source files were unchanged, no full-file read occurred, and all recorded source windows matched independent bytes/hashes. **Only three of four rounds performed the newly requested fresh read.** The fourth discussed historical evidence instead. Narrative review found the missing owner check correctly identified and static reasoning distinguished from runtime proof, but this does not excuse the missing recheck. The last response also supplied incorrect byte spans for the two functions; native window offsets were correct. The independent narrative review records the actual source ranges. Model-generated location annotations therefore remain a report-verification gap. Baseline live sampling finished four rounds but performed fresh reads in only two and read the full file once despite the window-only instruction. Baseline lacked complete per-dispatch usage auditing, so a real-service token savings percentage is **not available**. The 74.6% figure belongs exclusively to the controlled, same-completed-work comparison.

A preceding six-round live trial crossed process restarts without context failure after the reasoning-envelope fix, but it had prompt-adherence failures documented in the measurements. No hours-long quota saturation or physical Android test was performed. The original user's 429 did not reproduce here; its numerical TPM and scope remain unresolved. The remaining fresh-evidence compliance gap prevents claiming fully reliable autonomous investigation or parity with AlphaCode. User review and native acceptance checks remain necessary.

A source archive is provided for review/build; no GitHub push, merge, release/tag change or paid service request occurred. Its Cargo version remains the base alpha.3 version, so identify this candidate by source commit/hash rather than interpreting the version string as a published upgrade.
