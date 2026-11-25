// Ink in a real browser, through the development host: node tests/ink.mjs [chromium|webkit]
// Picks a pen from the shared Draw controls, draws a stroke with the pointer, undoes and redoes
// it, erases it with the eraser's drag, saves to .xlsx, reopens, and checks that a viewer cannot
// draw.
import assert from 'node:assert/strict'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
const kit = page.frameLocator('#kit')
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const run = (command, params = {}) => page.evaluate(async ({ command, params }) => (await window.devHost.connection().ai.run([{ command, params }], false)).completed[0].result, { command, params })
const until = async (check, message) => {
  for (let n = 0; n < 100; n += 1) { if (await check().catch(() => false)) return; await page.waitForTimeout(100) }
  assert.fail(message)
}
// Shapes and text boxes (the workbook's own chart is a drawing object too).
const inks = async () => (await run('drawing.describe', {})).objects.filter((o) => o.kind === 'ink')
const HEADER = { x: 46, y: 21 }
const open = async (query = '') => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US${query}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(500)
}
// The page point of sheet point (x, y) at 100%.
const at = async (x, y) => {
  const region = await kit.getByTestId('sheet-canvas-region').boundingBox()
  return { x: region.x + HEADER.x + x, y: region.y + HEADER.y + y }
}
const drag = async (from, to) => {
  const a = await at(from[0], from[1])
  const b = await at(to[0], to[1])
  await page.mouse.move(a.x, a.y)
  await page.mouse.down()
  await page.mouse.move((a.x + b.x) / 2, (a.y + b.y) / 2, { steps: 3 })
  await page.mouse.move(b.x, b.y, { steps: 3 })
  await page.mouse.up()
}

try {
  await open()
  await kit.getByTestId('ribbon-tab-draw').click()
  await kit.getByTestId('sheet-ink-pen').click()
  await until(async () => (await run('ink.state')).tool === 'pen', 'the pen is in hand')
  await kit.getByTestId('sheet-ink-weight').click()
  await kit.getByTestId('sheet-ink-weight-3').click()
  await until(async () => (await run('ink.state')).width === 3, 'the pen is 3 pt')
  const before = (await inks()).length
  await drag([300, 300], [420, 330])
  await until(async () => (await inks()).length === before + 1, 'one stroke on release')
  const stroke = (await inks()).at(-1).ink[0]
  assert.equal(stroke.width, 3)
  assert.ok(Math.abs(stroke.points[0][0] - 300) < 3 && Math.abs(stroke.points.at(-1)[0] - 420) < 3, JSON.stringify(stroke.points))
  await kit.getByTestId('sheet-undo').click()
  await until(async () => (await inks()).length === before, 'undo takes the stroke back')
  await kit.getByTestId('sheet-redo').click()
  await until(async () => (await inks()).length === before + 1, 'redo puts it back')
  console.log('PASS draw a stroke with the shared pen, with undo')

  await kit.getByTestId('sheet-ink-highlighter').click()
  await until(async () => (await run('ink.state')).tool === 'highlighter', 'the highlighter is in hand')
  await drag([300, 400], [400, 400])
  await until(async () => (await inks()).some((o) => o.ink[0].tool === 'highlighter'), 'a highlighter stroke')
  await kit.getByTestId('sheet-ink-eraser').click()
  await drag([360, 280], [360, 340])
  await until(async () => (await inks()).length === before + 1 && (await inks()).at(-1).ink[0].tool === 'highlighter', 'the eraser takes the pen stroke it crosses')
  console.log('PASS the highlighter, and the eraser drag')

  await kit.getByTestId('sheet-ink-select').click()
  const { base64 } = await run('file.saveBytes', { format: 'xlsx' })
  const reopen = (name) => page.evaluate(async ({ data, name }) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
    await window.devHost.open(name, bytes.buffer)
  }, { data: base64, name })
  await reopen('Ink.xlsx')
  await until(async () => (await inks()).some((o) => o.ink[0].tool === 'highlighter'), 'the stroke reopens as ink')
  console.log('PASS save as .xlsx and reopen')

  await open('&role=viewer')
  await reopen('Ink.xlsx')
  await until(async () => (await inks()).length >= 1, 'the viewer sees the ink')
  await assert.rejects(() => run('drawing.ink', { strokes: [{ points: [[10, 10], [20, 20]] }] }), /read-only|not allowed|only/i)
  assert.deepEqual(errors, [])
  console.log('PASS a viewer cannot draw')
} catch (error) {
  // Outside the repository: its asset check counts every untracked file.
  const shot = join(tmpdir(), `kition-sheet-ink-${which}-failure.png`)
  await page.screenshot({ path: shot })
  console.log(`screenshot: ${shot}`)
  throw error
} finally {
  await browser.close()
  await server.close()
}
