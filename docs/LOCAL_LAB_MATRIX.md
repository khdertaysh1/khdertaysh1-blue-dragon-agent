# Opt-in local web lab matrix

This matrix exercises a separate checkout of `khdertaysh1/Between-Bytes-Web-Labs` pinned to `c396c90f2e7ed080f71368ce9edb5cd630098b09`. It does not clone, edit, push, or publish that repository. Run only against this reviewed synthetic lab revision. The tracked checkout must be clean.

```sh
python3 scripts/lab_matrix.py /absolute/path/to/blue-dragon --lab /absolute/path/to/Between-Bytes-Web-Labs
```

The application server and its sidecar bind to ephemeral loopback ports. The Python harness denies non-loopback socket binds/connections, disables HTTP proxy use and redirect following, bounds response size and request counts, and shuts down the servers. Blue Dragon runs in temporary state/workspace directories with a scripted loopback model, exact preview-path read scope, explicit approval input, and a 30-second process deadline. Its three GET observations per marker case and one GET per policy case are independently paired with native evidence IDs. Synthetic cookie/token values stay in memory or temporary sessions; the saved summary contains no raw credentials or response bodies. Response hashes use canonical JSON serialization, not wire bytes.

The Python audit hook is not an operating-system network sandbox and does not govern the Rust subprocess. Only reviewed loopback URLs are supplied to that subprocess. External-request count is an observation of this harness, not packet-capture proof. This is not a template for testing arbitrary sites.

## Cases

Three marker comparisons and three response-header reviews use 15 loopback GETs. The harness compares the native evidence with the fixtures and rejects unusable observations.
