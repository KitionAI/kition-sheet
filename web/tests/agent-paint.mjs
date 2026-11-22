// An agent's steps repaint the person's grid only when they change what it shows (kition-sdk's
// `agentSkipsUnchangedPaint`, turned on in `src/App.tsx`; regression case
// `regressions/issue-agent-reads-repaint`). Reads, and the agent selecting on its own, ask the
// engine for no paint and leave the person's paint in progress to land; a value, a fill, a
// conditional format, a recalculation of RAND(), frozen panes and the zoom an agent sets are
// repainted, and the person sees them.
// node tests/agent-paint.mjs [chromium|webkit]
import { startGolden } from './golden-harness.mjs'

const h = await startGolden({ prefix: 'PAINT-' })
const { page, kit, step, section, until, ai, exec, details, nameBox, clickCell, signature, tab, openEditor } = h

// Every engine render the canvas asks for (each paint, whole or a patch), counted where the editor
// sends it to the engine's worker.
await page.addInitScript(() => {
  const send = Worker.prototype.postMessage
  globalThis.__renders = 0
  Worker.prototype.postMessage = function (message, ...rest) {
    const method = message?.request?.method
    if (method === 'render' || method === 'renderArea') globalThis.__renders += 1
    return send.call(this, message, ...rest)
  }
})

/** How many renders the canvas has asked the engine for so far. */
const renders = () => kit.locator('body').evaluate(() => globalThis.__renders ?? -1)
/** Waits until the canvas asks for no more renders (the paints in flight landed). */
const quiet = async () => {
  for (let i = 0; i < 40; i += 1) {
    const before = await renders()
    await page.waitForTimeout(500)
    if ((await renders()) === before) return
  }
}

/**
 * What the grid shows over a cell (every layer composed; `col`, `row` from 0, the sheet at its
 * top-left at 100%), inset from its gridlines: a fingerprint, the colour at its middle, and how many
 * dark (text) pixels its left and right thirds hold.
 */
const cellShows = (col, row) =>
  kit.getByTestId('sheet-canvas-region').evaluate(
    (element, [x0, y0, w, h]) => {
      const box = element.getBoundingClientRect()
      const out = document.createElement('canvas')
      out.width = w
      out.height = h
      const context = out.getContext('2d')
      for (const canvas of element.querySelectorAll('canvas')) {
        const rect = canvas.getBoundingClientRect()
        if (!rect.width) continue
        const ratio = canvas.width / rect.width
        const sx = (box.left + x0 - rect.left) * ratio
        const sy = (box.top + y0 - rect.top) * ratio
        context.drawImage(canvas, sx, sy, w * ratio, h * ratio, 0, 0, w, h)
      }
      const data = context.getImageData(0, 0, w, h).data
      let hash = 0
      let left = 0
      let right = 0
      for (let y = 0; y < h; y += 1) {
        for (let x = 0; x < w; x += 1) {
          const i = (y * w + x) * 4
          hash = (hash * 31 + data[i] + data[i + 1] * 7 + data[i + 2] * 13) | 0
          if (Math.max(data[i], data[i + 1], data[i + 2]) < 110) {
            if (x < w / 3) left += 1
            else if (x >= (2 * w) / 3) right += 1
          }
        }
      }
      const m = (Math.floor(h / 2) * w + Math.floor(w / 4)) * 4
      return { hash: String(hash), middle: [data[m], data[m + 1], data[m + 2]], left, right }
    },
    [46 + col * h.grid.cw + 3, 21 + row * h.grid.rh + 3, Math.round(h.grid.cw) - 6, Math.round(h.grid.rh) - 6],
  )

try {
  await openEditor('')
  // The cases work on a sheet of their own, which the person shows (an agent's new sheet stays the
  // agent's until the person clicks its tab).
  await exec('home.insertSheet', { name: 'Paint' })
  await tab('Paint').click()
  await until(async () => (await details()).sheet === 'Paint', 30)
  await clickCell(0, 0)
  await quiet()

  await section('PAINT-1 an agent reading the workbook and selecting on its own asks for no paint', async () => {
    const start = await renders()
    step('PAINT-1 the grid was painted through the counted worker', start > 0, String(start))
    const shown = await signature()
    const box = await nameBox()
    for (let i = 1; i <= 20; i += 1) await exec('cell.get', { cell: `A${i}` })
    await ai([
      { command: 'document.inspect' },
      { command: 'sheet.read', params: {} },
      { command: 'selection.set', params: { range: 'H30' } },
      { command: 'selection.stats' },
      { command: 'history.list' },
      { command: 'formulas.evaluate', params: { formula: '=1+2' } },
    ])
    await page.waitForTimeout(800)
    const after = await renders()
    step('PAINT-1 26 read-only agent steps asked the engine for no render', after === start, `${start} → ${after}`)
    step('PAINT-1 the person sees the same grid, their own cell still selected', (await signature()) === shown && (await nameBox()) === box, `${box} → ${await nameBox()}`)
  })

  await section('PAINT-2 the person’s entry lands while an agent keeps reading', async () => {
    await clickCell(2, 4)
    const blank = await cellShows(2, 4)
    await page.keyboard.type('7')
    await page.keyboard.press('Enter')
    // A number entered is drawn at the cell's right; while it was typed it showed at the left.
    // The agent reads all the while: each read used to repaint the whole grid and drop the paint
    // in flight.
    let reads = 0
    const landed = await until(async () => {
      await exec('cell.get', { cell: 'C5' })
      await exec('cell.get', { cell: 'A1' })
      reads += 2
      const now = await cellShows(2, 4)
      return now.right > blank.right + 3 && now.left <= blank.left + 1
    }, 40)
    step('PAINT-2 the entered 7 shows right-aligned in C5 while the agent reads', landed, `after ${reads} reads`)
    step('PAINT-2 C5 holds 7', (await exec('cell.get', { cell: 'C5' })).value === 7)
  })

  await section('PAINT-3 an agent’s value is repainted', async () => {
    await quiet()
    const start = await renders()
    const blank = await cellShows(3, 2)
    await exec('cell.set', { cell: 'D3', input: '12345' })
    const shows = await until(async () => (await cellShows(3, 2)).right > blank.right + 3, 30)
    step('PAINT-3 the person sees 12345 in D3', shows)
    step('PAINT-3 the step asked for a render', (await renders()) > start)
  })

  await section('PAINT-4 an agent’s fill is repainted', async () => {
    await quiet()
    await exec('home.fillColor', { range: 'E3', color: '#FF0000' })
    const red = await until(async () => {
      const [r, g, b] = (await cellShows(4, 2)).middle
      return r > 200 && g < 60 && b < 60
    }, 30)
    step('PAINT-4 the person sees E3 filled red', red, JSON.stringify((await cellShows(4, 2)).middle))
  })

  await section('PAINT-5 an agent’s conditional format is repainted', async () => {
    await exec('cell.set', { cell: 'F3', input: '5' })
    await quiet()
    await exec('home.conditionalFormat', { range: 'F3', rule: { type: 'cellIs', operator: 'greaterThan', value: 3, style: { fill: '#00B050' } } })
    const green = await until(async () => {
      const [r, g, b] = (await cellShows(5, 2)).middle
      return g > 140 && r < 60 && b < 120
    }, 30)
    step('PAINT-5 the person sees F3 filled green by its rule', green, JSON.stringify((await cellShows(5, 2)).middle))
  })

  await section('PAINT-6 an agent’s Calculate Now draws RAND()’s new number', async () => {
    await exec('cell.set', { cell: 'G3', input: '=RAND()' })
    await quiet()
    const value = (await exec('cell.get', { cell: 'G3' })).value
    const before = await cellShows(6, 2)
    const start = await renders()
    await exec('formulas.calculateNow')
    step('PAINT-6 the value changed', (await exec('cell.get', { cell: 'G3' })).value !== value)
    const redrawn = await until(async () => (await cellShows(6, 2)).hash !== before.hash, 30)
    step('PAINT-6 the person sees the new number in G3', redrawn)
    step('PAINT-6 the step asked for a render', (await renders()) > start)
  })

  await section('PAINT-7 an agent’s frozen panes are repainted', async () => {
    await quiet()
    const shown = await signature()
    const start = await renders()
    await exec('view.freezePanes', { cell: 'B3' })
    const redrawn = await until(async () => (await signature()) !== shown && (await renders()) > start, 30)
    step('PAINT-7 the person sees the freeze lines', redrawn)
    await exec('view.unfreezePanes')
  })

  await section('PAINT-8 an agent’s zoom is repainted', async () => {
    await quiet()
    const shown = await signature()
    const start = await renders()
    await exec('view.zoom', { percent: 150 })
    const redrawn = await until(async () => (await signature()) !== shown && (await renders()) > start, 30)
    step('PAINT-8 the person sees the grid at 150%', redrawn)
    await exec('view.zoom', { percent: 100 })
  })
} catch (error) {
  await h.finish(error)
}
await h.finish()
