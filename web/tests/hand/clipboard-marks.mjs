// The selection's frame and the copy's marching ants by hand, as WPS shows them (plan stage 2,
// section 2 rows 14 and 16; walkthrough B3, J3, J4; WPS W-A1, W-A3, W-F1, W-F3, W-F4, W-F6):
// a dragged selection has a frame round all of it; Cmd+C puts dashed ants round the copy; Enter
// on the target pastes once and ends the copy (Cmd+V then pastes nothing more); Cmd+V twice gives
// two copies; Escape ends the copy and leaves the system clipboard's text; a 2 × 2 copy pasted
// into 4 × 4 cells fills them four times. A person's mouse and keys; the engine's state and what
// is drawn over the grid read back.
// node tests/hand/clipboard-marks.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 300) => {
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
/** The rectangle of cells `from`–`to` on the page. */
async function rect(e, from, to) {
  const a = await e.cellPoint(from, { fx: 0, fy: 0 })
  const b = await e.cellPoint(to, { fx: 1, fy: 1 })
  return { x0: a.x, y0: a.y, x1: b.x, y1: b.y }
}
const same = (a, b) => Math.abs(a.x0 - b.x0) < 2 && Math.abs(a.y0 - b.y0) < 2 && Math.abs(a.x1 - b.x1) < 2 && Math.abs(a.y1 - b.y1) < 2
/** The frames drawn over the grid now (dashed ones when `dashed`). */
const frames = async (e, dashed) => ((await e.marks())?.frames ?? []).filter((item) => item.dashed === dashed)
const framed = async (e, dashed, area) => (await frames(e, dashed)).some((item) => same(item, area))
const set = (e, range, values) => e.agent([{ command: 'range.setValues', params: { range, values } }])

const list = [
  ['b3-selection-outline', async (e) => {
    await drag(e, 'N2', 'P6')
    const area = await rect(e, 'N2', 'P6')
    await e.until(() => framed(e, false, area), 20)
    assert(await framed(e, false, area), `a frame round N2:P6: ${JSON.stringify(await frames(e, false))}`)
    const cell = await rect(e, 'N2', 'N2')
    assert(!(await framed(e, false, cell)), 'the active cell has no frame of its own inside it')
  }],
  ['j3-marching-ants', async (e) => {
    await set(e, 'N10', [['a', 'b'], ['c', 'd']])
    await drag(e, 'N10', 'O11')
    await press(e, 'ControlOrMeta+c', 400)
    const source = await rect(e, 'N10', 'O11')
    await e.until(() => framed(e, true, source), 30)
    assert(await framed(e, true, source), `ants round N10:O11: ${JSON.stringify(await frames(e, true))}`)
    assert(!(await framed(e, false, source)), 'in place of its frame')
    assert.equal((await e.details()).copied?.range, 'N10:O11')
    await press(e, 'Escape', 400)
    await e.until(async () => (await e.details()).copied == null)
    assert.equal((await e.details()).copied, null, 'Escape ended the copy')
    await e.until(async () => !(await framed(e, true, source)), 20)
    assert(!(await framed(e, true, source)), 'the ants are gone')
    if (e.which === 'chromium') {
      const text = await e.frame().evaluate(() => navigator.clipboard.readText())
      assert.match(text, /a\tb/, `the system clipboard keeps the text: ${JSON.stringify(text)}`)
    }
    await e.click('R10')
    await press(e, 'ControlOrMeta+v', 600)
    assert.equal(await e.value('R10'), null, 'the ended copy pastes nothing')
  }],
  ['j4-enter-pastes', async (e) => {
    await set(e, 'N14', [['x']])
    await e.click('N14')
    await press(e, 'ControlOrMeta+c', 400)
    await e.until(async () => (await e.details()).copied?.range === 'N14')
    await e.click('P14')
    await press(e, 'Enter', 400)
    await e.until(async () => (await e.value('P14')) === 'x')
    assert.equal(await e.value('P14'), 'x', 'Enter pasted')
    assert.equal((await e.details()).active, 'P14', 'and stayed')
    assert.equal((await e.details()).copied, null, 'the copy ended')
    assert(!(await framed(e, true, await rect(e, 'N14', 'N14'))), 'no ants')
    await e.click('P16')
    await press(e, 'ControlOrMeta+v', 600)
    assert.equal(await e.value('P16'), null, 'Cmd+V does not paste it again')
    // Cmd+V twice: two copies, the ants still there.
    await e.click('N14')
    await press(e, 'ControlOrMeta+c', 400)
    await e.click('R14')
    await press(e, 'ControlOrMeta+v', 400)
    await e.click('R16')
    await press(e, 'ControlOrMeta+v', 400)
    await e.until(async () => (await e.value('R16')) === 'x')
    assert.deepEqual([await e.value('R14'), await e.value('R16')], ['x', 'x'], 'two pastes, two copies')
    assert.equal((await e.details()).copied?.range, 'N14', 'still copied')
    await press(e, 'Escape')
  }],
  ['f4-tiles', async (e) => {
    await set(e, 'N20', [[1, 2], [3, 4]])
    await drag(e, 'N20', 'O21')
    await press(e, 'ControlOrMeta+c', 400)
    await drag(e, 'Q20', 'T23')
    await press(e, 'ControlOrMeta+v', 600)
    await e.until(async () => (await e.value('T23')) === 4)
    assert.deepEqual([await e.value('Q20'), await e.value('S20'), await e.value('Q22'), await e.value('T23')], [1, 1, 1, 4], `a 2 × 2 copy fills 4 × 4 four times: ${JSON.stringify([await e.value('Q20'), await e.value('S20'), await e.value('Q22'), await e.value('T23'), (await e.details()).selection])}`)
    await press(e, 'Escape')
  }],
]

await segments('clipboard-marks', list, (which) => open({ which }))
