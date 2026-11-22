# Blue Dragon R6 — Android ARM64 / Termux

R6 restores the blue Abyss terminal UI as the default for interactive use. The plain command line is available with `run-private.sh chat`.

- Native tool results distinguish session-only browser and HTTP evidence from local files.
- After the provider reaches its response limit following successful actions, R6 may request one compact summary. Tools are disabled during this request and completed actions are not replayed.
- Reports are instructed to distinguish current browser observations, static references and earlier evidence.

[Download the Termux package](./blue-dragon-r6-android-arm64.zip)

```bash
pkg install curl unzip
curl -fL 'https://raw.githubusercontent.com/khdertaysh1/khdertaysh1-blue-dragon-agent/refs/heads/releases/blue-dragon-r6/releases/blue-dragon-r6/blue-dragon-r6-android-arm64.zip' -o "$HOME/blue-dragon-r6-android-arm64.zip"
printf '%s  %s\n' '95a4ab82684ec382b70bd78d9ae4188e997996bb3bb4bdcee6ca11d82a9ef0a5' "$HOME/blue-dragon-r6-android-arm64.zip" | sha256sum -c - \
  && unzip -oq "$HOME/blue-dragon-r6-android-arm64.zip" -d "$HOME/blue-dragon-trials" \
  && bash "$HOME/blue-dragon-trials/blue-dragon-r6/run-private.sh"
```

Tested source revision: `d592e43595537fa31152c77e13613b6d75564a25`, based on the separate R5 recovery line. The ZIP contains the matching source tarball, Git bundle, manifest and test logs. [Review the R5 → R6 patch](./r5-to-r6.patch).

Validation: 337 Rust tests, 36 scripted local acceptance cases (including real PTY cases), Clippy, and an API 26 Android ARM64 cross-build. The finished binary has not yet been run on a physical Android device or tested with a live model. Installation uses an isolated R6 workspace and state directory.
