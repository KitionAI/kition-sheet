// Comment threads the host keeps (Kit UI protocol 2.6 section 7k): the engine's anchors go to the
// host, its threads come back as markers at their ranges' top-right corners, and a click target
// is placed only where the corner is in view and not under a pinned band.
import test from 'node:test'
import assert from 'node:assert/strict'
import { MARKER_TARGET, commentMarkers, commentPlaces, hostCommenting, markerTarget, refusalReason, selectionAnchor, toScreen } from './commentAnchors.ts'

const anchor = (...ids) => ({ ids, label: 'Sheet1!B3' })
const thread = (id, extra = {}) => ({ id, anchor: anchor(`s-${id}`, 'r', 'c'), resolved: false, count: 1, ...extra })

test('comments go to the host only when it keeps them and lets this person comment', () => {
  const host = (anchors, services) => ({ commentAnchors: anchors, hasService: (service) => services.includes(service) })
  assert.deepEqual(hostCommenting(host(true, ['comment.create'])), { marks: true, creates: true })
  assert.deepEqual(hostCommenting(host(true, [])), { marks: true, creates: false }, 'a viewer: markers, no new comments')
  assert.deepEqual(hostCommenting(host(false, ['comment.create'])), { marks: false, creates: false }, 'a host that keeps no comments: the file\'s own')
})

test('asks the engine for the selection\'s anchor and refuses an empty one', async () => {
  const calls = []
  const run = async (command, params) => {
    calls.push([command, params])
    return JSON.stringify({ anchor: anchor('s1', 'r1', 'c1') })
  }
  assert.deepEqual(await selectionAnchor(run), anchor('s1', 'r1', 'c1'))
  assert.deepEqual(calls, [['editor.commentAnchor', {}]])
  await assert.rejects(selectionAnchor(async () => JSON.stringify({ anchor: { ids: [] } })))
  await assert.rejects(selectionAnchor(async () => 'not json'))
})

test('finds the anchors by their ids, one place per anchor whatever the engine answers', async () => {
  let asked = null
  const run = async (command, params) => {
    asked = [command, params]
    return JSON.stringify({ places: [{ found: true, sheet: 'Sheet1', range: 'B3', active: true, rect: [110, 61, 64, 20] }] })
  }
  const places = await commentPlaces(run, [anchor('a', 'b', 'c'), anchor('d', 'e', 'f')])
  assert.deepEqual(asked, ['editor.commentPlaces', { anchors: [{ ids: ['a', 'b', 'c'] }, { ids: ['d', 'e', 'f'] }] }], 'labels stay with the host')
  assert.deepEqual(places, [{ found: true, sheet: 'Sheet1', range: 'B3', active: true, rect: [110, 61, 64, 20] }, { found: false }])
  assert.deepEqual(await commentPlaces(async () => 'oops', [anchor('a', 'b', 'c')]), [{ found: false }])
  assert.deepEqual(await commentPlaces(run, []), [], 'no anchors, no engine call')
})

test('marks open threads on the active sheet at their range\'s top-right corner', () => {
  const threads = [thread('t1', { count: 3 }), thread('t2', { resolved: true }), thread('t3'), thread('t4'), thread('t5')]
  const places = [
    { found: true, active: true, rect: [110, 61, 64, 20] },
    { found: true, active: true, rect: [0, 0, 10, 10] },
    { found: true, active: false, sheet: 'Other' },
    { found: false },
  ]
  assert.deepEqual(commentMarkers(threads, places), [{ thread: 't1', x: 174, y: 61, count: 3 }])
})

test('places a marker\'s click target where the canvas draws its corner', () => {
  const view = { x: 0, y: 0, zoom: 1, width: 800, height: 600 }
  const pins = { left: 46, top: 21 }
  const marker = { thread: 't1', x: 174, y: 61, count: 1 }
  assert.deepEqual(toScreen(view, pins, 174, 61), { x: 174, y: 61 })
  assert.deepEqual(markerTarget(view, pins, marker), { left: 174 - MARKER_TARGET, top: 61 })
  // Scrolled and zoomed: the corner moves with the cells.
  assert.deepEqual(markerTarget({ ...view, x: 20, y: 10, zoom: 2 }, pins, marker), { left: 308 - MARKER_TARGET, top: 102 })
  // Scrolled under the headings, or out of the canvas: no target.
  assert.equal(markerTarget({ ...view, y: 60 }, pins, marker), null)
  assert.equal(markerTarget({ ...view, x: 150 }, pins, marker), null)
  assert.equal(markerTarget({ ...view, width: 100 }, pins, marker), null)
})

test('a person cancelling is not an error', () => {
  assert.equal(refusalReason({ code: -32000, data: { reason: 'cancelled' } }), 'cancelled')
  assert.equal(refusalReason(new Error('x')), null)
  assert.equal(refusalReason(null), null)
})
