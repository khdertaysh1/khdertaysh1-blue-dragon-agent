# Command argument repair (integrated follow-up)

The confirmed Termux record contains a valid JSON object whose `program` is a string, but whose `args` and `timeout_seconds` have the wrong JSON types. The previous dispatcher required an array and integer and returned `Missing args array` before asking for approval. The schema already advertised the correct types. There was no central property-type validation/compatibility layer, and CLI/TUI collapsed outcomes into “failed or denied”. Earlier `Missing or invalid program` records are a different shape failure; without those raw calls their exact cause remains unknown. We do not attribute either failure to streaming without evidence.

## Actual path and changes

| Stage | Code | Behavior |
|---|---|---|
| Advertised schema | `src/tools.rs::definitions`, `src/tool_selection.rs::filter/discover` | `args: array<string>`, timeout integer remain advertised; no loosening to string schemas |
| Request projection | `src/agent.rs::execute_turn`, `src/reasoning/context.rs` | Snapshot exact dispatched definitions; schema selection/context compression do not rewrite property types |
| Streaming | `src/streaming.rs::Assembly::{event,finish}` | Accumulates serialized fragments; regression preserves the entire original arguments string and Unicode, with no duplication or type coercion |
| Whole batch | `src/provider_contract.rs::validate_completion` | Incomplete/non-object argument envelopes or interrupted/limited streams cannot dispatch a batch |
| Central arguments | `src/tool_arguments.rs::prepare/validate` | Validate types, required properties, enums, schema alternatives, advertised bounds and additional-property restrictions before approval. Not a complete general-purpose JSON Schema implementation (external `$ref` resolution is not provided); native tool validators remain in place |
| Narrow normalization | `src/tool_arguments.rs::prepare` | Only `run_command`: decode `args` JSON string once; require bounded strings; parse canonical ASCII decimal timeout string into 1..300. Reject recursive encodings, malformed JSON, wrong elements, excess limits, NULs, whitespace/sign/fraction/leading-zero ambiguity. Never guess `program`, map `cmd`/`command`, invoke a shell or change argument contents |
| Original and effective records | `src/agent.rs::execute_turn` | Original assistant tool-call arguments remain byte-for-byte in the transcript. Separate result metadata records operations, effective arguments and that normalization grants no permission. Original request/result digests remain verifiable |
| Approval and execution | `src/tools.rs::run_command_with_timeout` | Same user approval; preview is escaped JSON of the actual argv/timeout, not ambiguous space-joining. `Command::new().args()` without shell expansion. Denial starts no process |
| Bounded correction | `src/agent.rs`, `src/reasoning/loops.rs` | Typed diagnostics with tool, field, expected/actual type and a value-free example. Identical invalid calls are blocked; at most one corrective opportunity after invalid input. Unresolved invalid input cannot become final success merely through prose. Unknown timeout/cancellation outcomes are not replayed |
| UI/local inspection | `src/agent.rs::AgentEvent`, `src/tui.rs`, `src/main.rs` | Distinguish invalid arguments, denial, spawn failure/OS code, nonzero exit, timeout and cancellation. `/tool-result EVIDENCE_ID` revalidates the saved pair locally: no provider request and no execution. It conservatively omits all arbitrary string values and unknown object keys (including argv, URLs, tokens and stdout), preserving types/structure/lengths/numbers and a safe outcome summary. Private originals are unchanged |

Command timeout/cancellation only establishes the handling of the direct process here. This change does not claim new descendant process-group guarantees or new Windows process-tree support. Successful process exit is not proof of an entire investigation.

## Acceptance

`scripts/smoke_command_arguments.py` uses the actual CLI with a loopback scripted provider. The exact reported curl payload is normalized and presented for approval, then denied; the external site is never contacted. A separate local Python fixture with the same bad types executes only after approval and returns independently checked stdout, exit code, and an append-only marker proving one execution. Cases cover denial, repeated malformed calls, bounded correction, invalid final prose, missing executable, nonzero exit, streamed fragments, truncated streams, and 40-column TUI approval/denial with Arabic/English input. Every request after discovery checks the actual advertised command schema. A dedicated forced-projection case also checks a positive projection level while the provider still receives array/integer command types. Saved-record inspection is checked for no provider calls, no replay and no exposed synthetic command strings.

Unit tests cover original payload conversion, missing/wrong program, valid/invalid/nested encoded args, non-string elements, argument/count limits, numeric/text/out-of-range timeouts, Unicode/metacharacter preservation, approval, cancellation and timeout. Existing projection tests continue to check schema reservation and paired history. The integration runner and cross-platform CI now include the command regression.

The independent nameless explicit-range finding is fixed in this same branch: `Read bytes 1000..1100 again.` retains an unresolved request or resolves a prior trusted source; it no longer disappears just because no filename or `source/file/window` word is present. CLI and unit regressions reject prose-only success.

## Boundaries and delivery

No real Android/Termux device was available. Linux PTY width 40 is a terminal-layout test, not a phone test. Any optional live trial is separately recorded and approves only the exact local `python3 -c print('BLUE_COMMAND_LIVE')` command; it cannot approve arbitrary generated commands. It does not prove malformed arguments occur in that live run, sustained provider capacity or long-session reliability.

This follows the existing `fix/explicit-source-ranges` work, retaining the reviewed source/evidence/context changes. No new parallel project, credential copying, paid fallback, existing-tag replacement or published-asset modification. Upload/release remains coordinated separately; no command is advertised as installing an unshipped fix. After a reviewed corrected version is actually available, its immutable tag/asset and checksum must be used for the Termux update instruction, preserving existing sessions/configuration and source edits.


## Live observation in this work

The optional trial on the pre-final privacy-only build `73c54edfbd1f52e2fc6e917d8ee366c163a620348bb68228fc3bf325e69f424f` failed after about 30 seconds at the five-step guard: the free model repeatedly used discovery and never requested command approval. No command was approved or executed. This is an unresolved live orchestration failure, not evidence of successful command execution or a quota-capacity result. No same-request retry, model rotation or increased step limit was used to hide it. The final build differs in local inspection redaction/UI handling only; no final-build live success is claimed.


Final local evidence: [validation manifest](measurements/command-arguments/validation.json), [13 CLI/PTY cases](measurements/command-arguments/cli.json), [35 integrated groups](measurements/command-arguments/qualification.json), and [separate failed live observation](measurements/command-arguments/live.json). 351 Rust tests and all-target Clippy passed. Qualification covered the final runtime; the subsequently added forced-projection harness case also passed on that same executable. No Windows/macOS/physical Android execution or final-build live success is claimed.
