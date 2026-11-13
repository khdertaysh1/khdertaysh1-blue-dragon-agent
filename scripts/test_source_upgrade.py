"""Reproduce the user's old-branch/untracked-Cargo.lock error, then fix safely."""
from pathlib import Path
import os
import subprocess
import tempfile

updater = Path("scripts/update-source.sh").resolve()


def command(args, cwd, env=None, ok=True):
    result = subprocess.run(args, cwd=cwd, env=env, capture_output=True, text=True, timeout=20)
    assert (result.returncode == 0) == ok, result.stdout + result.stderr
    return result


with tempfile.TemporaryDirectory() as temp:
    root = Path(temp)
    upstream = root / "upstream"
    upstream.mkdir()
    command(["git", "init", "-b", "main"], upstream)
    for key, value in [("user.name", "Fixture"), ("user.email", "fixture@example.invalid")]:
        command(["git", "config", key, value], upstream)
    (upstream / "Cargo.toml").write_text('[package]\nname = "blue-dragon"\nversion = "0.3.0-alpha.1"\n')
    command(["git", "add", "."], upstream)
    command(["git", "commit", "-m", "old source without lock"], upstream)
    old_commit = command(["git", "rev-parse", "HEAD"], upstream).stdout.strip()
    (upstream / "Cargo.lock").write_text("upstream tracked lock\n")
    (upstream / "Cargo.toml").write_text('[package]\nname = "blue-dragon"\nversion = "0.4.0-alpha.4"\n')
    command(["git", "add", "."], upstream)
    command(["git", "commit", "-m", "new version tracks lock"], upstream)
    checkout = root / "checkout"
    command(["git", "clone", str(upstream), str(checkout)], root)
    command(["git", "switch", "-c", "old-user-branch", old_commit], checkout)
    (checkout / "Cargo.lock").write_text("user-generated-lock\n")
    # Prove this fixture actually exhibits the reported failure.
    failed = command(["git", "switch", "main"], checkout, ok=False)
    assert "Cargo.lock" in failed.stderr
    public = "https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent.git"
    command(["git", "remote", "set-url", "origin", public], checkout)
    command(["git", "config", f"url.{upstream.as_uri()}.insteadOf", public], checkout)
    tools = root / "tools"
    tools.mkdir()
    cargo = tools / "cargo"
    cargo.write_text("""#!/bin/sh
test "$*" = 'build --locked --release' || exit 10
mkdir -p target/release
printf '#!/bin/sh\necho blue-dragon 0.4.0-alpha.4\n' > target/release/blue-dragon
chmod +x target/release/blue-dragon
""")
    cargo.chmod(0o755)
    state = root / "state"
    env = dict(os.environ, PATH=f"{tools}:{os.environ['PATH']}", XDG_DATA_HOME=str(state))
    result = command(["bash", str(updater)], checkout, env)
    assert "blue-dragon 0.4.0-alpha.4" in result.stdout
    assert (checkout / "Cargo.lock").read_text() == "upstream tracked lock\n"
    backups = list(state.glob("blue-dragon/source-backups/*/Cargo.lock"))
    assert len(backups) == 1 and backups[0].read_text() == "user-generated-lock\n"
    # Never discard a tracked local edit, even if it prevents the upgrade.
    (checkout / "Cargo.toml").write_text("local development changes\n")
    result = command(["bash", str(updater)], checkout, env, ok=False)
    assert "Tracked local changes" in result.stderr
    assert (checkout / "Cargo.toml").read_text() == "local development changes\n"
print("Legacy source upgrade reproduced the checkout conflict, preserved the generated lockfile, reached main, kept --locked, and refused dirty tracked files")
