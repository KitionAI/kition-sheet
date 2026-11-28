// The editor's own keys as WPS has them (plan stage 0A, section 2 rows 9, 10, 12, 20, 21;
// walkthrough A8, C7, H4, I2, Q1; WPS W-B12, W-B13, W-H2, W-I5, W-I9, W-M5, W-O1): Page Down and
// Up a screenful, Cmd+Shift+Z redoes, Cmd+0 hides columns (the zoom stays), Cmd+Shift+= and Cmd+-
// insert and delete whole rows, Cmd+Shift+L the filter, Cmd+F / Ctrl+H / Ctrl+G open their dialogs.
// A person's keys and clicks; the engine's state read back.
// node tests/hand/keys.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 250) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
const cellRow = (ref) => Number(/\d+$/.exec(ref)[0])
const sheetOf = async (e) => {
  const name = (await e.details()).sheet
  return (await e.ask('file.inspect')).sheets.find((sheet) => sheet.name === name)
}
/** How many whole rows (or columns) the grid shows below (beside) its headings, as Page Down moves. */
async function screenful(e, across = false) {
  const box = await e.surface()
  const { view, pins } = await e.canvasState()
  const [width, height] = (await e.details()).defaultSize
  return across ? Math.floor((box.width / view.zoom - pins.left) / width) : Math.floor((box.height / view.zoom - pins.top) / height)
}
/** Clicks a column's heading (`C`), as a person selects the column. */
async function clickHeading(e, column) {
  const at = await e.cellPoint(`${column}1`)
  const box = await e.surface()
  const { pins, view } = await e.canvasState()
  await e.page.mouse.click(at.x, box.y + (pins.top / 2) * view.zoom)
  await e.until(async () => (await e.details()).selection === `${column}:${column}`)
}
const dialogOpen = (e) => e.kit.locator('[role=dialog]').first().isVisible().catch(() => false)

const list = [
  ['c7-page-keys', async (e) => {
    await e.click('B3')
    const rows = await screenful(e)
    assert(rows > 10, `the grid shows ${rows} rows`)
    const before = (await e.canvasState()).view
    await press(e, 'PageDown')
    await e.until(async () => (await e.details()).active !== 'B3')
    assert.equal((await e.details()).active, `B${3 + rows}`, `Page Down moves the active cell ${rows} rows down`)
    const [, height] = (await e.details()).defaultSize
    await e.until(async () => Math.abs((await e.canvasState()).view.y - (before.y + rows * height)) < 1)
    assert(Math.abs((await e.canvasState()).view.y - (before.y + rows * height)) < 1, 'and the view as far, so the cell stays where it was on the screen')
    await press(e, 'PageUp')
    await e.until(async () => (await e.details()).active === 'B3')
    assert.equal((await e.details()).active, 'B3', 'Page Up goes back to B3')
    await e.until(async () => Math.abs((await e.canvasState()).view.y - before.y) < 1)
    assert(Math.abs((await e.canvasState()).view.y - before.y) < 1, 'and the view back')
    await press(e, 'Shift+PageDown')
    await e.until(async () => (await e.details()).selection !== 'B3')
    const shown = await e.details()
    assert.equal(shown.selection, `B3:B${3 + rows}`, 'Shift+Page Down extends the selection a screenful')
    assert.equal(shown.active, 'B3', 'the active cell stays')
    await press(e, 'Shift+PageUp')
    await e.until(async () => (await e.details()).selection === 'B3')
    assert.equal((await e.details()).selection, 'B3', 'Shift+Page Up takes the screenful back')
    const cols = await screenful(e, true)
    await press(e, 'Alt+PageDown')
    await e.until(async () => (await e.details()).active !== 'B3')
    assert.equal(cellRow((await e.details()).active), 3, 'Alt+Page Down stays on the row')
    assert.notEqual((await e.details()).active, 'B3', `Alt+Page Down moves ${cols} columns right`)
    await press(e, 'Alt+PageUp')
    await e.until(async () => (await e.details()).active === 'B3')
    assert.equal((await e.details()).active, 'B3', 'Alt+Page Up goes back')
  }],
  ['i2-redo-key', async (e) => {
    await e.click('J2')
    await e.page.keyboard.type('redo me', { delay: 15 })
    await press(e, 'Enter')
    assert.equal(await e.value('J2'), 'redo me')
    await press(e, 'ControlOrMeta+z', 400)
    await e.until(async () => (await e.value('J2')) == null)
    assert.equal(await e.value('J2'), null, 'Cmd+Z undoes the typing')
    await press(e, 'ControlOrMeta+Shift+z', 400)
    await e.until(async () => (await e.value('J2')) === 'redo me')
    assert.equal(await e.value('J2'), 'redo me', 'Cmd+Shift+Z redoes it')
    await press(e, 'ControlOrMeta+z', 400)
    await press(e, 'ControlOrMeta+y', 400)
    await e.until(async () => (await e.value('J2')) === 'redo me')
    assert.equal(await e.value('J2'), 'redo me', 'Cmd+Y still redoes')
  }],
  ['a8-cmd-0-hides-column', async (e) => {
    await e.click('A1')
    const zoom = await e.zoom()
    await clickHeading(e, 'C')
    await press(e, 'ControlOrMeta+0', 400)
    await e.until(async () => (await sheetOf(e)).hiddenColumns.includes('C'))
    assert.deepEqual((await sheetOf(e)).hiddenColumns, ['C'], 'Cmd+0 hides column C')
    assert.equal(await e.zoom(), zoom, `the zoom stays ${zoom}%`)
    await press(e, 'ControlOrMeta+Shift+0', 400)
    await e.until(async () => (await sheetOf(e)).hiddenColumns.length === 0)
    assert.deepEqual((await sheetOf(e)).hiddenColumns, [], 'Cmd+Shift+0 shows it again')
    assert.equal(await e.zoom(), zoom, 'the zoom still stays')
  }],
  // The sample's chart lies over H3:L18: these cells are clear of it.
  ['insert-delete-rows', async (e) => {
    await e.agent([{ command: 'cell.set', params: { cell: 'T8', input: 'row eight' } }])
    await e.click('T8')
    // The row's heading: a whole row.
    const at = await e.cellPoint('A8')
    const box = await e.surface()
    const { pins, view } = await e.canvasState()
    await e.page.mouse.click(box.x + (pins.left / 2) * view.zoom, at.y)
    await e.until(async () => (await e.details()).selection === '8:8')
    await press(e, 'ControlOrMeta+Shift+Equal', 400)
    await e.until(async () => (await e.value('T9')) === 'row eight')
    assert.equal(await e.value('T9'), 'row eight', 'Cmd+Shift+= inserts a row above the selected row')
    await press(e, 'ControlOrMeta+Minus', 400)
    await e.until(async () => (await e.value('T8')) === 'row eight')
    assert.equal(await e.value('T8'), 'row eight', 'Cmd+- deletes the selected row')
    assert.equal(await e.zoom(), 100, 'neither zooms')
    // With only a cell selected Cmd+- deletes nothing (WPS asks first, plan stage 7): it zooms out.
    await e.click('T12')
    await e.click('T8')
    await e.until(async () => (await e.details()).selection === 'T8')
    await e.wait(200)
    await press(e, 'Meta+Minus', 400)
    assert.equal(await e.value('T8'), 'row eight', 'Cmd+- on a cell deletes no row')
    assert.equal(await e.zoom(), 90, 'it zooms out 10 points')
    await press(e, 'Meta+Shift+Equal', 400)
    assert.equal(await e.value('T9'), null, 'Cmd+Shift+= on a cell inserts no row')
    assert.equal(await e.zoom(), 100, 'it zooms in 10 points')
  }],
  ['filter-key', async (e) => {
    await e.click('A2')
    await press(e, 'ControlOrMeta+Shift+L', 400)
    await e.until(async () => (await sheetOf(e)).autofilter != null)
    assert.notEqual((await sheetOf(e)).autofilter, null, 'Cmd+Shift+L turns the filter on')
    await press(e, 'ControlOrMeta+Shift+L', 400)
    await e.until(async () => (await sheetOf(e)).autofilter == null)
    assert.equal((await sheetOf(e)).autofilter, null, 'and off again')
    await press(e, 'ControlOrMeta+Shift+F', 400)
    assert.equal((await sheetOf(e)).autofilter, null, 'Cmd+Shift+F is not the filter\'s key')
  }],
  ['q1-find-keys', async (e) => {
    await e.click('A1')
    for (const key of ['ControlOrMeta+f', 'Control+h', 'Control+g']) {
      await press(e, key, 400)
      await e.until(() => dialogOpen(e), 30)
      assert(await dialogOpen(e), `${key} opens its dialog`)
      await press(e, 'Escape', 300)
      await e.until(async () => !(await dialogOpen(e)), 30)
    }
  }],
]

await segments('keys', list, (which) => open({ which }))
