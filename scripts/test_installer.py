"""Offline installer regressions with local curl/uname fixtures; no real installs."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import tempfile

installer = Path("scripts/install.sh").resolve()
tag = "v0.4.0-alpha.4"
repo = "khdertaysh1/khdertaysh1-blue-dragon-agent"


def executable(path, text):
    path.write_text(text)
    path.chmod(0o755)


with tempfile.TemporaryDirectory() as temp:
    root = Path(temp)
    fixtures = root / "fixtures"
    tools = root / "tools"
    fixtures.mkdir()
    tools.mkdir()
    bindir = root / "private storage" / "bin"
    home = root / "home"
    home.mkdir()
    checkout = home / "existing-checkout"
    checkout.mkdir()
    (checkout / "Cargo.lock").write_text("user generated lock; preserve me")
    asset = "blue-dragon-android-arm64"
    binary = "#!/bin/sh\nprintf 'blue-dragon 0.4.0-alpha.4\\n'\n"
    executable(fixtures / asset, binary)
    (fixtures / "LICENSES.txt").write_text("Fixture license notices\n")

    def manifest():
        (fixtures / "SHA256SUMS.txt").write_text("".join(
            f"{hashlib.sha256((fixtures / name).read_bytes()).hexdigest()}  {name}\n"
            for name in [asset, "LICENSES.txt"]))

    manifest()
    release = {"tag_name": tag, "draft": False, "prerelease": True, "assets": [
        {"name": name, "browser_download_url": f"https://github.com/{repo}/releases/download/{tag}/{name}"}
        for name in [asset, "SHA256SUMS.txt", "LICENSES.txt"]]}
    (fixtures / "releases.json").write_text(json.dumps([release]))
    (fixtures / "release.json").write_text(json.dumps(release))
    executable(tools / "curl", """#!/usr/bin/env python3
import os, pathlib, shutil, sys
if os.environ.get('TEST_DOWNLOAD_FAILURE'): raise SystemExit(22)
args=sys.argv[1:]; url=next(a for a in args if a.startswith('https://'))
out=args[args.index('-o')+1]; fixtures=pathlib.Path(os.environ['TEST_FIXTURES'])
if '/releases?' in url: name='releases.json'
elif '/releases/tags/' in url: name='release.json'
else: name=url.rsplit('/',1)[-1]
shutil.copyfile(fixtures/name,out)
""")
    executable(tools / "uname", """#!/bin/sh
case "$1" in
  -s) echo Linux;;
  -m) echo "${TEST_ARCH:-aarch64}";;
  -o) echo Android;;
esac
""")
    env = dict(os.environ, PATH=f"{tools}:{os.environ['PATH']}", HOME=str(home),
               PREFIX=str(bindir.parent), TEST_FIXTURES=str(fixtures))

    def run(*args, ok=True, extra=None):
        current = dict(env, **(extra or {}))
        result = subprocess.run(["bash", str(installer), *args], env=current,
                                cwd=checkout, text=True, capture_output=True, timeout=10)
        assert (result.returncode == 0) == ok, result.stdout + result.stderr
        assert not list(bindir.glob(".blue-dragon-install.*")), "Temporary payload left behind"
        assert (checkout / "Cargo.lock").read_text() == "user generated lock; preserve me"
        return result.stdout + result.stderr

    assert "Installed blue-dragon 0.4.0-alpha.4" in run()
    target = bindir / "blue-dragon"
    assert target.read_text() == binary
    assert (bindir / "blue-dragon.LICENSES.txt").exists()
    # Repeating an install must not destroy an older rollback copy.
    (bindir / "blue-dragon.previous").write_text("older rollback")
    assert "already installed" in run()
    assert (bindir / "blue-dragon.previous").read_text() == "older rollback"
    # Upgrade an old executable without any Git checkout or toolchain activity.
    executable(target, "#!/bin/sh\necho 'blue-dragon 0.3.0-alpha.1'\n")
    old = target.read_bytes()
    run("--version", tag)
    assert (bindir / "blue-dragon.previous").read_bytes() == old
    installed = target.read_bytes()
    # Every failure below must preserve the working installation.
    run(ok=False, extra={"TEST_DOWNLOAD_FAILURE": "1"})
    assert target.read_bytes() == installed
    (fixtures / asset).write_text(binary + "# corrupt payload\n")
    assert "SHA256 mismatch" in run(ok=False)
    assert target.read_bytes() == installed
    executable(fixtures / asset, "#!/bin/sh\necho 'blue-dragon 9.9.9'\n")
    manifest()
    assert "version mismatch" in run(ok=False)
    assert target.read_bytes() == installed
    executable(fixtures / asset, binary)
    manifest()
    checksums = (fixtures / "SHA256SUMS.txt").read_text()
    (fixtures / "SHA256SUMS.txt").write_text(checksums + checksums)
    run(ok=False)
    assert target.read_bytes() == installed
    assert "Unsupported Android architecture" in run(ok=False, extra={"TEST_ARCH": "armv7l"})
    run("--version", "../invalid", ok=False)
    assert target.read_bytes() == installed
    target.unlink()
    target.symlink_to(bindir / "blue-dragon.previous")
    assert "symlink" in run(ok=False)
print("Installer: fresh Termux-style install, repeated update, rollback preservation, source isolation, download/hash/version/architecture/symlink failures passed")
