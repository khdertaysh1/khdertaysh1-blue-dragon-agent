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

1. Validate a bounded HTTP(S) URL without embedded credentials. If no scope was configured, create a persisted read-only scope for the requested origin/path; widening it requires a user `/scope` command. Read-only mode refuses browser preparation and navigation.
2. Reuse an explicitly connected browser, or detect/start a dedicated installed Chromium/Chrome/Edge. Termux installs its package when needed; desktop systems report a missing browser without invoking an OS installer.
3. Open the URL and wait for its document's DOM-ready lifecycle, bounded to 15 seconds. Then record up to three seconds of body visibility/text stability. This is a measurable DOM criterion, not proof React finished. Use `browser wait_for` with an application-specific selector/text for stronger readiness criteria.
4. Read DOM text, visible controls, form structure (no field values), script URLs and bounded inline scripts. If the landing page has no form, follow **at most one same-origin registration/login link inside the configured path scope**.
5. Collect observed request metadata, selected security headers (CDP), and console output. Inspect up to four observed JavaScript responses, under a combined 128KB source budget. Source/sink matches are static indicators, not a verified data flow.
6. Only when the user has granted `interact` (or `mutate`) scope, and inspected scripts expose `location.hash`, `location.search` or `URLSearchParams`, and the page remains on the explicitly requested origin, run at most two fixed benign execution checks: a fragment and one already-present search-like query parameter (`q`, `query`, `search`, `s`, `term`, `keyword`). A clean baseline first verifies that the random property is absent. The proof payload only writes a random property on that page’s window; it reads no storage/cookies and sends no data elsewhere. Restore the page URL afterwards. Fragment proof is client-side; query execution needs further tracing to distinguish client from server reflection. Missing proof is not evidence of safety.
7. Send bounded, explicitly untrusted evidence to the configured model for analysis. The session/action report records the automatic review. Provider failures preserve that evidence and never cause completed browser actions to replay automatically.

During the URL turn, additional read actions reuse the scoped inspection grant. After the turn, ordinary approval rules apply. Automatic inspection collects passive evidence.



## Desktop and advanced connections

The same URL command works with detected Chrome/Chromium/Edge on Windows, Linux and macOS. `BLUE_DRAGON_BROWSER_BIN` selects an explicit installed executable. Firefox still uses its paired extension. Stock Android Chrome through authorized ADB remains an optional advanced connection; it is not required for the automatic Termux Chromium workflow. See [BROWSER.md](BROWSER.md).

## Verification

The browser smoke test covers passive evidence, consent for browser setup, and expiry of the inspection grant.

Official package source: [Termux Chromium](https://github.com/termux/termux-packages/tree/master/x11-packages/chromium). Package availability and download sizes are controlled by Termux.
