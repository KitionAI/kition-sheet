// The cell being typed edits like text (plan stage 5, section 2 rows 22–24; walkthrough D5, R5, I3;
// WPS W-C3–W-C5, W-H5; a Mac's text keys): a double-click puts the caret where it was pressed;
// Option+← / → go by words, Cmd+← / → to the line's ends; ↑ goes to a cell's line above; F2
// switches Enter and Edit mode; Cmd+Z takes the typing back and goes on typing; Cmd+A and a
// triple-click select the cell's text, Cmd+C copies the text (not the cell); none of it is an undo
// step of the workbook; the caret blinks without the cells being drawn again.
// A person's keys and clicks; the engine's state read back.
// node tests/hand/cell-editor.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 150) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
const TEXT = 'harbour north'
/** Puts `text` in `cell`, its column wide enough to show it, and leaves the sheet's typing alone. */
const put = async (e, cell, text) => {
  await e.agent([
    { command: 'column.setWidth', params: { cols: cell.replace(/\d+$/, ''), chars: 30 } },
    { command: 'cell.set', params: { cell, input: text } },
  ])
}
const typed = async (e, text, ms = 300) => {
  await e.page.keyboard.type(text)
  await e.wait(ms)
}
const enter = async (e, cell, want) => {
  await press(e, 'Enter', 300)
  await e.until(async () => (await e.value(cell)) === want, 40)
  assert.equal(await e.value(cell), want)
}

const list = [
  ['d5-double-click-caret', async (e) => {
    await put(e, 'Q2', TEXT)
    const at = await e.cellPoint('Q2', { fx: 0.12 })
    await e.page.mouse.dblclick(at.x, at.y)
    await e.until(async () => (await e.details()).editing === true, 40)
    const caret = (await e.details()).editCaret
    assert(caret > 0 && caret < TEXT.length, `the caret is where the cell was double-clicked, not at the end (${caret})`)
    await typed(e, 'X')
    await enter(e, 'Q2', `${TEXT.slice(0, caret)}X${TEXT.slice(caret)}`)
  }],
  ['r5-word-and-line-keys', async (e) => {
    await put(e, 'Q3', TEXT)
    await e.click('Q3')
    await press(e, 'F2')
    await press(e, 'Alt+ArrowLeft')
    await typed(e, 'Y')
    await enter(e, 'Q3', 'harbour Ynorth')
    await e.click('Q3')
    await press(e, 'F2')
    await press(e, 'Meta+ArrowLeft')
    await typed(e, 'S')
    await press(e, 'Meta+Shift+ArrowRight')
    await typed(e, 'T')
    await enter(e, 'Q3', 'ST')
  }],
  ['r5-lines', async (e) => {
    await e.agent([{ command: 'cell.set', params: { cell: 'Q12', input: 'one\ntwo' } }])
    await e.click('Q12')
    await press(e, 'F2')
    await press(e, 'ArrowUp')
    assert.equal((await e.details()).editing, true, '↑ stays in the cell: it goes to the first line')
    await typed(e, '!')
    await enter(e, 'Q12', 'one!\ntwo')
  }],
  ['f2-switches-modes', async (e) => {
    await e.click('Q4')
    await typed(e, 'ab')
    await press(e, 'F2')
    await press(e, 'ArrowLeft')
    assert.equal((await e.details()).editing, true, 'after F2 ← moves the caret')
    await typed(e, 'c')
    await enter(e, 'Q4', 'acb')
  }],
  ['d16-caret-blinks', async (e) => {
    // The caret blinks over the rendered grid: the canvas repaints from the frame it has (the
    // caret's line on, then off) and the engine draws no cells for it.
    await put(e, 'Q9', TEXT)
    await e.click('Q9')
    await press(e, 'F2', 400)
    await e.frame().evaluate(() => {
      const seen = (window.__caret = { renders: 0, on: 0, off: 0, frame: null })
      const surface = (canvas) => canvas?.classList?.contains('kui-canvas-surface')
      for (const P of [window.CanvasRenderingContext2D?.prototype, window.OffscreenCanvasRenderingContext2D?.prototype]) {
        if (!P) continue
        const put = P.putImageData
        P.putImageData = function (...args) {
          if (!surface(this.canvas)) seen.renders += 1
          return put.apply(this, args)
        }
        const draw = P.drawImage
        P.drawImage = function (...args) {
          if (surface(this.canvas)) {
            if (seen.frame !== null) seen.frame ? (seen.on += 1) : (seen.off += 1)
            seen.frame = false
          }
          return draw.apply(this, args)
        }
        const fill = P.fillRect
        P.fillRect = function (x, y, w, h) {
          if (surface(this.canvas) && w <= 2 && h > 4) seen.frame = true
          return fill.call(this, x, y, w, h)
        }
      }
    })
    // The cells typed in may still be drawn once or twice after F2 on a slow machine: the blink is
    // measured from a moment nothing is drawn for 700 ms.
    for (let tries = 0; tries < 10; tries += 1) {
      await e.frame().evaluate(() => Object.assign(window.__caret, { renders: 0, on: 0, off: 0, frame: null }))
      await e.wait(700)
      if ((await e.frame().evaluate(() => window.__caret.renders)) === 0) break
    }
    await e.frame().evaluate(() => Object.assign(window.__caret, { renders: 0, on: 0, off: 0, frame: null }))
    await e.wait(1500)
    const seen = await e.frame().evaluate(() => window.__caret)
    assert.equal(seen.renders, 0, `no cells drawn while the caret blinks (${JSON.stringify(seen)})`)
    assert(seen.on > 0 && seen.off > 0, `the caret is drawn in some frames and not in others (${JSON.stringify(seen)})`)
    await press(e, 'Escape')
  }],
  ['i3-undo-while-editing', async (e) => {
    await e.click('Q5')
    await typed(e, 'abc')
    await press(e, 'ControlOrMeta+z', 300)
    const shown = await e.details()
    assert.equal(shown.editing, true, 'Cmd+Z leaves the cell open')
    assert.equal(shown.editText, '', 'and takes back what was typed')
    await typed(e, 'd')
    await enter(e, 'Q5', 'd')
  }],
  ['r5-select-all-and-copy', async (e) => {
    await put(e, 'Q6', TEXT)
    await e.click('Q6')
    await press(e, 'F2')
    await press(e, 'ControlOrMeta+a')
    const shown = await e.details()
    assert.equal(shown.selection, 'Q6', 'Cmd+A selects the text, not the sheet')
    assert.equal([shown.editAnchor, shown.editCaret].sort((a, b) => a - b).join(), `0,${TEXT.length}`, 'the whole text is selected')
    if (e.which === 'chromium') {
      await press(e, 'ControlOrMeta+c', 400)
      const copied = await e.frame().evaluate(() => navigator.clipboard.readText())
      assert.equal(copied, TEXT, 'Cmd+C copies the text')
      assert.equal((await e.details()).copied, null, 'not the cell')
    }
    await typed(e, 'Z')
    await enter(e, 'Q6', 'Z')
  }],
  ['r5-paste-while-editing', async (e) => {
    // Cmd+V while typing pastes the system clipboard's text at the caret (chromium reads it).
    if (e.which !== 'chromium') return
    await put(e, 'Q8', 'ab')
    await e.frame().evaluate(() => navigator.clipboard.writeText('xy'))
    await e.click('Q8')
    await press(e, 'F2')
    await press(e, 'ArrowLeft')
    await press(e, 'ControlOrMeta+v', 400)
    await enter(e, 'Q8', 'axyb')
  }],
  ['r5-triple-click', async (e) => {
    await put(e, 'Q7', TEXT)
    const at = await e.cellPoint('Q7', { fx: 0.1 })
    await e.page.mouse.click(at.x, at.y, { clickCount: 3 })
    await e.wait(400)
    await typed(e, 'Q')
    await enter(e, 'Q7', 'Q')
  }],
]

await segments('cell-editor', list, (which) => open({ which }))
