# Common runtime read contracts

The reference is Alpha `773e3ba2f3e30e47d09cd5b751d224a1592dc804`. This branch contains independently reviewed delegate recovery fix `2181c7e` and leaves its review branch unchanged.

`scripts/benchmark_alpha_reads.py` adds a common, intentionally narrow actual CLI fixture: short file content, UTF-8 content, selected source range excluding neighboring lines, and missing-file error reporting. Blue uses byte windows; Alpha uses line ranges. The adapter explicitly maps those coordinates and tolerates Alpha's line-number presentation. A deterministic loopback model emits one native read call, and assertions inspect its paired tool result on the next model request. Model conclusions and successful process exit alone are insufficient.

Each case uses a fresh temporary workspace/home and a scrubbed environment without inherited account credentials. The source file must remain unchanged. Alpha runs with explicit local provider/model, update checking, self-development, MCP loading and automatic follow-up disabled. This is configuration isolation, not OS network/process sandboxing. Fixtures are bounded to four model requests and a 45-second CLI timeout. Output files refuse overwrite. Reports record binary and harness hashes, adapted tool arguments, result hashes, elapsed time and explicit failures/not-run status. Failure may be an adapter problem and must not automatically be called a product defect.

Run:

```sh
python3 scripts/benchmark_alpha_reads.py --blue /absolute/blue-dragon --alpha /absolute/alphacode --repeats 3 --output new-comparison.json
python3 scripts/test_benchmark_alpha_reads.py
```

Omitting `--alpha` runs only Blue preflight and records Alpha as `not_run`. CI and local qualification run this preflight and the harness negative controls without downloading or building Alpha. Mocked transport tests do not qualify the actual Alpha adapter. Negative controls reject unpaired/model-only claims, wrong call IDs and out-of-range content; an SSE/UTF-8 control checks fixture transport.

Build preparation: the initial offline reference build was blocked by missing `rayon`. A permitted `cargo fetch --locked` populated the Cargo cache; no system package or executable was installed and no Alpha source changed. A subsequent offline locked build uses a separate target directory and two jobs. Exact build/run outcome is recorded in the validation record for this branch.

These four contracts do not evaluate autonomous reasoning, security policy equivalence, broad tool coverage, agents, browser, model quality, phone memory/performance or task success. Startup-inclusive timing across different builds is not a speed ranking. No parity percentage or universal superiority score is computed. No push, merge or release is part of this work.

## Recorded result

Harness commit `7a4de29`: both actual pinned executables passed all four cases with three repetitions each (Blue 12/12, Alpha 12/12), exactly two fixture provider requests per sample. All 28 integrated Blue qualification groups also passed, including 24/24 existing runtime regression samples. Group log hashes, harness hash and both binary hashes were checked. Rust source is unchanged from independently reviewed `2181c7e`; the previous 278-test/Clippy result was not rerun or presented as a new run here.

Blue binary SHA256: `ab3de1d8436189f4a6aa3217b85839e037fb4c6a5c59caf8551e5a9228ea47aa` (18,304,784 bytes). Alpha binary SHA256: `7bdc7d6268e5b21f5480eb7b72eba5e1da5ffd3f1549d148bf6de4f16278fbca` (306,480,528 bytes). Both are retained in `/workspace/scratch/58a10643c600/candidate-builds`. [Structured results and build provenance](../validation/alpha-common-read-readiness.json).

The no-default-features attempt failed because it omitted the reference's default serde feature. The default build then ended with rustc SIGKILL; its exact cause is unproven. A lower-memory build with default features, one job, incremental disabled, and Alpha-package opt-level 0/debug 0/codegen-units 256 successfully translated the code. Linking initially lacked `libxkbcommon.so`; the existing system `libxkbcommon.so.0` was made available under that name through a symlink in an isolated build directory, supplied via `LIBRARY_PATH`. No library was downloaded for this step and no system file or Alpha source was modified. The final offline locked build succeeded. This build profile is for functional comparison, not release or speed qualification.

Logs: `/tmp/blue-alpha-comparison-qualification`, `/tmp/blue-alpha-common-read-validated.json`, and the four build logs whose hashes appear in the structured record. The new harness remains available for independent review. These results close the previous executable/adapter blocker only for these four read contracts; the comprehensive development and wider comparison remain open.

## Oracle correction after independent review

Review of `7a4de29` found two benchmark false positives: fixture text embedded in a denied/failed result could pass a positive read case, and any Blue `ok:false` could pass the missing-file case. Both were reproduced against the frozen harness with mocked CLI transport. They are oracle defects, not demonstrated runtime product defects. Earlier runs must be interpreted with this weakness; they do not establish the stronger corrected contract.

Fix `266237b` checks denied/unknown/stale markers before content, rejects failed results for successful reads, and requires a specific missing-file error instead of permission failure or arbitrary `ok:false`. An explicit successful result cannot establish a missing-file error. Error-prefixed plain results also fail positive reads. The checks examine the native result envelope, not JSON text inside the file content.

Eight harness tests passed, including the 13-flag matrix across both adapters and all three positive read cases, missing-error controls and transport-level rejection. Both actual pinned executables passed the corrected four contracts three times each (12/12 each). [Corrected evidence](../validation/alpha-read-oracle-correction.json). This validation reran the affected harness and real CLI fixtures; it does not claim a fresh full qualification/Rust run. Source runtime and all prior build limitations remain unchanged. Independent review of this correction is pending; no remote publication occurred.
