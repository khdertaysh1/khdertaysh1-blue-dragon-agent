Blue Dragon 0.9.0-alpha.6 — release candidate, pending independent review, final CI and coordinated publication.

- Preserve recent tool context before archiving paired rounds, retain original history, and expose bounded derived continuity checkpoints without treating them as fresh evidence or instructions.
- Require native fresh-source evidence for supported explicit reread requests, including byte ranges and nameless range requests. Missing, stale, denied or wrong-range observations do not establish verification. Unsupported ranges remain unresolved.
- Validate advertised tool arguments before dispatch. For run_command only, decode a JSON-string args array once and parse a canonical numeric timeout string within bounds. Preserve original arguments and record normalization separately; never guess a program, invoke a shell implicitly or grant approval through normalization.
- Show distinct argument, denial, spawn, nonzero-exit, timeout and cancellation outcomes. Provide redacted local /tool-result inspection without provider calls or command replay. Bound malformed-call correction and block unchanged invalid repetitions.
- Accept one JSON-string encoding of an exact registered discovery name, record the conversion and retain human approvals. Equivalent unchanged selections remain repetition-blocked; changing tool phases still works. Step limits are unchanged.

The frozen implementation candidate 1a41627 passed 353 Rust tests, 15 command CLI/PTY cases and 35 pinned local qualification groups. These are implementation measurements, not substitutes for checks on the release commit. A subsequent single live free-route print task required one specific approval and produced native stdout with exit code 0 in three requests (14.48 seconds, no observed 429). That live model emitted correct argument types; malformed-name and command-type repair were demonstrated with scripted local providers. The earlier live trial stalled at five discovery steps. Model outputs differed, so this is not a controlled performance comparison or evidence of sustained service capacity.

No physical Termux, long-session reliability, new Windows/macOS execution or real-browser validation is claimed by this preparation. Narrow 40x24 Linux PTY tests are not phone tests. Existing alpha.5 tags/assets stay unchanged; the coordinating parent handles GitHub after independent review and required CI. See [continuity evidence](CONTINUITY_EVIDENCE_AUDIT.md), [command repair](COMMAND_ARGUMENT_REPAIR.md), [discovery repair](DISCOVERY_COMMAND_REPAIR.md) and [candidate gates](CANDIDATE_TRIAL_PLAN.md).

---

Blue Dragon 0.9.0-alpha.5 — historical preparation notes for the published baseline.

- Separate the soft input traffic target from the configured/catalog context bound, reserve the actual default Kilo output limit, and preserve original instructions/evidence and tool pairing.
- Reduce already-selected oversized schema sets, advertise browser capability for URL-bearing prompts without widening permissions, and continue analysis with explicit failed/unknown initial browser evidence.
- Keep initial browser failure recoverable in interactive chat, but make one-shot `inspect`, URL aliases and URL-only `run` return failure without sending a model request when initial inspection is denied or fails. Preserve the failed observation and partial report.
- Fix recurring discovery transitions: browser → file_outline → browser → file_outline now works; unchanged duplicate discovery remains blocked and turn step limits remain enforced.
- Prevent an unchanged context-recovery request from being resent. Distinguish latest projection failure from the last successful request.
- Add explicit opt-in concrete free-model fallback restricted to verified free/tool-capable routes with adequate known context. Keep automatic routing pinned and shared/unknown quota failures non-rotating. No paid fallback or quota bypass.

Controlled local acceptance achieved 30/30 fresh-source exchanges versus 7/30 on alpha.4; it is scripted loopback evidence, not a service-capacity claim. The pinned live trial achieved only two successful fresh-source exchanges; the third omitted the required read and failed acceptance. There is no demonstrated 20–30-turn live stability, no guarantee against 429, no measured numerical TPM ceiling and no physical Android qualification. A normal CLI return is not semantic task success. See [source comparison and evidence](context-continuity-review.md).

The coordinating parent handles GitHub after review and required CI. Existing tags/assets are unchanged. Cross-builds do not count as phone testing; final Chromium/Firefox and platform CI remain required. Exact local release-candidate evidence is recorded separately.

---

Blue Dragon 0.9.0-alpha.4 — historical published developer-preview preparation notes.

- Preserve the selected `kilo-auto/free` gateway route instead of replacing it with concrete model IDs after failures. No paid fallback or shared-quota bypass.
- Share cached catalog refreshes and suppress duplicate concurrent requests. Record bounded metadata-only provider traffic, separating estimates/reservations from reported usage and program-default cooldown from provider Retry-After.
- Bound free-route input/output budgets and initially advertise focused, discoverable tools. Preserve original instructions/evidence, archive completed reasoning envelopes and older tool rounds, and retain source text/offsets in compacted windows. Pending tool batches cannot be hidden to fit context.
- Coordinate optional known RPM/TPM and local free-hourly admission across sessions using the same application state. Keep high-RPM storage readable with equal bounded read/write limits, atomic oversized-write rejection and expiry recovery.
- Require successful, nonempty, relevant source reads in the live-trial verifier. Failed/denied/not-executed/cancelled results and empty or irrelevant windows cannot establish a fresh comparison.
- Preserve PR18's published alpha.3 status, existing functionality, user approvals and licenses. No new scanner, credentials, paid service or device-validation claim.

Historical controlled measurements reduced estimated input from 158,822 to 40,300 for the same completed workload. A historical live trial observed ten requests without 429, but only three of four rounds performed the requested fresh read and some model-authored byte spans were wrong. These measurements do not establish this tag's live capacity, quota scope, full reasoning reliability or AlphaCode parity. The original TPM limit remains numerically unknown. See [full evidence and limitations](provider-capacity-review.md).

Platform/real-browser CI remain publication gates. Physical Android has not been tested. The version bump does not replace published alpha.3 assets. The coordinating parent handles GitHub; this local preparation does not publish anything.

---

Blue Dragon 0.9.0-alpha.3 — historical published developer-preview notes.

- Preserve redacted HTTP status, provider reason/code/request ID and cooldown provenance. Redact bounded complete fields before clipping to prevent partial credential disclosure.
- Validate tool identities and complete arguments before dispatch; reject explicit incomplete or unknown streamed stops. Retain partial text as historical data, never executable results.
- Coordinate local request admission and provider cooldowns across parent/child sessions sharing the same home, origin and credential identity. Never replay uncertain requests or evade shared limits by switching models.
- Preserve long-session constraints and original history, use bounded discoverable tool selection, and preserve supported text blocks in Anthropic translation instead of silently discarding constraints.
- Add bounded child edit proposals with pinned evidence, parent diff approval, version/conflict checks, durable journals, conditional rollback and independent final readback. Children cannot directly write. Multi-file replacement is not filesystem-atomic.
- Reject bare carriage returns in proposed/existing text; display CRLF and literal backslashes explicitly in approval diffs without changing file bytes.
- Retain the approved Abyss identity and existing browser, source, evidence, memory, coverage and MCP capabilities. No external vulnerability scanners added.

Exact release-candidate validation is recorded separately. Development-source tests do not establish this tag's CI status. Local fixtures do not prove live-model capability, Alpha equivalence, real-browser behavior or physical Android execution. Platform and real-browser CI remain publication gates; unavailable device testing remains explicitly untested. Existing tags and published assets are unchanged.

See [candidate and trial plan](CANDIDATE_TRIAL_PLAN.md), [provider reliability](PROVIDER_RELIABILITY.md), [provider review fixes](PROVIDER_BOUNDARY_FIXES.md), and [supervised edit limits](SUPERVISED_EDIT_PROPOSALS.md).

---

Blue Dragon 0.9.0-alpha.2 — historical Abyss identity notes.

- Integrate the selected full-wing geometric dragon into README as a genuine GIF, with a static SVG alternative and reproducible rendering script.
- Keep the banner static with cyan BLUE and white DRAGON; apply the same palette and split wordmark in the terminal.
- Offer opt-in decorative terminal energy using BLUE_DRAGON_MOTION=1; suppress it for approval prompts, compact layouts, NO_COLOR and reduced-motion preference. Ordinary Termux text terminals do not display the README bitmap mascot.
- Make the PTY title oracle SGR-aware without removing cursor controls, retaining the required BLUE DRAGON title assertion and adding ANSI/chunk-boundary regression cases.
- This is a new release candidate, not a replacement of the 0.9.0-alpha.1 tag. No model/tool/approval-policy behavior changes. Physical Android validation remains separate.

---

Blue Dragon 0.9.0-alpha.1 — historical integrated package notes.

- Read large local UTF-8 files with bounded byte windows and streaming search, including long lines and safe continuation offsets. Preserve the existing small-file editing contract and remote source cache.
- Track explicitly planned path/method/account/scenario test cells with pinned evidence versions and derived not-tested/blocked/inconclusive/tested states. No coverage percentages or implied vulnerability/authentication proof.
- Link existing hypotheses to checked case evidence, preserve earlier observations after inconclusive retests, and reconstruct bounded retest recipes without automatic execution.
- Recover research context, scope changes and evidence references after restart; preserve the parent browser's sole mutable ownership and distinguish current connection state from historical page evidence.
- Run real independent read-only model agents with separate sessions/evidence, parent and peer messages, two-child concurrency, durable shared request/tool/output quotas, short waits, cancellation and collected results. No child shell, writes, external fetch, browser, MCP or self-approval. Contradictory summaries remain reviewable, not automatically resolved.
- Restore interrupted child activity as unknown after process death; never replay uncertain requests. Existing background shell jobs remain separate from delegation.

See INTEGRATED_ALPHA_COMPARISON.md, DELEGATED_AGENTS.md, TEST_COVERAGE_LEDGER.md and LOCAL_FILE_WINDOWS.md for evidence boundaries. This is not Alpha parity, a live-provider benchmark, or physical Android verification. Platform CI and actual device trials must cover the final candidate before making those claims.

- Inspect bounded child evidence with mandatory result pins, revalidate reports and memory sources without replay, and distinguish historical captures from current truth.
- Use approved stdio MCP tools and exact catalog resource URIs; resource text remains untrusted and is not automatically secret-redacted.
- Navigate local syntactic outlines and bounded browser scrolling with explicit scope and visibility checks.
- Obtain a separate CI device-trial kit with deterministic loopback fixtures and per-binary evidence. This does not replace real browser CI or actual Android trials.

See [the candidate and trial plan](CANDIDATE_TRIAL_PLAN.md) for remaining gaps and publication gates. `0.9.0-alpha.1` is a naming decision, not a benchmark result or guarantee.
