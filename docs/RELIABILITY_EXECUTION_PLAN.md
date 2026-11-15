# Supervised research/coding reliability work

This tracks the user's 24-part request. A local passing test does not mean the
whole request is complete. Preserve published releases, Abyss identity, original
session history and unrelated user changes. Do not add external vulnerability
scanners. Publish one coordinated package only after final review and CI.

| Priority | Current evidence / remaining work |
| --- | --- |
| 1 Provider diagnosis | Selected HTTP/error/request-ID fields, redaction and cooldown provenance added; arbitrary prose-secret detection is limited. |
| 2 Request compatibility | Structural preflight and complete argument parsing added; Anthropic text-block constraints preserved with unsupported blocks rejected. Full model/schema compatibility remains. |
| 3 Stop reasons | Response limit/filter/refusal/empty/malformed/connection outcomes separated; partial text is historical, not executable evidence. |
| 4 Request consumption | Shared durable local quota and cooldowns added; automatic transient POST retry and configurable quota profiles remain deferred. |
| 5 Model selection | Active child model corrected and reported model recorded; alternate live-provider/catalog validation remains. |
| 6 Long sessions | Existing bounded projection preserves originals and constraints; one explicit context-error recovery remains. Semantic supersession still needs work. |
| 7 Relevant tools | Definition/usage metrics and explicit discoverable phase selection added; full-schema context overflow falls back visibly to core tools. Automatic semantic phase choice remains. |
| 8 Large files | Existing versioned remote cache and bounded UTF-8 local windows/search; covered by existing qualification, not a phone run. |
| 9 Code investigation | Existing syntactic symbol/import/re-export/call/source chains; unresolved edges and lack of runtime-flow proof remain explicit. |
| 10 Browser | Existing managed browser, readiness, frame/scroll/form controls and exception-envelope correction; no new real-browser validation claimed here. |
| 11 Scope/approval | Existing action/URL scope and parent approval remain; not a network firewall for page scripts or redirects. |
| 12 Hypotheses | Existing findings, competing hypotheses, next tests and retest deltas; no automatic semantic winner. |
| 13 Coverage | Existing path/method/account/scenario ledger with evidence-derived states; no authentication or full-site coverage inference. |
| 14 Comparisons | Existing pinned controlled request/source comparisons and local positive/negative labs; tool success is not vulnerability proof. |
| 15 Memory/evidence | Existing workspace isolation and source-pin revalidation; complete limit alignment and broader privacy-bounded retrieval remain. |
| 16 Recovery | Existing interrupted/unknown checkpoints plus new private partial-response retention; no uncertain action replay. |
| 17 Commands | Existing bounded monitored jobs, cancellation and process-group tests; Windows limitations remain explicit. |
| 18 Agents | Existing separate sessions/evidence and bounded concurrency, plus shared provider admission; nested delegation/isolated mutable workspaces remain separate work. |
| 19 Multi-file edits | Bounded child proposal/parent approval/journal path integrated on this development branch, including independent final readback and unknown-outcome handling; still requires independent review and cross-platform validation. |
| 20 Reports | Existing evidence revalidation and deterministic partial report, plus redacted provider diagnostics; model prose is not self-verifying. |
| 21 Android UI | Approved narrow-layout Abyss identity preserved; physical keyboard/startup/cancellation tests unavailable here. |
| 22 End-to-end | Existing pinned positive/protected loopback qualification and new fault tests; scripted providers do not establish autonomous live-model performance. |
| 23 Alpha comparison | Latest 1.1.8 source reviewed. Historical deterministic comparisons do not establish parity with this revision; no invented score. |
| 24 Device delivery | New code is local development, not a published Termux binary. Exact-candidate Android CI and a physical-device trial remain required. |

Release acceptance remains open: record exact source/binary hashes; rerun the
complete qualification on the final executable; require Windows/macOS/Linux and
real-browser CI; build Android ARM64; then provide that artifact with its checksum
and a short English trial prompt. A phone must be available before claiming the
large-file/restart scenario, Android system termination or narrow-screen behavior
passed on physical hardware. Do not relabel a Linux/mock/cross-build result.
