# Manual local workspace review

Base: official Blue Dragon **v1.1.13.0**, commit
`9d02f9ab8d8bb9cc8a7cc6b6117b82c887e04e62`.

`scripts/local_workspace.py` is an independent, manually invoked Python standard-library
utility. It is not registered with the agent, imported by Rust, advertised to a provider,
or integrated into slash commands. It does not contact providers, MCP, browsers, or the
network. It has no general command execution or file-application endpoint.
No existing source, Arabic presentation, workflow, dependency, version, or packaging file
is changed by this contribution. Python 3.9+ and Git (only for Git review) are required.

## Manual interface

Supply `--root PATH` explicitly. Subcommands:

| Command | Inputs | Result |
|---|---|---|
| `files` | none | Sorted relative paths, sizes, exclusions, traversal limit |
| `read` | relative path, `--offset`, `--bytes` | UTF-8 byte window, next offset, window hash, file size |
| `search` | pattern, optional `--regex` | Literal/regex line matches, skipped files, byte/file coverage |
| `git` | optional `--staged`, `--path` | Staged or unstaged diff plus porcelain status (NUL-separated in JSON) |
| `preview` | workspace-relative JSON manifest | Multi-file create/update/delete preview, conflicts, content hashes |
| `sessions` | workspace-relative directory, query, required `--role user\|assistant\|tool` | Explicit source file and turn; one role at a time |
| `test-report` | workspace-relative UTF-8 JUnit XML | Counts of supplied testcase records; freshness unverified |

Example with a synthetic local workspace:

```sh
python scripts/local_workspace.py --root /path/to/local-workspace files
python scripts/local_workspace.py --root /path/to/local-workspace read src/example.txt --offset 0 --bytes 16384
python scripts/local_workspace.py --root /path/to/local-workspace search 'TODO|FIXME' --regex
python scripts/local_workspace.py --root /path/to/local-workspace git --staged --path src/example.txt
```

Successful commands and dispatch errors emit one JSON document with `schema_version: 1`
and `ok`; exit status is 0 or 1. Argument-usage errors use argparse's normal stderr and
exit 2. JSON escapes control characters and Unicode; this does not alter the existing
Arabic rendering and decoded text remains identical. There is no terminal-control passthrough.

## Multi-file manifest

```json
{"changes":[{"path":"example.txt","expected_sha256":null,"content":"new file\n"}]}
```

For existing files, supply their complete SHA-256. `null` as the expected hash means
create only if absent; `content: null` means preview deletion. Unknown fields, duplicate
paths, path escapes, links, missing parents, and stale hashes are rejected. Changes
are capped at 32, 1 MiB per file and 8 MiB combined old/new text. Diff text is capped
at 65536 characters per change, with explicit truncation and newline flags.
`approval_digest` binds the manifest only: **it is not authorization and there is no
apply command**. A successful preview is not a reservation; concurrent edits can
invalidate it. Use existing reviewed editing workflows separately if needed.

## Limits and trust boundary

- Read windows: 4..65536 bytes, valid UTF-8 boundaries, no whole-file size limit.
  Hashes cover the returned window, not the whole file. Metadata changes during a read
  are rejected; callers should not mutate the source while paging it.
- Search: at most 200 inventoried files, 1 MiB per file, 8 MiB total, 200 matches,
  and 500 characters per excerpt. Regex operates per line in a disposable child process
  with a three-second deadline. No match after timeout is presented as complete.
- Inventory omits named build/dependency directories and symlinks; `complete` describes
  bounded traversal, and `skipped` remains significant. Search marks exclusions as
  incomplete. Directory walking is capped at 200 visited directories; directory listing
  itself depends on the filesystem and is not an OS resource sandbox.
- Sessions: explicit directory only; up to 16 MB per snapshot and 32 MB total. Only
  string contents of the selected role are searched. Structured content blocks and
  other products' session formats are not imported. Corrupt input is reported as skipped.
- Git: only fixed status/diff commands, no shell, prompts, pager, textconv or external diff.
  Filters/includes and all config outside a narrow allowlist are refused before spawning.
  Worktree-pointer/shared-storage configurations are intentionally unsupported. Output
  is capped at 1 MiB per command and execution at five seconds. No fetched objects,
  commits, hooks, or index writes are requested. Git can create transient internal OS
  resources; the helper's contract is no intentional workspace mutation.
- Filesystem protections reject symlinks, hardlinks, traversal and nonregular files.
  These are application checks, **not a hardened sandbox against an adversary concurrently
  replacing parent directories or Git metadata**. Use an owned, quiescent local workspace.
- JUnit reports are inputs, not fresh execution evidence. No project tests are executed.
  UTF-8 only; DTD/entities refused. Counts reflect testcase records, not suite attributes.
- Runtime portability, phone keyboard/bidi, performance and crash durability of the
  main application were not tested by this work.

## Verification

`PYTHONDONTWRITEBYTECODE=1 python scripts/test_local_workspace.py` runs synthetic-only
unit/integration tests. It creates disposable files and tiny local Git repositories.
No full Blue build, release bundle, browser, external endpoint or live model is involved.
The test suite blocks sockets for manual read/list operations; it does not claim a
simulated-provider completion test because this interface has no provider path.

GitHub publication is withheld: the existing `pull_request` workflow builds/packages
and uploads complete executable artifacts even for draft PRs. The workflow is unchanged.
