/**
 * Scrolling at the grid's edge during a drag (plan stage 1; walkthrough B10; WPS W-A11, W-A12):
 * while a press is held and the pointer is past the edge of the part of the grid that scrolls
 * (below or right of the canvas, or above or left of it while the view can go back that way), a
 * timer pans the view every frame, the pointer still or not, and sends the drag again from the
 * pointer's last place, held at the edge, so the selection (a fill, a move, a reference pointed
 * at, a column resized) reaches the cells that come into view. The further past the edge, the
 * faster: WPS's 2.3 rows a second at 13 px, 5 at 43 px, and across 40 columns a second at 68 px.
 * Inside the grid nothing scrolls. A column or row being resized does not scroll back towards its
 * start; selecting characters of the cell being typed and the Draw tab's pens do not scroll.
 *
 * The engine's tool decides what the drag is; this only wraps kit-editor's canvas tool, so every
 * sample still goes through the tool bridge's one ordered queue (the latest drag kept).
 */
import type { KitPointer, KitTool, RasterPins } from '@kition/sdk/editor'

type Context = Parameters<KitTool['begin']>[1]
type Gesture = NonNullable<ReturnType<KitTool['begin']>>

/**
 * What the wrapper reads at each frame: the engine tool's cursor now, whether a pen is up, the
 * sheet's default column width and row height (document pixels), the pinned headings and frozen
 * panes, and the canvas's size (CSS pixels).
 */
export type EdgeScrollRead = () => { cursor: string; drawing: boolean; cell: [number, number]; pins: RasterPins | null; size: { width: number; height: number } | null }

/** Rows (columns) a second at `over` CSS pixels past the edge, WPS's measured pace in between. */
export function edgeSpeed(over: number, across: boolean): number {
  if (over <= 0) return 0
  const [at13, slope, most] = across ? [2.3, (40 - 2.3) / (68 - 13), 120] : [2.3, (5 - 2.3) / (43 - 13), 60]
  return Math.min(most, Math.max(1, at13 + (over - 13) * slope))
}

export function edgeScrolling(inner: KitTool, read: EdgeScrollRead): KitTool {
  return {
    ...inner,
    begin(pointer: KitPointer, context: Context) {
      const gesture = inner.begin(pointer, context) as Gesture | void
      if (!gesture) return gesture
      let last = pointer
      let held: Context = context
      let frame = 0
      let then = 0
      /** Which ways the drag may scroll now. */
      const ways = () => {
        const { cursor, drawing } = read()
        if (drawing || cursor === 'text') return null
        return { back: !/^(ew|ns)-resize$/.test(cursor), across: cursor !== 'ns-resize', down: cursor !== 'ew-resize' }
      }
      /** The pointer's distance past each edge of the scrolling part (CSS pixels), and where it is held. */
      const edges = (at: KitPointer) => {
        const { size, pins: pinned } = read()
        const allowed = ways()
        if (!size || !allowed) return null
        // The canvas's place on the page, from the sample's two positions.
        const origin = { x: at.clientX - at.canvasX, y: at.clientY - at.canvasY }
        const box = { left: origin.x, top: origin.y, right: origin.x + size.width, bottom: origin.y + size.height }
        const { zoom, x, y } = held.viewport.view
        const pins = pinned ?? { left: 0, top: 0 }
        const left = box.left + pins.left * zoom
        const top = box.top + pins.top * zoom
        const over = {
          right: allowed.across ? at.clientX - (box.right - 1) : 0,
          down: allowed.down ? at.clientY - (box.bottom - 1) : 0,
          left: allowed.across && allowed.back && x > 0 ? left + 1 - at.clientX : 0,
          up: allowed.down && allowed.back && y > 0 ? top + 1 - at.clientY : 0,
        }
        const clientX = over.right > 0 ? box.right - 1 : over.left > 0 ? left + 1 : at.clientX
        const clientY = over.down > 0 ? box.bottom - 1 : over.up > 0 ? top + 1 : at.clientY
        return { over, clientX, clientY }
      }
      /** The sample with the pointer held at the edge, in the document's pixels as the view is now. */
      const place = (at: KitPointer): KitPointer => {
        const edge = edges(at)
        if (!edge || (edge.clientX === at.clientX && edge.clientY === at.clientY)) return at
        const point = held.viewport.toImage(edge.clientX, edge.clientY)
        return { ...at, x: point.x, y: point.y }
      }
      const stop = () => {
        if (frame) window.cancelAnimationFrame(frame)
        frame = 0
      }
      const tick = (now: number) => {
        frame = 0
        const edge = edges(last)
        const { over } = edge ?? { over: { right: 0, down: 0, left: 0, up: 0 } }
        if (!edge || (over.right <= 0 && over.down <= 0 && over.left <= 0 && over.up <= 0)) return
        const seconds = then ? Math.min(0.1, Math.max(0, (now - then) / 1000)) : 1 / 60
        then = now
        const [colWidth, rowHeight] = read().cell
        const view = held.viewport.view
        const dx = (edgeSpeed(over.right, true) - edgeSpeed(over.left, true)) * colWidth * seconds
        const dy = (edgeSpeed(over.down, false) - edgeSpeed(over.up, false)) * rowHeight * seconds
        const x = Math.max(0, view.x + dx)
        const y = Math.max(0, view.y + dy)
        if (x !== view.x || y !== view.y) {
          held.viewport.panTo(x, y)
          gesture.move?.(place(last), held)
        }
        frame = window.requestAnimationFrame(tick)
      }
      const schedule = () => {
        if (frame) return
        const edge = edges(last)
        if (!edge || (edge.over.right <= 0 && edge.over.down <= 0 && edge.over.left <= 0 && edge.over.up <= 0)) return
        then = 0
        frame = window.requestAnimationFrame(tick)
      }
      return {
        move(at: KitPointer, now: Context) {
          last = at
          held = now
          gesture.move?.(place(at), now)
          schedule()
        },
        end(at: KitPointer, now: Context) {
          stop()
          gesture.end?.(place(at), now)
        },
        cancel(now: Context) {
          stop()
          gesture.cancel?.(now)
        },
      }
    },
  }
}
