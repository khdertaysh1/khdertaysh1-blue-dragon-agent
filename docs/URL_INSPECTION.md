# Paste a URL and inspect it

Blue Dragon 0.5.0-alpha.2 accepts a URL directly:

```sh
blue-dragon 'https://example.com'
```

Equivalent: `blue-dragon inspect 'https://example.com'`. You can also paste just the URL into the existing chat or TUI. These paths prepare the browser and collect evidence before the model starts reasoning. No `/browser attach`, debugging port, pairing token, ADB, root, proot, Python or Node is needed for this local Chromium path.

## Termux first use

When Chromium is missing, the URL workflow invokes **only the official Termux package manager**, in order:

```text
pkg install -y x11-repo
pkg install -y chromium
```

The first download can be large and needs free storage/network access. It is a one-time dependency installation, not recompilation of Blue Dragon. Subsequent URL tasks reuse the installed package; a chat/TUI also reuses its running browser. The install log is kept in Blue Dragon's application-data directory as `browser-install.log`. A package/mirror error stops with that path; no incompatible desktop binary or third-party install script is substituted. The package manager has a 15-minute deadline per package; cancellation is checked while it runs.

The agent launches **Termux Chromium**, not the separate Chrome Android app. With a usable DISPLAY/Wayland session it opens a visible window; otherwise it uses real Chromium headless in the background. X11 is not required for background inspection. This does not automatically launch or configure the Termux:X11 Android application.

The official Termux Chromium package cannot provide the desktop Chromium sandbox. Only the dedicated Termux child is launched with `--no-sandbox` and a temporary profile; Android's Termux application boundary still applies. Existing Chrome/Firefox profiles are not reused. Desktop launches do not receive sandbox-disabling flags. This platform limitation is recorded in browser status.

**Physical Android verification is still pending.** CI validates the Android binary by cross-build; the first-use package ordering/reuse is simulated, while browser rendering/inspection is exercised using real Linux Chromium. Device/vendor/package compatibility is not guaranteed by those tests.

## What runs automatically

1. Validate an HTTP(S) URL without embedded credentials. Read-only mode refuses browser preparation and navigation.
2. Reuse an explicitly connected browser, or detect/start a dedicated installed Chromium/Chrome/Edge. Termux installs its package when needed; desktop systems report a missing browser without invoking an OS installer.
3. Open the URL and wait for its document's DOM-ready lifecycle, bounded to 15 seconds. A slow/dynamic application may still be loading after that deadline.
4. Read DOM text, visible controls, form structure (no field values), script URLs and bounded inline scripts. If the landing page has no form, follow **at most one same-origin registration/login link**.
5. Collect observed request metadata, selected security headers (CDP), and console output. Inspect up to four observed JavaScript responses, under a combined 128KB source budget. Source/sink matches are static indicators, not a verified data flow.
6. If inspected scripts expose `location.hash`, `location.search` or `URLSearchParams`, and the page remains on the explicitly requested origin, run at most two fixed benign execution checks: a fragment and one already-present search-like query parameter (`q`, `query`, `search`, `s`, `term`, `keyword`). The proof payload only writes a random property on that page’s window; it reads no storage/cookies and sends no data elsewhere. Restore the page URL afterwards. Fragment proof is client-side; query execution needs further tracing to distinguish client from server reflection. Missing proof is not evidence of safety.
7. Send bounded, explicitly untrusted evidence to the configured model for analysis. The session/action report records the automatic review. Provider failures preserve that evidence and never cause completed browser actions to replay automatically.

During this one URL task, additional browser snapshot/network/console/observed-source/screenshot reads do not ask for individual confirmations. The grant ends when the turn succeeds, fails or is cancelled. Navigation beyond the initial review, clicks, filling fields and arbitrary page JavaScript keep the existing per-action approval. This URL workflow does not create accounts or submit forms. The only automatic execution payloads are the fixed, bounded window-property canaries above; arbitrary model-provided evaluation still requires approval. Page content is not permission to run commands, install packages or access unrelated workspace files.

The result is an automatic **initial browser-based review**, not a complete crawler or a guaranteed vulnerability scanner. DOM XSS needs observed execution and a reproducible source-to-sink path before it can be reported as confirmed. The canary tests cover only the two documented URL inputs; they are not a comprehensive payload suite. Cross-origin login pages are not automatically followed. Captchas, login walls, failed navigation and missing response bodies are reported as limits. No promise of bypassing browser/site access controls is made.

## Desktop and advanced connections

The same URL command works with detected Chrome/Chromium/Edge on Windows, Linux and macOS. `BLUE_DRAGON_BROWSER_BIN` selects an explicit installed executable. Firefox still uses its paired extension. Stock Android Chrome through authorized ADB remains an optional advanced connection; it is not required for the automatic Termux Chromium workflow. See [BROWSER.md](BROWSER.md).

## Verification

`scripts/smoke_url_inspection.mjs` exercises a real Chromium and a synthetic local model endpoint: bare URL command, chat paste, DOM/form metadata, JavaScript indicators, console/network/security headers, no approval responses, benign DOM canary execution versus safe text rendering, no form submission, grant expiry, read-only refusal and simulated Termux package installation/reuse. No live model quota or package repository is consumed by CI.

Official package source: [Termux Chromium](https://github.com/termux/termux-packages/tree/master/x11-packages/chromium). Package availability and download sizes are controlled by Termux.
