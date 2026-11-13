# Blue Dragon visual identity

The identity combines **an electric-blue dragon** (guided action) with a **neural brain and circuit nodes** (context, tools, verification). It is an original visual language rather than a recreation of another project's artwork.

## Asset inventory

| Asset | Format | Purpose |
| --- | --- | --- |
| `assets/blue-dragon-hero.webp` | Optimized WebP | Main README hero artwork, composed from the project's original generated dragon-brain concept |
| `assets/dragon-mark.svg` | SVG | Compact brand logo |
| `assets/dragon-mind.svg` | SVG, decorative SMIL/CSS animation | Explains the inspection and approval workflow |
| `assets/tui-preview.svg` | SVG | **Illustrative concept** for PR #1, not a live screenshot |

## Motion and interactivity

README files on GitHub do **not** execute arbitrary client JavaScript. The README uses native expandable `<details>` sections, a Mermaid diagram and SVG motion where the browser/GitHub renderer supports it. Every SVG remains readable without animation. GitHub may sanitize or cache remote media, so animation is not guaranteed to play in every viewer.

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
