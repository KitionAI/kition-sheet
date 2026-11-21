/**
 * Comment threads the host keeps itself (Kit UI protocol 2.6 section 7k, capability
 * `comment-anchors`; Kition One): a new comment on the selected cells goes to the host as
 * `comment.create` with the engine's anchor (`editor.commentAnchor`), the host's threads come back
 * as `comments/show` and are marked at their range's top-right corner, where the engine finds
 * their anchors now (`editor.commentPlaces`), and a click on a marker sends `comments/activated`.
 *
 * Coexistence with the workbook's own comments and notes: in such a host, New Comment goes to the
 * host; comments and notes already in the file stay in the file, and the Review tab's other
 * comment and note commands keep acting on them. Elsewhere New Comment writes into the file.
 */
import type { KitCommentAnchor, KitShownThread } from '@kition/sdk/protocol'

export type RunEngine = (command: string, params: Record<string, unknown>) => Promise<string>

/** What `editor.commentPlaces` answers for one anchor. */
export type CommentPlace = { found: boolean; sheet?: string; range?: string; active?: boolean; rect?: [number, number, number, number] }

/** A thread's marker: the document point of its range's top-right corner. */
export type CommentMarker = { thread: string; x: number; y: number; count: number }

/** The canvas view (`KitCanvasView`) and the document's pinned bands, as the canvas maps points. */
export type MarkerView = { x: number; y: number; zoom: number; width: number; height: number }
export type Pins = { left: number; top: number }

/** A marker's triangle and its click target, in CSS pixels. */
export const MARKER_SIZE = 8
export const MARKER_TARGET = 16

/** The host's comment features this person has: marking the host's threads, and commenting. */
export function hostCommenting(host: { commentAnchors: boolean; hasService: (service: 'comment.create') => boolean }) {
  return { marks: host.commentAnchors, creates: host.commentAnchors && host.hasService('comment.create') }
}

function parse(text: string): unknown {
  try {
    return JSON.parse(text)
  } catch {
    return null
  }
}

/** The selection's anchor, from the engine. */
export async function selectionAnchor(run: RunEngine): Promise<KitCommentAnchor> {
  const answer = parse(await run('editor.commentAnchor', {})) as { anchor?: KitCommentAnchor } | null
  if (!answer?.anchor || !Array.isArray(answer.anchor.ids) || answer.anchor.ids.length === 0) throw new Error('the selection has no anchor')
  return answer.anchor
}

/** Where each anchor is now (`found: false` for each when the engine answers nothing usable). */
export async function commentPlaces(run: RunEngine, anchors: readonly KitCommentAnchor[]): Promise<CommentPlace[]> {
  if (anchors.length === 0) return []
  const answer = parse(await run('editor.commentPlaces', { anchors: anchors.map((anchor) => ({ ids: anchor.ids })) })) as { places?: unknown } | null
  const places = Array.isArray(answer?.places) ? (answer.places as CommentPlace[]) : []
  return anchors.map((_, index) => places[index] ?? { found: false })
}

/**
 * The markers to draw: one per open thread whose anchor is on the active sheet (resolved threads
 * are not marked, as kit-editor's own comment markers leave them out).
 */
export function commentMarkers(threads: readonly KitShownThread[], places: readonly CommentPlace[]): CommentMarker[] {
  const markers: CommentMarker[] = []
  threads.forEach((thread, index) => {
    const place = places[index]
    if (thread.resolved || !place?.found || !place.active || !Array.isArray(place.rect)) return
    const [x, y, width] = place.rect
    if (![x, y, width].every(Number.isFinite)) return
    markers.push({ thread: thread.id, x: x + width, y, count: thread.count })
  })
  return markers
}

/** Where a document point is on the canvas, in CSS pixels (the canvas's own `toScreen`). */
export function toScreen(view: MarkerView, pins: Pins, x: number, y: number) {
  return { x: (x < pins.left ? x : x - view.x) * view.zoom, y: (y < pins.top ? y : y - view.y) * view.zoom }
}

/**
 * A marker's click target on the canvas (its top-left corner, CSS pixels), or `null` when the
 * corner is out of view or scrolled under a pinned band (frozen panes, the headings).
 */
export function markerTarget(view: MarkerView, pins: Pins, marker: CommentMarker) {
  const at = toScreen(view, pins, marker.x, marker.y)
  const underLeft = marker.x >= pins.left && at.x < pins.left * view.zoom
  const underTop = marker.y >= pins.top && at.y < pins.top * view.zoom
  if (underLeft || underTop || at.x <= 0 || at.y < 0 || at.x > view.width || at.y >= view.height) return null
  return { left: at.x - MARKER_TARGET, top: at.y }
}

/** Why a host refused a service (`error.data.reason`), if it says. */
export function refusalReason(error: unknown): string | null {
  const data = (error as { data?: { reason?: unknown } } | null)?.data
  return typeof data?.reason === 'string' ? data.reason : null
}
