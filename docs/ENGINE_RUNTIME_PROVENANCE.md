# Native provenance and review-engine integration

Local continuation of 438b127 on improve/engine-runtime. Nothing uploaded.

## Automatic URL inspection

New native inspections record the exact requested URL and a SHA256 of the native
result before the explanatory user-message wrapper is created. Recovery accepts
this path only for the expected observation/tool/role, with the known wrapper,
matching requested URL and matching result hash. Wrapper text alone does not
create provenance. Changed results fail validation. Older checkpoints without
this metadata remain explicitly unavailable; there is no guessed migration.

The report provides a local checkpoint request reference and verified response
hash without exporting query values. The form review module can now consume a
verified automatic snapshot as well as a paired browser snapshot. Tests cover
save/load, missing provenance, modified result rejection and private-value
omission. This is an integrity check against the stored native metadata, not
protection against an adversary rewriting the entire checkpoint and its hash.
Actual browser collection for this new path still needs real-browser CI.

New record_finding calls revalidate their cited request/results instead of
trusting cached observation summaries. A synthetic older checkpoint demonstrates
that a stored status can look supported under the former metadata-only check,
while the new path rejects its historical original result. This prevents the
old metadata from being used to create a new supported finding after resume.

## End-to-end engine smoke

scripts/smoke_review_engines.py runs the real CLI, a scripted local model endpoint
and a real loopback HTTP redirect. It verifies the complete tool dispatch and
persistence path. The allowed case makes exactly one native GET, follows no
redirect and invokes the model three times. The empty-scope case makes zero
native GETs and both tools refuse execution/review. Engine output omits fixture
secrets and does not claim vulnerability confirmation. The reviewed evidence ID
is now source_evidence_id, separate from the agent-assigned result evidence_id.
The script is added to the existing three-platform CI runtime step for a future
authorized upload; that CI has not run for this branch.

## Targeted module follow-up

source_artifact trace accepts an optional artifact_id. It can inspect a chosen
cached file beyond the first-eight-source default window, without downloading
anything. Explicit import alias bindings suggest bounded follow-up queries in
the cached target (for example transform -> decode). Declaration candidates make
that follow-up useful for locating function definitions. Dynamic dispatch,
shadowing, import-map resolution and semantic taint remain unproven. Suggestions
are not automatically executed and do not expand scope or download sources.

## Vision preflight

vision_status reports only locally configured adapter support, capture
availability and the existing image-history destination lock. It performs zero
network requests, cannot verify live access/pricing, and never assumes that a
free/gateway model accepts images. Existing separate image approval and no
alternate-provider forwarding rules remain in force.

## Validation

169 Rust tests passed on Linux; Clippy all-targets with warnings denied, release
build, formatting and diff checks passed. The final executable passed both CLI
engine smoke cases. Its SHA256 is
f389361548b07c5e3595ba2b5afea578a04adeba1516a7fb009d3530ac11cdf7.
No physical Termux, native Windows/macOS, live-provider vision or real-browser
integration run was performed for this branch. The modules remain passive
hypothesis review; full active multi-type scanning, semantic dataflow and
standalone replay remain unfinished. No Alpha parity claim is made.
