// Dragging the selection's border and the headings' lines by hand, as WPS works them (plan stage
// 2, section 2 rows 17 and 18; walkthrough F4, H1; WPS W-P1–W-P3, W-P6, W-I1): Cmd dragging the
// border copies, Option (Alt) dragging moves; a double-click on the bottom border goes to the last
// row of the data; dragging a column's edge shows its new width beside the line. A person's
// mouse and keys; the engine's state and what is drawn over the grid read back.
// node tests/hand/move.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const mod = process.platform === 'darwin' ? 'Meta' : 'Control'
const set = (e, range, values) => e.agent([{ command: 'range.setValues', params: { range, values } }])
/** A point on the bottom border of the selected cell `ref`, just inside it. */
const bottomBorder = async (e, ref) => {
  const at = await e.cellPoint(ref, { fx: 0.5, fy: 1 })
  return { x: at.x, y: at.y - 1 }
}
/** Drags from `from` by (`dx`, `dy`) with `keys` held. */
async function dragBy(e, from, dx, dy, keys = []) {
  for (const key of keys) await e.page.keyboard.down(key)
  await e.page.mouse.move(from.x, from.y)
  await e.page.mouse.down()
  await e.page.mouse.move(from.x + dx, from.y + dy, { steps: 8 })
  await e.wait(120)
  await e.page.mouse.up()
  for (const key of keys) await e.page.keyboard.up(key)
  await e.wait(300)
}
/** The page distance from cell `a` to cell `b` (their middles). */
const between = async (e, a, b) => {
  const p = await e.cellPoint(a)
  const q = await e.cellPoint(b)
  return { dx: q.x - p.x, dy: q.y - p.y }
}

const list = [
  ['p2-cmd-drag-copies', async (e) => {
    await set(e, 'N2', [['copy me']])
    await e.click('N2')
    const from = await bottomBorder(e, 'N2')
    const { dx } = await between(e, 'N2', 'P2')
    await e.page.keyboard.down(mod)
    await e.page.mouse.move(from.x, from.y)
    await e.page.mouse.move(from.x + 1, from.y)
    await e.until(async () => (await e.cursor()) === 'copy', 20)
    const shown = await e.cursor()
    await e.page.keyboard.up(mod)
    assert.equal(shown, 'copy', 'Cmd over the border: the copy cursor')
    await dragBy(e, from, dx, 0, [mod])
    await e.until(async () => (await e.value('P2')) === 'copy me')
    assert.deepEqual([await e.value('N2'), await e.value('P2')], ['copy me', 'copy me'], 'Cmd copied, the original stays')
  }],
  ['p3-alt-drag-moves', async (e) => {
    await set(e, 'N4', [['move me']])
    await e.click('N4')
    const from = await bottomBorder(e, 'N4')
    const { dx } = await between(e, 'N4', 'P4')
    await e.page.keyboard.down('Alt')
    await e.page.mouse.move(from.x, from.y)
    await e.page.mouse.move(from.x + 1, from.y)
    await e.wait(200)
    const shown = await e.cursor()
    await e.page.keyboard.up('Alt')
    assert.equal(shown, 'move', 'Option over the border: the move cursor')
    await dragBy(e, from, dx, 0, ['Alt'])
    await e.until(async () => (await e.value('P4')) === 'move me')
    assert.deepEqual([await e.value('N4'), await e.value('P4')], [null, 'move me'], 'Option moved')
  }],
  ['p6-double-click-border', async (e) => {
    await set(e, 'R2', [[1], [2], [3], [4], [5]])
    await e.click('R2')
    const at = await bottomBorder(e, 'R2')
    await e.page.mouse.move(at.x, at.y)
    await e.page.mouse.dblclick(at.x, at.y)
    await e.until(async () => (await e.details()).active === 'R6')
    const shown = await e.details()
    assert.equal(shown.active, 'R6', 'to the last row of the data')
    assert.equal(shown.editing, false, 'not typing')
  }],
  ['h1-width-readout', async (e) => {
    const edge = await e.cellPoint('T1', { fx: 1, fy: 0 })
    const { pins, view } = await e.canvasState()
    const box = await e.surface()
    const y = box.y + (pins.top / 2) * view.zoom
    await e.page.mouse.move(edge.x, y)
    await e.until(async () => (await e.cursor()) === 'ew-resize', 20)
    await e.page.mouse.down()
    await e.page.mouse.move(edge.x + 30, y, { steps: 6 })
    await e.wait(200)
    const texts = (await e.marks())?.texts ?? []
    await e.page.mouse.up()
    assert(texts.some((item) => /^\d+\.\d\d \(\d+ px\)$/.test(item.text)), `a width readout: ${JSON.stringify(texts.map((item) => item.text))}`)
  }],
]

await segments('move', list, (which) => open({ which }))
