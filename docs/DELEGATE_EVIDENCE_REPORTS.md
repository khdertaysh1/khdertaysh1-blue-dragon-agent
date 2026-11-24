# Revalidated delegate evidence in existing reports

`/audit-report`, `/report` and the `task_continuity` tool now include a `delegate_evidence` section. This is a product integration, not another tool or comparison-only fixture. Existing supervisor collection remains available; reports now independently revalidate its references against current child checkpoint files instead of exporting remembered captures as trusted evidence.

Each known child's saved session must match the child identity and workspace. Both request and result pins must exist, agree with the supervisor's retained references, and pass the existing audit request/result pairing check. Changed, unpinned, missing, denied, failed, unknown and stale results are excluded and counted. Duplicate evidence IDs fail pairing. After restart, incomplete team recovery is explicitly flagged; a failed directory scan is not presented as a complete empty team.

The new section contains child session/evidence identifiers, tool names, request/result hashes, historical integrity labels and valid native source-capture metadata. It reuses the existing source-version reconciliation without selecting a winning source or inferring semantic agreement. It does not include child summaries, tasks, mail, tool bodies or argument contents. This omission applies to the new section; the rest of the existing parent report retains its existing behavior. Workspace-relative source paths and hashes are still metadata, not automatic anonymization.

Reads are bounded to eight known checkpoints of at most 256 KiB each, with a streaming size cap, and at most 64 evidence references across the report. Omitted references are counted. The smaller loader reuses existing session identity/workspace validation; normal sessions retain their prior 16 MiB limit. A missing checkpoint for a known agent appears unreadable, and its retained references fail revalidation. Active checkpoints can lag a tool's in-memory completion, so a temporarily unavailable reference remains unavailable rather than triggering a replay.

Requesting a report does not invoke a model, consume delegate request/tool quotas, execute a tool, resume a child, read current project source files or reconnect a browser. The report is generated on request and is not a promise that an earlier stored report remains valid forever. Validated pins establish a historical checkpoint pair only; they do not validate a model conclusion, prove current file state, authenticate the whole store or defeat external filesystem races.

## Tests and integration

Product tests cover same-manager revalidation after tampering, missing pins, repinning a modified checkpoint, duplicate IDs, workspace mismatch, oversized/deleted checkpoints, correct restoration, raw-content/summary omission, unchanged request budgets, the 64-reference cap, incomplete directory recovery, and nine denied/unknown/stale cases with valid hashes.

The actual CLI consistency fixture now calls `/audit-report` and `/report` before tampering, after tampering and after restoration. It checks expected evidence counts and verifies that neither parent nor child provider requests occur. The existing process-death recovery tests remain part of integrated qualification; no physical Termux process-death test is claimed.

Alpha source review at `773e3ba2f3e30e47d09cd5b751d224a1592dc804` inspected report retention in `alphacode_app_core/server/swarm.rs` and finding/report gating in `alphacode_security_core/research_session.rs`. Blue already had bounded independent child sessions and source reconciliation; the identified gap was their connection to normal reports. This implementation reuses those capabilities without copying Alpha's retention policy or claiming equivalent research/report semantics. The earlier shared edit/read comparisons do not measure this new report behavior against Alpha.

No new child permissions, write access, browser sharing, approval bypass or automatic unknown-operation retry is introduced. Independent review, platform/browser CI, physical phone and live-provider checks remain outstanding for this extension.

## Local validation record

Tested source `63dce90` on `improve/delegate-evidence-report`: 282 Rust tests, strict Clippy, offline locked release build and all 30 integrated qualification groups passed. The existing runtime regression passed 24/24 samples. Every qualification log hash and the pinned binary hash were rechecked. [Structured validation](https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/blob/2b7dbc840c203c0b8093611eb281d8b09b037815/validation/delegate-report-readiness.json). Logs: `/tmp/blue-delegate-report-qualification`.

Binary `blue-dragon-0.9.0-delegate-report-linux-x64`: 18,347,408 bytes, SHA256 `c90e52a99926e7dec7789cee7cf851e53096357e34ead676f3e6e201b48a26f3`; retained under `/workspace/scratch/58a10643c600/candidate-builds`. The preceding edit workflow comparison passed independent review within its stated scripted scope; this product report extension still needs independent review. No remote publication occurred.
