// The editor's flow in a real browser, through the development host: node tests/flow.mjs [chromium|webkit]
// Starts the Vite dev server itself, opens dev.html in Chinese, and prints PASS or FAIL per step.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
if (which === 'chromium') await context.grantPermissions(['clipboard-read', 'clipboard-write'])
const page = await context.newPage()
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 60) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const inspect = () => page.evaluate(async () => window.devHost.connection().ai.inspect())
const details = async () => (await inspect()).details

try {
  await page.goto(`${origin}/dev.html?modes=classic&locale=zh-CN`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  const kit = page.frameLocator('#kit')
  await page.waitForTimeout(1000)
  const cell = (ref) => page.evaluate(async (ref) => (await window.devHost.connection().ai.run([{ command: 'cell.get', params: { cell: ref } }], false)).completed[0].result, ref)
  step('Chinese tab names', (await kit.getByTestId('ribbon-tab-home').textContent()) === '开始')
  step('opens at 100%', (await kit.getByTestId('kit-zoom').textContent()) === '100%')
  step('the formula bar shows A1', (await kit.getByTestId('sheet-formula').inputValue()) === 'Region')

  // Click B2; type 42 and Enter.
  const region = await kit.getByTestId('sheet-canvas-region').boundingBox()
  await page.mouse.click(region.x + 142, region.y + 51)
  await until(async () => (await kit.getByTestId('sheet-name-box').inputValue()) === 'B2')
  step('selects a cell by clicking it', (await kit.getByTestId('sheet-name-box').inputValue()) === 'B2', await kit.getByTestId('sheet-name-box').inputValue())
  await page.keyboard.type('42')
  await page.keyboard.press('Enter')
  await until(async () => (await cell('B2')).input === '42')
  step('types into the cell', (await cell('B2')).input === '42')
  step('Enter moves down', (await kit.getByTestId('sheet-name-box').inputValue()) === 'B3')

  // Rich text in the cell editor: double-click A2 ("North 北区"), select "North", Cmd/Ctrl+B, Enter.
  await page.mouse.dblclick(region.x + 46 + 32, region.y + 21 + 20 + 10)
  await page.waitForTimeout(150)
  await page.keyboard.press('Home')
  for (let i = 0; i < 5; i += 1) await page.keyboard.press('Shift+ArrowRight')
  await page.keyboard.press('ControlOrMeta+b')
  await page.keyboard.press('Enter')
  const runsOf = async (ref) => (await cell(ref)).runs ?? []
  await until(async () => (await runsOf('A2'))[0]?.font?.bold === true)
  const inCell = await runsOf('A2')
  step('bolds characters selected in the cell editor', inCell[0]?.text === 'North' && inCell[0]?.font?.bold === true && inCell[1]?.text === ' 北区', JSON.stringify(inCell))
  step('formats only the characters, not the cell', (await cell('A2')).style?.font?.bold !== true)

  // And in the formula bar: select "北区", Cmd/Ctrl+I.
  await kit.getByTestId('sheet-name-box').fill('A2')
  await kit.getByTestId('sheet-name-box').press('Enter')
  await until(async () => (await kit.getByTestId('sheet-formula').inputValue()) === 'North 北区')
  await kit.getByTestId('sheet-formula').focus()
  await kit.getByTestId('sheet-formula').evaluate((field) => field.setSelectionRange(6, 8))
  await kit.getByTestId('sheet-formula').press('ControlOrMeta+i')
  const italic = async () => (await runsOf('A2')).some((run) => run.text === '北区' && run.font?.italic === true)
  await until(italic)
  step('italicises characters selected in the formula bar', await italic(), JSON.stringify(await runsOf('A2')))
  await kit.getByTestId('sheet-formula').blur()

  // The formula bar enters a formula into the active cell.
  await page.mouse.click(region.x + 142, region.y + 51 + 7 * 20)
  await page.waitForTimeout(150)
  const target = await kit.getByTestId('sheet-name-box').inputValue()
  await kit.getByTestId('sheet-formula').fill('=AVERAGE(B2:B5)')
  await kit.getByTestId('sheet-formula').press('Enter')
  await until(async () => (await cell(target)).input === '=AVERAGE(B2:B5)')
  step('enters a formula from the formula bar', (await cell(target)).input === '=AVERAGE(B2:B5)', target)

  // The Name Box goes to a cell; the ✓ button enters the field.
  await kit.getByTestId('sheet-name-box').fill('D4')
  await kit.getByTestId('sheet-name-box').press('Enter')
  await until(async () => (await kit.getByTestId('sheet-formula').inputValue()) === '95')
  step('the Name Box goes to a cell', (await kit.getByTestId('sheet-formula').inputValue()) === '95')
  await kit.getByTestId('sheet-formula').fill('96')
  await kit.getByTestId('sheet-formula-enter').click()
  await until(async () => (await cell('D4')).input === '96')
  step('the ✓ button enters the formula field', (await cell('D4')).input === '96')

  // Bold and a fill color from the ribbon.
  await kit.getByTestId('sheet-cmd-format.bold').click()
  await until(async () => (await kit.getByTestId('sheet-cmd-format.bold').getAttribute('aria-pressed')) === 'true')
  step('bolds the cell', (await kit.getByTestId('sheet-cmd-format.bold').getAttribute('aria-pressed')) === 'true')
  await kit.getByTestId('sheet-fill').click()
  await kit.getByTestId('sheet-fill-colors').locator('button').nth(43).click()
  await until(async () => JSON.stringify((await cell('D4')).style?.fill ?? {}).includes('Rgb'))
  step('fills the cell', JSON.stringify((await cell('D4')).style?.fill ?? {}).includes('Rgb'))

  // Sheet tabs: add one, rename it, go back.
  await kit.getByTestId('sheet-add').click()
  await until(async () => (await kit.locator('[data-testid^="sheet-tab-"]').count()) === 2)
  step('adds a sheet', (await kit.locator('[data-testid^="sheet-tab-"]').count()) === 2)
  await kit.locator('[data-testid^="sheet-tab-"]').nth(1).dblclick()
  await kit.getByTestId('sheet-rename').fill('Notes')
  await kit.getByTestId('sheet-rename').press('Enter')
  await until(async () => (await kit.getByTestId('sheet-tab-Notes').count()) === 1)
  step('renames a sheet', (await kit.getByTestId('sheet-tab-Notes').count()) === 1)
  await kit.getByTestId('sheet-tab-Sheet1').click()
  await until(async () => (await kit.getByTestId('sheet-status').textContent())?.startsWith('Sheet1'))
  step('switches sheets', (await kit.getByTestId('sheet-status').textContent())?.startsWith('Sheet1'))

  // Format as Table shows the Table Design tab.
  await kit.getByTestId('sheet-name-box').fill('A1:F6')
  await kit.getByTestId('sheet-name-box').press('Enter')
  await page.waitForTimeout(200)
  await kit.getByTestId('sheet-format-table').click()
  await kit.getByTestId('sheet-table-TableStyleMedium2').click()
  // WPS asks first: Format As Table's dialog, where converting to a table is one choice.
  await kit.getByTestId('sheet-fat-table').check()
  await kit.getByTestId('sheet-fat-ok').click()
  await until(async () => (await kit.getByTestId('ribbon-tab-tableDesign').count()) === 1)
  step('formats a table and shows Table Design', (await kit.getByTestId('ribbon-tab-tableDesign').count()) === 1)

  // Copy goes to the system clipboard (Chromium grants it here).
  if (which === 'chromium') {
    await kit.getByTestId('sheet-name-box').fill('B1:C2')
    await kit.getByTestId('sheet-name-box').press('Enter')
    await page.waitForTimeout(200)
    await kit.getByTestId('sheet-copy').click()
    await page.waitForTimeout(300)
    const copied = await page.evaluate(() => navigator.clipboard.readText())
    step('copies to the system clipboard', copied.startsWith('Q1\tQ2'), JSON.stringify(copied))
  }

  // Frozen panes: with B2 frozen, the first row and column stay put while the rest scrolls.
  await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'view.freezePanes', params: { cell: 'B2' } }], false))
  await page.mouse.move(region.x + 300, region.y + 200)
  await page.mouse.wheel(300, 600)
  await page.waitForTimeout(500)
  await page.mouse.click(region.x + 46 + 64 + 32, region.y + 21 + 10)
  await until(async () => /^[C-Z]1$/.test(await kit.getByTestId('sheet-name-box').inputValue()))
  const topRow = await kit.getByTestId('sheet-name-box').inputValue()
  step('the frozen row stays in view as the sheet scrolls', /^[C-Z]1$/.test(topRow), topRow)
  await page.mouse.click(region.x + 46 + 32, region.y + 21 + 20 + 10)
  await until(async () => /^A\d+$/.test(await kit.getByTestId('sheet-name-box').inputValue()))
  const firstColumn = await kit.getByTestId('sheet-name-box').inputValue()
  step('the frozen column stays in view as the sheet scrolls', /^A([2-9]\d|\d{3,})$/.test(firstColumn), firstColumn)
  const heading = await kit.locator('[data-testid="sheet-canvas-region"] canvas').first().evaluate((canvas) => {
    const ratio = canvas.width / canvas.clientWidth
    return [...canvas.getContext('2d').getImageData(Math.round(300 * ratio), Math.round(4 * ratio), 1, 1).data].slice(0, 3)
  })
  step('the column headings stay in view', heading.every((channel) => channel >= 235 && channel <= 250), JSON.stringify(heading))
  await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'view.unfreezePanes', params: {} }], false))

  const run = await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'cell.set', params: { cell: 'H2', input: 'Agent' } }], false))
  await page.waitForTimeout(300)
  step('the agent edits the live workbook', run.completed[0].changed && (await page.evaluate(() => window.devHost.changes.at(-1).by)) === 'agent')
  await kit.getByTestId('sheet-save').click()
  await until(async () => (await page.evaluate(() => window.devHost.files.get('Sample.xlsx').version)) > 1)
  step('saves through the host', (await page.evaluate(() => window.devHost.files.get('Sample.xlsx').version)) > 1)
  // The saved workbook's shared strings hold "North" as a bold run.
  const strings = await page.evaluate(async () => {
    const bytes = new Uint8Array(window.devHost.files.get('Sample.xlsx').bytes)
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    let end = bytes.length - 22
    while (end > 0 && view.getUint32(end, true) !== 0x06054b50) end -= 1
    let at = view.getUint32(end + 16, true)
    for (let n = view.getUint16(end + 10, true); n > 0; n -= 1) {
      const [method, size, nameLength, extra, comment, offset] = [view.getUint16(at + 10, true), view.getUint32(at + 20, true), view.getUint16(at + 28, true), view.getUint16(at + 30, true), view.getUint16(at + 32, true), view.getUint32(at + 42, true)]
      const name = new TextDecoder().decode(bytes.subarray(at + 46, at + 46 + nameLength))
      at += 46 + nameLength + extra + comment
      if (name !== 'xl/sharedStrings.xml') continue
      const start = offset + 30 + view.getUint16(offset + 26, true) + view.getUint16(offset + 28, true)
      const data = bytes.subarray(start, start + size)
      const plain = method === 0 ? data : new Uint8Array(await new Response(new Blob([data]).stream().pipeThrough(new DecompressionStream('deflate-raw'))).arrayBuffer())
      return new TextDecoder().decode(plain)
    }
    return ''
  })
  const saved = /<r><rPr><b\/>(?:(?!<\/r>).)*<t[^>]*>North<\/t><\/r>/.test(strings)
  step('the saved file has the bold run', saved, saved ? '' : strings.slice(0, 300))
} catch (error) {
  failures += 1
  console.log(`FAIL ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
