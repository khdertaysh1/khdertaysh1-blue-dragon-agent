# Abyss identity integration

The selected **second** identity is the full-wing geometric dragon, not the small first emblem and not the later generated flying dragons. The banner retains the selected emblem, cyan BLUE, white DRAGON and deep-navy background.

## README

`assets/abyss-banner.svg` is static. `assets/abyss-dragon.gif` is a real animated image with orbit/wing movement, not JavaScript. `assets/abyss-dragon.svg` is the motion-free alternative. GitHub README does not run the interactive Site controls or custom JavaScript. The GIF is decorative, never evidence of an agent action or vulnerability. GIF motion cannot be paused by an embedded README button; use the linked still image.

## Termux and other terminals

Ordinary text terminals cannot render this SVG/GIF dragon reliably. The TUI therefore uses the same navy/cyan/white identity and split-color wordmark, not a claim that a bitmap mascot is running in Termux. Prompt, transcript and approval controls retain their space, including narrow phone layouts.

Decorative header energy is **off by default** to avoid distraction and extra phone motion. Opt in with:

```sh
BLUE_DRAGON_MOTION=1 blue-dragon --provider free tui
```

Omit that variable, set it to `0`, or set `BLUE_DRAGON_REDUCED_MOTION=1` to disable it. `NO_COLOR` also suppresses decorative motion (this does not introduce a complete monochrome theme). Energy changes only while the agent is busy, is hidden in compact headers and freezes at approval prompts. It is not a progress percentage. Existing terminal redraw cadence is unchanged.

## Validation limits

Ratatui TestBackend tests cover the split wordmark, disabled/busy/approval motion states and existing small-screen/approval rendering. An actual Android/Termux device still requires a device smoke test. This change does not alter model, browser, tools or approval policy.

To reproduce the GIF, install the development-only `cairosvg` and `pillow` Python packages, then run `python scripts/render_abyss_identity.py`. The source SVG is the selected second-scene vector artwork; the renderer applies 24 deterministic ring/wing transforms. No runtime dependency is added to the Rust application.
