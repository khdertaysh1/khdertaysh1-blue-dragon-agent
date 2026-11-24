"""Compare a local JS control-flow oracle with Blue Dragon's structured summary."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, help="Test the configured model using two local fixture files")
    args = parser.parse_args()
    binary = args.binary.resolve() if args.binary else None
    for condition in ("flag", "!flag"):
        code = f'function decide(flag) {{ if ({condition}) throw Error("stopped"); return 42; }}'
        runner = """
            const result = [true, false].map(flag => {
                try { return String(decide(flag)); } catch (error) { return "throw"; }
            });
            console.log(JSON.stringify(result));
        """
        expected = json.loads(subprocess.check_output(["node", "-e", code + runner], text=True))
        summary = f"true={expected[0]}, false={expected[1]}"
        if binary:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "branch.js").write_text(code, encoding="utf-8")
                task = ("What happens for true and false? In the summary field write exactly "
                        "true=OUTCOME, false=OUTCOME, replacing each OUTCOME with 42 or throw.")
                result = subprocess.run(
                    [str(binary), "--root", str(root), "--read-only", "--no-stream", "chat"],
                    input=f"/review-file branch.js -- {task}\n/exit\n",
                    env={**os.environ, "BLUE_DRAGON_HOME": str(root / "state"), "NO_COLOR": "1"},
                    capture_output=True, text=True, check=True, timeout=180,
                )
                summaries = re.findall(r"Summary \(model\): ([^\r\n]+)", result.stdout)
                if summaries != [summary]:
                    raise SystemExit(f"Expected {summary!r}, observed {summaries!r}")
        print(f"{condition}: {summary} ({'agent matched' if binary else 'oracle only'})")


if __name__ == "__main__":
    main()
