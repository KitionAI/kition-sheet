// What the hand tests share (web/tests/hand/*.mjs, plan "Sheet usable by hand"): a Vite server and
// one browser on dev.html, a person's input (page.mouse, page.keyboard), and reads that are no
// input of a person: the engine's state through the development host's agent connection, the
// canvas's view and pins from the editor's React tree.
// A test file runs its segments in order; `node tests/hand/<file>.mjs <chromium|webkit> [segment…]`
// runs only the named ones (a regression case names its segment).
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

/** Column letters for a column from 0 (`1` → `B`), and back. */
export const letters = (col) => {
  let out = ''
  for (let n = col + 1; n > 0; n = Math.floor((n - 1) / 26)) out = String.fromCharCode(65 + ((n - 1) % 26)) + out
  return out
}
export const colIndex = (name) => [...name].reduce((n, ch) => n * 26 + ch.charCodeAt(0) - 64, 0) - 1
const parseCell = (ref) => {
  const match = /^([A-Z]+)(\d+)$/.exec(ref)
  if (!match) throw new Error(`not a cell: ${ref}`)
  return { col: colIndex(match[1]), row: Number(match[2]) - 1 }
}

/**
 * Records what the editor draws over the grid on the canvas surface, frame by frame (a frame starts
 * where the rendered grid is copied in): small filled squares (handles) and dashed strokes, in the
 * canvas's CSS pixels. Read with `marks()`.
 */
const MARKS_INIT = () => {
  const marks = (window.__marks = { cur: { squares: [], dashed: 0 }, frames: 0 })
  const P = window.CanvasRenderingContext2D?.prototype
  if (!P) return
  const surface = (canvas) => canvas?.classList?.contains('kui-canvas-surface')
  const at = (ctx, x, y) => {
    const m = ctx.getTransform()
    const d = window.devicePixelRatio || 1
    return [(m.a * x + m.c * y + m.e) / d, (m.b * x + m.d * y + m.f) / d]
  }
  const wrap = (name, fn) => {
    const original = P[name]
    P[name] = function (...args) {
      try {
        if (surface(this.canvas)) fn(this, args)
      } catch {}
      return original.apply(this, args)
    }
  }
  wrap('drawImage', () => {
    marks.cur = { squares: [], dashed: 0 }
    marks.frames += 1
  })
  wrap('fillRect', (ctx, [x, y, w, h]) => {
    const p = at(ctx, x, y)
    const q = at(ctx, x + w, y + h)
    if (Math.abs(q[0] - p[0]) <= 14 && Math.abs(q[1] - p[1]) <= 14) marks.cur.squares.push({ x: p[0], y: p[1], w: q[0] - p[0], h: q[1] - p[1], fill: String(ctx.fillStyle) })
  })
  wrap('stroke', (ctx) => {
    if (ctx.getLineDash().length) marks.cur.dashed += 1
  })
}

/** Opens dev.html (the sample workbook) in `which` browser at `width` × `height`; Single line unless `query` says otherwise. */
export async function open({ which = process.argv[2] ?? 'chromium', query = 'locale=en-US', width = 1600, height = 1000 } = {}) {
  const server = await createServer({ configFile: new URL('../../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
  await server.listen()
  const browser = await (which === 'webkit' ? webkit : chromium).launch()
  const context = await browser.newContext({ viewport: { width, height } })
  if (which === 'chromium') await context.grantPermissions(['clipboard-read', 'clipboard-write'])
  await context.addInitScript(MARKS_INIT)
  const page = await context.newPage()
  page.setDefaultTimeout(10_000)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html${query ? `?${query}` : ''}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90_000 })
  const kit = page.frameLocator('#kit')
  const frame = () => page.frames().find((item) => item !== page.mainFrame())
  const wait = (ms) => page.waitForTimeout(ms)
  const until = async (check, tries = 60, gap = 100) => {
    for (let index = 0; index < tries; index += 1) {
      if (await check().catch(() => false)) return true
      await page.waitForTimeout(gap)
    }
    return false
  }
  await until(() => kit.getByTestId('kit-canvas-surface').isVisible())
  await until(() => kit.getByTestId('sheet-strip').isVisible())

  /** Runs commands as the agent: setup and reads, never a person's action. */
  const agent = (commands) => page.evaluate(async (list) => (await window.devHost.connection().ai.run(list, false)).completed?.map((item) => item.result) ?? [], commands)
  const ask = async (command, params = {}) => (await agent([{ command, params }]))[0]
  /** The person's editor state, as the editor reads it (`inspect().details`). */
  const details = async () => {
    const info = await page.evaluate(async () => window.devHost.connection().ai.inspect())
    return info.details ?? info.inspected ?? info
  }
  /** A cell's value (`cell.get`). */
  const value = async (ref) => (await ask('cell.get', { cell: ref }))?.value ?? null
  const zoom = async () => Number((await kit.getByTestId('kit-zoom').textContent()).replace('%', ''))
  const surface = () => kit.getByTestId('kit-canvas-surface').boundingBox()
  /** The canvas's view (document point at its top-left, zoom) and pins (headings and frozen panes), read from the editor's React tree. */
  const canvasState = () =>
    frame().evaluate(() => {
      const element = document.querySelector('[data-testid=kit-canvas-surface]')
      if (!element) return null
      const key = Object.keys(element).find((name) => name.startsWith('__reactFiber'))
      let view = null
      let pins = null
      for (let fiber = element[key], depth = 0; fiber && depth < 120 && (!view || !pins); depth += 1, fiber = fiber.return) {
        const props = fiber.memoizedProps
        if (!view && props?.viewMemory?.current) view = { ...props.viewMemory.current }
        if (!pins && props?.doc?.pinned) pins = { ...props.doc.pinned }
      }
      return { view, pins }
    })
  /** The active sheet's layout as the engine keeps it: sizes of its own (px), hidden lines, defaults. */
  const layout = async () => {
    const shown = await details()
    const sheet = (await ask('file.inspect')).sheets.find((item) => item.name === shown.sheet)
    return { sheet, defaultSize: shown.defaultSize ?? [sheet.defaultColWidth, sheet.defaultRowHeight] }
  }
  /**
   * The point on the page of a cell (`D10`), `fx`, `fy` across it (0.5 the middle): from the
   * engine's layout (sizes of their own, hidden lines) and the canvas's view and pins, so it holds
   * at any zoom and scroll.
   */
  const cellPoint = async (ref, { fx = 0.5, fy = 0.5 } = {}) => {
    const { col, row } = parseCell(ref)
    const { sheet, defaultSize } = await layout()
    const { view, pins } = await canvasState()
    const [frozenRows, frozenCols] = (await details()).freeze ?? [0, 0]
    const hiddenCols = new Set(sheet.hiddenColumns ?? [])
    const hiddenRows = new Set((sheet.hiddenRows ?? []).map(Number))
    const width = (c) => (hiddenCols.has(letters(c)) ? 0 : (sheet.colWidths?.[letters(c)] ?? defaultSize[0]))
    const height = (r) => (hiddenRows.has(r + 1) ? 0 : (sheet.rowHeights?.[String(r + 1)] ?? defaultSize[1]))
    // The headings: the pins less the frozen columns and rows.
    let x = pins?.left ?? 0
    let y = pins?.top ?? 0
    for (let c = 0; c < frozenCols; c += 1) x -= width(c)
    for (let r = 0; r < frozenRows; r += 1) y -= height(r)
    for (let c = 0; c < col; c += 1) x += width(c)
    for (let r = 0; r < row; r += 1) y += height(r)
    x += width(col) * fx
    y += height(row) * fy
    const box = await surface()
    const sx = x < (pins?.left ?? 0) ? x : x - view.x
    const sy = y < (pins?.top ?? 0) ? y : y - view.y
    return { x: box.x + sx * view.zoom, y: box.y + sy * view.zoom }
  }
  /** Clicks a cell with the mouse (press, release) and waits until the engine made it active. */
  const click = async (ref, options = {}) => {
    const at = await cellPoint(ref)
    const box = await surface()
    if (at.x < box.x || at.y < box.y || at.x > box.x + box.width || at.y > box.y + box.height) throw new Error(`${ref} is out of view (${Math.round(at.x)}, ${Math.round(at.y)})`)
    await page.mouse.move(at.x, at.y)
    await page.mouse.down(options)
    await page.mouse.up(options)
    await until(async () => (await details()).active === ref, 30)
  }
  /** Setup, not a person's step: wheels the view back to the sheet's top-left. */
  const home = async () => {
    const box = await surface()
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    for (let index = 0; index < 4; index += 1) {
      const { view } = await canvasState()
      if (view.x <= 0 && view.y <= 0) break
      await page.mouse.wheel(-(view.x + 100) * view.zoom, -(view.y + 100) * view.zoom)
      await wait(250)
    }
  }
  /** The element with the keyboard in the editor's frame, in short. */
  const active = () =>
    frame().evaluate(() => {
      const element = document.activeElement
      if (!element || element === document.body) return 'body'
      return `${element.tagName.toLowerCase()}${element.className && typeof element.className === 'string' ? `.${element.className.split(' ')[0]}` : ''}`
    })
  /** What is drawn over the grid now: small filled squares (on the page) and how many dashed strokes. */
  const marks = async () => {
    const drawn = await frame().evaluate(() => window.__marks?.cur ?? null)
    const box = await surface()
    if (!drawn) return null
    return { dashed: drawn.dashed, squares: drawn.squares.map((item) => ({ ...item, x: box.x + item.x, y: box.y + item.y })) }
  }
  /** The pointer's look over the grid (the engine tool's cursor). */
  const cursor = () => kit.locator('.kui-canvas-surface').evaluate((element) => element.style.cursor)
  const close = async () => {
    await browser.close().catch(() => undefined)
    await server.close().catch(() => undefined)
  }
  return { which, page, kit, frame, wait, until, agent, ask, details, value, zoom, surface, canvasState, layout, cellPoint, click, home, active, marks, cursor, errors, close }
}

/**
 * Runs a test file's segments, each `[name, fn]`, in order: only those named on the command line
 * when any are. Prints PASS or FAIL per segment and exits non-zero on a failure.
 */
export async function segments(title, list, start) {
  const which = process.argv[2] ?? 'chromium'
  const wanted = process.argv.slice(3)
  const chosen = wanted.length ? list.filter(([name]) => wanted.includes(name)) : list
  if (wanted.length && chosen.length !== wanted.length) {
    console.log(`FAIL unknown segment among ${wanted.join(', ')}`)
    process.exit(1)
  }
  const editor = await start(which)
  let failed = 0
  try {
    for (const [name, fn] of chosen) {
      try {
        await fn(editor)
        console.log(`PASS ${title} ${name} (${which})`)
      } catch (error) {
        failed += 1
        console.log(`FAIL ${title} ${name} (${which}): ${error instanceof Error ? error.message.split('\n').slice(0, 4).join(' | ') : String(error)}`)
        await editor.page.keyboard.press('Escape').catch(() => undefined)
        await editor.page.mouse.up().catch(() => undefined)
      }
    }
    if (editor.errors.length) {
      failed += 1
      console.log(`FAIL page errors: ${editor.errors.slice(0, 5).join(' | ')}`)
    }
  } finally {
    await editor.close()
  }
  process.exit(failed ? 1 : 0)
}
