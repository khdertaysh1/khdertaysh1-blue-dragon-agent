#!/usr/bin/env python3
"""Run isolated loopback fixtures on a supplied binary; no downloads or installs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import time


def sha(path):
    with path.open('rb') as stream:
        digest = hashlib.sha256()
        for block in iter(lambda: stream.read(65536), b''):
            digest.update(block)
        return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path, help='New directory; never overwritten')
    parser.add_argument('--timeout', type=int, default=120)
    args = parser.parse_args()
    if os.name != 'posix':
        parser.error('This trial runner supports POSIX hosts, including Termux; use platform CI on Windows')
    if not 15 <= args.timeout <= 300:
        parser.error('timeout must be 15..300 seconds')
    root = Path(__file__).resolve().parent.parent
    manifest = json.loads((root / 'trial-manifest.json').read_text())
    # The manifest is a transport consistency check, not a signature or trust root.
    for name, digest in manifest['files'].items():
        path = root / name
        if path.is_symlink() or root not in path.resolve().parents or sha(path) != digest:
            parser.error(f'Trial kit integrity mismatch: {name}')
    binary = args.binary.resolve(strict=True)
    before = sha(binary)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    home = output / 'isolated-home'
    home.mkdir()
    env = {k: v for k, v in os.environ.items() if k in {'PATH', 'LANG', 'LC_ALL', 'TMPDIR', 'TMP', 'TEMP'}}
    env.update(HOME=str(home), XDG_CONFIG_HOME=str(home / 'config'), XDG_DATA_HOME=str(home / 'data'),
               PYTHONDONTWRITEBYTECODE='1', NO_COLOR='1')
    report = {'version': manifest['version'], 'source_commit': manifest['source_commit'], 'kit_source_dirty': manifest['source_dirty'],
              'binary_sha256': before, 'kit_manifest_sha256': sha(root / 'trial-manifest.json'),
              'host_system': platform.system(), 'host_machine': platform.machine(),
              'python_version': platform.python_version(), 'groups': [], 'status': 'running',
              'limits': ['Deterministic loopback providers only; no live model benchmark',
                         'Authenticated mock browser only; no DOM or real Chromium test',
                         'Host metadata does not prove a physical Android device',
                         'No Android system kill, background survival or battery test']}
    def save():
        (output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    save()
    try:
        version = subprocess.check_output([str(binary), '--version'], env=env, cwd=home,
                                          text=True, timeout=15).strip()
        if version != f"blue-dragon {manifest['version']}":
            raise ValueError(f'Binary version mismatch: {version}')
        for name in manifest['groups']:
            start = time.monotonic()
            log = output / f'{name}.log'
            with log.open('wb') as stream:
                process = subprocess.Popen([sys.executable, str(root / 'scripts' / f'{name}.py'), str(binary)],
                                           env=env, cwd=home, stdout=stream, stderr=subprocess.STDOUT,
                                           start_new_session=True)
                try:
                    code = process.wait(timeout=args.timeout)
                    status = 'passed' if code == 0 else 'failed'
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    code = process.wait()
                    status = 'timeout'
                except BaseException:
                    if process.poll() is None:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait()
                    raise
            report['groups'].append({'name': name, 'status': status, 'exit_code': code,
                                     'seconds': round(time.monotonic() - start, 3), 'log_sha256': sha(log)})
            save()
            print(f'{name}: {status}', flush=True)
        report['status'] = 'passed' if all(g['status'] == 'passed' for g in report['groups']) else 'failed'
    except (Exception, KeyboardInterrupt) as error:
        report['status'] = 'failed'
        report['error'] = str(error)
    finally:
        report['binary_unchanged'] = binary.is_file() and sha(binary) == before
        if not report['binary_unchanged']:
            report['status'] = 'failed'
        save()
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    sys.exit(main())
