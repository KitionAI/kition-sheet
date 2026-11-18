# Hardware parity

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft Excel (Microsoft 365), Excel for Mac 16.113.4

The hardware Excel uses, per platform, against Kition Sheet. Part of
[target-app-parity.md](target-app-parity.md). **Hardware: ~40% ready, 25–40 h** (estimated).

| Hardware feature | Excel | Kition Sheet (the editor in `web/` and the engine) | Gap and estimate |
|---|---|---|---|
| Multi-core recalculation | Multi-threaded recalculation on all cores (Windows and Mac), configurable thread count | Single-threaded everywhere (no threading in `crates/engine/calc`) | Parallel recalc of independent dependency chains on native builds; WASM stays single-threaded or uses workers later: 12–18 h |
| Pen and touch (Draw tab) | Pen pressure and tilt, eraser end, touch mode, Ink to Shape, ink replay | Pen, highlighter, eraser, Ink to Shape with mouse/pen input; **no pressure or tilt**, no touch mode | Pressure-sensitive strokes; touch-friendly ribbon spacing: 4–6 h |
| Trackpad and mouse | Smooth scrolling, pinch zoom, horizontal scroll, Magic Mouse | Wheel and trackpad scrolling on both axes, and zoom with Ctrl or ⌘ and the wheel, in kit-editor's document canvas | Verify pinch zoom on each platform: 1–2 h |
| High-DPI and display scaling | yes | yes (kit-editor's canvas draws at the page's device pixel ratio) | — |
| Accessibility hardware (screen readers, braille displays, switch control) | Narrator, JAWS, NVDA, VoiceOver | The ribbon and dialogs are kit-editor's HTML controls; the grid is drawn on a canvas and its cells are not exposed as an accessible tree | Counted under UI ([ui-parity.md](ui-parity.md)) |
| Printers | System print with driver options | No print command in the editor (`file.print` is left to the host); PDF export through the host | Counted under print |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
