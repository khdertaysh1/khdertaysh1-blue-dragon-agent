# CDP capture limits and extension connection epochs

Local continuation from fda7dcd. No upload, merge or release.

## Corrected producer completeness

Independent review identified a missing limit: CDP evicted events after 300
without an eviction counter, while policy review interpreted absent counters as
not partial. The extension had counters, so its tests did not cover this producer.

The CDP network producer now records ring evictions and exports a recent bounded
window within 180000 serialized bytes. Output includes entries_omitted,
ring_entries_omitted and the dedicated-tab/connection scope of that count. The
policy engine now returns upstream_capture_partial null and completeness_known
false when legacy omission metadata is absent. A known positive omission means
partial; both zero counters are needed for known non-partial upstream capture.
The engine's own scan bounds remain separately reported. Neither means exhaustive
browser traffic or security coverage.

A Rust regression feeds 301 Network.responseReceived events into the same
record_network producer used by Cdp::record, takes the production network_snapshot,
and passes its paired result to response_policy_review. It verifies one eviction,
partial upstream capture and no vulnerability confirmation. A large-URL variant
also verifies bounded export with visible omissions. This uses actual producer
functions but synthetic CDP event payloads, not a live Chromium transport.

## Connection-bound dispatch

The authenticated extension bridge increments a runtime connection_epoch on each
accepted connection. Browser actions capture that epoch before scope/approval
checks. If it changes before dispatch, the action is not executed. The bridge
checks the expected epoch and selects the command sender under the same mutex,
so a reconnect cannot redirect an old action to the replacement connection.
A sender already selected belongs to the old connection; disconnect leaves an
unknown result rather than replaying the action.

After an action and post-action scope reads, a changed epoch causes the result
to be withheld as unknown. Returned native evidence carries the dispatch epoch,
explicitly scoped to the current bridge instance only. Epochs are not persistent
session IDs, browser identities, page equality or authentication proof. They
cannot be compared across process/bridge restarts. No previous page, login or
pending action is restored automatically.

The real local authenticated WebSocket fixture rejects a stale-epoch click before
queueing; its peer receives only the intended snapshot. The disconnect/reconnect
fixture observes epoch 1 then 2 and confirms that the interrupted click is never
replayed. These fixtures emulate the extension peer and are not a graphical
browser test. CDP connection behavior remains separate.

## Validation

199 Rust tests passed, Clippy all targets with warnings denied, release build and
format/diff checks passed. Existing JS policy/form/collector contracts passed.
All 14 local CLI scenarios (coverage, 12 engine cases and continuity) passed on
the final binary; recorded results and hash are in
validation/browser-continuity-state-smoke.jsonl.

No physical Termux, native Windows/macOS, graphical browser or live model/vision
provider was tested. Syntactic dataflow, account/session stability and the broader
six work areas remain incomplete. This batch does not claim Alpha parity or
vulnerability proof.
