"""Exercise the shared page collector in Chromium against two local scripts."""
import functools
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import shutil
import tempfile
from threading import Thread
import unittest

from playwright.sync_api import sync_playwright


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


class BrowserReviewTests(unittest.TestCase):
    def test_script_identity_click_effect_and_control_flow(self):
        script = Path(__file__).resolve().parents[1] / "browser-extension/shared/page.js"
        with tempfile.TemporaryDirectory() as directory, sync_playwright() as playwright:
            root = Path(directory)
            (root / "index.html").write_text(
                '<h1>Closed</h1><button id="toggle">Toggle</button><button id="noop">Idle</button>'
                '<link rel="modulepreload" href="vendor.js"><script type="module" src="app.js"></script>'
            )
            (root / "vendor.js").write_text('export const label = "Open";')
            (root / "app.js").write_text(
                'import {label} from "./vendor.js";'
                'document.querySelector("#toggle").onclick = () => document.querySelector("h1").textContent = label;'
                'window.branchA = flag => { if (flag) throw Error("stopped"); return 42; };'
                'window.branchB = flag => { if (!flag) throw Error("stopped"); return 42; };'
            )
            handler = functools.partial(QuietHandler, directory=directory)
            server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
            self.addCleanup(server.server_close)
            thread = Thread(target=server.serve_forever, daemon=True)
            thread.start()
            self.addCleanup(thread.join)
            self.addCleanup(server.shutdown)
            browser = playwright.chromium.launch(executable_path=shutil.which("chromium"))
            try:
                page = browser.new_page()
                responses = {}
                page.on("response", lambda response: responses.update({response.url: response}))
                page.goto(f"http://127.0.0.1:{server.server_port}/index.html")
                page.add_script_tag(content=script.read_text())
                snapshot = page.evaluate("blueDragonPage('snapshot', {compact:true})")
                entries = [r["src"] for r in snapshot["script_tags"] if r["type"] == "module"]
                self.assertEqual(entries, [page.url.replace("index.html", "app.js")])
                self.assertEqual(responses[entries[0]].body(), (root / "app.js").read_bytes())
                change = page.evaluate("blueDragonPage('click', {selector:'#toggle'})")
                self.assertEqual(change["changed_fields"], ["heading"])
                noop = page.evaluate("blueDragonPage('click', {selector:'#noop'})")
                self.assertEqual(noop["effect"], "unverified")
                outcomes = page.evaluate("""[branchA, branchB].map(fn => [true, false].map(flag => {
                    try { return fn(flag); } catch (e) { return e.message; }
                }))""")
                self.assertEqual(outcomes, [["stopped", 42], [42, "stopped"]])
            finally:
                browser.close()


if __name__ == "__main__":
    unittest.main()
