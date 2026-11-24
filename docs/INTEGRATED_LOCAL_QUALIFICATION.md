# Qualification of the combined local change chain

The recent development branches are a linear ancestor chain through `bace56b`, not independent patches requiring a merge. An isolated worktree based on that commit qualified one unchanged release binary against all selected workflows together. No remote branch, pull request, merge or release was changed.

## Reproduce

Build the candidate from the intended checkout, then provide an already reviewed, unchanged local Between Bytes checkout at `c396c90f2e7ed080f71368ce9edb5cd630098b09`:

```sh
python3 scripts/qualify_local.py --binary /absolute/path/to/blue-dragon --lab /absolute/path/to/lab --output /absolute/path/to/new-results
```

The harness does not clone, download, install a browser or contact live providers. It runs selected reviewed scripts sequentially, with isolated HOME/config/data roots and an allowlisted environment. Existing result directories are refused. Each group has a 120-second default timeout (configurable 15–300), a local log, harness/log hashes, elapsed time and explicit status. The summary is saved between groups; failure in one group does not prevent later groups from running. Required unavailable runtimes/platform groups remain `not_run`, making the overall result incomplete. Binary SHA256 is checked before and after the matrix. Source commit and dirty state are recorded so uncommitted harness changes are not hidden.

On timeout POSIX process groups are terminated; Windows only terminates the immediate process, so Windows process-tree cleanup is not qualified. Neither environment filtering nor Python socket guards provide OS network isolation. Inputs are trusted local test scripts and the pinned reviewed lab. Logs are local fixture diagnostics; review before sharing.

## Observed results

All 21 selected groups passed on Linux, with 61.620 seconds summed group wall time in this run (not a performance guarantee). They cover continuity and interrupted tasks; coverage/evidence integrity; cached source versions, chain/flow/call mapping, discovery and dynamic references; selective review engines; actual lab comparison/report workflows; offline model configuration; TUI and active-job exit; PTY backpressure; mocked installer/source-upgrade/release-payload integrity; and shared browser form/network producer contracts. The latter run JavaScript with mocked browser APIs, not a graphical browser.

The deterministic runtime benchmark passed eight cases with three repetitions each (24/24). It covers atomic edits/rejection, denied writes, exit behavior, cached symbols, interrupted resume, archived arguments and session identity. Its local scripted-model timings do not measure provider latency, model quality or real-site speed. Alpha was explicitly not run because no validated executable/adapter was supplied; there is no parity score or speed comparison.

The full source checkout also passed 233 Rust tests, clippy with warnings denied and formatting. The release binary remained unchanged (`detailed SHA256 in validation/integrated-local-qualification.json`). The selected smoke scripts now honor `BLUE_DRAGON_BINARY`, avoiding accidental use of another checkout's hardcoded binary.

Evidence: `validation/integrated-local-qualification.json` records every group; `validation/integrated-runtime.json` records all runtime samples. Full logs remain in the recorded local log directory. Tests are heterogeneous; group counts are not security-coverage percentages. Previous per-feature reports preserve their own exact request counts, independent oracles and sensitivity controls.

## Remaining verification boundaries

No graphical browser was started: the known environment socket restriction was not bypassed. No physical Termux device, Windows/macOS runtime or live text/vision provider was tested. Installer/package behavior uses local command fixtures. Browser contracts do not prove real frame/navigation/rendering behavior. Local test passage does not establish vulnerability absence, exploitability, production safety, complete semantic dataflow or Alpha equivalence.

Each checkpoint also writes `junit.xml` alongside `summary.json`. It contains one test case per planned group. Failed exits, timeouts and skipped or unfinished groups have distinct outcomes. The JSON report retains log hashes and binary provenance.
