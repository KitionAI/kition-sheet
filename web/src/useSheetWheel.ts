/**
 * The wheel over the grid with Ctrl or Cmd (plan stage 0A; walkthrough A7; WPS W-S1, and section 2
 * "Cmd+滚轮保留为缩放"): a mouse notch zooms 15 points about the pointer, a trackpad pinch
 * continuously, within 10%–400% (`zoom.ts`). Taken in the capture phase, before the shared
 * canvas's own wheel handler, which zoomed 2.72 times a notch. The wheel without them scrolls as
 * the canvas does.
 */
import { useEffect, type RefObject } from 'react'
import type { KitCanvasHandle, RasterPins } from '@kition/sdk/editor'
import { notchZoom, pinchZoom, wheelNotches, zoomAround } from './zoom'

export function useSheetWheel(region: RefObject<HTMLElement | null>, canvas: RefObject<KitCanvasHandle | null>, pins: () => RasterPins | null) {
  useEffect(() => {
    const element = region.current
    if (!element) return
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) return
      const target = event.target instanceof Element ? event.target : null
      const surface = target?.closest('.kui-canvas')?.querySelector('canvas')
      const handle = canvas.current
      const view = handle?.view()
      if (!surface || !handle || !view) return
      event.preventDefault()
      event.stopPropagation()
      const notches = wheelNotches(event)
      let zoom = view.zoom
      if (notches) for (let index = 0; index < Math.min(notches, 5); index += 1) zoom = notchZoom(zoom, event.deltaY < 0 ? 1 : -1)
      else zoom = pinchZoom(view.zoom, event.deltaY)
      zoomAround(handle, zoom, surface, pins(), event.clientX, event.clientY)
    }
    element.addEventListener('wheel', wheel, { capture: true, passive: false })
    return () => element.removeEventListener('wheel', wheel, { capture: true })
  }, [region, canvas, pins])
}
