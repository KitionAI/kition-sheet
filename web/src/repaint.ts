/**
 * The repaint the grid owes after the session's changes, when React took several of them in one
 * render.
 *
 * Why it was written: kit-editor's session (`@kition/sdk/editor`, `useKitDocument`) used to keep
 * the last refresh in one React state, `{ revision, damage }`: each refresh added 1 to `revision`
 * and REPLACED `damage` (null: repaint the whole grid; an area: repaint only it; the empty area
 * `{0,0,0,0}`: repaint nothing). When two refreshes landed before React rendered, the canvas saw
 * only the second one's area. A click on a checkbox makes two: the press ticks the box (the
 * revision moves: the whole grid) and the release, which changes nothing more, asks for the empty
 * area; so does an agent's read with `agentSkipsUnchangedPaint`. Taken together, the canvas
 * repainted nothing and the box stayed drawn as it was while the formula bar showed TRUE
 * (INSC-C53/C60 on a loaded WebKit runner).
 *
 * `repaintFor` puts that right from what the canvas was last handed: a change whose `revision`
 * moved by more than one stands for several refreshes, and it repaints the whole grid unless the
 * last one asked for nothing AND nothing the grid draws from moved meanwhile (the same document,
 * its revision and its `inspect()`: the rule kit-editor's `agentSkipsUnchangedPaint` itself trusts),
 * so an agent's reads taken together still paint nothing.
 *
 * No longer needed: since `@kition/sdk` 0.5.2-stage.15 (kition-sdk b8fcc7d and 1515603) the
 * editor itself merges the areas every refresh since the last committed change owes, so a change's
 * `damage` already covers all of them (the whole grid wins; the empty area adds nothing). On such a
 * build `repaintFor` stays correct: it only ever widens a multi-refresh change to the whole grid,
 * which repaints more, never less. It stays while the case regressions/issue-checkbox-paint-lost
 * runs its tests (`src/repaint.test.mjs`); removing both changes that case's steps.
 */

/** A document area (document pixels); x1 ≤ x0 or y1 ≤ y0 is the empty area: nothing to repaint. */
export interface Area {
  x0: number
  y0: number
  x1: number
  y1: number
}

/** kit-editor's refresh as the canvas takes it: a counter and what to repaint (null: everything). */
export interface Change {
  revision: number
  damage: Area | null
}

/** What the grid draws from, when the canvas was handed a change. */
export interface Drawn {
  /** The open document (a reopened workbook is another). */
  doc: unknown
  /** The engine's revision. */
  revision: number
  /** The engine's `inspect()`, as JSON. */
  inspected: string
}

/** What the canvas was last handed: the change's counter and what the grid drew from then. */
export interface Handed {
  change: number
  drawn: Drawn
}

export const isEmpty = (area: Area | null): boolean => area != null && (area.x1 <= area.x0 || area.y1 <= area.y0)

/** The change to hand the canvas, given what it was handed last (null: nothing yet). */
export function repaintFor(handed: Handed | null, change: Change, drawn: Drawn): Change {
  // One refresh (or the first): its own area is right.
  if (handed == null || change.revision <= handed.change + 1) return change
  const still = handed.drawn.doc === drawn.doc && handed.drawn.revision === drawn.revision && handed.drawn.inspected === drawn.inspected
  if (still && isEmpty(change.damage)) return change
  return change.damage === null ? change : { revision: change.revision, damage: null }
}
