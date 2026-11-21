// The shared harness of the golden e2e tests (web/tests/golden-*.mjs): the development host in a
// real browser (node tests/golden-<module>.mjs [chromium|webkit]), and the helpers that drive the
// sheet tab bar, the grid and the engine. Each module's file names its case ids with its prefix
// (UI-C5, MENU-C12); `GOLDEN_ONLY=C5,C12` runs only those cases.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

/** Starts the host and a browser; `prefix` is the module's case prefix (`UI-`). */
export async function startGolden({ prefix }) {
  const which = process.argv[2] ?? 'chromium'
  const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
  await server.listen()
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  const browser = await (which === 'webkit' ? webkit : chromium).launch()
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
  const page = await context.newPage()
  const errors = []
  let failures = 0
  page.on('pageerror', (error) => errors.push(error.message))
  const step = (name, ok, detail = '') => {
    if (!ok) failures += 1
    console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
  }
  /**
   * `GOLDEN_ONLY=C83,C86` runs only the cases named (while working on them); unset, every case
   * runs. A section named `MENU-C46/C48 …` runs for either id.
   */
  const only = process.env.GOLDEN_ONLY?.split(',').map((id) => id.trim())
  const idsOf = (name) => (name.split(' ')[0] ?? '').replace(prefix, '').split('/')
  /** Runs a case's body; an exception fails that case and the run goes on. */
  const section = async (name, body) => {
    if (only && !idsOf(name).some((id) => only.includes(id))) return
    try {
      await body()
    } catch (error) {
      step(name, false, error instanceof Error ? error.message.split('\n')[0] : String(error))
      await page.keyboard.press('Escape').catch(() => undefined)
      await page.mouse.up().catch(() => undefined)
    }
  }
  const until = async (check, tries = 60) => {
    for (let i = 0; i < tries; i += 1) {
      if (await check().catch(() => false)) return true
      await page.waitForTimeout(100)
    }
    return false
  }
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)

  const kit = page.frameLocator('#kit')
  let docPath = 'Sample.xlsx'
  /**
   * The grid's pitch where no column or row has its own size (document pixels at 100%), as the
   * engine lays it out (`inspect().defaultSize`): read when the editor opens a workbook, so the
   * cases click cells by the layout rather than by numbers written down (module M10 made a new
   * sheet WPS's 69 × 19).
   */
  const grid = { cw: 69, rh: 19 }
  const readGrid = async () => {
    const size = (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details?.defaultSize
    if (Array.isArray(size) && size.length === 2) Object.assign(grid, { cw: size[0], rh: size[1] })
  }

  const ai = (steps) => page.evaluate(async (steps) => window.devHost.connection().ai.run(steps, false), steps)
  const exec = async (command, params = {}) => (await ai([{ command, params }])).completed[0]?.result
  const details = async () => (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
  const workbook = () => exec('document.inspect')
  const order = async () => (await workbook()).sheets.map((sheet) => sheet.name)
  const activeSheet = async () => (await details()).sheet
  const q = (name) => `'${name.replace(/'/g, "''")}'`
  const cellOf = async (ref) => exec('cell.get', { cell: ref })

  const tab = (name) => kit.getByTestId(`sheet-tab-${name}`)
  const strip = () => kit.getByTestId('sheet-strip')
  const tabNames = () => strip().locator('[role="tab"]').evaluateAll((tabs) => tabs.map((tab) => tab.getAttribute('data-testid').slice('sheet-tab-'.length)))
  const selectedTabs = () => strip().locator('[role="tab"][aria-selected="true"]').evaluateAll((tabs) => tabs.map((tab) => tab.getAttribute('data-testid').slice('sheet-tab-'.length)))
  const nameBox = () => kit.getByTestId('sheet-name-box').inputValue()
  const formulaBar = () => kit.getByTestId('sheet-formula').inputValue()
  const region = async () => kit.getByTestId('sheet-canvas-region').boundingBox()
  /** Column letters for a column from 0 (`1` → `B`). */
  const letters = (col) => {
    let out = ''
    for (let n = col + 1; n > 0; n = Math.floor((n - 1) / 26)) out = String.fromCharCode(65 + ((n - 1) % 26)) + out
    return out
  }
  /** Whether the editor's text input (kit-editor's TextSink) has the keyboard. */
  const sinkFocused = () => kit.getByTestId('kit-text-sink').evaluate((element) => element.ownerDocument.activeElement === element)
  /**
   * Clicks a cell by its column and row (from 0), at 100% with the sheet at its top-left, and waits
   * until typing reaches it: the engine has made it the active cell and the editor's text input
   * has the keyboard back (kit-editor gives it back on the frame after the click; a fixed pause
   * lost the first keys on a loaded machine).
   */
  const clickCell = async (col, row) => {
    const box = await region()
    await page.mouse.click(box.x + 46 + col * grid.cw + grid.cw / 2, box.y + 21 + row * grid.rh + grid.rh / 2)
    const cell = `${letters(col)}${row + 1}`
    await until(async () => {
      const shown = await details()
      return (shown.active === cell || shown.editing === true) && (await sinkFocused())
    }, 15)
  }
  const goTo = async (ref) => {
    await kit.getByTestId('sheet-name-box').fill(ref)
    await kit.getByTestId('sheet-name-box').press('Enter')
    await until(async () => (await details()).selection === ref)
  }
  /** Clicks a tab and waits until it is the only selected one. */
  const choose = async (name) => {
    await tab(name).click()
    return until(async () => same(await selectedTabs(), [name]) && (await activeSheet()) === name)
  }
  /** Whether the tab bar and the workbook both show `names` (visible tabs, left to right). */
  const showsTabs = (names) => until(async () => same(await tabNames(), names))

  /**
   * A new workbook's sheets, by command: `names` in order, each with its own text in A1 (`<name>内容`, "<name> content")
   * and a number in B2, the first one active. `hidden` names are hidden afterwards.
   */
  async function build(names, { fill = true, hidden = [], active = names[0] } = {}) {
    const existing = await order()
    const steps = existing.includes('__setup__') ? [{ command: 'sheet.unhide', params: { sheet: '__setup__' } }] : [{ command: 'home.insertSheet', params: { name: '__setup__' } }]
    for (const name of existing) if (name !== '__setup__') steps.push({ command: 'home.deleteSheet', params: { sheet: name } })
    names.forEach((name, index) => steps.push({ command: 'home.insertSheet', params: { name, before: index } }))
    steps.push({ command: 'home.deleteSheet', params: { sheet: '__setup__' } })
    if (fill) {
      names.forEach((name, index) => {
        steps.push({ command: 'cell.set', params: { cell: `${q(name)}!A1`, input: `${name}内容` } })
        steps.push({ command: 'cell.set', params: { cell: `${q(name)}!B2`, input: String((index + 1) * 10) } })
      })
    }
    steps.push({ command: 'sheet.activate', params: { sheet: active } })
    for (const name of hidden) steps.push({ command: 'sheet.hide', params: { sheet: name } })
    if (hidden.length) steps.push({ command: 'sheet.activate', params: { sheet: active } })
    await ai(steps)
    const shown = names.filter((name) => !hidden.includes(name))
    if (!(await showsTabs(shown))) throw new Error(`setup: tabs ${JSON.stringify(await tabNames())}`)
    // The agent's sheet.activate moves only the agent's own editor state (KHP C3b): the person
    // clicks the tab, as the cases start on that sheet.
    await choose(active)
  }
  const numbered = (count) => Array.from({ length: count }, (_, index) => `工作表${index + 1}`)
  /** What the grid shows in A1 for the active sheet (the sheet's own text). */
  const shownA1 = async () => (await cellOf('A1')).value

  /** Presses on a tab and drags it to `to` (page pixels); `release: false` keeps it held. */
  async function drag(name, to, { hold = 0, release = true, steps = 14 } = {}) {
    const box = await tab(name).boundingBox()
    const from = { x: box.x + Math.min(24, box.width / 2), y: box.y + box.height / 2 }
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    await page.mouse.move(from.x + 8, from.y, { steps: 2 })
    await page.mouse.move(to.x, to.y, { steps })
    if (hold) await page.waitForTimeout(hold)
    if (release) await page.mouse.up()
  }
  /** The gap right after a tab, and the start of a tab (page pixels). */
  const after = async (name) => {
    const box = await tab(name).boundingBox()
    return { x: box.x + box.width + 1, y: box.y + box.height / 2 }
  }
  const before = async (name) => {
    const box = await tab(name).boundingBox()
    return { x: box.x + 3, y: box.y + box.height / 2 }
  }
  /** The end of the tabs (past the last tab). */
  const end = async () => {
    const names = await tabNames()
    const last = await tab(names[names.length - 1]).boundingBox()
    return { x: last.x + last.width + 10, y: last.y + last.height / 2 }
  }

  /** Opens the sheet menu from the active tab's arrow, or by right-clicking a tab. */
  const arrowMenu = async () => {
    await kit.getByTestId('sheet-menu-arrow').click()
    await kit.getByTestId('sheet-menu').waitFor()
  }
  const rightClick = async (name) => {
    await tab(name).click({ button: 'right' })
    await kit.getByTestId('sheet-menu').waitFor()
  }
  /** The rows of the open sheet menu, top to bottom. */
  const menuRows = () => kit.getByTestId('sheet-menu').locator(':scope > [role^="menuitem"], :scope > .kui-submenu > [role="menuitem"]').evaluateAll((rows) => rows.map((row) => row.textContent.trim()))
  /** Points at Move Sheet and answers which directions its submenu offers. */
  const moveChoices = async () => {
    await kit.getByTestId('sheet-menu-move').hover()
    await page.waitForTimeout(200)
    return { left: (await kit.getByTestId('sheet-menu-move-left').count()) === 1, right: (await kit.getByTestId('sheet-menu-move-right').count()) === 1 }
  }
  const moveBy = async (direction) => {
    await kit.getByTestId('sheet-menu-move').hover()
    await kit.getByTestId(`sheet-menu-move-${direction}`).click()
  }
  const closeMenus = async () => {
    await page.keyboard.press('Escape')
    await until(async () => (await kit.getByTestId('sheet-menu').count()) === 0 && (await kit.getByTestId('sheet-list').count()) === 0, 20)
  }
  /** The sheet list's rows: name, whether checked (the current sheet), and whether disabled (hidden). */
  const listRows = async () => {
    await kit.getByTestId('sheet-list-button').click()
    await kit.getByTestId('sheet-list').waitFor()
    return kit
      .getByTestId('sheet-list')
      .locator('[role^="menuitem"]')
      .evaluateAll((rows) => rows.map((row) => ({ name: row.getAttribute('data-testid').slice('sheet-list-item-'.length), checked: row.getAttribute('aria-checked') === 'true', disabled: row.disabled, text: row.textContent })))
  }
  /** Whether a tab lies wholly inside the visible part of the tab strip. */
  const inView = async (name) => {
    const tabBox = await tab(name).boundingBox()
    const stripBox = await strip().boundingBox()
    return tabBox != null && tabBox.x >= stripBox.x - 1 && tabBox.x + tabBox.width <= stripBox.x + stripBox.width + 1
  }

  /** Saves through the host and opens the saved file again, as closing and reopening it does. */
  async function saveAndReopen(as = docPath) {
    const was = await page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, docPath)
    await kit.getByTestId('sheet-save').click()
    if (!(await until(async () => (await page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, docPath)) > was))) throw new Error('save did not reach the host')
    await page.evaluate(async ([from, to]) => {
      const file = window.devHost.files.get(from)
      document.getElementById('status').textContent = ''
      await window.devHost.open(to, file.bytes.slice(0))
    }, [docPath, as])
    docPath = as
    await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
    await kit.getByTestId('sheet-strip').waitFor()
    await page.waitForTimeout(600)
    await readGrid()
  }
  /** Marks the workbook changed (so it can be saved) without touching the sheets under test. */
  const touch = () => exec('cell.set', { cell: 'Z99', input: String(Date.now() % 1000) })

  /** A part of the saved .xlsx (`xl/workbook.xml`), as text; null when the file has no such part. */
  const zipPart = (part) =>
    page.evaluate(
      async ([path, part]) => {
        const bytes = new Uint8Array(window.devHost.files.get(path).bytes)
        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
        let end = bytes.length - 22
        while (end > 0 && view.getUint32(end, true) !== 0x06054b50) end -= 1
        let at = view.getUint32(end + 16, true)
        for (let n = view.getUint16(end + 10, true); n > 0; n -= 1) {
          const [method, size, nameLength, extra, comment, offset] = [view.getUint16(at + 10, true), view.getUint32(at + 20, true), view.getUint16(at + 28, true), view.getUint16(at + 30, true), view.getUint16(at + 32, true), view.getUint32(at + 42, true)]
          const name = new TextDecoder().decode(bytes.subarray(at + 46, at + 46 + nameLength))
          at += 46 + nameLength + extra + comment
          if (name !== part) continue
          const start = offset + 30 + view.getUint16(offset + 26, true) + view.getUint16(offset + 28, true)
          const data = bytes.subarray(start, start + size)
          const plain = method === 0 ? data : new Uint8Array(await new Response(new Blob([data]).stream().pipeThrough(new DecompressionStream('deflate-raw'))).arrayBuffer())
          return new TextDecoder().decode(plain)
        }
        return null
      },
      [docPath, part],
    )
  const unescapeXml = (text) => text.replace(/&quot;/g, '"').replace(/&apos;/g, "'").replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')

  /** The saved .xlsx's sheet names in order and its active tab (from `xl/workbook.xml`). */
  async function savedWorkbook() {
    const xml = await zipPart('xl/workbook.xml')
    if (xml == null) return null
    const sheets = [...xml.matchAll(/<sheet\b[^>]*\bname="([^"]*)"[^>]*>/g)].map((match) => ({ name: unescapeXml(match[1]), hidden: /state="(hidden|veryHidden)"/.test(match[0]) }))
    const activeTab = Number(/activeTab="(\d+)"/.exec(xml)?.[1] ?? 0)
    return { sheets, activeTab }
  }

  /** The colour (RGBA) the grid shows at a point of the canvas region (CSS pixels from its top-left), every layer composed. */
  const pixel = (x, y) =>
    kit.getByTestId('sheet-canvas-region').evaluate((element, [x, y]) => {
      const out = document.createElement('canvas')
      out.width = 1
      out.height = 1
      const context = out.getContext('2d')
      const box = element.getBoundingClientRect()
      for (const canvas of element.querySelectorAll('canvas')) {
        const rect = canvas.getBoundingClientRect()
        if (!rect.width) continue
        const ratio = canvas.width / rect.width
        context.drawImage(canvas, Math.round((box.left + x - rect.left) * ratio), Math.round((box.top + y - rect.top) * ratio), 1, 1, 0, 0, 1, 1)
      }
      return [...context.getImageData(0, 0, 1, 1).data]
    }, [x, y])

  /** A fingerprint of what the grid shows (every layer composed): equal while the view and cells are unchanged. */
  const signature = () =>
    kit.getByTestId('sheet-canvas-region').evaluate((element) => {
      const box = element.getBoundingClientRect()
      const out = document.createElement('canvas')
      out.width = Math.round(box.width)
      out.height = Math.round(box.height)
      const context = out.getContext('2d')
      for (const canvas of element.querySelectorAll('canvas')) {
        const rect = canvas.getBoundingClientRect()
        if (rect.width) context.drawImage(canvas, rect.left - box.left, rect.top - box.top, rect.width, rect.height)
      }
      const data = context.getImageData(0, 0, out.width, out.height).data
      let hash = 0
      for (let i = 0; i < data.length; i += 4) hash = (hash * 31 + data[i] + data[i + 1] * 7 + data[i + 2] * 13) | 0
      return String(hash)
    })

  /** The row at the top of the grid (from the Name Box after clicking the first cell in view). */
  const topRow = async () => {
    await clickCell(0, 0)
    await page.waitForTimeout(150)
    return Number((await nameBox()).replace(/^[A-Z]+/, ''))
  }

  async function openEditor(query) {
    await page.goto(`${origin}/dev.html?modes=classic&locale=zh-CN${query}`)
    await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
    await kit.getByTestId('sheet-strip').waitFor()
    await page.waitForTimeout(800)
    docPath = 'Sample.xlsx'
    await readGrid()
  }

  /** Ends the run: closes the browser and the server, prints the result, and exits (1 on a failure or `crash`). */
  async function finish(crash) {
    if (crash) {
      failures += 1
      console.log(`FAIL ${crash instanceof Error ? crash.message.split('\n')[0] : String(crash)}`)
    }
    if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
    await browser.close()
    await server.close()
    console.log(`${which}: ${failures ? `${failures} failed` : 'all passed'}`)
    process.exit(failures ? 1 : 0)
  }

  return {
    which, page, kit, errors, step, section, until, same, ai, exec, details, workbook, order, activeSheet, q, cellOf,
    tab, strip, tabNames, selectedTabs, nameBox, formulaBar, region, clickCell, sinkFocused, goTo, choose, showsTabs, build, numbered, shownA1,
    drag, after, before, end, arrowMenu, rightClick, menuRows, moveChoices, moveBy, closeMenus, listRows, inView,
    saveAndReopen, touch, savedWorkbook, zipPart, unescapeXml, pixel, signature, topRow, openEditor, finish, grid, readGrid,
    /** The file the editor has open (the host's path). */
    docPath: () => docPath,
    setDocPath: (path) => {
      docPath = path
    },
  }
}
