"""Manual offline Python AST navigation for explicitly selected files only.

This module is not an agent tool. It never imports/executes inspected code, follows
imports, walks directories, or launches a provider, subprocess, or network request.
"""
import argparse
import ast
import hashlib
import json
import os
from pathlib import Path
import stat
import sys

MAX_FILES = 32
FILE_BYTES = 262144
TOTAL_BYTES = 2097152
MAX_RECORDS = 2000


def read_source(root, raw, limit):
    parts = raw.split('/')
    if (not raw or len(raw) > 512 or '\\' in raw or ':' in raw
            or any(ord(c) < 32 for c in raw)
            or any(p in ('', '.', '..', '.git') for p in parts)):
        raise ValueError('Expected a plain workspace-relative path')
    path = root
    for part in parts:
        path = path / part
        if path.is_symlink():
            raise ValueError('Symlink paths are unsupported')
    if path.suffix != '.py':
        raise ValueError('Only explicit .py files are supported')
    before = path.stat()
    if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
        raise ValueError('Expected a regular non-hardlinked file')
    if before.st_size > limit:
        raise ValueError('File or total byte budget exceeded')
    flags = os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0)
    with os.fdopen(os.open(path, flags), 'rb') as stream:
        opened = os.fstat(stream.fileno())
        identity = lambda x: (x.st_dev, x.st_ino, x.st_size, x.st_mtime_ns, x.st_ctime_ns)
        if identity(opened) != identity(before):
            raise ValueError('File changed before read')
        data = stream.read(limit + 1)
        if len(data) > limit or identity(os.fstat(stream.fileno())) != identity(before):
            raise ValueError('File changed during read or exceeded budget')
    if identity(path.stat()) != identity(before):
        raise ValueError('File changed during read')
    return data


def dotted(node):
    names = []
    while isinstance(node, ast.Attribute):
        names.append(node.attr)
        node = node.value
    if not isinstance(node, ast.Name):
        return '<dynamic>'
    return '.'.join([node.id] + list(reversed(names)))


def records(tree):
    # Iterative traversal avoids adding recursive visitor stack requirements.
    stack = [(tree, '')]
    while stack:
        node, scope = stack.pop()
        result = None
        child_scope = scope
        if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            child_scope = '.'.join(filter(None, (scope, node.name)))
            result = {'kind': 'class' if isinstance(node, ast.ClassDef) else
                      'async_function' if isinstance(node, ast.AsyncFunctionDef) else 'function',
                      'name': node.name, 'qualified_name': child_scope}
        elif isinstance(node, (ast.Import, ast.ImportFrom)):
            result = {'kind': 'import', 'name': ', '.join(alias.name for alias in node.names),
                      'module': node.module if isinstance(node, ast.ImportFrom) else None,
                      'relative_level': node.level if isinstance(node, ast.ImportFrom) else 0,
                      'bindings': [{'name': alias.name, 'alias': alias.asname} for alias in node.names]}
        elif isinstance(node, ast.Call):
            result = {'kind': 'call_reference', 'name': dotted(node.func), 'resolved': False}
        if result is not None:
            yield {**result, 'lexical_scope': scope, 'line': node.lineno,
                   'end_line': node.end_lineno, 'column_utf8_bytes': node.col_offset}
        children = list(ast.iter_child_nodes(node))
        stack.extend((child, child_scope) for child in reversed(children))


def navigate(root, paths, query=None):
    if not 1 <= len(paths) <= MAX_FILES or len(set(paths)) != len(paths):
        raise ValueError('Supply 1..32 distinct explicit file paths')
    if query is not None and (not query or len(query) > 256):
        raise ValueError('Query must contain 1..256 characters')
    root = Path(root).resolve(strict=True)
    if not root.is_dir():
        raise ValueError('Root is not a directory')
    output, total, emitted = [], 0, 0
    for raw in paths:
        row = {'path': raw, 'status': 'error', 'records': [], 'coverage_complete': False}
        try:
            data = read_source(root, raw, min(FILE_BYTES, TOTAL_BYTES - total))
            total += len(data)
            row.update({'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)})
            # ast.parse(bytes) honors Python encoding cookies; never imports the file.
            tree = ast.parse(data, filename=raw)
            found, matches = 0, 0
            row['status'] = 'parsed'
            row['coverage_complete'] = True
            for record in records(tree):
                found += 1
                searchable = ' '.join(str(record.get(k) or '') for k in ('name', 'qualified_name', 'module'))
                if query is not None and query.casefold() not in searchable.casefold():
                    continue
                matches += 1
                if emitted >= MAX_RECORDS:
                    row['coverage_complete'] = False
                    continue
                row['records'].append(record)
                emitted += 1
            row.update({'records_examined': found, 'matching_records': matches,
                        'records_returned': len(row['records'])})
            if not row['coverage_complete']:
                row['diagnostic'] = {'code': 'record_limit', 'message': 'Matching records omitted by global limit'}
        except SyntaxError as error:
            row['diagnostic'] = {'code': 'syntax_error', 'message': error.msg,
                                 'line': error.lineno, 'column_characters_1_based': error.offset}
        except (OSError, ValueError, RecursionError, MemoryError) as error:
            row['diagnostic'] = {'code': 'read_or_parse_error', 'message': str(error)}
        output.append(row)
    complete = all(row['coverage_complete'] for row in output)
    return {'schema_version': 1, 'ok': complete, 'coverage_complete': complete,
            'scope': 'explicit files only; symbols, imports and syntactic calls, not name resolution',
            'query': query, 'python_parser': sys.version.split()[0],
            'limits': {'files': MAX_FILES, 'file_bytes': FILE_BYTES, 'total_bytes': TOTAL_BYTES,
                       'records': MAX_RECORDS},
            'coverage': {'requested_files': len(paths), 'parsed_files': sum(r['status'] == 'parsed' for r in output),
                         'bytes_read': total, 'records_returned': emitted}, 'files': output}


def readable(result):
    # Escape untrusted terminal controls; decoded JSON remains lossless.
    quote = lambda value: json.dumps(value, ensure_ascii=True)
    lines = [f"Coverage: {'complete' if result['coverage_complete'] else 'INCOMPLETE'} (explicit files only)"]
    for row in result['files']:
        lines.append(f"{quote(row['path'])}: {row['status']} sha256={row.get('sha256', 'unavailable')}")
        for record in row['records']:
            name = record.get('qualified_name', record['name'])
            lines.append(f"  {record['line']}:{record['column_utf8_bytes']} {record['kind']} {quote(name)}")
        if 'diagnostic' in row:
            lines.append('  diagnostic: ' + quote(row['diagnostic']))
    return '\n'.join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True, type=Path)
    parser.add_argument('--query')
    parser.add_argument('--format', choices=['json', 'text'], default='json')
    parser.add_argument('paths', nargs='+')
    args = parser.parse_args(argv)
    try:
        result = navigate(args.root, args.paths, args.query)
    except (OSError, ValueError) as error:
        print(json.dumps({'schema_version': 1, 'ok': False, 'error': str(error)}))
        return 2
    print(json.dumps(result, ensure_ascii=True) if args.format == 'json' else readable(result))
    return 0 if result['coverage_complete'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
