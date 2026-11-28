// Scrolling at the grid's edge during a drag, as WPS does (plan stage 1, section 2 row 3; walkthrough
// B10; WPS W-A11, W-A12): a drag held past the bottom or the right edge scrolls the view, the pointer
// still, and the selection reaches the cells that come into view; inside the grid nothing scrolls;
// the fill handle and pointing at cells for a formula scroll too. A person's mouse; the engine's
// state and the canvas's view read back.
// node tests/hand/edge-scroll.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { colIndex, open, segments } from './harness.mjs'

/** The last row (column) of a selection like `B2:B40`, from 0. */
const lastRow = (selection) => Number(/(\d+)$/.exec(selection)?.[1] ?? 0) - 1
const lastCol = (selection) => colIndex(/:([A-Z]+)\d+$/.exec(selection)?.[1] ?? 'A')

/** Back to the top-left at 100%, with nothing being dragged. */
async function fresh(e) {
  await e.page.mouse.up().catch(() => undefined)
  await e.home()
  await e.until(async () => {
    const { view } = await e.canvasState()
    return view.x <= 0 && view.y <= 0
  })
}

/** Presses at `from`, drags to `to` (page points) in steps, then holds still for `ms`. */
async function holdAt(e, from, to, ms) {
  await e.page.mouse.move(from.x, from.y)
  await e.page.mouse.down()
  await e.page.mouse.move(to.x, to.y, { steps: 10 })
  await e.wait(ms)
}

const list = [
  ['b10-edge-down', async (e) => {
    await fresh(e)
    await e.click('B2')
    const box = await e.surface()
    const from = await e.cellPoint('B2')
    await holdAt(e, from, { x: from.x, y: box.y + box.height + 40 }, 2000)
    const { view } = await e.canvasState()
    const during = (await e.details()).selection
    await e.page.mouse.up()
    await e.wait(300)
    assert(view.y > 40, `the view scrolled down with the pointer still (${view.y})`)
    const shownRows = Math.floor(box.height / 19)
    const selection = (await e.details()).selection
    assert(lastRow(selection) > shownRows, `the selection reaches rows that came into view: ${selection} (during ${during})`)
    assert(selection.startsWith('B2:B'), `from B2 down column B: ${selection}`)
  }],
  ['b10-edge-right', async (e) => {
    await fresh(e)
    await e.click('B3')
    const box = await e.surface()
    const from = await e.cellPoint('B3')
    const width = e.page.viewportSize().width
    const past = Math.max(8, Math.min(40, width - 2 - (box.x + box.width)))
    await holdAt(e, from, { x: box.x + box.width + past, y: from.y }, 2000)
    const { view } = await e.canvasState()
    await e.page.mouse.up()
    await e.wait(300)
    assert(view.x > 30, `the view scrolled right (${view.x}, ${past} px past)`)
    const selection = (await e.details()).selection
    assert(lastCol(selection) > Math.floor(box.width / 69), `the selection reaches columns that came into view: ${selection}`)
  }],
  ['b10-inside-no-scroll', async (e) => {
    await fresh(e)
    await e.click('C2')
    const box = await e.surface()
    const from = await e.cellPoint('C2')
    await holdAt(e, from, { x: from.x, y: box.y + box.height - 6 }, 1500)
    const { view } = await e.canvasState()
    await e.page.mouse.up()
    assert.equal(view.y, 0, 'the pointer on the grid\'s last row does not scroll')
  }],
  ['b10-fill-handle-edge', async (e) => {
    await fresh(e)
    await e.agent([{ command: 'cell.set', params: { cell: 'R2', input: '1' } }])
    await e.click('R2')
    const box = await e.surface()
    const at = await e.cellPoint('R2', { fx: 1, fy: 1 })
    await e.page.mouse.move(at.x - 1, at.y - 1)
    await e.until(async () => (await e.cursor()) === 'crosshair', 20)
    await holdAt(e, { x: at.x - 1, y: at.y - 1 }, { x: at.x - 10, y: box.y + box.height + 30 }, 1500)
    const { view } = await e.canvasState()
    await e.page.mouse.up()
    await e.wait(400)
    assert(view.y > 20, `the fill handle's drag scrolled (${view.y})`)
    const selection = (await e.details()).selection
    const end = lastRow(selection)
    assert(end > Math.floor(box.height / 19), `filled past the first view: ${selection}`)
    assert.equal(await e.value(`R${end + 1}`), end, 'counting on to the last cell')
  }],
  ['b10-pointing-edge', async (e) => {
    await fresh(e)
    await e.click('T2')
    await e.page.keyboard.type('=SUM(')
    await e.wait(200)
    const box = await e.surface()
    const from = await e.cellPoint('U2')
    await holdAt(e, from, { x: from.x, y: box.y + box.height + 30 }, 1500)
    const { view } = await e.canvasState()
    await e.page.mouse.up()
    await e.wait(300)
    const text = (await e.details()).editText ?? ''
    await e.page.keyboard.press('Escape')
    assert(view.y > 20, `pointing scrolled (${view.y})`)
    const row = Number(/U2:U(\d+)/.exec(text)?.[1] ?? 0)
    assert(row > Math.floor(box.height / 19), `the reference reaches rows that came into view: ${text}`)
  }],
]

await segments('edge-scroll', list, (which) => open({ which }))
