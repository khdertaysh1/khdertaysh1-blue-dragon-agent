"""Verify the exact release asset set and write checksums/third-party notices."""
from pathlib import Path
import hashlib
import json
import os
import tomllib
import zipfile

folder = Path("release-payload")
required = {"blue-dragon-android-arm64", "blue-dragon-linux-x64",
            "blue-dragon-macos-arm64", "blue-dragon-windows-x64.exe"}
archives = {"blue-dragon-browser-firefox.xpi", "blue-dragon-browser-chromium.zip"}
required |= archives
actual = {p.name for p in folder.iterdir()}
if actual != required:
    raise SystemExit(f"Unexpected release assets; missing={required-actual}, extra={actual-required}")
for name in required:
    if name in archives:
        with zipfile.ZipFile(folder / name) as archive:
            if "manifest.json" not in archive.namelist() or archive.testzip():
                raise SystemExit(f"Invalid extension: {name}")
        continue
    if (folder / name).stat().st_size < 100_000:
        raise SystemExit(f"Unexpectedly small binary: {name}")
version = tomllib.loads(Path("Cargo.toml").read_text())["package"]["version"]
(folder / "LICENSES.txt").write_text(
    Path("LICENSE").read_text() + "\n\n--- Third-party attribution ---\n" +
    Path("third_party/NOTICE.md").read_text() + "\n" +
    Path("third_party/AlphaCode-LICENSE").read_text(), encoding="utf-8")
(folder / "release.json").write_text(json.dumps({
    "version": version, "commit": os.environ["GITHUB_SHA"],
    "browser_extension_signed": False,
    "android_min_api": 26, "physical_android_tested": False,
    "assets": sorted(required),
}, indent=2) + "\n")
checksums = "".join(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n"
                    for p in sorted(folder.iterdir()) if p.is_file())
(folder / "SHA256SUMS.txt").write_text(checksums)
with open(os.environ["GITHUB_OUTPUT"], "a") as out:
    print(f"tag=v{version}", file=out)
