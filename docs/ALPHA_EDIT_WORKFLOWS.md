# Shared edit workflows with independent filesystem checks

This branch extends reviewed read-oracle fix `266237b`, retaining the full integrated package. The read comparison branch and its oracle-fix branch remain unchanged.

The new actual CLI fixture runs five predetermined workflows on synthetic files:

- Read, uniquely edit, and read back an ASCII configuration.
- Read, edit, and read back UTF-8 text containing Arabic and an emoji.
- Read, request a nonexistent match, and verify rejection and unchanged bytes.
- Read, request a duplicated match without replace-all, and verify rejection and unchanged bytes.
- Read, reject an absent match, apply a valid edit, and read back the result.

Expected byte states are literal fixtures independent of either implementation and of its returned prose. After every paired tool result, the harness checks the exact target bytes and an unrelated sibling's bytes. It checks them again at process completion. Checks reject non-regular replacement paths and read only expected length plus one byte. Positive native results must pass the corrected fresh/success status gate; negative results must identify the intended match failure. Permission errors, unknown outcomes, stale results and successful responses cannot substitute for expected rejection. Final model text and exit code alone never establish a pass.

The five workflows validate selected tool chains and error recovery with scripted calls. They do not evaluate autonomous planning, self-directed debugging, atomic multi-edit equivalence, whitespace repair equivalence, concurrent-writer safety or model quality. Alpha repairs some whitespace-only mismatches while Blue's exact edit requires a unique literal match; these cases deliberately use exact shared inputs and do not hide that distinction.

## Permissions, setup and observed differences

Every case gets a fresh workspace/home and scrubbed environment. The task expressly authorizes edits only to synthetic `config.txt`. Both CLIs keep their normal approval settings; four affirmative stdin lines answer ordinary prompts for those edits. No auto-approve/read-only override or scope expansion is supplied. This does not establish approval-policy equivalence. The sibling check is not a complete filesystem audit or OS isolation.

The deterministic HTTP endpoint enforces a 4 MiB request-body cap and a strict request-count bound. Alpha is told the synthetic endpoint supports a 65,536-token context using its documented `ALPHACODE_OPENROUTER_CONTEXT_WINDOW` setting. The initial attempt with its 16,384 default failed its context guard before the workflow completed; this was an adapter-capacity mismatch, not a functional score. No live provider capability is inferred. No context guard is removed.

Blue's normal post-edit completion review causes one additional completion-only provider turn in successful mutation cases. The fixture permits at most one such turn and rechecks the disk; it emits no further tools. Initial attempts incorrectly classified this review as an unexpected request. The final harness preserves and records it instead of disabling it. Product acceptance checks may still be unverified; filesystem fixture assertions do not promote the product's own task-check status.

Alpha uses the previously documented default-feature, lower-memory local build; Blue uses the pinned release binary. No timing ranking is appropriate. Alpha update checks, self-development, external MCP loading and automatic task follow-up remain disabled as in the reviewed read fixture; filesystem permissions and approval enforcement are not changed.

## Running and review

```sh
python3 scripts/test_benchmark_alpha_workflows.py
python3 scripts/benchmark_alpha_workflows.py --blue /absolute/blue-dragon --alpha /absolute/alphacode --repeats 3 --output new-workflows.json
```

The report refuses overwrite and records both binary hashes, harness and imported oracle hashes, paired result hashes, per-step disk-state hashes and completion-only turns. Binary hashes are rechecked after measurement. Missing Alpha is explicitly `not_run`; CI/local qualification runs only the Blue preflight and negative controls without rebuilding Alpha or downloading anything.

Negative controls cover success prose without a write, partial mutation despite a rejection, sibling damage, oversized and wrong-type replacements, denied/unknown/stale results, unrelated permission errors, and stale readback. Exact run evidence is recorded in the validation file for this branch. No remote push, merge, tag or release is part of this work; actual phone/browser/live-model evaluation and broader capability comparison remain open.

## Validated local result

Tested harness commit `80c92b3`: Blue 15/15 and Alpha 15/15 (five workflows, three repetitions each). Each side supplied 48 paired native tool results with independently checked disk state. Blue used nine completion-only follow-ups across the successful mutation samples; Alpha used none. This is an observed workflow difference, not a quality or efficiency ranking.

Eight workflow-oracle tests and eight inherited read-oracle tests passed. All 30 integrated qualification groups passed, including the existing 24/24 deterministic runtime regression samples. Every qualification log hash, both executable hashes and both imported harness hashes were checked. Rust runtime source remains identical to the reviewed recovery fix; no new Rust-test/Clippy run is claimed for these Python/CI/documentation changes.

[Structured evidence](../validation/alpha-edit-workflow-readiness.json). Raw local reports: `/tmp/blue-alpha-edit-validated.json` and `/tmp/blue-alpha-edit-qualification`. Binary identities remain Blue `ab3de1d8436189f4a6aa3217b85839e037fb4c6a5c59caf8551e5a9228ea47aa` and Alpha `7bdc7d6268e5b21f5480eb7b72eba5e1da5ffd3f1549d148bf6de4f16278fbca`. Independent review of the new workflow harness remains pending. No remote publication occurred.
