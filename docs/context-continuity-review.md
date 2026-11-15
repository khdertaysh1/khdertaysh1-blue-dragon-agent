# Context continuity repair (local review candidate)

Base: Blue Dragon `8fb0f2a` (published alpha.4). Reference: AlphaCode
`d49f8830dbf1b79f82ad94aee8045c475a3752a6` (1.1.8). Alpha root AGENTS.md was read;
Blue has no root AGENTS.md. No Alpha implementation or credentials were copied.
No version/tag/release changes are included.

## Proven local cause and implementation

`provider_traffic::input_budget` supplies Kilo's **8192 estimated input traffic
target**. Previously `Agent::execute_turn` converted it to a 13312-token hard
context ceiling. `reasoning::context::build` then reserved at least 4096 output
tokens even when the request actually capped output at 2048, plus schemas and
1024 margin. Large preserved user instructions could therefore stop locally
well below the configured 32768 context. This is separate from provider TPM.

`context::build_with_budget` now projects completed historical tool rounds and
assistant text toward the soft input target, accounting for schemas. If no tier
fits that target, it uses the smallest projection that fits the **hard** bound.
User/system instructions, pending calls and their arguments remain intact;
original history is unchanged and recoverable. It reserves 2048 for the default
Kilo request rather than imposing a 4096 floor. Hard bound, soft target, reserve,
source, and target-exceeded status are exposed separately. Hard overflow still
fails before dispatch. Known RPM/TPM admission and shared cooldowns still apply:
exceeding the traffic target is not permission to exceed a known quota.

The one context-error recovery may rebuild a smaller request. A fingerprint
check now stops an identical rebuilt request before dispatch. The conservative
recovery ceiling is labelled as such, not an advertised model capacity.
`Agent::context_status` separates the latest local projection failure and the
last successful request from current request metrics.

`tool_selection::core` consistently prefers window reads. `for_prompt` makes
browser schemas available for HTTP(S) URLs inside natural-language prompts,
without granting scope or clicking/navigating. `discover_once` blocks the same
catalog/selection request in an unchanged phase, resets per user turn, and
allows restoring tools after a different phase. Initial bare-URL browser failure
is saved as failed/unknown evidence and analysis can continue; it does not
silently replay a failed interaction. A browser status result is not DOM evidence. An already-selected oversized schema set can shrink to the core plus active browser before a hard failure; selecting tools no longer disables this fallback.

`--free-model-fallback` is explicit opt-in for a selected concrete Kilo free
model, and `--no-rotation` overrides it. Both current and destination routes must
be in the free/tool-capable catalog; destination context must be known and at
least the current effective limit. The automatic alias remains pinned. Shared
or unknown-scope 429, generic 400, partial responses and paid/cross-provider
routes do not trigger this fallback. Switching names does not add shared quota.
Catalog refresh and single-flight code were retained, not duplicated.

## Comparison to actual Alpha code

- `src/alphacode_base/compaction.rs::hard_compact_with` computes a safe cutoff
  that cannot split tool call/result pairs, aborts superseded background
  compaction, and builds an emergency summary. Blue uses local request
  projections over the original transcript and does not add background
  summarizer requests. Unlike Alpha's emergency summary, this change does not
  summarize unique user instructions heuristically or assert that all goals
  can fit indefinitely.
- `src/alphacode_app_core/agent/compaction.rs::note_compaction_applied` resets
  cache/tool locks/provider session state; its context-limit recovery works on
  provider messages. Blue rebuilds schemas/projection each request and verifies
  a changed request before its single recovery dispatch.
- `src/alphacode_app_core/agent/free_pool_rotation.rs` contains
  `is_automatic_free_route`, `should_implicitly_rotate_for_provider`, and
  `next_free_model`. The automatic route is pinned in the reviewed code; the
  module also has a concrete pool and quarantine map. Blue retains transparent
  actual-model reporting and adds opt-in, catalog/context-qualified concrete
  fallback; it does not copy hidden route presentation or infer model-specific
  quotas from an unclassified TPM error.
- Earlier endpoint/catalog/cache/dispatch comparison remains in
  [provider-capacity-review.md](provider-capacity-review.md). This revision does
  not claim a new Alpha runtime benchmark or overall equivalence.

## Reproducible controlled acceptance

```sh
python3 scripts/accept_context_continuity.py \
  --before /path/to/alpha4-blue-dragon --after target/debug/blue-dragon
```

The same 30 Arabic prompts, file, context/output target and three process-resume
boundaries are used for both binaries. A scripted loopback provider requests
native bounded file reads and, when advertised, browser status. The independent
oracle checks native success, exact nonempty bytes, offsets, evidence identity,
SHA-256, preserved user prompts and unchanged disk. Negative controls reject
failed/denied/stale/cancelled/unknown, cached, empty, and forged results.

| Measurement | alpha.4 | Candidate |
|---|---:|---:|
| Fresh-source verified exchanges | 7 / 30 | 30 / 30 |
| Provider completion requests | 15 | 60 |
| Local context-error mentions | 23 | 0 |
| Native browser status calls | 0 | 30 |
| Original user prompts retained | 30 | 30 |

See [controlled.json](measurements/context-continuity/controlled.json) for binary
hashes, timings, schema lists, packet sizes and final budget components. Extra
requests after repair reflect more completed work, not a tokens-saved claim.
This fixture explicitly uses RPM=120 to avoid turning a rapid deterministic
run into a pacing test. It contains no synthetic 429 and makes no live-capacity,
reasoning-quality, DOM, or phone claim.

An exploratory larger prompt fixture reached the real 32768 configured bound
near the end (minimum projected messages 27762 versus available 27751). That
failure was retained as a limitation; the committed representative workload fits
the hard window. Unit negative tests still require rejecting oversized unique
instructions and incomplete tool batches. Arbitrarily long instructions are not
made to fit by silently deleting them.

## Validation and remaining limits

- 337 Rust tests; formatting and clippy with warnings denied.
- CLI continuity: no replayed commands or live restored jobs.
- Provider boundaries: privacy clipping, streaming/nonstream context limits,
  refusal, pause, truncation and unknown stop reasons; no tool side effects.
- Offline model defaults, paid rejection, saved selections and key isolation.
- Integrated mock-browser fixture: two concurrent children, denied capability
  expansion, one unknown click without replay, revoked scope prevents I/O,
  and restart reporting makes zero provider requests.
- Chromium CDP test was attempted but could not launch: environment denied
  `socket()` in `process_singleton_posix.cc`. No workaround of that restriction
  was attempted. Mock browser results do not count as Chromium validation.
- No physical Termux, Windows or macOS run. No distributable Android binary or
  published update is produced by this branch. CI and device acceptance remain.
- Actual free-service capacity and model reasoning are external. Local success
  cannot guarantee 20–30 live useful exchanges. See the separate live record.
- Unique user instructions are preserved rather than semantically condensed.
  A model-generated rolling semantic checkpoint and calibrated tokenizer are
  not added here. Estimates remain conservative character heuristics. The
  automatic gateway's actual context/TPM limits can remain unknown.

For local review after building this branch:

```sh
BLUE_DRAGON_MAX_OUTPUT_TOKENS=2048 ./target/release/blue-dragon \
  --provider free --context-tokens 32768 chat
```

Use the configured context only when appropriate to the route; a known smaller
catalog capacity clamps it. `/context` reports the sources. Keep automatic free
routing as the default. Only opt into concrete free fallback after deliberately
selecting a catalog entry. No paid fallback is enabled and no new API key is
bundled. A Termux update command must name the eventual reviewed/published
artifact; do not claim this local branch is already available through installer.

## Pinned live trial (acceptance failed)

A synthetic local source comparison requested 30 exchanges on the free automatic
route. The pinned executable was SHA-256
`844b912f0e14f20c1a7b32236463d033e34cee29dfd46898004cbbd23aaf304d`.
This was the candidate before the final selected-schema fallback addition;
the measured live workload did not exercise that fallback. It is not a live
qualification of the final source tree.

The upstream reported `stealth/glyph-cluster`. Three CLI turns returned normally,
but only **2/3** met the independent fresh-source condition; the third supplied no
new source read. The harness stopped immediately on that acceptance failure.
There were 6 completion requests, 0 observed 429, 22373 reported input tokens and
2243 reported output tokens, over about 110.453 seconds including two 20-second
pauses. Peak reported rolling-minute total was 19226 tokens; the separately
labelled estimate-plus-output reservation peak was 41049. These are observations,
not quota values or a sustained-capacity guarantee. No model switch or paid
request occurred. The earlier exploratory trial was stopped and is excluded:
it used a changing executable and had already missed fresh rechecks.

[Live summary](measurements/context-continuity/live-summary.json),
[traffic](measurements/context-continuity/live-traffic.json), and
[independent verification](measurements/context-continuity/live-verification.json)
retain the distinction between a normal CLI return and successful investigation.
There is no same-time live alpha.4 or AlphaCode baseline, no hour-long run, and
no proven numeric TPM capacity. The requested 20–30 **live useful** exchanges
remain unachieved; service quota and model adherence cannot be guaranteed by
this client repair.

## Review correction: recurring discovery phases

Independent review reproduced browser → file_outline → browser → file_outline
blocking the fourth selection in dcdc508. `discover_once` now validates the
requested phase first, allows every actual selection change, and resets its
page/selection history on change. Unchanged duplicate discovery stays blocked.
`scripts/smoke_discovery_cycle.py` reproduces the old binary failure and checks
the corrected real CLI sequence, native outline execution, unchanged disk, and
the existing three-step limit. This is loopback testing, not browser execution
or live provider capacity evidence.
