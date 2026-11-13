# Blue Dragon visual identity

The identity combines **an electric-blue dragon** (guided action) with **circuit nodes** (context, tools, verification). It is an original visual language rather than a recreation of another project's artwork.

## Asset inventory

| Asset | Format | Purpose |
| --- | --- | --- |
| `assets/blue-dragon-hero.webp` | Optimized WebP | Main README hero artwork, composed from the project's original generated dragon-brain concept |
| `assets/dragon-head.webp` | WebP with alpha | Corrected companion dragon head |
| `assets/dragon-mark.svg` | SVG | Compact brand logo using the same corrected head |
| `assets/dragon-mind.svg` | SVG, decorative SMIL/CSS animation | Explains the inspection and approval workflow |
| `assets/tui-preview.svg` | SVG | **Illustrative concept** for PR #1, not a live screenshot |
| `assets/neural-console-preview.gif` | Animated GIF | Multi-frame illustrative terminal walkthrough that plays in ordinary GitHub README image viewers; **not a real captured session** |

## Dragon identity
The current character is an intact scaled blue dragon: no exposed brain, cranial window, cutaway, chip or anatomical diagram. The hero, workflow and compact mark share this character. Preserve the original cyber composition, travelling signals, rotating orbit and pulses.
The workflow SVG and logo embed a small PNG of the companion head, with no remote dependencies or JavaScript.

## Motion and interactivity

README files on GitHub do **not** execute arbitrary client JavaScript. To ensure motion still plays inside GitHub, the terminal walkthrough is an optimized animated GIF rather than an embedded web application. The README uses native expandable `<details>` sections, a Mermaid diagram and SVG motion where the browser/GitHub renderer supports it. Every SVG remains readable without animation. GitHub may sanitize or cache remote media, so animation is not guaranteed to play in every viewer.

The banners and diagrams represent the intended engineering workflow; they do **not** imply that pending features have shipped. In particular, the full-screen TUI remains in a draft PR until reviewed and merged.

## Palette

- Background: `#061025` / `#091b39`
- Cyan: `#09dcff`
- Blue: `#3984ff`
- Violet: `#ad5cff`
- Text: `#edf9ff`
- Muted: `#98afcc`

The source image artwork and SVGs belong to Blue Dragon branding. Avoid using the AI-generated concept as a false screenshot of program execution.

## Accessibility and maintenance

All README images include alt text. SVG diagrams provide `<title>` and `<desc>`, and decorative animations include reduced-motion fallbacks. Prefer static images over animations when showing real CLI output; real screenshots should identify their platform and release.

## Corrected artwork — 13 November 2026
The raster hero and companion mascot were redrawn with the built-in image generator, then exported as WebP. Prompt direction: recognizable right-facing blue dragon; swept-back horns, distinct snout/eye/jaw and narrow scaled neck; a small neural brain fully enclosed in the upper rear cranium, behind the eye and above the jaw hinge. No brain in the throat or neck, no disconnected brain mass. Navy/cyan palette, restrained glow, consistent character.

The generated hero is assets/blue-dragon-hero.webp; the transparent companion is assets/dragon-head.webp. The native workflow and mark SVGs embed an optimized PNG of that companion, replacing the ambiguous abstract crest. No third-party logo or source artwork is used. The SVG geometry/text remain editable; pulse motion is decorative.

## Motion restoration
The original neural-workflow layout is retained: all six travelling SVG signal animations, rotating orbit, grid, glow filters and original panels. Only the ambiguous central dragon is replaced. The hero retains its original cyber frames, badges, icon connections and typography, with the corrected cranial brain. The previous minimal composition is superseded. Built-in image editing used both the original hero and the corrected dragon as references; surrounding identity was preserved.

## Final user revision: intact dragon
The cranial cutaway was removed from both raster assets with built-in image editing. Prompt: remove only the brain and its entire window/frame, seamlessly fill with intact blue scales, preserve pose, cyber composition, typography, icons, lighting and transparency. Current assets are assets/blue-dragon-hero.webp and assets/dragon-head.webp; both native SVGs embed the intact head. Earlier cutaway descriptions above refer to superseded artwork.
