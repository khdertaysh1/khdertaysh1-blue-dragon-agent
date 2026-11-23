"""Human-invoked offline patch checking and Markdown heading navigation.

Read-only, explicit files only. No agent integration, execution, imports of target
code, Git invocation, network, providers, or background processes.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat

CAP = 262144
MAX_FILES = 32


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(root, raw):
    if (not raw or len(raw) > 512 or '\\' in raw or ':' in raw
            or any(ord(c) < 32 for c in raw)
            or any(p in ('', '.', '..', '.git') for p in raw.split('/'))):
        raise ValueError('Expected a plain relative path')
    path = root
    for part in raw.split('/'):
        path /= part
        if path.is_symlink():
            raise ValueError('Symlinks are unsupported')
    before = path.stat()
    identity = lambda s: (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
    if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or before.st_size > CAP:
        raise ValueError('Expected a regular non-hardlinked file of at most 256 KiB')
    with os.fdopen(os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0)), 'rb') as stream:
        if identity(os.fstat(stream.fileno())) != identity(before):
            raise ValueError('File changed before read')
        data = stream.read(CAP + 1)
        if len(data) > CAP or identity(os.fstat(stream.fileno())) != identity(before):
            raise ValueError('File changed during read')
    if identity(path.stat()) != identity(before):
        raise ValueError('File changed during read')
    text = data.decode('utf-8')
    if '\0' in text:
        raise ValueError('Binary input unsupported')
    return data, text


def selection(root, files):
    if not 1 <= len(files) <= MAX_FILES or len(files) != len(set(files)):
        raise ValueError('Select 1..32 distinct explicit files')
    root = Path(root).resolve(strict=True)
    if not root.is_dir():
        raise ValueError('Root must be a directory')
    return root


def parse_patch(text):
    """Strict unified text subset: same-path updates, LF, ordered exact hunks."""
    if '\r' in text or not text.endswith('\n'):
        raise ValueError('Patch must use LF and end in newline')
    lines = [line + '\n' for line in text.split('\n')[:-1]]
    files, i = [], 0
    while i < len(lines):
        if not lines[i].startswith('--- a/') or i + 1 >= len(lines) or not lines[i+1].startswith('+++ b/'):
            raise ValueError('Expected --- a/path and +++ b/path; extended Git/binary/rename patches unsupported')
        old, new = lines[i][6:-1], lines[i+1][6:-1]
        if old != new or not old or '\t' in old:
            raise ValueError('Only same-path updates without timestamps are supported')
        if any(f['path'] == old for f in files):
            raise ValueError('Duplicate file section')
        i += 2
        hunks = []
        while i < len(lines) and not lines[i].startswith('--- a/'):
            match = re.fullmatch(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@[^\n]*\n', lines[i])
            if not match:
                raise ValueError('Malformed or unsupported hunk header')
            a, ac, b, bc = (int(match[1]), int(match[2] or 1), int(match[3]), int(match[4] or 1))
            if (ac and not a) or (bc and not b) or max(a, ac, b, bc) > 1000000 or ac + bc == 0:
                raise ValueError('Invalid hunk range')
            i += 1
            before, after = [], []
            while len(before) < ac or len(after) < bc:
                if i >= len(lines) or lines[i][0] not in ' +-':
                    raise ValueError('Incomplete hunk body')
                line = lines[i]
                if line[0] in ' -':
                    before.append(line[1:])
                if line[0] in ' +':
                    after.append(line[1:])
                if len(before) > ac or len(after) > bc:
                    raise ValueError('Hunk body exceeds declared counts')
                i += 1
            hunks.append((a, ac, b, bc, before, after))
        if not hunks:
            raise ValueError('File section contains no hunks')
        files.append({'path': old, 'hunks': hunks})
        if len(files) > MAX_FILES:
            raise ValueError('Too many patch files')
    if not files:
        raise ValueError('Empty patch')
    return files


def patch_review(root, patch, files):
    root = selection(root, files)
    data, text = read(root, patch)
    sections = parse_patch(text)  # Entire patch validated before file review.
    if {f['path'] for f in sections} != set(files):
        raise ValueError('Selected files must exactly match all patch paths')
    results = []
    for section in sections:
        row = {'path': section['path'], 'applicable': False}
        try:
            original, content = read(root, section['path'])
            row['source_sha256'] = sha(original)
            if '\r' in content or (content and not content.endswith('\n')):
                raise ValueError('Source must use LF with a final newline (or be empty)')
            source = [line + '\n' for line in content.split('\n')[:-1]]
            result, cursor = [], 0
            for number, (a, ac, b, bc, before, after) in enumerate(section['hunks'], 1):
                start = a - 1 if ac else a
                new_start = b - 1 if bc else b
                if start < cursor or start > len(source):
                    raise ValueError(f'Hunk {number}: overlapping, unordered or out-of-range old position')
                result.extend(source[cursor:start])
                if new_start != len(result):
                    raise ValueError(f'Hunk {number}: inconsistent new position')
                if source[start:start+ac] != before:
                    raise ValueError(f'Hunk {number}: exact context/removal mismatch')
                result.extend(after)
                cursor = start + ac
            result.extend(source[cursor:])
            proposed = ''.join(result).encode('utf-8')
            if len(proposed) > CAP:
                raise ValueError('Proposed content exceeds 256 KiB')
            row.update({'applicable': True, 'hunks': len(section['hunks']),
                        'proposed_sha256': sha(proposed), 'proposed_bytes': len(proposed)})
        except (OSError, ValueError) as error:
            row['diagnostic'] = str(error)
        results.append(row)
    return {'schema_version': 1, 'ok': all(r['applicable'] for r in results),
            'patch_sha256': sha(data), 'applied': False, 'mode': 'exact-position-no-fuzz',
            'files': results, 'coverage': {'selected': len(files), 'reviewed': len(results)}}


def doc_index(root, files, query=None):
    root = selection(root, files)
    if query is not None and (not query or len(query) > 256):
        raise ValueError('Query must contain 1..256 characters')
    rows = []
    for raw in files:
        row = {'path': raw, 'status': 'error', 'headings': []}
        try:
            if Path(raw).suffix.lower() not in ('.md', '.markdown'):
                raise ValueError('Expected an explicit Markdown file')
            data, text = read(root, raw)
            row.update({'sha256': sha(data), 'status': 'indexed', 'coverage_complete': True})
            fence, hierarchy = None, []
            for number, line in enumerate(text.splitlines(), 1):
                opening = re.match(r'^ {0,3}(`{3,}|~{3,})(.*)$', line)
                if fence:
                    if re.fullmatch(r' {0,3}' + re.escape(fence[0]) + '{' + str(fence[1]) + r',}\s*', line):
                        fence = None
                    continue
                if opening:
                    fence = (opening[1][0], len(opening[1]))
                    continue
                heading = re.match(r'^ {0,3}(#{1,6})(?:[ \t]+(.*)|$)', line)
                if not heading:
                    continue
                level = len(heading[1])
                title = re.sub(r'[ \t]+#+[ \t]*$', '', heading[2] or '').strip()
                while hierarchy and hierarchy[-1]['level'] >= level:
                    hierarchy.pop()
                parents = [p['title'] for p in hierarchy]
                hierarchy.append({'level': level, 'title': title})
                if query is None or query.casefold() in title.casefold():
                    if len(row['headings']) >= 500:
                        row['coverage_complete'] = False
                        continue
                    row['headings'].append({'line': number, 'level': level, 'title': title, 'parents': parents})
            if fence:
                row['diagnostic'] = 'Unclosed fenced block: remaining lines treated as code'
                row['coverage_complete'] = False
        except (OSError, ValueError) as error:
            row.update({'coverage_complete': False, 'diagnostic': str(error)})
        rows.append(row)
    return {'schema_version': 1, 'ok': all(r['coverage_complete'] for r in rows),
            'grammar': 'ATX headings outside fenced blocks only; not full CommonMark',
            'query': query, 'files': rows,
            'coverage': {'selected': len(files), 'indexed': sum(r['status'] == 'indexed' for r in rows)}}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True)
    sub = parser.add_subparsers(dest='command', required=True)
    patches = sub.add_parser('patch-check')
    patches.add_argument('--patch', required=True)
    patches.add_argument('files', nargs='+')
    docs = sub.add_parser('docs')
    docs.add_argument('--query')
    docs.add_argument('files', nargs='+')
    args = parser.parse_args(argv)
    try:
        result = patch_review(args.root, args.patch, args.files) if args.command == 'patch-check' else doc_index(args.root, args.files, args.query)
    except (OSError, ValueError) as error:
        print(json.dumps({'schema_version': 1, 'ok': False, 'error': str(error)}, ensure_ascii=True))
        return 2
    print(json.dumps(result, ensure_ascii=True, indent=2))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
