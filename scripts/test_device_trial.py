"""Exercise the extracted kit runner and fail-closed controls without a real device."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import zipfile

if os.name != 'posix':
    raise SystemExit('POSIX runner test required')
repo = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory() as temp:
    root = Path(temp)
    archive = root / 'kit.zip'
    subprocess.run([sys.executable, str(repo / 'scripts/package_trial_kit.py'), '--output', str(archive)], check=True)
    kit = root / 'kit'
    with zipfile.ZipFile(archive) as z:
        z.extractall(kit)
    manifest_path = kit / 'trial-manifest.json'
    manifest = json.loads(manifest_path.read_text())
    binary = root / 'binary'
    binary.write_text('#!/bin/sh\necho "blue-dragon wrong-version"\n')
    binary.chmod(0o755)
    def run(output):
        return subprocess.run([sys.executable, str(kit / 'scripts/device_trial.py'),
                               '--binary', str(binary), '--output', str(output)], capture_output=True, text=True)
    output = root / 'wrong-version'
    assert run(output).returncode != 0
    result = json.loads((output / 'result.json').read_text())
    assert result['status'] == 'failed' and not result['groups'] and 'version mismatch' in result['error']
    before = (output / 'result.json').read_bytes()
    assert run(output).returncode != 0
    assert (output / 'result.json').read_bytes() == before
    script = kit / 'scripts/smoke_continuity.py'
    original = script.read_bytes()
    script.write_bytes(original + b'\n# inconsistent kit\n')
    output = root / 'tamper'
    assert 'integrity mismatch' in run(output).stderr
    assert not output.exists()
    # A negative fixture is explicitly re-pinned to test runner aggregation,
    # not the unchanged product fixture. It must not yield a passing report.
    binary.write_text(f'#!/bin/sh\necho "blue-dragon {manifest["version"]}"\n')
    for name in manifest['groups']:
        path = kit / 'scripts' / f'{name}.py'
        path.write_text('raise SystemExit(7)\n')
        manifest['files'][f'scripts/{name}.py'] = hashlib.sha256(path.read_bytes()).hexdigest()
    manifest_path.write_text(json.dumps(manifest))
    output = root / 'failure'
    assert run(output).returncode != 0
    result = json.loads((output / 'result.json').read_text())
    assert result['status'] == 'failed' and len(result['groups']) == 6
    assert all(g['status'] == 'failed' and g['exit_code'] == 7 for g in result['groups'])
    assert result['binary_unchanged']
print('Device kit controls passed: version mismatch, overwrite refusal, tamper rejection, failed-group aggregation')
