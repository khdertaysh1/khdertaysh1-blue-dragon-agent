"""Offline, human-invoked toolkit. Never connected to the Blue Dragon agent."""
import argparse
import ast
import hashlib
import json
import math
from pathlib import Path
import sys
import tempfile

import local_python_navigation as navigation
import local_review_batch as review
import local_toolkit_evidence as evidence
import local_transcript_diagnostics as diagnostics

MAX_EDGES = 1000
MAX_DIFFS = 500
MAX_NODES = 20000


def import_map(root, files):
    root = review.selection(root, files)
    modules, reports, parsed = {}, [], []
    total = 0
    for raw in files:
        row = {'path': raw, 'status': 'error'}
        try:
            data = navigation.read_source(root, raw, min(navigation.FILE_BYTES, navigation.TOTAL_BYTES - total))
            total += len(data)
            row['sha256'] = hashlib.sha256(data).hexdigest()
            tree = ast.parse(data, filename=raw)
            parts = raw[:-3].split('/')
            package = parts[-1] == '__init__'
            if package:
                parts.pop()
            if not parts or not all(part.isidentifier() for part in parts):
                raise ValueError('Use an explicit import root and identifier-named modules')
            module = '.'.join(parts)
            modules.setdefault(module, []).append(raw)
            row.update({'status': 'parsed', 'module': module})
            parsed.append((raw, parts if package else parts[:-1], tree))
        except (OSError, ValueError, SyntaxError, RecursionError) as error:
            row['diagnostic'] = str(error)
        reports.append(row)
    edges, omitted = [], 0
    for raw, package, tree in parsed:
        for node in ast.walk(tree):
            requests = []
            if isinstance(node, ast.Import):
                requests = [(a.name, None) for a in node.names]
            elif isinstance(node, ast.ImportFrom):
                if node.level and node.level > len(package):
                    requests = [(None, 'relative import beyond supplied package root')]
                else:
                    prefix = package[:len(package) - node.level + 1] if node.level else []
                    target = '.'.join(prefix + ([node.module] if node.module else []))
                    # from . import x: x may be a submodule or a symbol, never assume.
                    requests = [(target, 'from-import symbols are not resolved')]
                    if not node.module:
                        requests += [('.'.join(filter(None, (target, a.name))), 'submodule candidate only')
                                     for a in node.names if a.name != '*']
            for target, note in requests:
                if len(edges) == MAX_EDGES:
                    omitted += 1
                    continue
                candidates = sorted(modules.get(target, []))
                status = 'candidate' if len(candidates) == 1 else 'ambiguous' if candidates else 'unresolved'
                edges.append({'source': raw, 'line': node.lineno, 'module': target,
                              'status': status, 'candidates': candidates, 'note': note,
                              'runtime_resolution_verified': False})
    edges.sort(key=lambda e: (e['source'], e['line'], e['module'] or ''))
    return {'schema_version': 1, 'ok': all(r['status'] == 'parsed' for r in reports) and not omitted,
            'scope': 'explicit import-root-relative files; syntactic candidates only',
            'files': reports, 'imports': edges,
            'ambiguous_modules': {k: sorted(v) for k, v in sorted(modules.items()) if len(v) > 1},
            'coverage': {'selected': len(files), 'parsed': len(parsed), 'bytes_read': total,
                         'edges_returned': len(edges), 'edges_omitted': omitted},
            'unresolved_is_not_missing_dependency': True}


def strict_json(text):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError('Duplicate JSON key')
            result[key] = value
        return result
    def invalid(value):
        raise ValueError('Non-finite JSON number: ' + value)
    result = json.loads(text, object_pairs_hook=pairs, parse_constant=invalid)
    # Validate depth/node count before recursive equality or canonical encoding.
    stack, count = [(result, 0)], 0
    while stack:
        value, depth = stack.pop()
        count += 1
        if isinstance(value, float) and not math.isfinite(value):
            raise ValueError('Non-finite JSON number')
        if depth > 64 or count > MAX_NODES:
            raise ValueError('JSON depth/node budget exceeded')
        if isinstance(value, dict):
            stack.extend((v, depth + 1) for v in value.values())
        elif isinstance(value, list):
            stack.extend((v, depth + 1) for v in value)
    return result


def json_compare(root, before, after):
    root = review.selection(root, [before, after])
    a_bytes, a_text = review.read(root, before)
    b_bytes, b_text = review.read(root, after)
    a, b = strict_json(a_text), strict_json(b_text)
    missing = object()
    def kind(v):
        return 'absent' if v is missing else type(v).__name__
    def fingerprint(v):
        return None if v is missing else review.sha(json.dumps(v, sort_keys=True, ensure_ascii=True, allow_nan=False).encode())
    def escape(key):
        return str(key).replace('~', '~0').replace('/', '~1')
    stack, changes, omitted = [('', a, b)], [], 0
    while stack:
        path, old, new = stack.pop()
        if type(old) is dict and type(new) is dict:
            for key in sorted(set(old) | set(new), reverse=True):
                stack.append((path + '/' + escape(key), old.get(key, missing), new.get(key, missing)))
        elif type(old) is list and type(new) is list:
            for i in reversed(range(max(len(old), len(new)))):
                stack.append((path + '/' + str(i), old[i] if i < len(old) else missing,
                              new[i] if i < len(new) else missing))
        elif type(old) is not type(new) or old != new:
            if len(changes) >= MAX_DIFFS:
                omitted += 1
                continue
            changes.append({'pointer': path, 'change': 'added' if old is missing else 'removed' if new is missing else 'changed',
                            'before_type': kind(old), 'after_type': kind(new),
                            'before_sha256': fingerprint(old), 'after_sha256': fingerprint(new)})
    return {'schema_version': 1, 'ok': not omitted, 'equal': not changes and not omitted,
            'coverage_complete': not omitted, 'changes': changes, 'changes_omitted': omitted,
            'before': {'path': before, 'sha256': review.sha(a_bytes)},
            'after': {'path': after, 'sha256': review.sha(b_bytes)},
            'scope': 'JSON structure and values; raw scalar values omitted, not semantic equivalence'}


def self_test():
    # Fixed benign fixtures; no production/user files are written or executed.
    with tempfile.TemporaryDirectory(prefix='manual-toolkit-selftest-') as directory:
        root = Path(directory)
        fixtures = {'a.py': 'import b\ndef hello():\n    return 1\n', 'b.py': 'VALUE = 1\n',
                    'README.md': '# Guide\n## Usage\n', 'old.json': '{"version":1}',
                    'new.json': '{"version":2}',
                    'change.diff': '--- a/b.py\n+++ b/b.py\n@@ -1 +1 @@\n-VALUE = 1\n+VALUE = 2\n'}
        for name, text in fixtures.items():
            (root / name).write_text(text)
        checks = {
            'python_outline': navigation.navigate(root, ['a.py'])['ok'],
            'patch_dry_run': review.patch_review(root, 'change.diff', ['b.py'])['ok'],
            'markdown_headings': len(review.doc_index(root, ['README.md'])['files'][0]['headings']) == 2,
            'import_candidates': import_map(root, ['a.py', 'b.py'])['imports'][0]['candidates'] == ['b.py'],
            'json_difference': json_compare(root, 'old.json', 'new.json')['changes'][0]['pointer'] == '/version',
            'inputs_unchanged': all((root / name).read_text() == text for name, text in fixtures.items())}
    return {'schema_version': 1, 'ok': all(checks.values()), 'checks': checks, 'fixtures': 'temporary synthetic only'}


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    description = '''Manual offline developer toolkit (no agent integration).
Commands:
  python     existing Python AST navigation (pass --root and named .py files)
  review     existing patch-check/docs (pass --root, then subcommand and files)
  imports    cross-file Python module candidates on explicit files
  json-diff  bounded structural JSON comparison; never executes source
  trees      compare named local files in two explicitly supplied trees
  receipts   inspect declared tests and explicit local execution receipts
  diagnose   inspect explicit sanitized structured historical receipts
  self-test  deterministic local walkthrough on tiny temporary fixtures
Use COMMAND --help for details. No command applies edits or follows imports/links.'''
    if not argv or argv[0] in ('-h', '--help'):
        print(description)
        return 0
    command, rest = argv[0], argv[1:]
    if command == 'python':
        return navigation.main(rest)
    if command == 'review':
        return review.main(rest)
    parser = argparse.ArgumentParser(prog='local_developer_toolkit.py ' + command)
    if command in ('imports', 'json-diff', 'diagnose'):
        parser.add_argument('--root', required=True)
        parser.add_argument('files', nargs='+' if command in ('imports', 'diagnose') else 2)
    elif command == 'trees':
        parser.add_argument('--left', required=True)
        parser.add_argument('--right', required=True)
        parser.add_argument('files', nargs='+')
    elif command == 'receipts':
        parser.add_argument('--root', required=True)
        parser.add_argument('--declarations', required=True)
        parser.add_argument('receipts', nargs='+')
    elif command != 'self-test':
        parser.error('Unknown command; use --help')
    args = parser.parse_args(rest)
    try:
        if command == 'diagnose':
            result = diagnostics.diagnose(args.root, args.files)
        elif command == 'trees':
            result = evidence.tree_compare(args.left, args.right, args.files)
        elif command == 'receipts':
            result = evidence.receipt_review(args.root, args.declarations, args.receipts)
        else:
            result = (import_map(args.root, args.files) if command == 'imports' else
                  json_compare(args.root, *args.files) if command == 'json-diff' else self_test())
    except (OSError, ValueError, SyntaxError, RecursionError) as error:
        print(json.dumps({'schema_version': 1, 'ok': False, 'error': 'Diagnostic input selection invalid' if command == 'diagnose' else str(error)}, ensure_ascii=True))
        return 2
    print(json.dumps(result, sort_keys=True, ensure_ascii=True, indent=2))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
