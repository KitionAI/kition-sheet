// An agent's steps repaint the person's grid only when they change what it shows (kition-sdk's
// `agentSkipsUnchangedPaint`, turned on in `src/App.tsx`; regression case
// `regressions/issue-agent-reads-repaint`). Reads, and the agent selecting on its own, ask the
// engine for no paint and leave the person's paint in progress to land; a value, a fill, a
// conditional format, a recalculation of RAND(), frozen panes and the zoom an agent sets are
// repainted, and the person sees them.
// A person's edit is drawn when its repaint and a "paint nothing" (the release of a checkbox click,
// an agent's read) reach React together (regression case `regressions/issue-checkbox-paint-lost`).
// node tests/agent-paint.mjs [chromium|webkit]
import { startGate } from './gate-harness.mjs'

const h = await startGate({ prefix: 'PAINT-' })
const { page, kit, step, section, until, ai, exec, details, nameBox, clickCell, signature, tab, openEditor } = h

// Every engine render the canvas asks for (each paint, whole or a patch), counted where the editor
// sends it to the engine's worker; the engine tool calls (a click's press and release, a key) and the
// requests still unanswered, the same way.
await page.addInitScript(() => {
  const send = Worker.prototype.postMessage
  globalThis.__renders = 0
  globalThis.__settled = 0
  const waiting = new Set()
  const workers = new Map()
  globalThis.__waiting = () => waiting.size
  Worker.prototype.postMessage = function (message, ...rest) {
    const method = message?.request?.method
    if (method === 'render' || method === 'renderArea') globalThis.__renders += 1
    // A settled tool call ends a gesture: a click's release, a key.
    if (method === 'tool' && message.request.settled === true) globalThis.__settled += 1
    if (!workers.has(this)) {
      const n = workers.size
      workers.set(this, n)
      // Heard after the editor's own handler, so the answer has been taken in by then.
      this.addEventListener('message', (event) => {
        if (event.data != null && typeof event.data === 'object' && 'id' in event.data) waiting.delete(`${n}:${event.data.id}`)
      })
    }
    if (message != null && typeof message === 'object' && 'id' in message) waiting.add(`${workers.get(this)}:${message.id}`)
    return send.call(this, message, ...rest)
  }
  // React renders a state change in a task its scheduler posts to itself (`port.postMessage(null)`).
  // Held, the editor's state changes meanwhile reach React together, in one render, as they do on a
  // loaded machine when the engine answers before React gets to render.
  const post = MessagePort.prototype.postMessage
  const held = []
  globalThis.__holding = false
  MessagePort.prototype.postMessage = function (message, ...rest) {
    if (globalThis.__holding && message === null) {
      held.push(this)
      return undefined
    }
    return post.call(this, message, ...rest)
  }
  globalThis.__release = () => {
    globalThis.__holding = false
    for (const port of held.splice(0)) post.call(port, null)
  }
})

/** How many renders the canvas has asked the engine for so far. */
const renders = () => kit.locator('body').evaluate(() => globalThis.__renders ?? -1)
/** How many settled engine tool calls (a click's release, a key) the editor has made, and how many requests are still unanswered. */
const settled = () => kit.locator('body').evaluate(() => globalThis.__settled ?? -1)
const waiting = () => kit.locator('body').evaluate(() => globalThis.__waiting?.() ?? -1)
/** Holds React's renders (the editor's state changes wait), and lets them go in one render. */
const hold = () => kit.locator('body').evaluate(() => { globalThis.__holding = true })
const release = () => kit.locator('body').evaluate(() => globalThis.__release())
/** What the grid shows of the checkbox in a cell (`col`, `row` from 0): checked, empty, or none. */
const box = async (col, row) => {
  const cx = 46 + col * h.grid.cw + Math.floor(h.grid.cw / 2)
  const cy = 21 + row * h.grid.rh + Math.floor(h.grid.rh / 2)
  const inner = await h.pixel(cx - 3, cy - 3)
  const edge = await h.pixel(cx - 6, cy)
  const dark = (p) => p[0] < 140 && p[1] < 140 && p[2] < 140
  if (dark(inner) && dark(edge)) return 'checked'
  const grey = edge[0] > 130 && edge[0] < 205 && Math.abs(edge[0] - edge[2]) < 16
  const white = inner[0] > 235 && inner[1] > 235 && inner[2] > 235
  return grey && white ? 'empty' : 'none'
}
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
  await exec('sheet.insert', { name: 'Paint' })
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
      { command: 'file.inspect' },
      { command: 'sheet.read', params: {} },
      { command: 'select.set', params: { range: 'H30' } },
      { command: 'select.stats' },
      { command: 'history.list' },
      { command: 'formula.evaluate', params: { formula: '=1+2' } },
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
    await exec('format.fillColor', { range: 'E3', color: '#FF0000' })
    const red = await until(async () => {
      const [r, g, b] = (await cellShows(4, 2)).middle
      return r > 200 && g < 60 && b < 60
    }, 30)
    step('PAINT-4 the person sees E3 filled red', red, JSON.stringify((await cellShows(4, 2)).middle))
  })

  await section('PAINT-5 an agent’s conditional format is repainted', async () => {
    await exec('cell.set', { cell: 'F3', input: '5' })
    await quiet()
    await exec('conditionalFormat.apply', { range: 'F3', rule: { type: 'cellIs', operator: 'greaterThan', value: 3, style: { fill: '#00B050' } } })
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
    await exec('formula.calculateNow')
    step('PAINT-6 the value changed', (await exec('cell.get', { cell: 'G3' })).value !== value)
    const redrawn = await until(async () => (await cellShows(6, 2)).hash !== before.hash, 30)
    step('PAINT-6 the person sees the new number in G3', redrawn)
    step('PAINT-6 the step asked for a render', (await renders()) > start)
  })

  // The press of a click on a checkbox ticks it (the engine's revision moves: the whole grid is
  // repainted); the release changes nothing more and asks to paint nothing. kit-editor keeps one
  // change for React, the last: when both answers reach React in one render, the canvas used to be
  // told to paint nothing, and the box stayed drawn empty while the formula bar showed TRUE
  // (INSC-C53/C60 on a loaded WebKit runner).
  await section('PAINT-9 a checkbox clicked while React is busy is drawn checked once it renders', async () => {
    await exec('checkbox.insert', { range: 'B8' })
    await quiet()
    const before = await box(1, 7)
    const region = await kit.getByTestId('sheet-canvas-region').boundingBox()
    const start = await settled()
    const painted = await renders()
    let answered = false
    let asked = 0
    await hold()
    try {
      await page.mouse.click(region.x + 46 + h.grid.cw + h.grid.cw / 2, region.y + 21 + 7 * h.grid.rh + h.grid.rh / 2)
      // The press and the release have both been answered while React waited.
      answered = await until(async () => (await settled()) > start && (await waiting()) === 0, 50)
      asked = (await renders()) - painted
    } finally {
      await release()
    }
    await quiet()
    step('PAINT-9 B8 is drawn checked after the held render, with no further input', before === 'empty' && (await box(1, 7)) === 'checked', `${before} → ${await box(1, 7)}; answered while held: ${answered}, renders asked meanwhile: ${asked}`)
    step('PAINT-9 B8 holds TRUE', (await exec('cell.get', { cell: 'B8' })).value === true)
  })

  // An agent reads between the person's tick (Space) and its paint: each read asks to paint nothing
  // (agentSkipsUnchangedPaint), and must not replace the tick's repaint.
  await section('PAINT-10 the person’s tick is drawn though an agent reads before it is painted', async () => {
    await exec('checkbox.insert', { range: 'B10' })
    const region = await kit.getByTestId('sheet-canvas-region').boundingBox()
    // Beside the box: selects B10 without ticking it.
    await page.mouse.click(region.x + 46 + h.grid.cw + 6, region.y + 21 + 9 * h.grid.rh + 10)
    await until(async () => (await details()).active === 'B10', 30)
    await quiet()
    const before = await box(1, 9)
    const start = await settled()
    let ticked = false
    await hold()
    try {
      await page.keyboard.press(' ')
      await until(async () => (await settled()) > start && (await waiting()) === 0, 50)
      // The agent's reads run after the key (they wait for the person's input in flight).
      for (let i = 0; i < 30 && !ticked; i += 1) ticked = (await exec('cell.get', { cell: 'B10' })).value === true
    } finally {
      await release()
    }
    await quiet()
    step('PAINT-10 B10 is drawn checked after the agent’s reads and the held render, with no further input', ticked && before === 'empty' && (await box(1, 9)) === 'checked', `${before} → ${await box(1, 9)}; ticked: ${ticked}`)
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
