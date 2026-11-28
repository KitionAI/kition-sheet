// The keyboard reaches the grid from the first key and keeps reaching it (plan stage 0A;
// walkthrough K1, K2, B15): a person's keys and clicks only, the engine's state read back through
// the development host. The first segment needs the editor just opened, nothing clicked.
// node tests/hand/focus.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const list = [
  ['k1-first-key', async (e) => {
    // Nothing clicked yet: the workbook just opened, A1 drawn as the active cell.
    assert.equal((await e.details()).active, 'A1', 'A1 is the active cell')
    await e.page.keyboard.type('first', { delay: 20 })
    await e.page.keyboard.press('Enter')
    await e.until(async () => (await e.value('A1')) === 'first')
    assert.equal(await e.value('A1'), 'first', 'typing right after the workbook opened goes into A1')
    assert.equal((await e.details()).active, 'A2', 'and Enter moves down')
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the grid has the keyboard')
  }],
  ['k2-name-box', async (e) => {
    const box = e.kit.getByTestId('sheet-name-box')
    await box.click()
    for (const key of ['D', '5']) await e.page.keyboard.press(key)
    await e.page.keyboard.press('Enter')
    await e.until(async () => (await e.details()).active === 'D5')
    assert.equal((await e.details()).active, 'D5', 'the Name Box goes to D5')
    await e.page.keyboard.type('named', { delay: 20 })
    await e.page.keyboard.press('Enter')
    await e.until(async () => (await e.value('D5')) === 'named')
    assert.equal(await e.value('D5'), 'named', 'typing after the Name Box\'s Enter goes into D5')
    await e.page.keyboard.press('ArrowRight')
    await e.until(async () => (await e.details()).active === 'E6')
    assert.equal((await e.details()).active, 'E6', 'and the arrows move the active cell')
    // Escape in the Name Box gives the keyboard back too.
    await box.click()
    await e.page.keyboard.press('Escape')
    await e.wait(200)
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'Escape in the Name Box gives the grid the keyboard')
  }],
  ['b15-typing-scrolls-back', async (e) => {
    await e.click('B2')
    const box = await e.surface()
    await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    await e.page.mouse.wheel(0, 3000)
    await e.until(async () => (await e.canvasState()).view.y > 2000)
    assert((await e.canvasState()).view.y > 2000, 'the view scrolled away from B2')
    await e.page.keyboard.type('blind', { delay: 20 })
    await e.until(async () => (await e.canvasState()).view.y < 100)
    const { view } = await e.canvasState()
    const [, y0, , y1] = (await e.details()).activeRect
    assert(view.y <= y0 && y1 - view.y <= box.height / view.zoom, `typing brings B2 back into view (view at ${view.y}, B2 at ${y0}–${y1})`)
    assert.equal((await e.details()).editing, true, 'B2 is being typed')
    await e.page.keyboard.press('Enter')
    await e.until(async () => (await e.value('B2')) === 'blind')
    assert.equal(await e.value('B2'), 'blind', 'the text is in B2')
  }],
]

await segments('focus', list, (which) => open({ which }))
