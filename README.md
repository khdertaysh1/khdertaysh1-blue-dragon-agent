# Restoration R2 trial download

Separate Android ARM64 trial from source 4de97e3c11785ef76cee9d147f907dcf6d50b95b. Complete source and Git bundle, build manifest, checksums, isolated launcher and test logs are inside the ZIP. Not the missing 393ffb0 source and not a public Release. Existing main/tags/assets remain unchanged.

Local scripted-provider and real PTY tests passed. Android cross-built for API26+, not run on a phone. Real Chromium was blocked by the execution environment; no live-model acceptance claimed. See RESTORATION_R2.md for remaining limits. This download branch is publicly accessible and contains no provider credentials or personal sessions.

Extract inside Termux HOME and run `bash blue-dragon-restoration-r2/run-private.sh`. Trial settings, sessions and workspace are separate from the original installation. Package version still reads 0.9.0-alpha.3; source identity is 4de97e3.
