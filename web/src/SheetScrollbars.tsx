import { useSyncExternalStore, type RefObject } from 'react'
import type { KitCanvasHandle } from '@kition/sdk/editor'
import { Scrollbar } from '@kition/tessera/ui'
import type { ViewStore } from './HostComments'

/**
 * The grid's scrollbars, along its right and bottom edges (Excel, WPS): each shows where the view
 * is in the sheet and moves it. The sheet's length is what the engine draws (`width` and `height`
 * in document pixels: the cells in use and the reach the view has asked for), so it grows as the
 * view nears its end. They follow the view store, so a scroll renders them alone.
 */
export function SheetScrollbars({ views, canvas, width, height, controls, vertical, horizontal }: {
  views: ViewStore
  canvas: RefObject<KitCanvasHandle | null>
  width: number
  height: number
  controls: string
  vertical: string
  horizontal: string
}) {
  const view = useSyncExternalStore(views.subscribe, views.get)
  if (!view || !(view.zoom > 0)) return null
  const x = Math.max(0, view.x)
  const y = Math.max(0, view.y)
  const pageX = view.width / view.zoom
  const pageY = view.height / view.zoom
  return (
    <>
      <Scrollbar label={vertical} controls={controls} value={y} max={Math.max(0, height - pageY, y)} page={pageY} onChange={(value) => canvas.current?.panTo(x, value)} />
      <Scrollbar label={horizontal} controls={controls} orientation="horizontal" value={x} max={Math.max(0, width - pageX, x)} page={pageX} onChange={(value) => canvas.current?.panTo(value, y)} />
    </>
  )
}
