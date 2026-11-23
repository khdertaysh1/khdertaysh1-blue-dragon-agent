"""Manual offline tree comparison and declared-test receipt inspection; no execution."""
import difflib
import json
from pathlib import Path
import re
import local_review_batch as local

MAX_TESTS = 1000
MAX_RECEIPTS = 32
DIFF_CHARS = 16384


def tree_compare(left, right, files):
    left = local.selection(left, files)
    right = local.selection(right, files)
    rows = []
    for raw in files:
        row = {'path': raw, 'status': 'unreadable', 'diff_complete': False}
        sides = []
        for label, root in [('left', left), ('right', right)]:
            try:
                data, text = local.read(root, raw)
                row[label] = {'exists': True, 'sha256': local.sha(data), 'bytes': len(data)}
                sides.append(text)
            except FileNotFoundError:
                row[label] = {'exists': False}
                sides.append(None)
            except (OSError, ValueError) as error:
                row[label] = {'exists': None, 'diagnostic': str(error)}
                sides.append(None)
        if any(row[label]['exists'] is None for label in ['left', 'right']):
            rows.append(row)
            continue
        a, b = sides
        if a is None and b is None:
            row['diagnostic'] = 'Selected path absent in both trees; no comparison evidence'
        else:
            row['status'] = 'added' if a is None else 'removed' if b is None else 'unchanged' if a == b else 'modified'
            old, new = (a or '').splitlines(True), (b or '').splitlines(True)
            if row['status'] == 'unchanged':
                row.update({'diff': '', 'diff_complete': True})
            elif max(len(old), len(new)) > 2000 or any(len(line) > 4096 for line in old + new):
                row['diagnostic'] = 'Text diff omitted: line/line-length budget exceeded; hashes remain valid'
            else:
                output = ''.join(difflib.unified_diff(old, new, fromfile='left/' + raw if a is not None else '/dev/null',
                                                     tofile='right/' + raw if b is not None else '/dev/null'))
                row.update({'diff': output[:DIFF_CHARS], 'diff_complete': len(output) <= DIFF_CHARS,
                            'left_final_newline': a.endswith('\n') if a is not None else None,
                            'right_final_newline': b.endswith('\n') if b is not None else None})
        rows.append(row)
    complete = all(r['status'] != 'unreadable' and r['diff_complete'] for r in rows)
    return {'schema_version': 1, 'ok': complete, 'coverage_complete': complete,
            'scope': 'explicit paths only, not a complete tree inventory', 'files': rows,
            'coverage': {'selected': len(files), 'readable_comparisons': sum(r['status'] != 'unreadable' for r in rows)},
            'limits': {'files': local.MAX_FILES, 'bytes_per_file': local.CAP, 'diff_chars_per_file': DIFF_CHARS}}


def load_json(root, raw):
    data, text = local.read(root, raw)
    def unique(pairs):
        result = {}
        for k, v in pairs:
            if k in result:
                raise ValueError('Duplicate JSON key')
            result[k] = v
        return result
    def reject(value):
        raise ValueError('Invalid JSON constant')
    return json.loads(text, object_pairs_hook=unique, parse_constant=reject), local.sha(data)


def name_ok(name):
    return isinstance(name, str) and 0 < len(name) <= 200 and not any(ord(c) < 32 for c in name)


def revision_ok(value):
    return isinstance(value, str) and re.fullmatch(r'[A-Za-z0-9._-]{1,128}', value) is not None


def receipt_review(root, declarations, receipts):
    root = local.selection(root, receipts)
    declaration, declaration_hash = load_json(root, declarations)
    if not isinstance(declaration, dict) or set(declaration) != {'revision', 'tests'}:
        raise ValueError('Declarations require exactly revision and tests')
    expected, names = declaration['revision'], declaration['tests']
    if (not revision_ok(expected) or not isinstance(names, list) or not 1 <= len(names) <= MAX_TESTS
            or not all(name_ok(n) for n in names) or len(set(names)) != len(names)):
        raise ValueError('Invalid revision or 1..1000 distinct declared test names')
    observations = {name: [] for name in names}
    receipt_rows, undeclared = [], []
    for raw in receipts:
        row = {'path': raw, 'state': 'malformed'}
        try:
            document, fingerprint = load_json(root, raw)
            row['sha256'] = fingerprint
            if not isinstance(document, dict) or set(document) != {'revision', 'results'}:
                raise ValueError('Receipt requires exactly revision and results')
            results = document['results']
            if not revision_ok(document['revision']) or not isinstance(results, list) or len(results) > MAX_TESTS:
                raise ValueError('Invalid revision or results count')
            seen = set()
            for item in results:
                if (not isinstance(item, dict) or set(item) != {'name', 'status'} or not name_ok(item['name'])
                        or item['status'] not in ('passed', 'failed', 'skipped') or item['name'] in seen):
                    raise ValueError('Invalid/duplicate test result; entire receipt rejected')
                seen.add(item['name'])
            fresh = document['revision'] == expected
            row.update({'state': 'matching_revision' if fresh else 'stale', 'revision': document['revision'],
                        'result_count': len(results)})
            for item in results:
                evidence = {'receipt': raw, 'status': item['status'], 'revision_matches': fresh}
                if item['name'] not in observations:
                    undeclared.append({'name': item['name'], 'state': 'absent_declaration', **evidence})
                else:
                    observations[item['name']].append(evidence)
        except (OSError, ValueError, RecursionError) as error:
            row['diagnostic'] = str(error)
        receipt_rows.append(row)
    malformed = any(r['state'] == 'malformed' for r in receipt_rows)
    rows = []
    for name in names:
        evidence = observations[name]
        current = [e for e in evidence if e['revision_matches']]
        if not evidence:
            status = 'missing_execution_evidence'
        elif not current:
            status = 'stale'
        elif len(current) != 1 or malformed:
            status = 'uncertain'
        else:
            status = current[0]['status']
        rows.append({'name': name, 'state': status, 'evidence': evidence})
    complete = not malformed and not undeclared and all(r['state'] in ('passed', 'failed', 'skipped') for r in rows)
    return {'schema_version': 1, 'ok': complete, 'coverage_complete': complete,
            'declarations': {'path': declarations, 'sha256': declaration_hash, 'revision': expected},
            'tests': rows, 'undeclared_results': undeclared, 'receipts': receipt_rows,
            'all_declared_reported_passed': complete and all(r['state'] == 'passed' for r in rows),
            'execution_verified': False,
            'trust': 'Local user-supplied receipt assertions only; revision equality is not execution or authenticity proof'}
