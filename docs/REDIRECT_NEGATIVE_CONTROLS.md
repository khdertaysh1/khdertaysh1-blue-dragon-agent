# Controlled redirect negative observations

Previously, `redirect_controlled_comparison` accepted only three redirect responses, so a baseline redirect, an HTTP 403 variant, and a matching repeated baseline could not be described by this engine. It now preserves that negative HTTP observation without converting it into a security conclusion.

Normal and repeated controls must still have observed redirect status and usable native Location metadata. All three evidence references must be distinct, chronological, paired to native `web_fetch` calls, in current read scope, nonhistorical and integrity-checked. Baseline and repeat normalized arguments must match, with one normalized argument field changed for the variant. Denied, unknown and explicitly unexecuted actions are rejected even if a numeric status is present. The generic comparison policy is unchanged.

| Observed variant | Stable controls | Result |
| --- | --- | --- |
| Redirect | Yes | Existing metadata-only comparison |
| HTTP 400, 403 or 422 | Yes | `variant_http_rejection_observed` |
| Other non-redirect HTTP status, including 200, 401, 429 or 503 | Yes | `inconclusive_variant_non_redirect_response` |
| Any admitted variant | No | `inconclusive_control_changed` |

A non-redirect variant has `destination_metadata_changed: null`, since there is no observed redirect destination to compare. `protection_verified`, `causality_proven` and `vulnerability_confirmed` remain false. HTTP rejection does not prove the intended policy caused it, rule out another redirect path, or establish protection. Login changes, rate limits and server failures must not be promoted into a negative security result. The stable-control comparison covers status and destination origin/path only, not cookies, account state, response bodies, query values, timing or server state.

## Reproduction and evidence

```sh
python3 scripts/smoke_review_engines.py /absolute/path/to/blue-dragon
```

The script runs a real Blue Dragon CLI with temporary state, scoped and approved native GET requests, native observation IDs/digests, and a scripted local model. It asserts against the synthetic HTTP server's chosen responses and request log. Three added cases exercise 302/403/302, 302/503/302, and 302/403/302 with a changed repeated destination. Each sends exactly three loopback GETs, performs no destination follow, and adds no requests during review. These are synthetic HTTP controls, not a browser or real-site exploit oracle. There is no live provider or external target.

Validation: 206 Rust tests; release build; clippy with warnings denied; 15 CLI review scenarios including all three added cases. Unit cases additionally cover HTTP 400/422/200/401/429 and denied/historical/unknown/unexecuted evidence. An initial test-fixture compilation error attempted to clone a Session; it was corrected by using the supported serialization round trip, and the complete suite then passed. Machine-readable CLI results are in `validation/redirect-negative-smoke.jsonl`.

This independent local continuation is based on `6700966`. The pinned Between Bytes matrix from that checkpoint remains separate; this batch adds three real-CLI cases to the standalone synthetic redirect fixture, not three additional Between Bytes cases. No browser execution, physical Termux device, Windows/macOS runtime, or live model provider was tested. No upload, merge or release is part of this batch.
