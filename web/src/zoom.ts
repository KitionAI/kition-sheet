/**
 * The grid's zoom steps (plan stage 0A, section 2 row 11; walkthrough A7, A8, A10, A11; WPS W-S1,
 * W-S2, W-R4). A mouse wheel notch with Ctrl or Cmd adds or takes 15 points (100% → 115% → 130%,
 * down 85%, 70%) about the pointer; a trackpad pinch stays continuous. The status bar's − and +
 * go 10 points at a time to a multiple of 10 (129% → 130% → 140%). Every way stays within
 * 10%–400%. The zoom is the view's (a person's setting, never a change of the workbook).
 */
import type { KitCanvasHandle, RasterPins } from '@kition/sdk/editor'

export const MIN_ZOOM = 0.1
export const MAX_ZOOM = 4
/** The points one mouse wheel notch adds or takes. */
const NOTCH_POINTS = 15
/** A pinch's zoom per pixel of wheel delta (the shared canvas's continuous zoom). */
const PINCH_SPEED = 0.01
const EPSILON = 1e-6

export const clampZoom = (zoom: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Number.isFinite(zoom) ? zoom : 1))

/** The status bar's − and +: the next multiple of 10% past `zoom`, up (`1`) or down (`-1`). */
export function stepZoom(zoom: number, direction: 1 | -1) {
  const tens = (zoom * 100) / 10
  const next = direction > 0 ? Math.floor(tens + EPSILON) + 1 : Math.ceil(tens - EPSILON) - 1
  return clampZoom((next * 10) / 100)
}

/** One mouse wheel notch from `zoom`: 15 points up (`1`) or down (`-1`). */
export function notchZoom(zoom: number, direction: 1 | -1) {
  const percent = Math.round(zoom * 100) + direction * NOTCH_POINTS
  return clampZoom(percent / 100)
}

/** A pinch (or a wheel too fine to be notches) zooms continuously, by its delta. */
export const pinchZoom = (zoom: number, deltaY: number) => clampZoom(zoom * Math.exp(-deltaY * PINCH_SPEED))

/**
 * How many mouse wheel notches a wheel event is, or 0 for a trackpad's scroll or pinch: a wheel
 * counted in lines or pages, or in whole multiples of about 100 pixels (a browser's notch).
 */
export function wheelNotches(event: Pick<WheelEvent, 'deltaMode' | 'deltaY'>) {
  const delta = Math.abs(event.deltaY)
  if (event.deltaMode === 1) return delta > 0 ? Math.max(1, Math.round(delta / 3)) : 0
  if (event.deltaMode === 2) return delta > 0 ? 1 : 0
  if (delta < 50 || !Number.isInteger(event.deltaY)) return 0
  const notches = Math.round(delta / 100)
  return notches >= 1 && Math.abs(delta - notches * 100) <= 10 * notches ? notches : 0
}

/**
 * Zooms to `zoom`, keeping the cell under (`clientX`, `clientY`) still (the canvas's centre
 * without a point). The headings and frozen panes (`pins`) stay where they are, as the canvas
 * draws them.
 */
export function zoomAround(canvas: KitCanvasHandle, zoom: number, surface?: Element | null, pins?: RasterPins | null, clientX?: number, clientY?: number) {
  const next = clampZoom(zoom)
  const view = canvas.view()
  if (!view || !surface || clientX === undefined || clientY === undefined) {
    canvas.zoomTo(next)
    return
  }
  const box = surface.getBoundingClientRect()
  const sx = (clientX - box.left) / view.zoom
  const sy = (clientY - box.top) / view.zoom
  const { left, top } = pins ?? { left: 0, top: 0 }
  const x = sx < left ? view.x : view.x + sx - (clientX - box.left) / next
  const y = sy < top ? view.y : view.y + sy - (clientY - box.top) / next
  canvas.zoomTo(next)
  canvas.panTo(Math.max(0, x), Math.max(0, y))
}

/**
 * Zooms to `zoom` keeping the cell at the view's top-left where it is, as the status bar, View ›
 * Zoom and the keys do in a spreadsheet (the canvas's own zoom keeps the centre, which scrolled
 * the sheet left of its first column).
 */
export function zoomKeepingCorner(canvas: KitCanvasHandle, zoom: number) {
  const view = canvas.view()
  canvas.zoomTo(clampZoom(zoom))
  if (view) canvas.panTo(Math.max(0, view.x), Math.max(0, view.y))
}

/**
 * The canvas as kit-editor's frame sees it: its zoom keys and its Fit (which the editor's own keys
 * take first, `keys.ts`) give 100% and 10-point steps within 10%–400%, never the whole document
 * fitted into the window (walkthrough A8).
 */
export function steppedCanvas(canvas: { readonly current: KitCanvasHandle | null }): { readonly current: KitCanvasHandle | null } {
  return {
    get current() {
      const inner = canvas.current
      if (!inner) return null
      return {
        ...inner,
        fit: () => zoomKeepingCorner(inner, 1),
        zoomTo: (zoom: number) => zoomKeepingCorner(inner, zoom),
        zoomBy: (factor: number) => {
          const view = inner.view()
          if (view) zoomKeepingCorner(inner, stepZoom(view.zoom, factor >= 1 ? 1 : -1))
        },
      }
    },
  }
}
