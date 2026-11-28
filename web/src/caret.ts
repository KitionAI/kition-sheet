/**
 * The caret of the cell being typed, drawn over the rendered grid and blinking (plan stage 5,
 * walkthrough D16). The engine lays the cell's text out and draws it in the document's pixels; it
 * leaves the caret out of them and says where it is (`inspect().caretRect`, document points) and
 * in which colour (`caretColor`). The editor draws that line over the cached frame, so a blink
 * repaints the canvas from the frame it has and never asks the engine to draw the cells again.
 */
import { useMemo } from 'react'
import type { RasterDecoration } from '@kition/sdk/editor'

/** On for half the period, off for the other half; on again at once after every change. */
const BLINK_MS = 1060

/** Both decorations, `first` under `second`. */
export function bothDecorations(first: RasterDecoration | null, second: RasterDecoration | null): RasterDecoration | null {
  if (!first || !second) return first ?? second
  return (context, toScreen, view) => {
    context.save()
    first(context, toScreen, view)
    context.restore()
    second(context, toScreen, view)
  }
}

/**
 * The caret's decoration while a cell is typed (`null` otherwise), and whether the canvas should
 * repaint on its clock (it blinks).
 */
export function useCaret(rect: number[] | null | undefined, color: string | null | undefined): { decorate: RasterDecoration | null; blinking: boolean } {
  const key = rect?.join() ?? ''
  return useMemo(() => {
    if (!rect || rect.length < 4 || !color) return { decorate: null, blinking: false }
    const [x0, y0, , y1] = rect
    const since = performance.now()
    const decorate: RasterDecoration = (context, toScreen, view) => {
      if ((performance.now() - since) % BLINK_MS >= BLINK_MS / 2) return
      const top = toScreen(x0, y0)
      const bottom = toScreen(x0, y1)
      const width = Math.max(1, Math.round(Math.min(2, view.zoom)))
      context.fillStyle = color
      context.fillRect(Math.round(top.x), top.y, width, bottom.y - top.y)
    }
    return { decorate, blinking: true }
    // A new caret position (the key) starts the blink again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, color])
}
