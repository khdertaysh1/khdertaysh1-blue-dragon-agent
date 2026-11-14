# Real browser control

Blue Dragon 0.5 adds independently written browser control through a local CDP client or its own Firefox/Chromium extension. This drives an installed browser and executes page JavaScript; it is separate from `web_fetch`. No AlphaCode browser source or third-party bridge executable is included.

## Choose a connection

| System | Connection | Setup required |
|---|---|---|
| Windows x64 | `/browser launch` with Chrome/Edge, or extension | Installed browser; extension pairing if used |
| Linux x64 | `/browser launch` with Chromium/Chrome/Edge, or extension | Installed browser and working graphical session; headless is optional |
| macOS ARM64 | `/browser launch` with Chrome/Edge, or extension | Installed browser |
| Android / Termux ARM64 | Chrome CDP through authorized ADB forwarding | Android 11+ wireless debugging for phone-only pairing; Chrome running; no root |
| Firefox desktop | Own extension and `/browser start` | Firefox 140+; temporary developer loading or a signed extension |
| Firefox Android | Extension protocol and `/browser start` | Firefox 142+; a signed compatible add-on for regular installation; physical verification pending |

The CLI does not embed/download a browser. Safari and generic desktop control are not implemented. Desktop release builds are CI-tested on Windows/Linux/macOS, but real-browser integration runs on Linux. Android is cross-built, not physically tested. Extension archives are **unsigned developer builds**, not published or approved in browser stores. Stable Firefox's regular installation requires Mozilla signing; an unsigned XPI is not a one-click Android installation.

## Desktop: easiest CDP setup

Start the agent in your project:

```sh
blue-dragon --provider free tui
```

Inside its conversation:

```text
/browser launch
/browser status
```

Then ask, for example: “Open http://127.0.0.1:3000, inspect the page, fill the search box, click Search, and report the observed network requests and console.” Each browser action requires approval. `/browser launch headless` is available without a visible window. Browser detection checks PATH and common Windows/macOS Chrome/Edge installation paths. Use `/browser attach PORT` for a manually started local Chromium debugger. Chrome 136+ needs a separate `--user-data-dir`; do not expose the debugger to a network interface.

The launched browser uses a new temporary profile; it does not reuse personal cookies or tabs. Attached CDP sessions create a dedicated new tab and close only that tab on stop. When Blue Dragon launched the browser itself, stop also closes that browser process.

## Firefox / Chromium extension

Download `blue-dragon-browser-firefox.xpi` or `blue-dragon-browser-chromium.zip` from the same [release](https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/releases) as the executable. Checksums are in `SHA256SUMS.txt`.

For **desktop developer testing**, unpack the XPI/ZIP and load `manifest.json` from Firefox's `about:debugging` → This Firefox → Load Temporary Add-on. Chromium developer mode loads the unpacked directory via `chrome://extensions`; browsers may restrict developer extensions by policy. Firefox temporary extensions disappear on restart. This is not a signed store installation.

In Blue Dragon:

```text
/browser start
```

It prints a localhost URL and a private pairing token. Click the extension toolbar button, enter the port (default `9269`) and token, and save. `/browser status` should show `ready: true`. `/browser pair` shows the token again; `/browser stop` disconnects. A standalone transport process is available for diagnostics:

```sh
blue-dragon browser-bridge --port 9269
```

Keep pairing output private. The token is kept in local application data (`browser-pair.token`, Unix mode 0600) and extension local storage; it is not a provider API key and is never supplied to the model by the browser tool. If it leaks, stop the bridge, remove this token file, restart and pair again.

The extension asks for site permissions for browser inspection and screenshots. Its implementation only operates on its newly created managed tab and records that tab's traffic. Evaluation runs in the page's MAIN world, without extension storage/API privileges. No arbitrary remote JavaScript is imported. `web-ext` reports the expected dynamic-evaluation warning because an explicitly approved page console is a feature; store review/signing is still required.

## Android Chrome from Termux

Stock Chrome does not provide desktop-style extensions or accept desktop launch flags from Termux. Use Android's official authorized debugging connection instead. This path requires Android 11+ for phone-only wireless pairing; another authorized host can provide USB debugging for other versions.

1. In Android developer options, enable **Wireless debugging** on a trusted Wi-Fi connection. Open “Pair device with pairing code”.
2. In Termux install the official client package, pair using the pairing address/port and code shown by Android, then connect using the separate main debugging address/port:

```sh
pkg install android-tools -y
adb pair PHONE_IP:PAIRING_PORT
adb connect PHONE_IP:DEBUG_PORT
adb devices
```

3. Open Chrome on the phone. Forward its debugger socket locally:

```sh
adb forward tcp:9222 localabstract:chrome_devtools_remote
curl -fsS http://127.0.0.1:9222/json/version
blue-dragon --provider free tui
```

4. Inside Blue Dragon:

```text
/browser attach 9222
/browser status
```

Addresses and ports come from Android, not literal fixed values. Pairing and connection ports differ and may change. Device vendors can restrict same-phone ADB or debugging; if discovery/creation is unsupported, attachment fails explicitly. This procedure and device behavior still need physical-phone validation. No security settings are changed automatically. When finished: `/browser stop`, `adb forward --remove tcp:9222`, and disable wireless debugging if no longer needed.

## Available actions and boundaries

`browser` supports `status`, `open`, `navigate`, `snapshot`, `click`, `fill`, `evaluate`, `network`, `console`, `source`, `screenshot`, `close`. Manual test commands use the same approvals:

```text
/browser do {"action":"open","url":"http://127.0.0.1:3000"}
/browser do {"action":"snapshot"}
/browser do {"action":"network"}
/browser do {"action":"console"}
/browser do {"action":"screenshot"}
```

Snapshots include bounded visible text and selectors. Click/fill require one visible enabled control; iframe and closed-shadow-root automation is not provided. HTTP(S) URLs are accepted; credentials in URLs and privileged schemes are refused. Read-only mode denies browser interaction.

Network output is bounded request metadata, not a Burp interception proxy: cookies, authorization headers and request bodies are not captured by default. CDP `source` takes a previously observed response `request_id` and returns its bounded body; binary bodies can be base64. Extension `source` takes an observed `url` and fetches a **new unauthenticated copy**, not the original response. Extension console collection starts after first page interaction, whereas CDP subscribes before navigation. Screenshot JPEGs are saved locally; model vision upload is not implemented. Extension screenshots require the managed tab to be active.

Page content, console and errors are untrusted. Approving a browser action may send extracted website data to the configured model in the next request. The bridge listens only on 127.0.0.1, requires an extension Origin, a 256-bit token, protocol/capability negotiation and bounded messages. Reconnection never replays an action; a lost/expired reply has an unknown outcome, which must be inspected before retrying.

## Verification

`python scripts/build_browser_extension.py` builds both independent variants. `node scripts/smoke_browser.mjs` tests real Chromium CDP; `--extension` tests its extension; `--firefox` uses Firefox/geckodriver with a temporary add-on. CI installs pinned official test browsers. All pages are synthetic localhost fixtures: DOM, input/click, MAIN-world isolation, network, script source, console, screenshot and unmanaged-tab isolation. These are Linux browser tests, not evidence of physical Android or store approval.

Official setup references: [Chrome Android debugging](https://developer.chrome.com/docs/devtools/remote-debugging/), [Android authorized wireless debugging](https://developer.android.com/tools/adb#wireless), [Firefox extension signing](https://extensionworkshop.com/documentation/publish/signing-and-distribution-overview/), [Firefox Android development](https://extensionworkshop.com/documentation/develop/developing-extensions-for-firefox-for-android/).
