// An input method in the grid by hand, as WPS shows it (plan stage 3, section 2 row 19;
// walkthrough D13; WPS W-T1–W-T4): pinyin composing starts typing in the cell, shows there (and in
// the formula bar) without changing the cell, the undo history or the file; the chosen text goes in
// and the cell stays open, Enter then enters it; Enter and Escape while composing are the input
// method's; the text input the input method writes in sits at the cell, also zoomed and scrolled.
// The compositions are Chromium's (DevTools' Input.imeSetComposition and Input.insertText); in
// WebKit only what needs no composition runs. The engine's state read back.
// node tests/hand/ime.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 300) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
/** The DevTools session that composes, once per page. */
const sessions = new WeakMap()
const devtools = async (e) => {
  if (!sessions.has(e.page)) sessions.set(e.page, await e.page.context().newCDPSession(e.page))
  return sessions.get(e.page)
}
/** Composes `text` as an input method does (an empty text cancels the composition). */
const compose = async (e, text, ms = 250) => {
  const cdp = await devtools(e)
  await cdp.send('Input.imeSetComposition', { text, selectionStart: text.length, selectionEnd: text.length })
  await e.wait(ms)
}
/** The input method's choice: the text goes in and the composition ends. */
const choose = async (e, text, ms = 300) => {
  const cdp = await devtools(e)
  await cdp.send('Input.insertText', { text })
  await e.wait(ms)
}
const history = async (e) => JSON.stringify(await e.ask('history.list'))
/** Whether the grid's text input is at cell `ref` (its top-left within 4 px of the cell). */
async function inputAt(e, ref) {
  const field = await e.kit.locator('textarea.kui-text-sink').boundingBox()
  const a = await e.cellPoint(ref, { fx: 0, fy: 0 })
  const b = await e.cellPoint(ref, { fx: 1, fy: 1 })
  const near = field && field.x >= a.x - 4 && field.x <= b.x + 4 && field.y >= a.y - 4 && field.y <= b.y + 4
  return { near, field, cell: { x0: a.x, y0: a.y, x1: b.x, y1: b.y } }
}
const chromiumOnly = (e) => e.which !== 'chromium'

const list = [
  ['d13-ime-composition', async (e) => {
    if (chromiumOnly(e)) return
    await e.agent([{ command: 'cell.set', params: { cell: 'N30', input: 'old' } }])
    await e.click('N30')
    const before = await history(e)
    await compose(e, 'n')
    await e.until(async () => (await e.details()).preedit?.text === 'n')
    let shown = await e.details()
    assert.equal(shown.editing, true, 'the first letter starts typing in the cell')
    assert.equal(shown.editText, '', 'over what the cell held, and is not its text yet')
    await compose(e, 'ni')
    await compose(e, 'ni hao')
    await e.until(async () => (await e.details()).preedit?.text === 'ni hao')
    shown = await e.details()
    assert.equal(shown.preedit.text, 'ni hao', 'the composition shows in the cell')
    assert.equal(await e.value('N30'), 'old', 'the cell is unchanged')
    assert.equal(await history(e), before, 'no undo step')
    const bar = await e.kit.getByTestId('sheet-formula').inputValue().catch(() => null)
    if (bar !== null) assert.equal(bar, 'ni hao', 'the formula bar shows it too')
    const at = await inputAt(e, 'N30')
    assert(at.near, `the input method's field is at the cell: ${JSON.stringify(at)}`)
    await choose(e, '你好')
    await e.until(async () => (await e.details()).editText === '你好')
    shown = await e.details()
    assert.deepEqual([shown.editing, shown.editText, shown.preedit], [true, '你好', null], 'chosen: in the cell, still typing')
    assert.equal(await e.value('N30'), 'old', 'not entered yet')
    await press(e, 'Enter', 400)
    await e.until(async () => (await e.value('N30')) === '你好')
    assert.equal(await e.value('N30'), '你好', 'Enter enters it')
    assert.notEqual((await e.ask('cell.get', { cell: 'N30' })).value, 'n你好', 'the first letter did not go in as typed')
  }],
  ['d13-ime-keys', async (e) => {
    if (chromiumOnly(e)) return
    await e.click('N32')
    await compose(e, 'ma')
    await e.until(async () => (await e.details()).preedit?.text === 'ma')
    // Enter while composing is the input method's: the cell is not entered.
    await e.kit.locator('textarea.kui-text-sink').evaluate((field) => {
      field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 229, isComposing: true, bubbles: true, cancelable: true }))
      field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', keyCode: 229, isComposing: true, bubbles: true, cancelable: true }))
      field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Backspace', code: 'Backspace', keyCode: 229, isComposing: true, bubbles: true, cancelable: true }))
    })
    await e.wait(400)
    let shown = await e.details()
    assert.equal(shown.editing, true, 'Enter, Escape and Backspace while composing leave the cell typed')
    assert.equal(shown.active, 'N32', 'and in place')
    // Escape in the input method cancels the composition; the cell stays open, empty.
    await compose(e, '')
    await e.until(async () => (await e.details()).preedit == null)
    shown = await e.details()
    assert.deepEqual([shown.editing, shown.preedit], [true, null], 'the composition is gone, the cell still typed')
    // A second Escape gives the typing up: the cell stays empty.
    await press(e, 'Escape', 400)
    await e.until(async () => (await e.details()).editing === false)
    assert.equal((await e.details()).editing, false)
    assert.equal(await e.value('N32'), null, 'nothing went in')
  }],
  ['d13-ime-zoomed-scrolled', async (e) => {
    if (chromiumOnly(e)) return
    const box = await e.surface()
    await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    await e.page.mouse.wheel(0, 600)
    await e.wait(400)
    await e.page.keyboard.down('Control')
    await e.page.mouse.wheel(0, -100)
    await e.page.keyboard.up('Control')
    await e.wait(500)
    assert.notEqual(await e.zoom(), 100, 'zoomed')
    assert((await e.canvasState()).view.y > 0, 'scrolled')
    const { view } = await e.canvasState()
    // A cell in the middle of the view.
    const target = 'P' + Math.round(view.y / 21 + 15)
    await e.click(target)
    await compose(e, 'zhong')
    await e.until(async () => (await e.details()).preedit?.text === 'zhong')
    const at = await inputAt(e, target)
    assert(at.near, `zoomed and scrolled, the field is at ${target}: ${JSON.stringify(at)}`)
    await choose(e, '中')
    await press(e, 'Enter', 400)
    await e.until(async () => (await e.value(target)) === '中')
    assert.equal(await e.value(target), '中')
  }],
  ['d13-typing-without-composing', async (e) => {
    // Every browser: typing still goes straight into the cell, and the field follows the cell.
    await e.home()
    await e.click('R30')
    await e.page.keyboard.type('ab')
    await e.until(async () => (await e.details()).editText === 'ab')
    const at = await inputAt(e, 'R30')
    assert(at.near, `the field is at the cell being typed: ${JSON.stringify(at)}`)
    await press(e, 'Enter', 400)
    await e.until(async () => (await e.value('R30')) === 'ab')
    assert.equal(await e.value('R30'), 'ab')
  }],
]

await segments('ime', list, (which) => open({ which }))
