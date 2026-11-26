"""Exercise the graphical client API against a local model fixture."""
import json
import subprocess
import time
from urllib.error import HTTPError
from urllib.request import Request, urlopen


def check_gui(cli, env, session):
    with subprocess.Popen([*cli, "--resume", session, "gui", "--no-open"],
                          env=env, stdout=subprocess.PIPE, text=True) as process:
        try:
            assert "Blue Dragon GUI" in process.stdout.readline()
            url, token = process.stdout.readline().strip().split("/#")
            with urlopen(url, timeout=5) as page:
                assert b'id="compose"' in page.read()
                assert "frame-ancestors 'none'" in page.headers["Content-Security-Policy"]

            def api(action, *, origin=url, key=token, **fields):
                request = Request(url + "/api", json.dumps(dict(action=action, **fields)).encode(),
                                  headers={"Authorization": "Bearer " + key, "Origin": origin})
                with urlopen(request, timeout=5) as response:
                    return json.load(response)

            for overrides in [{"origin": "http://example.invalid"}, {"key": "incorrect"}]:
                try:
                    api("state", **overrides)
                except HTTPError as err:
                    assert err.code == 403
                else:
                    raise AssertionError("Unauthorized API request succeeded")
            state = api("state")
            assert state["session"] == session and ["Dragon", "fixture answer"] in state["rows"]
            api("send", session=session, text="Give a short response")
            for _ in range(250):
                state = api("state")
                if not state["busy"]:
                    break
                time.sleep(0.04)
            assert not state["busy"] and state["rows"][-1] == ["Dragon", "fixture answer"], state
            assert any(row["id"] == session for row in api("sessions"))
        finally:
            process.terminate()
            process.wait(timeout=5)
