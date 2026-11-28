/**
 * The caret of the cell being typed, drawn over the rendered grid and blinking (plan stage 5,
 * walkthrough D16). The engine lays the cell's text out and draws it in the document's pixels; it
 * leaves the caret out of them and says where it is (`inspect().caretRect`, document points) and
 * in which colour (`caretColor`). The editor draws that line over the cached frame, so a blink
 * repaints the canvas from the frame it has and never asks the engine to draw the cells again.
 * The canvas repaints only when the caret turns on or off (twice a second), not on a clock of its
 * own: a frozen pane of a thousand rows makes every repaint dear in WebKit.
 */
import { useEffect, useMemo, useState } from 'react'
import type { RasterDecoration } from '@kition/sdk/editor'

/** On for this long, then off for as long; on again at once after every change. */
const HALF_MS = 530

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

/** The caret's decoration while a cell is typed and the caret is on; `null` otherwise. */
export function useCaret(rect: number[] | null | undefined, color: string | null | undefined): RasterDecoration | null {
  const key = rect && rect.length >= 4 && color ? `${rect.join()}|${color}` : ''
  const [on, setOn] = useState(true)
  // A new caret position starts the blink again, on.
  useEffect(() => {
    setOn(true)
    if (!key) return
    const timer = window.setInterval(() => setOn((was) => !was), HALF_MS)
    return () => window.clearInterval(timer)
  }, [key])
  return useMemo(() => {
    if (!key || !on || !rect || !color) return null
    const [x0, y0, , y1] = rect
    return (context, toScreen, view) => {
      const top = toScreen(x0, y0)
      const bottom = toScreen(x0, y1)
      const width = Math.max(1, Math.round(Math.min(2, view.zoom)))
      context.fillStyle = color
      context.fillRect(Math.round(top.x), top.y, width, bottom.y - top.y)
    }
    // The key holds the rectangle and the colour.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, on])
}
