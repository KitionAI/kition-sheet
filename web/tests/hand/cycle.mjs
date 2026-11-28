// Moving the active cell by hand as WPS does (plan stage 0B, section 2 rows 4, 5, 8; walkthrough
// C2, C3, C6, C8, A6; WPS W-A10, W-B3, W-B4, W-B9–W-B11): Home, Cmd+Home and Cmd+End; Enter and
// Tab round a selection that stays; Enter after a run of Tabs back at its first column;
// Shift+Space and Ctrl+Space; the scrollbar's reach comes back after the last row.
// A person's keys, clicks and drags; the engine's state read back.
// node tests/hand/cycle.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 200) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
/** Drags with the mouse from one cell to another (press, moves, release). */
async function drag(e, from, to) {
  const a = await e.cellPoint(from)
  const b = await e.cellPoint(to)
  await e.page.mouse.move(a.x, a.y)
  await e.page.mouse.down()
  await e.page.mouse.move(b.x, b.y, { steps: 6 })
  await e.page.mouse.up()
  await e.until(async () => (await e.details()).selection === `${from}:${to}`)
}
const active = async (e) => (await e.details()).active

const list = [
  ['c6-home-end', async (e) => {
    // The used range's last cell, well past the sample's data.
    await e.agent([{ command: 'cell.set', params: { cell: 'AB400', input: 'last' } }])
    await e.click('G7')
    await press(e, 'Home')
    await e.until(async () => (await active(e)) === 'A7')
    assert.equal(await active(e), 'A7', 'Home goes to column A of the row')
    await press(e, 'End')
    assert.equal(await active(e), 'A7', 'End alone does nothing (no End mode)')
    await press(e, 'ControlOrMeta+End', 400)
    await e.until(async () => (await active(e)) === 'AB400')
    assert.equal(await active(e), 'AB400', 'Cmd+End goes to the used range\'s last cell')
    await e.until(async () => (await e.canvasState()).view.y > 1000)
    assert((await e.canvasState()).view.y > 1000, 'and the view follows it')
    await press(e, 'ControlOrMeta+Home', 400)
    await e.until(async () => (await active(e)) === 'A1')
    assert.equal(await active(e), 'A1', 'Cmd+Home goes to A1')
    await e.until(async () => (await e.canvasState()).view.y < 1)
    await e.click('G7')
    await press(e, 'Shift+Home')
    await e.until(async () => (await e.details()).selection === 'A7:G7')
    assert.equal((await e.details()).selection, 'A7:G7', 'Shift+Home selects to column A')
    await e.click('C3')
    await press(e, 'ControlOrMeta+Shift+End', 400)
    await e.until(async () => (await e.details()).selection === 'C3:AB400')
    assert.equal((await e.details()).selection, 'C3:AB400', 'Cmd+Shift+End selects to the last cell')
    assert.equal(await active(e), 'C3', 'the active cell stays')
    await e.agent([{ command: 'cell.set', params: { cell: 'AB400', input: '' } }])
    await press(e, 'ControlOrMeta+Home', 400)
  }],
  ['c3-enter-cycles', async (e) => {
    await drag(e, 'B2', 'C4')
    const seen = []
    for (let index = 0; index < 7; index += 1) {
      await press(e, 'Enter', 150)
      seen.push(await active(e))
    }
    assert.deepEqual(seen, ['B3', 'B4', 'C2', 'C3', 'C4', 'B2', 'B3'], 'Enter goes down each column of B2:C4, then round again')
    assert.equal((await e.details()).selection, 'B2:C4', 'the selection stays')
    await press(e, 'Shift+Enter', 150)
    assert.equal(await active(e), 'B2', 'Shift+Enter goes back')
    const tabs = []
    for (let index = 0; index < 3; index += 1) {
      await press(e, 'Tab', 150)
      tabs.push(await active(e))
    }
    assert.deepEqual(tabs, ['C2', 'B3', 'C3'], 'Tab goes along each row')
    assert.equal((await e.details()).selection, 'B2:C4', 'the selection still stays')
    // Typing in it: each value goes into the active cell and Enter goes on.
    await drag(e, 'N2', 'O3')
    for (const text of ['1', '2', '3', '4']) {
      await e.page.keyboard.type(text)
      await press(e, 'Enter', 150)
    }
    await e.until(async () => (await e.value('O3')) === 4)
    assert.deepEqual([await e.value('N2'), await e.value('N3'), await e.value('O2'), await e.value('O3')], [1, 2, 3, 4], 'typed in the selection, column by column')
    assert.equal((await e.details()).selection, 'N2:O3', 'and the selection stays')
  }],
  ['c2-tab-run-enter', async (e) => {
    await e.click('N7')
    for (const text of ['x', 'y']) {
      await e.page.keyboard.type(text)
      await press(e, 'Tab', 150)
    }
    await e.page.keyboard.type('z')
    await press(e, 'Enter', 250)
    await e.until(async () => (await active(e)) === 'N8')
    assert.equal(await active(e), 'N8', 'Enter after the Tabs goes back to column N, a row down')
    assert.deepEqual([await e.value('N7'), await e.value('O7'), await e.value('P7')], ['x', 'y', 'z'])
    await press(e, 'Tab')
    await press(e, 'Tab')
    await press(e, 'Enter', 250)
    assert.equal(await active(e), 'N9', 'without typing too')
    await press(e, 'Tab')
    await press(e, 'ArrowRight')
    await press(e, 'Enter', 250)
    assert.equal(await active(e), 'P10', 'an arrow ends the run')
  }],
  ['c8-row-column-keys', async (e) => {
    await e.click('N12')
    await press(e, 'Shift+Space', 300)
    await e.until(async () => (await e.details()).selection === '12:12')
    assert.equal((await e.details()).selection, '12:12', 'Shift+Space selects the row')
    assert.equal(await e.value('N12'), null, 'and types no space')
    await e.click('N13')
    await press(e, 'Control+Space', 300)
    await e.until(async () => (await e.details()).selection === 'N:N')
    assert.equal((await e.details()).selection, 'N:N', 'Ctrl+Space selects the column')
    await e.click('N14')
    await e.page.keyboard.type('a b')
    await press(e, 'Shift+Space')
    await e.page.keyboard.type('c')
    await press(e, 'Enter', 250)
    await e.until(async () => (await e.value('N14')) === 'a b c')
    assert.equal(await e.value('N14'), 'a b c', 'while typing, Shift+Space is a space')
  }],
  ['a6-scroll-range', async (e) => {
    const vertical = e.kit.getByTestId('sheet-grid-workspace').locator('[role="scrollbar"][aria-orientation="vertical"]')
    const reach = async () => Number(await vertical.getAttribute('aria-valuemax'))
    await e.click('R2')
    await e.wait(400)
    const before = await reach()
    await press(e, 'ControlOrMeta+ArrowDown', 600)
    await e.until(async () => (await active(e)) === 'R1048576')
    await e.until(async () => (await reach()) > before * 100)
    assert((await reach()) > before * 100, `at the last row the sheet reaches it (${await reach()})`)
    await press(e, 'ControlOrMeta+ArrowUp', 600)
    await e.until(async () => (await active(e)) === 'R1')
    await e.until(async () => (await reach()) < before * 4, 40)
    assert((await reach()) < before * 4, `back at the top the reach comes back (${before} → ${await reach()})`)
  }],
]

await segments('cycle', list, (which) => open({ which }))
