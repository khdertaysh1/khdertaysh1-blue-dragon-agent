"""Offline completeness, version and checksum regression for public releases."""
from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import tomllib

repo = Path.cwd()
with tempfile.TemporaryDirectory() as temp:
    root = Path(temp)
    for name in ["LICENSE", "Cargo.toml"]:
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(repo / name, target)
    shutil.copytree(repo / "third_party", root / "third_party")
    binary = root / "fixture-binary"
    binary.write_bytes(b"fixture" * 20_000)
    for os_name, arch in [("Linux", "X64"), ("Windows", "X64"), ("macOS", "ARM64"), ("Android", "ARM64")]:
        subprocess.run(["python", str(repo / "scripts/package_binary.py"), str(binary), os_name, arch],
                       cwd=root, check=True, capture_output=True)
    subprocess.run(["python", str(repo / "scripts/build_browser_extension.py")], check=True, capture_output=True)
    for name in ["blue-dragon-browser-firefox.xpi", "blue-dragon-browser-chromium.zip"]:
        shutil.copyfile(repo / "target/browser-extensions" / name, root / "release-payload" / name)
    output = root / "github-output"
    env = dict(os.environ, GITHUB_OUTPUT=str(output), GITHUB_SHA="f" * 40)
    subprocess.run(["python", str(repo / "scripts/prepare_release.py")], cwd=root, env=env,
                   check=True, capture_output=True)
    folder = root / "release-payload"
    metadata = json.loads((folder / "release.json").read_text())
    version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    assert metadata["version"] == version
    assert dict(line.split("=", 1) for line in output.read_text().splitlines()) == {
        "tag": f"v{version}", "prerelease": str("-" in version).lower()}
    assert metadata["physical_android_tested"] is False
    assert "Copyright" in (folder / "LICENSES.txt").read_text()
    for notice in (repo / "third_party").glob("*-LICENSE"):
        assert notice.read_text() in (folder / "LICENSES.txt").read_text()
    for line in (folder / "SHA256SUMS.txt").read_text().splitlines():
        digest, name = line.split()
        assert hashlib.sha256((folder / name).read_bytes()).hexdigest() == digest
    (folder / "blue-dragon-android-arm64").unlink()
    result = subprocess.run(["python", str(repo / "scripts/prepare_release.py")], cwd=root,
                            env=env, capture_output=True)
    assert result.returncode != 0
print("Release payload: complete platform set, version metadata, licenses and SHA256 verified; incomplete release rejected")
