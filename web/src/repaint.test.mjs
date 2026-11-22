// The grid repaints what the session's changes owe when React takes several in one render
// (regression case regressions/issue-checkbox-paint-lost; src/repaint.ts).
import test from 'node:test'
import assert from 'node:assert/strict'
import { isEmpty, repaintFor } from './repaint.ts'

// kit-editor's refresh as @kition/sdk 0.5.2 has it (kition-sdk
// packages/kit-editor/src/engine/useKitDocument.ts, `refresh`): each change adds 1 to the counter and
// replaces the area. React applies the updaters of one render in order, so the canvas sees only the
// last area of a batch.
const refresh = (previous, damage) => ({ revision: previous.revision + 1, damage })
const batch = (previous, ...areas) => areas.reduce(refresh, previous)
const NOTHING = { x0: 0, y0: 0, x1: 0, y1: 0 }
const doc = { name: 'Book1' }
const drawn = (revision, inspected = '{"active":"A1"}', on = doc) => ({ doc: on, revision, inspected })

test('a checkbox click taken in one render: the press ticks the box, the release asks for nothing; the whole grid is repainted', () => {
  const before = { revision: 7, damage: null }
  const handed = { change: before.revision, drawn: drawn(40) }
  // The press moved the engine's revision (40 → 41): the whole grid. The release changed nothing more.
  const taken = batch(before, null, NOTHING)
  assert.deepEqual(taken, { revision: 9, damage: NOTHING }, 'kit-editor keeps only the release’s empty area')
  assert.ok(isEmpty(taken.damage))
  assert.deepEqual(repaintFor(handed, taken, drawn(41)), { revision: 9, damage: null })
})

test('an agent’s read taken with the person’s entry: the entry is repainted', () => {
  const before = { revision: 3, damage: null }
  const handed = { change: 3, drawn: drawn(10) }
  const taken = batch(before, null, NOTHING, NOTHING)
  assert.deepEqual(repaintFor(handed, taken, drawn(11)), { revision: 6, damage: null })
})

test('a change of what inspect() shows (Gridlines, the zoom) taken with a read is repainted', () => {
  const handed = { change: 3, drawn: drawn(10, '{"gridlines":true}') }
  const taken = batch({ revision: 3, damage: null }, null, NOTHING)
  assert.equal(repaintFor(handed, taken, drawn(10, '{"gridlines":false}')).damage, null)
})

test('another document (the workbook reopened) is repainted', () => {
  const handed = { change: 3, drawn: drawn(10) }
  const taken = batch({ revision: 3, damage: null }, null, NOTHING)
  assert.equal(repaintFor(handed, taken, drawn(10, undefined, { name: 'Book1' })).damage, null)
})

test('an agent’s reads taken together, nothing drawn having moved, still paint nothing', () => {
  const handed = { change: 12, drawn: drawn(30) }
  const taken = batch({ revision: 12, damage: NOTHING }, NOTHING, NOTHING, NOTHING)
  const shown = repaintFor(handed, taken, drawn(30))
  assert.equal(shown, taken)
  assert.ok(isEmpty(shown.damage))
})

test('an area taken after other changes is not trusted alone: the whole grid', () => {
  const area = { x0: 10, y0: 20, x1: 80, y1: 40 }
  const handed = { change: 5, drawn: drawn(8) }
  const taken = batch({ revision: 5, damage: null }, { x0: 200, y0: 200, x1: 260, y1: 220 }, area)
  assert.deepEqual(repaintFor(handed, taken, drawn(8)), { revision: 7, damage: null })
})

test('one change at a time is handed on as it is: its own area, nothing, or the whole grid', () => {
  const handed = { change: 5, drawn: drawn(8) }
  for (const damage of [null, NOTHING, { x0: 1, y0: 2, x1: 3, y1: 4 }]) {
    const change = refresh({ revision: 5, damage: null }, damage)
    assert.equal(repaintFor(handed, change, drawn(damage === NOTHING ? 8 : 9)), change)
  }
  const first = { revision: 4, damage: NOTHING }
  assert.equal(repaintFor(null, first, drawn(1)), first)
})
