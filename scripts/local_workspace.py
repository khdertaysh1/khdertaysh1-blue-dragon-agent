"""Manual, read-only workspace review. Never imported or registered by the agent.

No provider, network, MCP, delegation, command runner, or mutation interface.
Only fixed Git inspection commands are executed. Output is one JSON document.
"""
import argparse
import configparser
import threading
import difflib
import hashlib
import json
import multiprocessing
import os
from pathlib import Path
import re
import stat
import subprocess
import xml.etree.ElementTree as ET

FILE_CAP = 1024 * 1024
SCAN_CAP = 8 * FILE_CAP
OUTPUT_CAP = FILE_CAP
FILE_LIMIT = 200
MATCH_LIMIT = 200
SKIP_DIRS = {'.git', '.blue-dragon', 'target', 'node_modules', '.venv', '__pycache__'}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def safe_path(root, raw, missing=False):
    if not isinstance(raw, str) or not raw or len(raw) > 512:
        raise ValueError('Expected a bounded relative path')
    parts = raw.split('/')
    if '\\' in raw or ':' in raw or any(ord(c) < 32 for c in raw):
        raise ValueError('Ambiguous path or control character')
    if any(p in ('', '.', '..', '.git') for p in parts):
        raise ValueError('Path traversal or Git metadata is not allowed')
    current = root
    for i, part in enumerate(parts):
        current = current / part
        try:
            info = current.lstat()
        except FileNotFoundError:
            if missing and i == len(parts) - 1:
                return current
            raise ValueError('Path does not exist: ' + raw)
        if stat.S_ISLNK(info.st_mode):
            raise ValueError('Symlinks are not supported: ' + raw)
        if i < len(parts) - 1 and not stat.S_ISDIR(info.st_mode):
            raise ValueError('Parent is not a directory')
    return current


def signature(info):
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def read_bytes(root, raw, cap=FILE_CAP):
    path = safe_path(root, raw)
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
        raise ValueError('Only regular, non-hardlinked files are supported')
    with path.open('rb') as stream:
        opened = os.fstat(stream.fileno())
        if signature(opened) != signature(before):
            raise ValueError('File changed before read')
        data = stream.read(cap + 1)
        if len(data) > cap:
            raise ValueError('File exceeds byte limit')
        if signature(os.fstat(stream.fileno())) != signature(before):
            raise ValueError('File changed during read')
    if safe_path(root, raw) != path or signature(path.lstat()) != signature(before):
        raise ValueError('File changed during read')
    return data


def decode(data):
    text = data.decode('utf-8')
    if '\0' in text:
        raise ValueError('Binary text is unsupported')
    return text


def window(root, raw, offset, count):
    if offset < 0 or not 4 <= count <= 65536:
        raise ValueError('Offset must be nonnegative; byte count must be 4..65536')
    path = safe_path(root, raw)
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
        raise ValueError('Only regular, non-hardlinked files are supported')
    if offset > before.st_size:
        raise ValueError('Offset exceeds file size')
    with path.open('rb') as stream:
        if signature(os.fstat(stream.fileno())) != signature(before):
            raise ValueError('File changed before read')
        stream.seek(offset)
        chunk = stream.read(count + 3)
        try:
            text = decode(chunk)
        except UnicodeDecodeError as error:
            if error.reason != 'unexpected end of data' or offset + len(chunk) >= before.st_size:
                raise ValueError('Invalid UTF-8 or offset inside a code point') from error
            text = decode(chunk[:error.start])
        data = text.encode('utf-8')[:count]
        while data:
            try:
                text = decode(data)
                break
            except UnicodeDecodeError as error:
                if error.reason != 'unexpected end of data':
                    raise
                data = data[:error.start]
        if not data:
            text = ''
        if signature(os.fstat(stream.fileno())) != signature(before):
            raise ValueError('File changed during read')
    safe_path(root, raw)
    if signature(path.lstat()) != signature(before):
        raise ValueError('File changed during read')
    return {'path': raw, 'offset': offset, 'next_offset': offset + len(data),
            'size': before.st_size, 'complete': offset + len(data) == before.st_size,
            'window_sha256': digest(data), 'text': text}


def inventory(root):
    files, skipped = [], []
    complete = True
    visited = 0
    directories = 0
    for base, dirs, names in os.walk(root, followlinks=False):
        directories += 1
        if directories > FILE_LIMIT:
            return {'files': files, 'skipped': skipped, 'complete': False, 'limit': FILE_LIMIT}
        dirs.sort()
        names.sort()
        kept = []
        for name in dirs:
            item = Path(base) / name
            if name in SKIP_DIRS or item.is_symlink():
                if len(skipped) < FILE_LIMIT:
                    skipped.append({'path': item.relative_to(root).as_posix(), 'reason': 'excluded directory or symlink'})
            else:
                kept.append(name)
        dirs[:] = kept
        for name in names:
            visited += 1
            if visited > FILE_LIMIT:
                complete = False
                return {'files': files, 'skipped': skipped, 'complete': complete, 'limit': FILE_LIMIT}
            raw = (Path(base) / name).relative_to(root).as_posix()
            try:
                info = safe_path(root, raw).lstat()
                if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                    raise ValueError('nonregular or hardlinked')
                files.append({'path': raw, 'bytes': info.st_size})
            except (OSError, ValueError) as error:
                skipped.append({'path': raw, 'reason': str(error)})
    return {'files': files, 'skipped': skipped, 'complete': complete, 'limit': FILE_LIMIT}


def _regex_worker(connection, pattern, corpus):
    try:
        expression = re.compile(pattern)
        matches = []
        truncated = False
        for path, text in corpus:
            for number, line in enumerate(text.splitlines(), 1):
                if expression.search(line):
                    if len(matches) == MATCH_LIMIT:
                        truncated = True
                        break
                    matches.append({'path': path, 'line': number, 'text': line[:500]})
            if truncated:
                break
        connection.send({'matches': matches, 'match_limit_reached': truncated})
    except (ValueError, re.error) as error:
        connection.send({'error': str(error)})
    finally:
        connection.close()


def search(root, pattern, regex=False):
    if not pattern or len(pattern) > 512:
        raise ValueError('Pattern must contain 1..512 characters')
    listing = inventory(root)
    corpus, skipped, total = [], list(listing['skipped']), 0
    for entry in listing['files']:
        try:
            if entry['bytes'] > FILE_CAP or total + entry['bytes'] > SCAN_CAP:
                raise ValueError('Per-file or total scan byte limit')
            data = read_bytes(root, entry['path'], min(FILE_CAP, SCAN_CAP - total))
            total += len(data)
            corpus.append((entry['path'], decode(data)))
        except (OSError, ValueError) as error:
            skipped.append({'path': entry['path'], 'reason': str(error)})
    ctx = multiprocessing.get_context('spawn')
    reader, writer = ctx.Pipe(duplex=False)
    process = ctx.Process(target=_regex_worker, args=(writer, pattern if regex else re.escape(pattern), corpus))
    process.start()
    writer.close()
    try:
        if not reader.poll(3):
            raise ValueError('Search timed out; no complete result is available')
        result = reader.recv()
    finally:
        reader.close()
        process.join(0.2)
        if process.is_alive():
            process.terminate()
            process.join()
    if 'error' in result:
        raise ValueError(result['error'])
    return {**result, 'scanned_files': len(corpus), 'scanned_bytes': total,
            'complete': listing['complete'] and not skipped and not result['match_limit_reached'],
            'skipped': skipped, 'inventory_complete': listing['complete'],
            'limits': {'files': FILE_LIMIT, 'file_bytes': FILE_CAP, 'scan_bytes': SCAN_CAP, 'matches': MATCH_LIMIT}}


def check_git_config(root):
    # Fail closed: Git status/diff can otherwise run configured clean filters.
    directory = root / '.git'
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError('Only a regular .git directory is supported; worktree pointers are unsupported')
    config = directory / 'config'
    if config.is_symlink() or not config.is_file() or config.stat().st_size > 65536:
        raise ValueError('Unsupported Git config')
    for raw in ('config.worktree', 'commondir', 'objects/info/alternates'):
        if (directory / raw).exists():
            raise ValueError('Shared Git storage or worktree config is unsupported')
    reader = configparser.RawConfigParser(strict=True)
    reader.read_string(config.read_text(encoding='utf-8'))
    allowed = {'core': {'repositoryformatversion', 'filemode', 'bare', 'logallrefupdates',
                        'ignorecase', 'precomposeunicode'},
               'user': {'name', 'email'}, 'remote': {'url', 'fetch'},
               'branch': {'remote', 'merge'}}
    for section in reader.sections():
        category = section.split(' ', 1)[0].lower()
        if category not in allowed or not set(reader[section]).issubset(allowed[category]):
            raise ValueError('Git config outside the manual-review allowlist: ' + section)


def bounded_git(command, env):
    # Only fixed inspection commands reach this function, never a user command.
    process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                               stderr=subprocess.DEVNULL, env=env)
    timer = threading.Timer(5, process.kill)
    timer.start()
    try:
        data = process.stdout.read(OUTPUT_CAP + 1)
        truncated = len(data) > OUTPUT_CAP
        if truncated:
            process.kill()
        code = process.wait()
        if code and not truncated:
            raise ValueError('Git inspection failed or timed out: ' + str(code))
        return data[:OUTPUT_CAP], truncated
    finally:
        timer.cancel()
        if process.poll() is None:
            process.kill()
            process.wait()
        process.stdout.close()


def git_review(root, staged=False, raw=None):
    check_git_config(root)
    if raw is not None:
        safe_path(root, raw, missing=True)
    env = {key: value for key, value in os.environ.items() if not key.startswith(('GIT_', 'LD_', 'DYLD_'))}
    env.update({'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': os.devnull,
                'GIT_TERMINAL_PROMPT': '0', 'GIT_OPTIONAL_LOCKS': '0', 'GIT_LITERAL_PATHSPECS': '1'})
    base = ['git', '--no-pager', '-c', 'core.fsmonitor=false', '-c', 'core.hooksPath=' + os.devnull,
            '-c', 'color.ui=false', '-C', str(root)]
    commands = [base + ['status', '--porcelain=v1', '-z', '--untracked-files=normal', '--ignore-submodules=all'],
                base + ['diff', '--no-ext-diff', '--no-textconv', '--ignore-submodules=all'] +
                (['--cached'] if staged else []) + ['--'] + ([raw] if raw else [])]
    outputs = []
    for command in commands:
        data, truncated = bounded_git(command, env)
        outputs.append({'text': data[:OUTPUT_CAP].decode('utf-8', errors='replace'), 'truncated': truncated})
    return {'scope': 'staged' if staged else 'unstaged', 'path': raw, 'status': outputs[0], 'diff': outputs[1]}


def preview(root, manifest):
    spec = json.loads(decode(read_bytes(root, manifest)))
    if not isinstance(spec, dict) or set(spec) != {'changes'} or not isinstance(spec['changes'], list):
        raise ValueError('Expected an object containing only changes')
    if not 1 <= len(spec['changes']) <= 32:
        raise ValueError('Expected 1..32 changes')
    seen, results, total = set(), [], 0
    for change in spec['changes']:
        if not isinstance(change, dict) or set(change) != {'path', 'expected_sha256', 'content'}:
            raise ValueError('Each change requires path, expected_sha256 and content')
        raw, expected, content = change['path'], change['expected_sha256'], change['content']
        path = safe_path(root, raw, missing=True)
        if raw in seen:
            raise ValueError('Duplicate path: ' + raw)
        seen.add(raw)
        old = read_bytes(root, raw) if path.exists() else None
        if expected != (digest(old) if old is not None else None):
            raise ValueError('Conflict: ' + raw)
        if content is not None and not isinstance(content, str):
            raise ValueError('Content must be UTF-8 text or null for deletion')
        new = content.encode('utf-8') if content is not None else None
        if old is None and new is None:
            raise ValueError('Cannot delete an absent file')
        if new is not None and len(new) > FILE_CAP:
            raise ValueError('Replacement exceeds file byte limit')
        total += len(old or b'') + len(new or b'')
        if total > SCAN_CAP:
            raise ValueError('Preview exceeds total byte limit')
        old_text, new_text = decode(old or b''), decode(new or b'')
        diff = ''.join(difflib.unified_diff(old_text.splitlines(True), new_text.splitlines(True),
                                          fromfile=raw if old is not None else '/dev/null',
                                          tofile=raw if new is not None else '/dev/null'))
        results.append({'path': raw, 'operation': 'create' if old is None else 'delete' if new is None else 'update',
                        'before_sha256': expected, 'after_sha256': digest(new) if new is not None else None,
                        'diff': diff[:65536], 'diff_truncated': len(diff) > 65536,
                        'old_ends_newline': old_text.endswith('\n'), 'new_ends_newline': new_text.endswith('\n')})
    return {'preview_only': True, 'applied': False, 'changes': results,
            'approval_digest': digest(json.dumps(spec, sort_keys=True, ensure_ascii=True).encode())}


def sessions(root, directory, query, role):
    folder = safe_path(root, directory)
    if not folder.is_dir() or not query or len(query) > 512:
        raise ValueError('Expected a session directory and 1..512 character query')
    listing = inventory(folder)
    matches, skipped, total = [], list(listing['skipped']), 0
    for entry in listing['files']:
        if not entry['path'].endswith('.json'):
            continue
        try:
            data = read_bytes(folder, entry['path'], min(16_000_000, 32_000_000 - total))
            total += len(data)
            document = json.loads(decode(data))
            if not isinstance(document, dict) or not isinstance(document.get('messages'), list):
                raise ValueError('Not a Blue session snapshot')
            for turn, message in enumerate(document['messages']):
                if not isinstance(message, dict) or message.get('role') != role:
                    continue
                content = message.get('content')
                if isinstance(content, str) and query.casefold() in content.casefold():
                    if len(matches) == MATCH_LIMIT:
                        return {'matches': matches, 'complete': False, 'skipped': skipped, 'role': role}
                    matches.append({'source': directory + '/' + entry['path'], 'role': role,
                                    'turn': turn, 'session_id': document.get('id'), 'excerpt': content[:500]})
        except (OSError, ValueError) as error:
            skipped.append({'path': entry['path'], 'reason': str(error)})
    return {'matches': matches, 'role': role, 'complete': listing['complete'] and not skipped,
            'skipped': skipped, 'scope': 'explicit directory only; one role; string content only'}


def test_report(root, raw):
    data = read_bytes(root, raw)
    xml_text = decode(data)
    if '<!DOCTYPE' in xml_text.upper() or '<!ENTITY' in xml_text.upper():
        raise ValueError('DTD and entities are unsupported')
    document = ET.fromstring(xml_text)
    if document.tag not in ('testsuite', 'testsuites'):
        raise ValueError('Expected a JUnit testsuite or testsuites document')
    cases = list(document.iter('testcase'))
    counts = {'passed': 0, 'failed': 0, 'errors': 0, 'skipped': 0}
    for case in cases:
        key = 'errors' if case.find('error') is not None else 'failed' if case.find('failure') is not None else 'skipped' if case.find('skipped') is not None else 'passed'
        counts[key] += 1
    return {'source': raw, 'sha256': digest(data), 'tests': len(cases), **counts,
            'executed_by_this_command': False, 'freshness': 'unverified',
            'note': 'Counts describe supplied testcase records, not independently verified execution.'}


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument('--root', required=True, type=Path)
    commands = result.add_subparsers(dest='command', required=True)
    commands.add_parser('files')
    read = commands.add_parser('read')
    read.add_argument('path')
    read.add_argument('--offset', type=int, default=0)
    read.add_argument('--bytes', type=int, default=16384)
    find = commands.add_parser('search')
    find.add_argument('pattern')
    find.add_argument('--regex', action='store_true')
    git = commands.add_parser('git')
    git.add_argument('--staged', action='store_true')
    git.add_argument('--path')
    patch = commands.add_parser('preview')
    patch.add_argument('manifest')
    history = commands.add_parser('sessions')
    history.add_argument('directory')
    history.add_argument('query')
    history.add_argument('--role', choices=['user', 'assistant', 'tool'], required=True)
    tests = commands.add_parser('test-report')
    tests.add_argument('path')
    return result


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        root = args.root.resolve(strict=True)
        if not root.is_dir():
            raise ValueError('Root must be a directory')
        if args.command == 'files':
            value = inventory(root)
        elif args.command == 'read':
            value = window(root, args.path, args.offset, args.bytes)
        elif args.command == 'search':
            value = search(root, args.pattern, args.regex)
        elif args.command == 'git':
            value = git_review(root, args.staged, args.path)
        elif args.command == 'preview':
            value = preview(root, args.manifest)
        elif args.command == 'sessions':
            value = sessions(root, args.directory, args.query, args.role)
        else:
            value = test_report(root, args.path)
        print(json.dumps({'schema_version': 1, 'ok': True, 'result': value}, ensure_ascii=True))
        return 0
    except (OSError, ValueError, subprocess.SubprocessError, EOFError, ET.ParseError, configparser.Error) as error:
        print(json.dumps({'schema_version': 1, 'ok': False, 'error': str(error)}, ensure_ascii=True))
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
