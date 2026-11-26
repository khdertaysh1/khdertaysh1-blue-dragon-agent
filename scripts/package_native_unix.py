"""Build a Linux amd64 .deb or an unsigned macOS arm64 .pkg from the CI binary."""
import argparse
from pathlib import Path
import plistlib
import platform
import shutil
import subprocess
import tempfile

from release_version import load_version


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    version, _ = load_version(repo)
    source = args.binary.resolve(strict=True)
    if subprocess.check_output([source, "--version"], text=True).strip() != f"blue-dragon {version}":
        parser.error("Binary and repository versions differ")
    target = (platform.system(), platform.machine().lower())
    linux = target == ("Linux", "x86_64")
    if not linux and target != ("Darwin", "arm64"):
        parser.error(f"Unsupported native installer target: {target}")
    args.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=args.output) as temp:
        work = Path(temp).resolve()
        root = work / "root"
        prefix = root / ("usr" if linux else "usr/local")
        executable = prefix / "bin/blue-dragon"
        executable.parent.mkdir(parents=True)
        shutil.copyfile(source, executable)
        executable.chmod(0o755)
        docs = prefix / "share/doc/blue-dragon"
        docs.mkdir(parents=True)
        for license_file in [repo / "LICENSE", repo / "third_party/NOTICE.md", *sorted((repo / "third_party").glob("*-LICENSE"))]:
            shutil.copyfile(license_file, docs / license_file.name)
        if linux:
            desktop = prefix / "share/applications/blue-dragon.desktop"
            desktop.parent.mkdir(parents=True)
            desktop.write_text("[Desktop Entry]\nType=Application\nName=Blue Dragon\n"
                               "Exec=/usr/bin/blue-dragon gui\nIcon=blue-dragon\nTerminal=false\n"
                               "Categories=Development;\n", encoding="utf-8")
            icon = prefix / "share/icons/hicolor/scalable/apps/blue-dragon.svg"
            icon.parent.mkdir(parents=True)
            shutil.copyfile(repo / "assets/dragon-mark.svg", icon)
            name = f"blue-dragon_{version}_amd64.deb"
            control = root / "DEBIAN/control"
            control.parent.mkdir()
            # dpkg-shlibdeps uses debian/control to locate the source package.
            (work / "debian").mkdir()
            (work / "debian/control").write_text("Source: blue-dragon\n", encoding="utf-8")
            deps = subprocess.check_output(["dpkg-shlibdeps", "-O", str(executable)], cwd=work, text=True)
            dependencies = dict(line.split("=", 1) for line in deps.splitlines() if "=" in line)
            depends = dependencies.get("shlibs:Depends", "")
            dependency_field = f"Depends: {depends}\n" if depends else ""
            control.write_text(f"Package: blue-dragon\nVersion: {version}\nArchitecture: amd64\n"
                               f"Maintainer: Blue Dragon project <noreply@github.com>\n{dependency_field}"
                               "Description: Blue Dragon coding assistant\n", encoding="utf-8")
            subprocess.run(["dpkg-deb", "--root-owner-group", "--build", root, work / name], check=True)
        else:
            app = root / "Applications/Blue Dragon.app/Contents"
            launcher = app / "MacOS/BlueDragon"
            launcher.parent.mkdir(parents=True)
            launcher.write_text('#!/bin/sh\ncd "$HOME" || exit 1\nexec /usr/local/bin/blue-dragon gui\n',
                                encoding="utf-8")
            launcher.chmod(0o755)
            (app / "Info.plist").write_bytes(plistlib.dumps({
                "CFBundlePackageType": "APPL", "CFBundleExecutable": "BlueDragon",
                "CFBundleIdentifier": "com.github.khdertaysh1.blue-dragon.desktop",
                "CFBundleName": "Blue Dragon", "CFBundleVersion": version,
            }))
            name = f"blue-dragon-{version}-macos-arm64.pkg"
            subprocess.run(["pkgbuild", "--root", root, "--install-location", "/", "--identifier",
                            "com.github.khdertaysh1.blue-dragon", "--version", version, work / name], check=True)
        (work / name).replace(args.output / name)


if __name__ == "__main__":
    main()
