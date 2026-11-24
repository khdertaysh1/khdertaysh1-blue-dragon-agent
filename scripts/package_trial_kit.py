#!/usr/bin/env python3
"""Bundle existing deterministic smoke fixtures for trials without Rust or Git."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from release_version import load_version
import zipfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', required=True, type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
names = ['smoke_continuity', 'smoke_delegates', 'smoke_delegate_consistency',
         'smoke_integrated_research', 'smoke_mcp_resources', 'smoke_memory_sources']
files = ['scripts/device_trial.py', 'scripts/fixture_intent.py', 'tests/fixtures/mcp_resource_server.py', 'LICENSE']
files += [f'scripts/{name}.py' for name in names]
files += ['docs/CANDIDATE_TRIAL_PLAN.md', 'VERSION']
version, package_version = load_version(root)
manifest = {'version': version, 'package_version': package_version,
            'source_commit': subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip(),
            'source_dirty': bool(subprocess.check_output(['git', '-C', str(root), 'status', '--porcelain'], text=True).strip()),
            'groups': names,
            'files': {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in files}}
args.output.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(args.output, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
    for name in files:
        archive.write(root / name, name)
    archive.writestr('trial-manifest.json', json.dumps(manifest, indent=2) + '\n')
print(args.output)
