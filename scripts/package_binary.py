"""Create a consistently named raw release binary after successful CI checks."""
from pathlib import Path
import shutil
import sys

source, os_name, arch = sys.argv[1:]
platform = {
    ("Linux", "X64"): "linux-x64",
    ("Windows", "X64"): "windows-x64.exe",
    ("macOS", "ARM64"): "macos-arm64",
    ("Android", "ARM64"): "android-arm64",
}.get((os_name, arch))
if not platform:
    raise SystemExit(f"Unsupported release platform: {os_name}/{arch}")
output = Path("release-payload")
output.mkdir(exist_ok=True)
shutil.copyfile(source, output / f"blue-dragon-{platform}")
