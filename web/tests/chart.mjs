// Charts in a real browser, through the development host: node tests/chart.mjs [chromium|webkit]
// Inserts a chart of the selected cells from the shared Insert › Chart menu, selects, moves and
// resizes it with the pointer, changes its type and elements from the shared Chart Design
// controls, sees it follow its cells, undoes, saves to .xlsx, reopens, edits it again, and checks
// that a viewer cannot change it.
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
// The workbook's own chart is there too: this test follows the chart it makes, by id.
let id = null
const all = async () => (await run('drawing.describe', {})).objects.filter((o) => o.kind === 'chart')
const chart = async () => (id ? (await run('drawing.describe', { ids: [id] })).objects[0] : null)
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
  for (const [cell, input] of [['A1', ''], ['B1', 'East'], ['C1', 'West'], ['A2', 'Q1'], ['B2', '10'], ['C2', '4'], ['A3', 'Q2'], ['B3', '20'], ['C3', '6']]) {
    await run('cell.set', { cell, input })
  }
  await run('selection.set', { range: 'A1:C3' })
  await kit.getByTestId('ribbon-tab-insert').click()
  await kit.getByTestId('sheet-chart-insert').click()
  const others = (await all()).length
  await kit.getByTestId('sheet-chart-insert-line').click()
  // The new chart is selected: describing the selection names it.
  await until(async () => {
    const [o] = await all()
    if (o?.chart.kind !== 'line') return false
    id = o.id
    return true
  }, 'a line chart of the selected cells')
  assert.deepEqual((await chart()).chart.categories, ['Q1', 'Q2'])
  await until(() => kit.getByTestId('ribbon-tab-chartDesign').isVisible(), 'Chart Design shows for the new chart')
  console.log('PASS insert a chart of the selected cells from the shared menu')

  // Click away, then select it with the pointer, drag it, and resize it by a handle.
  await run('selection.set', { range: 'A10' })
  let f = (await chart()).frame
  await drag([f.x + f.w / 2, f.y + f.h / 2], [f.x + f.w / 2 + 60, f.y + f.h / 2 + 30])
  await until(async () => Math.abs((await chart()).frame.x - (f.x + 60)) < 1.5, 'dragging moves the chart')
  f = (await chart()).frame
  await drag([f.x + f.w, f.y + f.h], [f.x + f.w + 40, f.y + f.h + 20])
  await until(async () => Math.abs((await chart()).frame.w - (f.w + 40)) < 1.5, 'the handle resizes the chart')
  console.log('PASS select, move and resize the chart with the pointer')

  await until(() => kit.getByTestId('ribbon-tab-chartDesign').isVisible(), 'Chart Design shows for the selected chart')
  await kit.getByTestId('ribbon-tab-chartDesign').click()
  assert.equal(await kit.getByTestId('sheet-chart-edit-data').count(), 0, 'the data are the cells: no Edit Data')
  await kit.getByTestId('sheet-chart-type').click()
  await kit.getByTestId('sheet-chart-type-column').click()
  await until(async () => (await chart()).chart.kind === 'column', 'Change Chart Type')
  await kit.getByTestId('sheet-chart-legend').click()
  await kit.getByTestId('sheet-chart-legend-top').click()
  await until(async () => (await chart()).chart.legend === 'top', 'the legend moves')
  await kit.getByTestId('sheet-chart-data-labels').click()
  await until(async () => (await chart()).chart.dataLabels === true, 'data labels')
  // The person's Undo takes the last change back.
  await kit.getByTestId('sheet-undo').click()
  await until(async () => (await chart()).chart.dataLabels === false, 'undo')
  await kit.getByTestId('sheet-redo').click()
  await until(async () => (await chart()).chart.dataLabels === true, 'redo')
  console.log('PASS type and elements from Chart Design, with undo')
  // Chart Elements: a trendline and error bars on every series.
  await kit.getByTestId('sheet-chart-trendline').click()
  await kit.getByTestId('sheet-chart-trendline-linear').click()
  await until(async () => (await chart()).chart.series.every((s) => s.trendline?.kind === 'linear'), 'a linear trendline')
  await kit.getByTestId('sheet-chart-error-bars').click()
  await kit.getByTestId('sheet-chart-error-bars-stdErr').click()
  await until(async () => (await chart()).chart.series.every((s) => s.errorBars?.kind === 'stdErr'), 'standard error bars')
  console.log('PASS a trendline and error bars from Chart Elements')
  // Format Series: the second series becomes a line on the secondary axis.
  await kit.getByTestId('sheet-chart-format-series').click()
  await kit.getByTestId('sheet-chart-series-which').selectOption('1')
  await kit.getByTestId('sheet-chart-series-kind').selectOption('line')
  await kit.getByTestId('sheet-chart-series-secondary').click()
  await kit.getByTestId('sheet-chart-series-ok').click()
  await until(async () => (await chart()).chart.series[1].secondary === true && (await chart()).chart.series[1].kind === 'line', 'a line on the secondary axis')
  console.log('PASS a combination on two axes from Format Series')

  // Format: the value axis's scale, where labels sit, and a series' look (as Word's and Slide's).
  await kit.getByTestId('sheet-chart-type').click()
  await kit.getByTestId('sheet-chart-type-line').click()
  await until(async () => (await chart()).chart.kind === 'line', 'a line chart again')
  await kit.getByTestId('sheet-chart-format-axis').click()
  await kit.getByTestId('sheet-chart-axis-max').fill('200')
  await kit.getByTestId('sheet-chart-axis-major').fill('50')
  await kit.getByTestId('sheet-chart-axis-ok').click()
  await until(async () => (await chart()).chart.yAxis?.max === 200 && (await chart()).chart.yAxis?.major === 50, 'Format Axis sets the scale')
  await kit.getByTestId('sheet-chart-label-position').click()
  await kit.getByTestId('sheet-chart-label-position-t').click()
  await until(async () => (await chart()).chart.dataLabels && (await chart()).chart.dataLabelPosition === 't', 'labels above the points')
  await kit.getByTestId('sheet-chart-format-series').click()
  await kit.getByTestId('sheet-chart-series-width').fill('3')
  await kit.getByTestId('sheet-chart-series-marker').selectOption('square')
  await kit.getByTestId('sheet-chart-series-smooth').check()
  await kit.getByTestId('sheet-chart-series-ok').click()
  await until(async () => {
    const s = (await chart()).chart.series[0]
    return s.lineWidth === 3 && s.marker === 'square' && s.smooth === true
  }, 'Format Series sets the line')
  await kit.getByTestId('sheet-undo').click()
  await until(async () => !(await chart()).chart.series[0].smooth, 'undo the series look')
  await kit.getByTestId('sheet-redo').click()
  await until(async () => (await chart()).chart.series[0].smooth === true, 'redo the series look')
  console.log('PASS axis, label position and series look from Format, with undo')

  // It shows its cells; an agent changing its data is told to change the cells.
  await run('cell.set', { cell: 'B3', input: '25' })
  await until(async () => (await chart()).chart.series[0].values[1] === 25, 'the chart follows its cells')
  await assert.rejects(() => run('drawing.chart', { id, categories: ['X', 'Y'] }), /unsupported/)
  await run('drawing.chart', { id, title: 'Revenue 收入' })
  const before = (await chart()).chart

  const { base64 } = await run('file.saveBytes', { format: 'xlsx' })
  await page.evaluate(async (data) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
    await window.devHost.open('Chart.xlsx', bytes.buffer)
  }, base64)
  await until(async () => {
    const mine = (await all()).find((o) => o.chart.title === 'Revenue 收入')
    id = mine?.id ?? null
    return (await all()).length === others + 1 && id != null
  }, 'the chart reopens beside the workbook\'s own')
  assert.deepEqual({ ...(await chart()).chart, series: (await chart()).chart.series.map(({ name, values }) => ({ name, values })) }, { ...before, series: before.series.map(({ name, values }) => ({ name, values })) })
  await run('drawing.chart', { id, kind: 'bar', gridlines: false })
  await until(async () => (await chart()).chart.kind === 'bar', 'the reopened chart changes again')
  console.log('PASS save as .xlsx, reopen and edit again')

  await open('&role=viewer')
  await page.evaluate(async (data) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
    await window.devHost.open('Chart.xlsx', bytes.buffer)
  }, base64)
  await until(async () => (await all()).length === others + 1, 'the viewer sees the chart')
  await assert.rejects(() => run('drawing.chart', { id, title: 'No' }), /read-only|not allowed|only/i)
  assert.deepEqual(errors, [])
  console.log('PASS a viewer cannot edit the chart')
} catch (error) {
  // Outside the repository: its asset check counts every untracked file.
  const shot = join(tmpdir(), `kition-sheet-chart-${which}-failure.png`)
  await page.screenshot({ path: shot })
  console.log(`screenshot: ${shot}`)
  throw error
} finally {
  await browser.close()
  await server.close()
}
