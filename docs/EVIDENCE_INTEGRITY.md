# Evidence integrity and paired-result review

Local continuation from 94a2f10; no remote upload, merge or release.

New native observations record a canonical JSON result SHA256. The agent also
records the parsed original tool-argument SHA256 before attaching the evidence
ID to the result. Validation excludes only that reserved top-level evidence ID
from the result digest; if present, the ID must match the referenced observation.
Scalar results use the same result envelope as the agent. This binds request and
result content to the saved observation, without exporting raw inputs or bodies.

The shared request resolver now rejects altered requests/results, empty call IDs,
duplicate results for one call, and pairing across a user/system boundary.
Distinct results in a single assistant tool batch remain supported. Engine
reviews, native finding validation, comparisons and reproduction reports use
this resolver. A change even outside an asserted status field invalidates the
saved evidence for rechecking. Existing automatic-inspection provenance retains
its separate verified envelope and request URL checks.

Legacy observations can omit these fields through serde defaults. Reports label
whether metadata contains both digests, only a result digest, an automatic
inspection envelope, or a legacy pair without digests. Missing digests are never
invented. These are consistency checks, not signatures: rewriting an entire
checkpoint and its digests is outside this guarantee. Legacy pairs have weaker
content-integrity guarantees. Hash equality does not establish vulnerability,
semantic correctness, authorization, freshness or independent observation.

## Validation

184 Rust tests passed on Linux. Regression coverage includes altered arguments,
altered native content, duplicate results, wrong embedded IDs, conversation
boundaries, scalar/object envelopes, legacy pairs, valid multi-tool batches and
invalidation of saved findings and selected engine reviews. Existing freshness
tests explicitly exercise legacy records without a digest so digest rejection
does not mask freshness-policy regressions. The initial two fixture failures
were resolved by distinguishing newly observed changed responses from mutated
records; the full suite was rerun successfully.

The real CLI plus a loopback scripted provider passed all three engine scenarios:
1 allowed GET, 0 denied GETs and 3 comparison GETs, with no followed redirects.
The smoke additionally recalculates each native checkpoint request/result hash
from its saved paired messages. Continuity smoke passed with zero replayed
commands, zero live jobs and the correct unknown-result message index.

Clippy all targets with warnings denied, release build, formatting and diff
checks passed. Runtime results and binary SHA256 are in
validation/evidence-integrity-smoke.jsonl.

No physical Termux, native Windows/macOS, live model/vision provider or graphical
browser test was run for this batch. No new active vulnerability engine or full
semantic dataflow was added. Browser enforcement, standalone redacted replay
reports and process recovery retain the limits of earlier batches. This batch
is evidence reliability work, not an Alpha parity measurement.
