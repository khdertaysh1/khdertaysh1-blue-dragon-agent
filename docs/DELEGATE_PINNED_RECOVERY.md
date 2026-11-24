# Pinned delegate recovery correction

Source `2181c7e` on `fix/delegate-pinned-recovery` extends frozen `e5c341f` without changing that review branch. Review confirmed that optional legacy audit digests allowed an unpinned source result to become a trusted capture. This supersedes the earlier reconciliation readiness claim for that case.

Recovery now requires both recorded request and result digests before audit pairing. Missing either excludes the evidence and increments the rejected counter; legacy compatibility elsewhere remains intact. Source captures also reject denied, unknown, failed and every existing stale-result marker. A surviving single capture does not establish agreement; omitted evidence remains explicit.

The expanded real CLI fixture passed six combinations of missing request/result/both digests with untouched or forged matching source hashes, plus valid pinned restore, differing/same source controls and tampered pinned results. No child requests replayed. The old pinned binary failed the new regression. Rust tests cover denied/unknown and all seven stale markers; 278 tests and strict Clippy passed. All 26 integrated qualification groups passed, and their log hashes and binary hash were verified.

Pinned binary SHA256: `ab3de1d8436189f4a6aa3217b85839e037fb4c6a5c59caf8551e5a9228ea47aa`. [Structured validation](https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/blob/2b7dbc840c203c0b8093611eb281d8b09b037815/validation/delegate-pinned-recovery-readiness.json). Logs: `/tmp/blue-dragon-pinned-recovery-qualification`.

No remote publication occurred. Independent review remains pending. Whole-store forgery and concurrent filesystem races remain outside the guarantee. Historical captured bytes do not prove current state or semantic agreement. No real Chromium, phone, live provider or remote CI validation was performed. Alpha runtime comparison remains blocked locally: no installed executable, and offline locked build of the reference failed because dependency `rayon` was unavailable; this is not a comparative performance result.
