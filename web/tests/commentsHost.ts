/**
 * A host that keeps comment threads itself, as Kition One does (Kit UI protocol 2.6 section 7k,
 * `KitEmbed 1.7` EM29 and EM30), for `tests/comments.mjs`: it serves `comment.create` (a new
 * thread, or the person cancelling), shows its threads to the kit (`comments/show`) and hears a
 * marker clicked (`comments/activated`). It keeps the workbook's replica beside the file
 * (capability `replica`), as One's managed documents always have one, so a viewer's and a
 * commenter's editor find the threads an editor anchored.
 *
 * `?role=viewer|commenter` opens the document as that person; `?comments=0` opens it in a host that
 * keeps no comments. `window.commentHostSeed` (set before the page loads) opens a file, replica and
 * threads another page saved (`commentHost.saved()`).
 */
import { mountKit, type KitConnection } from '@kition/sdk/host'
import { KitRpcError, type KitCommentAnchor, type KitShownThread } from '@kition/sdk/protocol'

type Seed = { file: string; version: string; snapshot: string; blobs: string; threads: KitShownThread[] }

const query = new URLSearchParams(location.search)
const role = query.get('role')
const keepsComments = query.get('comments') !== '0'
const status = document.getElementById('status') as HTMLElement
const frame = document.getElementById('kit') as HTMLIFrameElement
const seed = (window as unknown as { commentHostSeed?: Seed }).commentHostSeed

const encode = (bytes: ArrayBuffer) => {
  let text = ''
  for (const byte of new Uint8Array(bytes)) text += String.fromCharCode(byte)
  return btoa(text)
}
const decode = (text: string) => Uint8Array.from(atob(text), (c) => c.charCodeAt(0)).buffer

const threads: KitShownThread[] = seed?.threads ?? []
const created: Array<{ anchor: KitCommentAnchor; text?: string }> = []
const activated: string[] = []
let cancelNext = false
let connection: KitConnection | null = null
let file = seed ? decode(seed.file) : await (await fetch('../sample/Sample.xlsx')).arrayBuffer()
let version = seed?.version ?? '1'
let replica: { snapshot: ArrayBuffer; blobs: ArrayBuffer; fileStamp: string } | null = seed ? { snapshot: decode(seed.snapshot), blobs: decode(seed.blobs), fileStamp: seed.version } : null

const show = () => connection?.showComments(threads.map((thread) => ({ ...thread, anchor: { ...thread.anchor } })))

const host = {
  threads,
  created,
  activated,
  connection: () => connection,
  cancelNext: () => {
    cancelNext = true
  },
  resolve: (id: string, resolved = true) => {
    const thread = threads.find((item) => item.id === id)
    if (thread) thread.resolved = resolved
    show()
  },
  show,
  /** The file, its replica and the threads, for another page to open (`commentHostSeed`). */
  saved: (): Seed | null => (replica ? { file: encode(file), version, snapshot: encode(replica.snapshot), blobs: encode(replica.blobs), threads } : null),
}
;(window as unknown as { commentHost: typeof host }).commentHost = host

const readOnly = role === 'viewer' || role === 'commenter'
frame.src = `../index.html?${Date.now()}`
connection = await mountKit(frame, {
  origin: location.origin,
  host: { name: 'Comment test host', version: '1' },
  document: {
    path: 'Sample.xlsx',
    name: 'Sample.xlsx',
    bytes: file.slice(0),
    version,
    ...(readOnly ? { readOnly: true, comments: { me: `test-${role}`, name: role ?? '', canComment: role === 'commenter', othersReadOnly: true } } : {}),
  },
  theme: { colorScheme: 'light', tokens: {} },
  locale: 'en-US',
  shortcuts: [],
  ui: { modes: { allowed: ['classic'], default: 'classic' } },
  handlers: {
    read: () => ({ bytes: file.slice(0), version, size: file.byteLength }),
    stamp: () => ({ version }),
    write: ({ path, bytes }) => {
      file = bytes
      version = String(Number(version) + 1)
      return { path, version }
    },
    replicaRead: () => replica,
    replicaWrite: ({ snapshot, blobs, fileStamp }) => {
      replica = { snapshot, blobs, fileStamp }
    },
    changed: () => {},
    ...(keepsComments
      ? {
          services: {
            'comment.create': ({ anchor, text }) => {
              created.push({ anchor, text })
              if (cancelNext) {
                cancelNext = false
                throw new KitRpcError(-32000, 'the person cancelled', { reason: 'cancelled' })
              }
              const id = `thread-${threads.length + 1}`
              threads.push({ id, anchor, resolved: false, count: 1 })
              // As One does: the panel posts, then the editor shows the open threads again.
              queueMicrotask(show)
              return { thread: id }
            },
          },
          comments: { activated: (id: string) => activated.push(id) },
        }
      : {}),
  },
})
// As One's editor does after every read: the open threads, to a kit opened again too.
show()
status.textContent = 'open'
