#!/usr/bin/env python3
"""Drive the packaged save operation through a loopback CDP protocol fixture."""
import argparse
import asyncio
import base64
import hashlib
import json
import os
import subprocess
from pathlib import Path
import tempfile
import threading

from aiohttp import web


PAGE = "http://127.0.0.1:7770/"
BODY = ("/* بيانات اختبار */\n" * 14000).encode() + b"const x = 1;\n"
TAIL = b"const finalMarker = 42;"
LARGE = b"// " + b"x" * (2 * 1024 * 1024) + b"\n" + TAIL


def test_save(binary):
    retrieved = []
    browser_methods = []
    model_packets = []
    phase = {"mode": "idle", "requests": 0}
    websocket_port = []
    started = threading.Event()
    loop = asyncio.new_event_loop()

    async def new(_request):
        return web.json_response({
            "id": "fixture",
            "webSocketDebuggerUrl": f"ws://127.0.0.1:{websocket_port[0]}/devtools",
        })

    async def devtools(request):
        socket = web.WebSocketResponse()
        await socket.prepare(request)
        async for message in socket:
            data = json.loads(message.data)
            method = data["method"]
            browser_methods.append(method)
            result = {}
            if method == "Network.enable":
                for request_id, url, mime in [
                    ("script", PAGE + "test.js", "text/javascript"),
                    ("vendor", PAGE + "vendor.js", "text/javascript"),
                    ("evicted", PAGE + "old.js", "application/javascript"),
                    ("html", PAGE, "text/html"),
                    ("other", "http://127.0.0.1:7771/test.js", "text/javascript"),
                ]:
                    await socket.send_json({"method": "Network.responseReceived", "params": {
                        "requestId": request_id,
                        "response": {"url": url, "mimeType": mime, "status": 200},
                    }})
            elif method == "Runtime.evaluate":
                result = {"result": {"value": PAGE}}
            elif method == "Network.getResponseBody":
                retrieved.append(data["params"]["requestId"])
                if data["params"]["requestId"] == "evicted":
                    await socket.send_json({"id": data["id"], "error": {"code": -32000, "message": "Request content was evicted from inspector cache"}})
                    continue
                if data["params"]["requestId"] == "vendor":
                    result = {"body": "/* retained vendor */", "base64Encoded": False}
                else:
                    result = {"body": base64.b64encode(BODY).decode(), "base64Encoded": True}
            await socket.send_json({"id": data["id"], "result": result})
        return socket

    async def completion(request):
        packet = await request.json()
        model_packets.append(packet)
        phase["requests"] += 1
        mode = phase["mode"]
        if mode == "save":
            raise web.HTTPInternalServerError(text="Unexpected model request during direct browser save")
        if mode == "blocked":
            name, args = "browser", {"action": "open", "url": PAGE}
        elif mode == "capture-report":
            count = phase["requests"]
            if count == 1:
                name, args = "discover_tools", {"names": ["browser", "read_file_window"]}
            elif count == 2:
                name, args = "browser", {"action": "save_response", "request_id": "script", "path": "capture-report.js"}
            elif count == 3:
                name, args = "read_file_window", {"path": "capture-report.js", "byte_offset": len(BODY)-13, "byte_count": 13}
            elif count == 4:
                name, args = "browser", {"action": "network"}
            else:
                notes = next(json.loads(m["content"].split("\n", 1)[1])
                             for m in packet["messages"] if "current_turn_saved_bodies" in (m.get("content") or "")
                             and (m.get("content") or "").startswith("Blue Dragon working notes"))
                receipt = notes["current_turn_saved_bodies"][0]
                assert receipt["source_url"] == PAGE + "test.js" and receipt["http_status"] == 200
                assert receipt["later_read"]["excerpt"] == "const x = 1;\n"
                if count == 5:
                    response = "No script body was available."
                else:
                    assert count == 6 and not packet.get("tools"), packet
                    response = "const x = 1; declares x. " + receipt["later_read"]["evidence_id"]
                return web.json_response({"choices": [{"message": {"role": "assistant", "content": response},
                                                       "finish_reason": "stop"}]})
        elif mode == "good" and phase["requests"] == 1:
            name, args = "read_file_window", {
                "path": "saved.js", "byte_offset": len(BODY) - len(b"const x = 1;\n"),
                "byte_count": len(b"const x = 1;\n"),
            }
        elif mode == "good" and phase["requests"] == 2:
            result = json.loads(next(m["content"] for m in reversed(packet["messages"]) if m["role"] == "tool"))
            assert result["content"] == "const x = 1;\n"
            assert result["source_sha256"] == hashlib.sha256(BODY).hexdigest()
            return web.json_response({"choices": [{"message": {"role": "assistant",
                "content": "The verified window declares x with value 1."}, "finish_reason": "stop"}]})
        elif mode == "resume-review":
            checkpoint = next(json.loads(m["content"].split("citations:\n", 1)[1])
                              for m in packet["messages"]
                              if "verified_checkpoint_windows" in (m.get("content") or ""))
            result = checkpoint["verified_checkpoint_windows"][-1]
            assert not any(m["role"] == "tool" for m in packet["messages"])
            report = {"summary": "Restored review", "limits": "Opening bytes only",
                      "observations": [{"evidence_id": result["evidence_id"], "quote": "// xxx",
                                        "note": "Quote from the historical native window."}]}
            return web.json_response({"choices": [{"message": {"role": "assistant",
                "content": json.dumps(report)}, "finish_reason": "stop"}]})
        elif mode in {"review", "large", "bad-quote", "repair"}:
            count = phase["requests"]
            expected = {"file_info", "read_file_window", "search_file"}
            if mode in {"bad-quote", "repair"} and count == 2:
                expected = set()
            assert {t["function"]["name"] for t in packet.get("tools", [])} == expected, packet
            result = json.loads(next(m["content"] for m in reversed(packet["messages"]) if m["role"] == "tool"))
            if mode == "large" and count == 1:
                assert result["end_byte_offset"] == len(LARGE)
                assert result["content"].endswith(TAIL.decode())
                # A malformed local search must never run; the next call can still read.
                name, args = "search_file", {"path": "large.js", "query": 99}
            elif mode == "large" and count == 2:
                assert result["execution_status"] == "not_executed", result
                name, args = "read_file_window", {"path": "large.js", "byte_offset": len(LARGE) - len(TAIL),
                                                 "byte_count": len(TAIL)}
            else:
                if mode == "large":
                    assert result["content"] == TAIL.decode()
                    assert result["source_sha256"] == hashlib.sha256(LARGE).hexdigest()
                quote = {"review": "/* بيانات اختبار */", "large": TAIL.decode(),
                         "bad-quote": "FABRICATED_EXCERPT",
                         "repair": "x" * 141 if count == 1 else "// xxx"}[mode]
                report = {"summary": "Test fixture review", "limits": "Only shown windows observed",
                          "observations": [{"evidence_id": result["evidence_id"], "quote": quote,
                                            "note": "Exact fixture text."}]}
                return web.json_response({"choices": [{"message": {"role": "assistant",
                    "content": json.dumps(report)}, "finish_reason": "stop"}]})
        else:
            raise web.HTTPInternalServerError(text="Unexpected continuation model request")
        message = {"role":"assistant","content":None,"tool_calls":[{"id":f"{mode}-fixture-{phase['requests']}",
            "type":"function","function":{"name":name,"arguments":json.dumps(args)}}]}
        return web.json_response({"choices":[{"message":message,"finish_reason":"tool_calls"}]})

    async def setup():
        app = web.Application()
        app.router.add_put("/json/new", new)
        app.router.add_get("/devtools", devtools)
        app.router.add_post("/v1/chat/completions", completion)
        runner = web.AppRunner(app)
        await runner.setup()
        site = web.TCPSite(runner, "127.0.0.1", 0)
        await site.start()
        websocket_port.append(runner.addresses[0][1])
        started.set()
        return runner

    def serve():
        runner = loop.run_until_complete(setup())
        try:
            loop.run_forever()
        finally:
            loop.run_until_complete(runner.cleanup())
            loop.close()

    thread = threading.Thread(target=serve, daemon=True)
    thread.start()
    try:
        assert started.wait(5)
        with tempfile.TemporaryDirectory(prefix="bd-cdp-save-") as tmp:
            root = Path(tmp)
            work = root / "work"
            work.mkdir()

            def save(request_id, path, url=None):
                args = {"action": "save_response", "request_id": request_id, "path": path}
                if url is not None:
                    args["url"] = url
                return "/browser do " + json.dumps(args)

            commands = [
                f"/browser attach {websocket_port[0]}",
                '/browser do ' + json.dumps({"action": "source", "url": PAGE + "vendor.js"}), "y",
                save("script", "cancelled.js"), "n",
                save("vendor", "wrong.js", PAGE + "test.js"),
                save("evicted", "missing.js"), "y",
                save("evicted", "missing-again.js"),
                save("script", "saved.js", PAGE + "test.js"), "y",
                save("script", "saved.js"),
                save("script", "../outside.js"),
                save("other", "other.js"),
                save("html", "html.js"),
                "/exit",
            ]
            env = {"HOME": tmp, "BLUE_DRAGON_HOME": str(root / "state"),
                   "PATH": os.environ["PATH"], "NO_COLOR": "1", "TERM": "dumb"}
            endpoint = f"http://127.0.0.1:{websocket_port[0]}/v1"
            command = [str(binary), "--root", str(work), "--base-url", endpoint,
                       "--model", "fixture", "--no-stream", "--no-rotation"]

            def chat(lines, *, resume=False):
                argv = command + (["--resume", "latest"] if resume else []) + ["chat"]
                run = subprocess.run(argv, input="\n".join(lines) + "\n", env=env,
                                     text=True, capture_output=True, check=True, timeout=30)
                return run.stdout + run.stderr

            phase.update(mode="save", requests=0)
            output = chat(commands)
            assert "CDP no longer retains this response body" in output
            assert retrieved.count("evicted") == 1
            assert not model_packets
            assert sorted(p.name for p in work.iterdir() if not p.name.startswith(".")) == ["saved.js"], output
            assert (work / "saved.js").read_bytes() == BODY
            assert not (root / "outside.js").exists()
            assert "Destination already exists" in output
            assert "current page origin" in output
            assert "Only successful JavaScript responses" in output
            digest = hashlib.sha256(BODY).hexdigest()
            assert f'"sha256": "{digest}"' in output
            assert "does not match the expected script URL" in output, output
            assert "CDP -32000): Request content was evicted from inspector cache" in output, output
            assert "retained vendor" in output
            assert retrieved == ["vendor", "evicted", "script"], retrieved
            methods_after_save = list(browser_methods)

            phase.update(mode="blocked", requests=0)
            forbidden = chat(["/resume-file latest", "/exit"], resume=True)
            assert "None of the returned actions were executed" in forbidden, forbidden
            assert phase["requests"] == 1
            assert browser_methods == methods_after_save

            phase.update(mode="good", requests=0)
            continued = chat(["/saved-files", "/resume-file latest Summarize the final declaration.", "/exit"], resume=True)
            assert "Verified saved file" in continued, continued
            assert "browser_source" in continued and PAGE.rstrip("/") in continued, continued
            assert "Verified byte windows" in continued, continued
            assert "The verified window declares x with value 1." in continued, continued
            assert phase["requests"] == 2, phase
            assert browser_methods == methods_after_save
            for packet in model_packets:
                assert {t["function"]["name"] for t in packet["tools"]} == {"file_info", "read_file_window", "search_file"}
                assert "direct browser command" not in json.dumps(packet["messages"])

            (work / "saved.js").write_bytes(b"!" + BODY[1:])
            phase.update(mode="tampered", requests=0)
            changed = chat(["/resume-file latest", "/exit"], resume=True)
            assert "no longer matches its recorded size or SHA-256" in changed, changed
            assert phase["requests"] == 0
            assert browser_methods == methods_after_save

            phase.update(mode="save", requests=0)
            denied = chat([f"/browser attach {websocket_port[0]}", "/review-response script denied.js", "n", "/exit"])
            assert "Review stopped before model request" in denied and not (work / "denied.js").exists(), denied
            assert phase["requests"] == 0

            phase.update(mode="review", requests=0)
            reviewed = chat([f"/browser attach {websocket_port[0]}", f"/review-response {PAGE}test.js reviewed.js", "y", "/exit"])
            assert "Summary (model): Test fixture review" in reviewed, reviewed
            assert "Byte-window coverage:" in reviewed and phase["requests"] == 1, reviewed
            assert (work / "reviewed.js").read_bytes() == BODY
            assert retrieved == ["vendor", "evicted", "script", "script"], retrieved

            (work / "large.js").write_bytes(LARGE)
            phase.update(mode="large", requests=0)
            large = chat(["/review-file large.js -- Read the last declaration.", "/exit"])
            assert f"[{len(LARGE)-len(TAIL)}..{len(LARGE)} bytes]" in large, large
            assert f"2048/{len(LARGE)} unique bytes" in large, large
            assert "First unread byte: 0." in large, large
            assert "Search did not execute" in large, large
            assert phase["requests"] == 3 and (work / "large.js").read_bytes() == LARGE, large

            phase.update(mode="repair", requests=0)
            repaired = chat(["/review-file large.js", "/exit"])
            assert phase["requests"] == 2
            assert "observations[0].quote exceeds 140 characters" in repaired, repaired
            assert "Summary (model): Test fixture review" in repaired, repaired

            phase.update(mode="bad-quote", requests=0)
            invalid = chat(["/review-file large.js", "/exit"])
            assert phase["requests"] == 2
            assert "Quoted text was not present" in invalid and "FABRICATED_EXCERPT" not in invalid, invalid

            phase.update(mode="resume-review", requests=0)
            (work / "large.js").write_bytes(b"!" + LARGE[1:])
            changed = chat(["/resume-review", "/exit"], resume=True)
            assert "File changed since the review checkpoint" in changed, changed
            assert phase["requests"] == 0
            (work / "large.js").write_bytes(LARGE)
            continued = chat(["كمّل المراجعة", "/resume-review", "/exit"], resume=True)
            assert "Resuming with 1 verified byte windows" in continued, continued
            assert "Summary (model): Restored review" in continued, continued
            assert "No unfinished file review" in continued and phase["requests"] == 1, continued

            phase.update(mode="save", requests=0)
            with (work / "oversize.js").open("wb") as oversized:
                oversized.truncate(8 * 1024 * 1024 + 1)
            too_large = chat(["/review-file oversize.js", "/exit"])
            assert "exceeds" in too_large and phase["requests"] == 0, too_large

            phase.update(mode="capture-report", requests=0)
            report = chat([f"/browser attach {websocket_port[0]}",
                           "Save the retained script response with request ID script as capture-report.js. Read its last line and report it with the evidence ID. Do not reload.",
                           "y", "y", "/exit"])
            assert "const x = 1; declares x. E" in report and phase["requests"] == 6, report
            assert (work / "capture-report.js").read_bytes() == BODY
            return {"passed": True, "transport": "loopback CDP protocol fixture",
                    "bytes": len(BODY), "sha256": digest, "body_reads": len(retrieved),
                    "denial_no_file": True, "overwrite_blocked": True,
                    "traversal_blocked": True, "cross_origin_blocked": True,
                    "non_javascript_blocked": True, "cold_restart_resume": True,
                    "browser_calls_during_resume": 0, "same_size_tampering_blocked": True,
                    "provider_calls_for_tampered_file": 0, "unadvertised_browser_call_blocked": True,
                    "review_denial_stops_before_model": True, "browser_save_and_review": True,
                    "large_file_review_bytes": len(LARGE), "verified_tail_quote": True,
                    "invented_quote_rejected": True, "oversize_file_rejected": True,
                    "report_repair_without_tools": True, "cold_restart_review": True,
                    "changed_checkpoint_blocked_before_provider": True}
    finally:
        loop.call_soon_threadsafe(loop.stop)
        thread.join(timeout=6)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    print(json.dumps(test_save(args.binary.resolve()), indent=2))
