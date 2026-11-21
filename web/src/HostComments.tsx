import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react'
import { MessageSquarePlusIcon as MessageSquarePlus } from '@kition/tessera/icons/react'
import { CommandGroupButtons, RibbonButton, RibbonGroup, toast, useKitEditor, type HostConnection, type KitCanvasView, type KitEngine, type RasterDecoration } from '@kition/sdk/editor'
import type { KitShownThread } from '@kition/sdk/protocol'
import { MARKER_SIZE, MARKER_TARGET, commentMarkers, commentPlaces, hostCommenting, markerTarget, refusalReason, selectionAnchor, type CommentMarker, type Pins } from './commentAnchors'

/** The canvas's view, for the marker targets only (the editor around them does not re-render on a scroll). */
export function createViewStore() {
  let view: KitCanvasView | null = null
  const listeners = new Set<() => void>()
  return {
    get: () => view,
    set: (next: KitCanvasView) => {
      view = next
      for (const listener of listeners) listener()
    },
    subscribe: (listener: () => void) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
  }
}
export type ViewStore = ReturnType<typeof createViewStore>

/**
 * The host's comment threads in this editor (`commentAnchors.ts`): its threads' markers, the canvas
 * decoration that draws them, and New Comment for the selected cells.
 */
export function useHostComments(host: HostConnection, doc: KitEngine | null, change: unknown, sheet: string | undefined, failed: string) {
  const { marks, creates } = hostCommenting(host)
  const [threads, setThreads] = useState<KitShownThread[]>([])
  const [markers, setMarkers] = useState<CommentMarker[]>([])
  const docRef = useRef(doc)
  docRef.current = doc
  const run = useCallback((command: string, params: Record<string, unknown>) => {
    const open = docRef.current
    return open ? open.execute(command, params) : Promise.reject(new Error('no workbook is open'))
  }, [])

  useEffect(() => (marks ? host.onComments(setThreads) : undefined), [host, marks])
  // `comments/locate`: whether each anchor is in the workbook now.
  useEffect(() => {
    if (!marks) return
    host.locateComments(async (anchors) => (docRef.current ? (await commentPlaces(run, anchors)).map((place) => place.found === true) : anchors.map(() => false)))
  }, [host, marks, run])
  // The markers follow the threads, every change of the workbook (rows and columns inserted,
  // deleted, resized), and the sheet shown.
  useEffect(() => {
    if (!marks || !doc) return
    let current = true
    commentPlaces(run, threads.map((thread) => thread.anchor))
      .then((places) => {
        if (current) setMarkers(commentMarkers(threads, places))
      })
      .catch(() => {
        if (current) setMarkers([])
      })
    return () => {
      current = false
    }
  }, [marks, doc, change, sheet, threads, run])

  const decorate = useMemo<RasterDecoration | null>(() => {
    if (!markers.length) return null
    const color = getComputedStyle(document.documentElement).getPropertyValue('--kit-primary').trim() || '#1a8641'
    return (context, toScreen) => {
      context.fillStyle = color
      // A light edge keeps the marker visible on filled cells of the same hue.
      context.strokeStyle = 'rgba(255, 255, 255, 0.9)'
      context.lineWidth = 1
      for (const marker of markers) {
        const at = toScreen(marker.x, marker.y)
        context.beginPath()
        context.moveTo(at.x - MARKER_SIZE, at.y)
        context.lineTo(at.x, at.y)
        context.lineTo(at.x, at.y + MARKER_SIZE)
        context.closePath()
        context.fill()
        context.stroke()
      }
    }
  }, [markers])

  const [busy, setBusy] = useState(false)
  const create = useCallback(async () => {
    if (busy) return
    setBusy(true)
    try {
      const anchor = await selectionAnchor(run)
      await host.service('comment.create', { anchor })
    } catch (error) {
      if (refusalReason(error) !== 'cancelled') toast('error', failed, error instanceof Error ? error.message : String(error))
    } finally {
      setBusy(false)
    }
  }, [busy, failed, host, run])

  return { marks, creates, markers, decorate, create, busy }
}

/** A click target over each marker: it opens the thread in the host (`comments/activated`). */
export function CommentMarkerTargets({ markers, views, pins, onActivate, label }: { markers: CommentMarker[]; views: ViewStore; pins: Pins; onActivate: (thread: string) => void; label: (count: number) => string }) {
  const view = useSyncExternalStore(views.subscribe, views.get)
  if (!view || !markers.length) return null
  return (
    <>
      {markers.map((marker) => {
        const at = markerTarget(view, pins, marker)
        if (!at) return null
        return (
          <button
            key={marker.thread}
            type="button"
            className="sheet-comment-marker"
            style={{ left: at.left, top: at.top, width: MARKER_TARGET, height: MARKER_TARGET }}
            aria-label={label(marker.count)}
            title={label(marker.count)}
            data-testid="sheet-comment-marker"
            data-thread={marker.thread}
            onPointerDown={(event) => event.stopPropagation()}
            onClick={() => onActivate(marker.thread)}
          />
        )
      })}
    </>
  )
}

/** Review › Comments in a host that keeps comments: New Comment goes to the host; the file's own comment commands stay. */
export function HostCommentGroup({ onCreate, busy }: { onCreate: () => void; busy: boolean }) {
  const { phrase: p, session } = useKitEditor()
  return (
    <RibbonGroup label={p('Comments')} icon={MessageSquarePlus}>
      <RibbonButton icon={MessageSquarePlus} size="large" label={p('New Comment')} disabled={busy || !session.doc} onClick={onCreate} data-testid="sheet-host-comment" primary />
      <CommandGroupButtons path="Review/Comments" exclude={['review.newComment']} />
    </RibbonGroup>
  )
}
