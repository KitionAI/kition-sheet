// The fill handle by hand, as WPS works it (plan stage 1, section 2 rows 1, 2, 14; walkthrough F1,
// F2, F5; WPS W-A1, W-E1–W-E7): the square at the selection's corner, the thin cross over it, a drag
// that fills (a single number counting on, Cmd copying), two cells' step, text with a number,
// dates, weekdays, formulas, one undo step, a double-click filling down beside the data, and a drag
// after zooming and scrolling. A person's mouse and keys; the engine's state read back.
// node tests/hand/fill.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 250) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
/** The fill handle's point on the page: the corner of `ref` (the selection's last cell). */
const corner = (e, ref) => e.cellPoint(ref, { fx: 1, fy: 1 })
/** Drags the fill handle of the selection ending at `last` to the middle of `to`; `mods` held for the release. */
async function fillDrag(e, last, to, mods = []) {
  const from = await corner(e, last)
  const at = await e.cellPoint(to)
  await e.page.mouse.move(from.x - 1, from.y - 1)
  await e.until(async () => (await e.cursor()) === 'crosshair', 20)
  await e.page.mouse.down()
  await e.page.mouse.move(at.x, at.y, { steps: 8 })
  await e.wait(150)
  for (const key of mods) await e.page.keyboard.down(key)
  await e.page.mouse.up()
  for (const key of mods) await e.page.keyboard.up(key)
  await e.wait(300)
}
const values = async (e, refs) => {
  const out = []
  for (const ref of refs) out.push(await e.value(ref))
  return out
}
const set = (e, cell, input) => e.agent([{ command: 'cell.set', params: { cell, input } }])

const list = [
  ['f1-fill-handle', async (e) => {
    await set(e, 'M2', '5')
    await e.click('M2')
    const at = await corner(e, 'M2')
    await e.until(async () => (await e.marks())?.squares.some((s) => Math.abs(s.x + s.w / 2 - at.x) < 2 && Math.abs(s.y + s.h / 2 - at.y) < 2))
    const square = (await e.marks()).squares.find((s) => Math.abs(s.x + s.w / 2 - at.x) < 2 && Math.abs(s.y + s.h / 2 - at.y) < 2)
    assert(square && Math.abs(square.w) >= 5 && Math.abs(square.w) <= 9, `a small square at M2's corner: ${JSON.stringify(square)}`)
    await e.page.mouse.move(at.x, at.y)
    await e.until(async () => (await e.cursor()) === 'crosshair', 20)
    assert.equal(await e.cursor(), 'crosshair', 'the thin cross over the handle')
    await fillDrag(e, 'M2', 'M7')
    await e.until(async () => (await e.value('M7')) === 10)
    assert.deepEqual(await values(e, ['M2', 'M3', 'M4', 'M5', 'M6', 'M7']), [5, 6, 7, 8, 9, 10], 'a single number counts on (WPS W-E1)')
    assert.equal((await e.details()).selection, 'M2:M7', 'the filled block is selected')
    await press(e, 'ControlOrMeta+z', 400)
    await e.until(async () => (await e.value('M3')) == null)
    assert.deepEqual(await values(e, ['M2', 'M3', 'M7']), [5, null, null], 'one undo takes the fill back, the 5 stays where it was (F5)')
  }],
  ['f1-cmd-copies', async (e) => {
    await set(e, 'M10', '5')
    await e.click('M10')
    await fillDrag(e, 'M10', 'M12', ['ControlOrMeta'])
    await e.until(async () => (await e.value('M12')) === 5)
    assert.deepEqual(await values(e, ['M10', 'M11', 'M12']), [5, 5, 5], 'Cmd at the release copies (WPS W-E2)')
  }],
  ['f1-series', async (e) => {
    await e.agent([
      { command: 'range.setValues', params: { range: 'N2', values: [[1], [2]] } },
      { command: 'cell.set', params: { cell: 'O2', input: 'Item 1' } },
      { command: 'cell.set', params: { cell: 'P2', input: '2026-11-27' } },
      { command: 'cell.set', params: { cell: 'Q2', input: 'Monday' } },
    ])
    await e.click('N2')
    const end = await e.cellPoint('N3')
    await e.page.keyboard.down('Shift')
    await e.page.mouse.click(end.x, end.y)
    await e.page.keyboard.up('Shift')
    await e.until(async () => (await e.details()).selection === 'N2:N3')
    await fillDrag(e, 'N3', 'N6')
    await e.until(async () => (await e.value('N6')) === 5)
    assert.deepEqual(await values(e, ['N4', 'N5', 'N6']), [3, 4, 5], 'two cells go on by their step (W-E3)')
    for (const [col, third] of [['O', 'Item 3'], ['Q', 'Wednesday']]) {
      await e.click(`${col}2`)
      await fillDrag(e, `${col}2`, `${col}4`)
      await e.until(async () => (await e.value(`${col}4`)) === third)
      assert.equal(await e.value(`${col}4`), third, `${col}: ${third}`)
    }
    await e.click('P2')
    await fillDrag(e, 'P2', 'P4')
    await e.until(async () => typeof (await e.value('P4')) === 'number')
    assert.equal((await e.value('P4')) - (await e.value('P2')), 2, 'a date goes on a day a cell (W-E6)')
  }],
  ['f1-formula', async (e) => {
    await e.agent([
      { command: 'range.setValues', params: { range: 'L20', values: [[1], [2], [3]] } },
      { command: 'cell.set', params: { cell: 'M20', input: '=L20*2' } },
    ])
    await e.click('M20')
    await fillDrag(e, 'M20', 'M22')
    await e.until(async () => (await e.value('M22')) === 6)
    assert.equal((await e.ask('cell.get', { cell: 'M22' })).input, '=L22*2', 'the reference follows')
  }],
  ['f2-fill-double-click', async (e) => {
    const rows = Array.from({ length: 11 }, (_, i) => [`r${i}`])
    await e.agent([
      { command: 'range.setValues', params: { range: 'L30', values: rows } },
      { command: 'cell.set', params: { cell: 'M30', input: '1' } },
    ])
    await e.click('M30')
    const at = await corner(e, 'M30')
    await e.page.mouse.move(at.x - 1, at.y - 1)
    await e.page.mouse.dblclick(at.x - 1, at.y - 1)
    await e.until(async () => (await e.value('M40')) === 11)
    assert.equal(await e.value('M40'), 11, 'filled down beside the data (W-E7)')
    assert.equal(await e.value('M41'), null, 'and no further')
    assert.equal((await e.details()).editing, false, 'not typing')
  }],
  ['f1-zoomed-scrolled', async (e) => {
    const box = await e.surface()
    await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    await e.page.keyboard.down('Control')
    await e.page.mouse.wheel(0, -100)
    await e.wait(150)
    await e.page.mouse.wheel(0, -100)
    await e.page.keyboard.up('Control')
    await e.until(async () => (await e.zoom()) === 130)
    await e.page.mouse.wheel(0, 300)
    await e.wait(500)
    const { view } = await e.canvasState()
    assert(view.y > 0, 'scrolled')
    await set(e, 'N20', '7')
    await e.click('N20')
    await fillDrag(e, 'N20', 'N23')
    await e.until(async () => (await e.value('N23')) === 10)
    assert.deepEqual(await values(e, ['N21', 'N23']), [8, 10], 'the handle works at 130% and scrolled')
  }],
]

await segments('fill', list, (which) => open({ which }))
