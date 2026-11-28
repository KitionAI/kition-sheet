// Formula details (plan stage 6, items 1 and 2; walkthrough E6, E8): Enter on a formula that does
// not parse (`=1+`) leaves the cell typed with the reason under it, which goes when the formula is
// put right; Alt+= (and Cmd+Shift+T) types AutoSum's guess with the range selected and enters it
// only on Enter.
// A person's keys and clicks; the engine's state read back.
// node tests/hand/formula-details.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 250) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
const note = (e) => e.kit.getByTestId('sheet-formula-error')

const list = [
  ['e6-bad-formula-message', async (e) => {
    await e.click('B2')
    await e.page.keyboard.type('=1+')
    await press(e, 'Enter', 400)
    const shown = await e.details()
    assert.equal(shown.editing, true, 'the cell stays typed')
    assert.equal(shown.editText, '=1+')
    assert.equal(shown.active, 'B2', 'the active cell did not move')
    await e.until(() => note(e).isVisible(), 30)
    assert(await note(e).isVisible(), 'the reason shows under the cell')
    assert.match(await note(e).textContent(), /formula/i)
    await e.page.keyboard.type('2')
    await e.until(async () => !(await note(e).isVisible()), 30)
    assert.equal(await note(e).isVisible(), false, 'it goes when the text changes')
    await press(e, 'Enter', 400)
    await e.until(async () => (await e.value('B2')) === 3, 30)
    assert.equal(await e.value('B2'), 3)
  }],
  ['e8-autosum-confirms', async (e) => {
    await e.agent([2, 3, 4, 5].map((row) => ({ command: 'cell.set', params: { cell: `E${row}`, input: String(row * 10) } })))
    await e.click('E6')
    const steps = async () => (await e.ask('history.list'))?.undo?.length ?? 0
    const before = await steps()
    await press(e, 'Alt+Equal', 400)
    await e.until(async () => (await e.details()).editing === true, 30)
    const shown = await e.details()
    assert.equal(shown.editText, '=SUM(E2:E5)', 'the guess is typed')
    assert.deepEqual([shown.editAnchor, shown.editCaret], [5, 10], 'with its range selected')
    assert.equal(await steps(), before, 'not entered before Enter')
    await press(e, 'Enter', 400)
    await e.until(async () => (await steps()) === before + 1, 30)
    assert.equal(await steps(), before + 1, 'Enter enters it, one step')
    await e.until(async () => (await e.value('E6')) === 140, 30)
    assert.equal(await e.value('E6'), 140)
    await e.click('E6')
    await press(e, 'Delete')
    await press(e, process.platform === 'darwin' && process.env.PLAYWRIGHT_NO_UA_PLATFORM !== '1' ? 'Meta+Shift+T' : 'Control+Shift+T', 400)
    await e.until(async () => (await e.details()).editing === true, 30)
    assert.equal((await e.details()).editText, '=SUM(E2:E5)', 'Cmd+Shift+T does the same')
    await press(e, 'Escape')
  }],
]

await segments('formula-details', list, (which) => open({ which }))
