// Entering and selecting by hand as WPS does (plan stage 0B, section 2 rows 6, 7, 13, 15;
// walkthrough B6, B8, D10, D14; WPS W-A5, W-A6, W-C10, W-E6, W-G2): Cmd+Enter fills the
// selection; a typed date widens its column in one undo step; Backspace clears the selection
// without editing; Cmd+click adds a heading's column and takes a selected cell out.
// A person's keys, clicks and drags; the engine's state read back.
// node tests/hand/commit.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { colIndex, open, segments } from './harness.mjs'

const press = async (e, key, ms = 200) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
async function drag(e, from, to) {
  const a = await e.cellPoint(from)
  const b = await e.cellPoint(to)
  await e.page.mouse.move(a.x, a.y)
  await e.page.mouse.down()
  await e.page.mouse.move(b.x, b.y, { steps: 6 })
  await e.page.mouse.up()
  await e.until(async () => (await e.details()).selection === `${from}:${to}`)
}
/** A column heading's point on the page. */
async function heading(e, column) {
  const at = await e.cellPoint(`${column}1`)
  const box = await e.surface()
  const { pins, view } = await e.canvasState()
  return { x: at.x, y: box.y + (pins.top / 2) * view.zoom }
}
const sheetOf = async (e) => {
  const name = (await e.details()).sheet
  return (await e.ask('file.inspect')).sheets.find((sheet) => sheet.name === name)
}
/** Whether cell `ref` is inside one of the selection's areas (`B3:D3,B4,D4`). */
function selected(selection, ref) {
  const cell = (text) => {
    const match = /^([A-Z]+)(\d+)$/.exec(text)
    return match ? { col: colIndex(match[1]), row: Number(match[2]) } : null
  }
  const at = cell(ref)
  return selection.split(',').some((area) => {
    const [first, last = first] = area.split(':')
    const a = cell(first)
    const b = cell(last)
    return a && b && at.col >= a.col && at.col <= b.col && at.row >= a.row && at.row <= b.row
  })
}

const list = [
  ['d14-cmd-enter-fills', async (e) => {
    await drag(e, 'N2', 'N5')
    await e.page.keyboard.type('7')
    await press(e, 'ControlOrMeta+Enter', 400)
    await e.until(async () => (await e.value('N5')) === 7)
    assert.deepEqual([await e.value('N2'), await e.value('N3'), await e.value('N4'), await e.value('N5')], [7, 7, 7, 7], 'Cmd+Enter fills every selected cell')
    const shown = await e.details()
    assert.equal(shown.active, 'N2', 'the active cell stays')
    assert.equal(shown.selection, 'N2:N5', 'and the selection')
    assert.equal(shown.editing, false)
    await press(e, 'ControlOrMeta+z', 400)
    await e.until(async () => (await e.value('N5')) == null)
    assert.deepEqual([await e.value('N2'), await e.value('N5')], [null, null], 'one undo takes the fill back')
  }],
  ['d10-date-widens-column', async (e) => {
    await e.click('R2')
    const before = (await sheetOf(e)).colWidths?.R ?? (await e.details()).defaultSize[0]
    await e.page.keyboard.type('2026-11-27')
    await press(e, 'Enter', 400)
    await e.until(async () => (await sheetOf(e)).colWidths?.R > before)
    const after = (await sheetOf(e)).colWidths?.R
    assert(after > before, `the date's column widened (${before} → ${after})`)
    const text = (await e.ask('cell.get', { cell: 'R2' })).text
    assert(/2026/.test(text ?? ''), `the cell shows the date: ${text}`)
    await press(e, 'ControlOrMeta+z', 400)
    await e.until(async () => (await e.value('R2')) == null)
    assert.equal(await e.value('R2'), null, 'one undo takes the value back')
    assert.equal((await sheetOf(e)).colWidths?.R ?? before, before, 'and the width')
  }],
  ['g2-backspace-clears', async (e) => {
    await e.agent([{ command: 'range.setValues', params: { range: 'N8', values: [['a', 'b', 'c']] } }])
    await drag(e, 'N8', 'P8')
    await press(e, 'Backspace', 300)
    await e.until(async () => (await e.value('P8')) == null)
    assert.deepEqual([await e.value('N8'), await e.value('O8'), await e.value('P8')], [null, null, null], 'Backspace clears the selection')
    assert.equal((await e.details()).editing, false, 'and does not start editing')
  }],
  ['b8-cmd-click-heading', async (e) => {
    const c = await heading(e, 'C')
    const g = await heading(e, 'G')
    await e.page.mouse.click(c.x, c.y)
    await e.until(async () => (await e.details()).selection === 'C:C')
    await e.page.keyboard.down('Shift')
    await e.page.mouse.click(g.x, g.y)
    await e.page.keyboard.up('Shift')
    await e.until(async () => (await e.details()).selection === 'C:G')
    const j = await heading(e, 'J')
    const modifier = process.platform === 'darwin' ? 'Meta' : 'Control'
    await e.page.keyboard.down(modifier)
    await e.page.mouse.click(j.x, j.y)
    await e.page.keyboard.up(modifier)
    await e.until(async () => (await e.details()).selection === 'C:G,J:J')
    assert.equal((await e.details()).selection, 'C:G,J:J', 'Cmd+click on J\'s heading adds the column')
  }],
  ['b6-cmd-click-takes-out', async (e) => {
    await drag(e, 'N3', 'P6')
    const modifier = process.platform === 'darwin' ? 'Meta' : 'Control'
    const at = await e.cellPoint('O4')
    await e.page.keyboard.down(modifier)
    await e.page.mouse.click(at.x, at.y)
    await e.page.keyboard.up(modifier)
    await e.until(async () => !selected((await e.details()).selection, 'O4'))
    const selection = (await e.details()).selection
    assert(!selected(selection, 'O4'), `Cmd+click takes O4 out: ${selection}`)
    for (const ref of ['N3', 'P3', 'N4', 'P4', 'N6', 'P6']) assert(selected(selection, ref), `${ref} stays selected: ${selection}`)
  }],
]

await segments('commit', list, (which) => open({ which }))
