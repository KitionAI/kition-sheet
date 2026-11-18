// Synthetic regressions for formula reference colors and viewport-sized grids.
import assert from 'node:assert/strict'
import { mkdir } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 2560, height: 1200 } })
const failures = []
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const check = async (name, run) => {
  try { await run(); console.log(`PASS ${name}`) }
  catch (error) { failures.push(name); console.log(`FAIL ${name}: ${error.message}`) }
}
const until = async (test) => {
  for (let n = 0; n < 100; n += 1) {
    if (await test()) return
    await page.waitForTimeout(100)
  }
  assert.fail('condition did not become true')
}
try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  const kit = page.frameLocator('#kit')
  const formula = kit.getByTestId('sheet-formula')
  const name = kit.getByTestId('sheet-name-box')
  await formula.waitFor()
  const go = async (cell) => {
    await name.fill(cell)
    await name.press('Enter')
    await until(async () => await name.inputValue() === cell)
  }
  await check('wide grid fills the right edge beyond Z', async () => {
    const region = await kit.getByTestId('sheet-canvas-region').boundingBox()
    await page.waitForTimeout(500)
    await page.mouse.click(region.x + region.width - 80, region.y + 31)
    await until(async () => /^A[A-Z]1$/.test(await name.inputValue()))
  })
  await go('D2')
  const original = await formula.inputValue()
  await check('formula bar colors match reference outlines', async () => {
    await formula.fill('=B5+B10')
    await until(async () => await kit.locator('[data-formula-reference]').count() === 2)
    const colors = await kit.locator('[data-formula-reference]').evaluateAll((nodes) => nodes.map((node) => getComputedStyle(node).color))
    assert.notEqual(colors[0], colors[1])
    const region = await kit.getByTestId('sheet-canvas-region').boundingBox()
    const pixel = async (x, y) => kit.getByTestId('kit-canvas-surface').evaluate((canvas, { x, y }) => {
      const ratio = canvas.width / canvas.clientWidth
      return [...canvas.getContext('2d').getImageData(Math.round(x * ratio), Math.round(y * ratio), 1, 1).data]
    }, { x, y })
    // B5 and B10 have distinct visible frames drawn over the document.
    await page.waitForTimeout(300)
    const first = await pixel(110, 139)
    const second = await pixel(110, 239)
    assert.ok(first[3] > 0 && second[3] > 0, 'reference frames are visible')
    assert.notDeepEqual(first.slice(0, 3), second.slice(0, 3))
    assert.equal(`rgb(${first.slice(0, 3).join(', ')})`, colors[0])
    assert.equal(`rgb(${second.slice(0, 3).join(', ')})`, colors[1])
    await mkdir(new URL('../../target/gate/editor/', import.meta.url), { recursive: true })
    await page.screenshot({ path: new URL(`../../target/gate/editor/formula-${which}.png`, import.meta.url).pathname })
    assert.ok(region.width > 2000)
  })
  await formula.press('Escape')
  await check('Escape clears reference colors without entering the formula', async () => {
    await until(async () => await kit.locator('[data-formula-reference]').count() === 0)
    assert.equal(await formula.inputValue(), original)
  })
  await check('cell editing colors references and updates the formula bar', async () => {
    const region = await kit.getByTestId('sheet-canvas-region').boundingBox()
    await page.mouse.click(region.x + 46 + 3 * 64 + 32, region.y + 21 + 20 + 10)
    await page.keyboard.type('=B5+B10')
    await until(async () => await formula.inputValue() === '=B5+B10')
    await until(async () => await kit.locator('[data-formula-reference]').count() === 2)
    await page.keyboard.press('Escape')
  })
  await check('resizing and zooming out keep the grid across the viewport', async () => {
    await page.setViewportSize({ width: 3200, height: 1400 })
    const region = await kit.getByTestId('sheet-canvas-region').boundingBox()
    await page.mouse.move(region.x + 100, region.y + 100)
    await page.keyboard.down('Control')
    await page.mouse.wheel(0, 180)
    await page.keyboard.up('Control')
    await page.waitForTimeout(600)
    await page.mouse.click(region.x + region.width - 80, region.y + 30)
    await until(async () => /^[A-Z]{2,3}\d+$/.test(await name.inputValue()))
    const before = await name.inputValue()
    await page.mouse.move(region.x + 300, region.y + 300)
    await page.mouse.wheel(2500, 3000)
    await page.waitForTimeout(600)
    await page.mouse.click(region.x + region.width - 80, region.y + 30)
    await until(async () => await name.inputValue() !== before)
  })
  assert.deepEqual(errors, [])
} finally {
  await browser.close()
  await server.close()
}
assert.deepEqual(failures, [])
