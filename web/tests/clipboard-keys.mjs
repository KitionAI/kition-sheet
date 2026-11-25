// The clipboard keys in a real browser: Cmd/Ctrl+C copies the selection to the system clipboard,
// and Cmd/Ctrl+V pastes what the system clipboard holds, a spreadsheet's HTML table (merged
// cells, line breaks in a cell) before its tab-separated text.
// node tests/clipboard-keys.mjs [chromium|webkit]
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
if (which === 'chromium') await context.grantPermissions(['clipboard-read', 'clipboard-write'])
const page = await context.newPage()
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const until = async (check, what) => {
  for (let i = 0; i < 80; i += 1) {
    if (await check().catch(() => false)) return
    await page.waitForTimeout(100)
  }
  assert.fail(`timed out: ${what}`)
}
try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90_000 })
  const kit = page.frameLocator('#kit')
  const frame = page.frames().find((f) => f !== page.mainFrame())
  const run = (commands) => page.evaluate(async (commands) => (await window.devHost.connection().ai.run(commands, false)).completed, commands)
  const cell = async (ref) => (await run([{ command: 'cell.get', params: { cell: ref } }]))[0].result
  const nameBox = kit.getByTestId('sheet-name-box')
  // The person goes to `range` through the Name Box, and the grid takes the keyboard back: an
  // agent's select.set moves only the agent's own editor state (KHP C3b).
  const select = async (range) => {
    await nameBox.fill(range)
    await nameBox.press('Enter')
    await until(async () => (await nameBox.inputValue()) === range, `${range} selected`)
    await kit.locator('.kui-text-sink').evaluate((sink) => sink.focus())
  }
  const region = await kit.getByTestId('sheet-canvas-region').boundingBox()
  await page.waitForTimeout(800)

  // Copy: select A1:B2 by dragging, Cmd/Ctrl+C; Cmd/Ctrl+V at D20 pastes it from the system
  // clipboard (the browser's own paste, as a person's key press makes it).
  await page.mouse.move(region.x + 60, region.y + 30)
  await page.mouse.down()
  await page.mouse.move(region.x + 140, region.y + 50, { steps: 4 })
  await page.mouse.up()
  await until(async () => (await nameBox.inputValue()) === 'A1:B2', 'A1:B2 selected')
  const [a1, b2] = [(await cell('A1')).input, (await cell('B2')).input]
  await page.keyboard.press('ControlOrMeta+c')
  await page.waitForTimeout(300)
  await run([{ command: 'edit.clearClipboard', params: {} }])
  await select('D20')
  await page.keyboard.press('ControlOrMeta+v')
  await until(async () => (await cell('D20')).input === a1, 'the copied cells pasted at D20')
  assert.equal((await cell('E21')).input, b2)

  // Paste: a spreadsheet's HTML table, whose plain text has a cell with unquoted line breaks.
  const html = '<table><tr><td colspan="2" style="font-weight:700;background:#e8e6f4;text-align:center">Size</td></tr>'
    + '<tr><td rowspan="2">Ops</td><td>1. close<br>2. open</td></tr><tr><td>load</td></tr></table>'
  const text = 'Size\t\nOps\t1. close\n2. open\n\tload\n'
  await select('D10')
  const written = await frame.evaluate(async ({ html, text }) => {
    try {
      await navigator.clipboard.write([new ClipboardItem({ 'text/html': new Blob([html], { type: 'text/html' }), 'text/plain': new Blob([text], { type: 'text/plain' }) })])
      return true
    } catch {
      return false
    }
  }, { html, text })
  // WebKit on Linux (Playwright's WPE build) keeps only the plain text of what a page writes to the
  // clipboard: its paste event carries no text/html, so there the paste event a key press makes
  // brings the table, as where a browser keeps a script from writing the clipboard.
  const keepsHtml = !(which === 'webkit' && process.platform === 'linux')
  if (written && keepsHtml) {
    await page.keyboard.press('ControlOrMeta+v')
  } else {
    // This browser keeps a script from writing the clipboard: the paste event a key press makes.
    await kit.locator('.kui-text-sink').evaluate((sink, { html, text }) => {
      const data = new DataTransfer()
      data.setData('text/html', html)
      data.setData('text/plain', text)
      sink.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }))
    }, { html, text })
  }
  await until(async () => (await cell('D10')).input === 'Size', 'the table pasted at D10')
  assert.equal((await cell('E11')).input, '1. close\n2. open', 'a line break stays in its cell')
  assert.equal((await cell('E12')).input, 'load', 'the next row is the next row')
  assert.equal((await cell('D13')).input ?? '', '', 'nothing spills below the table')
  assert.equal((await cell('D10')).style?.font?.bold ?? (await cell('D10')).bold, true, 'the heading is bold')
  assert.deepEqual(errors, [], 'no page errors')
  console.log(`PASS kit.sheet clipboard keys (${which})`)
} finally {
  await browser.close()
  await server.close()
}
