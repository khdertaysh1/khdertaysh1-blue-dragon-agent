# Bounded source and audit workflow

## User-controlled scope

```
/scope allow https://authorized.example/app read anonymous
/scope allow https://authorized.example/app interact test-account-a
/scope allow https://authorized.example/app mutate test-account-a
/scope
/coverage
/audit-report
```

Only user CLI/TUI commands change scope. Grants persist with the session. `clear`
removes grants and keeps enforcement enabled. An origin/path prefix matches
path segments (`/app` does not match `/application`); ambiguous encoded separators
and traversal-like encodings fail closed. `read` permits scoped reads/navigation;
`interact` additionally permits the built-in fixed DOM canaries; `mutate` permits
proposing click/fill/select/check/arbitrary evaluation, still with separate action
approval. Those DOM interactions can submit forms or cause server mutations.

A bare URL establishes a read-only origin/path scope if none exists. Related
links outside that prefix are not automatically inspected. Existing configured
grants are never silently widened. In a general coding session without any
configured policy, legacy per-action approvals remain; configure `/scope` before
a scoped audit. A scope grant does not bypass read-only mode or approvals.

**Boundaries:** account labels are user declarations, not verified login identity.
Reconfigure when changing accounts. This is not a browser network firewall:
page scripts, subresources, HTTP redirects and click handlers can make requests
outside scope. The engine checks requested destinations and current top/frame
location before direct scoped actions, and checks automatic inspection redirects
before exposing page evidence. Arbitrary commands/MCP remain outside this policy
and require their own approvals. Cross-origin frames fail closed for DOM access.

## Large JavaScript and text

Ask the agent to use `source_artifact`:

```json
{"action":"fetch","url":"https://authorized.example/app/bundle.js"}
{"action":"search","artifact_id":"<returned sha256>","query":"location.hash"}
{"action":"read","artifact_id":"<returned sha256>","offset":12000,"count":4000}
{"action":"trace","query":"userInput"}
```

Fetch asks consent and downloads at most 8 MiB into private session-local app
storage. Limits: 32 URLs, 64 MiB per session, 30-second request/10-second connect
budget, UTF-8 text only, no cookies or redirects. SHA-256 validates later reads.
A repeated fetch—even after resume—reuses the historical copy without network
access; fresh remote validation needs a new session or a separately approved
ordinary fetch. No cache output is labeled fresh remote evidence. The model gets
metadata, not megabytes of source. Read uses exact UTF-8 byte boundaries and at
most 16,000 bytes; literal search returns at most 20 locations with short snippets.
The existing small `web_fetch` and browser-observed source paths retain their
limits. A downloaded copy is not necessarily the authenticated browser response.

Trace examines at most eight cached sources / 16 MiB using JavaScript ASTs with
parse/traversal deadlines. It returns assignment/call/return candidates and links
imports whose resolved URL is already cached. Search successive variable names
to inspect candidate chains. It **does not** resolve type/alias/shadowing behavior,
interprocedural taint, import maps, bundler module IDs, control flow or sanitizer
correctness. Omitted files, parse errors and limited scans are reported. An
`innerHTML` occurrence or an apparent sanitize call is not a security verdict.

## Readiness, frames and recovery

`browser wait_for` accepts `selector`, optional `text`, and `timeout_ms` (100–10000).
It observes one visible matching element after DOM loading and three equal text
samples separated by 100ms. `react_complete_proven` remains false. An app-specific
ready marker is more informative than a stable body; polling can miss later work.
DOM actions accept a single `frame_selector` for a same-origin iframe. Snapshot
lists frame metadata; it does not recursively claim frame coverage.

After a lost saved transport, a fresh read can separately request reconnection.
Previous pages, logins and uncertain actions are not replayed. A new connection
may have a blank page; scoped reads can correctly remain blocked until explicit
navigation. The extension needs its paired client online. No failed mutation is
automatically retried. Browser package setup reports progress without copying
package logs into model context.

## Coverage, hypotheses, comparisons and reproduction

- `audit_coverage` / `/coverage` lists at most 500 deduplicated targets and recent
  evidence IDs: discovered/not tested, observed/not security verified, or blocked/
  failed. URLs omit query values and fragments. Different tracking-token values
  collapse; this ledger is not a complete crawler or endpoint inventory.
- `record_finding` hypotheses require `rationale`, `next_test`, `confirms`, and
  `refutes`, plus existing summary/limitations/references. These remain model
  proposals, not execution permission. Supported citations are not semantic proof.
- `compare_observations` requires two paired tool evidence IDs, the same tool,
  and one changed serialized argument field. URL origin/path must stay fixed;
  query parameters are compared individually. Cached/denied/unknown results are
  rejected. Native metadata and response hashes are returned without raw values.
  Account, session, timing and server-state equivalence is not established.
- Fixed DOM canaries require interact scope and a clean random-property baseline.
  Positive output establishes only the observed window-property effect; a query
  result may be server-reflected. Negative means not observed in this bounded test,
  not safe. General confirmed-vulnerability classification is not automated.
- `/audit-report` emits a redacted evidence/reproduction index with target paths,
  input/result hashes, native facts and original message references. Exact inputs,
  raw responses and private hypothesis narrative stay in the local session, where
  `recall_context` retrieves them without rerunning tools. It is **not yet a complete
  standalone reproduction report**: a human must select/redact inputs, preconditions,
  steps and demonstrated impact. Paths can contain secrets and require review.

## Eleven requested acceptance areas

| Requirement | Implemented/testable mechanism | What remains unverified or incomplete |
| --- | --- | --- |
| 1. Large files | One download, disk cache, SHA validation, tail search/read after server closure | >8 MiB sources rejected; browser-authenticated large-body capture not unified |
| 2. Browser lifecycle | Setup/progress, explicit readiness, same-origin frames, read reconnect, no mutation replay | Actual React completion not asserted; current real-browser run blocked locally; cross-origin DOM unsupported |
| 3. Scope | Persisted user grants, origin/path/action checks before fetch and browser work | Login identities not verified; not a network firewall or command sandbox |
| 4. Coverage | Persisted deduplicated page/form/API/source discoveries and blocked/observed states | No exhaustive crawl, automatic account-state partition or full endpoint coverage |
| 5. Data flow | AST candidates, local transformation/use snippets and cached import links | No complete semantic/interprocedural taint or sanitizer proof |
| 6. Hypotheses | Required rationale, next test, confirmation and refutation conditions | Meaning and adequacy remain model/human judgment |
| 7. Controlled comparisons | One argument-field pair with native evidence; clean canary baseline | Hidden session/server/time factors not controlled; no generic autonomous experiment runner |
| 8. Long work | Existing jobs/partial reports plus progress, source reuse and session locks | Windows background jobs unsupported; no job survival after process restart |
| 9. Memory/context | Exact recall, bounded source excerpts, capped checkpoints, lexical scoped notes | Finite session/context limits; no semantic long-term memory guarantee |
| 10. Conclusions | Native assertions, evidence pairing, explicit unknown/negative/observed labels | Prose truth and exploitability not mechanically proven |
| 11. Reproduction | Redacted target/input/result hashes and local exact-history references | Full standalone redacted reproduction needs human review and remains incomplete |

All current test claims are listed in [PROFESSIONAL_REVIEW.md](PROFESSIONAL_REVIEW.md).
