// The grid has a scrollbar along its right edge and one along its bottom edge (Excel, WPS): each
// follows the view and moves it, and the sheet grows as the view nears its end.
// node tests/scrollbars.mjs [chromium|webkit]
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } })
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90_000 })
  const kit = page.frameLocator('#kit')
  const workspace = kit.getByTestId('sheet-grid-workspace')
  const region = kit.getByTestId('sheet-canvas-region')
  const vertical = workspace.locator('[role="scrollbar"][aria-orientation="vertical"]')
  const horizontal = workspace.locator('[role="scrollbar"][aria-orientation="horizontal"]')
  await vertical.waitFor({ timeout: 30_000 })
  await horizontal.waitFor({ timeout: 30_000 })
  const number = async (bar, name) => Number(await bar.getAttribute(name))
  const now = (bar) => number(bar, 'aria-valuenow')
  const until = async (message, check) => {
    let last = null
    for (let attempt = 0; attempt < 50; attempt += 1) {
      last = await check()
      if (last === true) return
      await page.waitForTimeout(100)
    }
    assert.fail(`${which}: ${message} (${JSON.stringify(last)})`)
  }

  // Where they are: the vertical one along the grid's right edge, the horizontal one along its bottom edge.
  const grid = await region.boundingBox()
  const v = await vertical.boundingBox()
  const h = await horizontal.boundingBox()
  assert(v.height > v.width * 4 && Math.abs(v.x - (grid.x + grid.width)) <= 1 && Math.abs(v.height - grid.height) <= 1, `the vertical scrollbar runs along the grid's right edge: ${JSON.stringify({ grid, v })}`)
  assert(h.width > h.height * 4 && Math.abs(h.y - (grid.y + grid.height)) <= 1 && Math.abs(h.width - grid.width) <= 1, `the horizontal scrollbar runs along the grid's bottom edge: ${JSON.stringify({ grid, h })}`)
  assert.equal(await horizontal.getAttribute('aria-label'), 'Horizontal sheet scroll')
  assert.equal(await vertical.getAttribute('aria-label'), 'Vertical sheet scroll')
  await until('a sheet opens at its top-left cell, with room to scroll both ways', async () => {
    const state = { x: await now(horizontal), y: await now(vertical), maxX: await number(horizontal, 'aria-valuemax'), maxY: await number(vertical, 'aria-valuemax') }
    return (state.x === 0 && state.y === 0 && state.maxX > 0 && state.maxY > 0) || state
  })

  // They follow the view: the wheel over the grid moves each one's position.
  await page.mouse.move(grid.x + grid.width / 2, grid.y + grid.height / 2)
  await page.mouse.wheel(300, 0)
  await until('the wheel to the right moves the horizontal scrollbar', async () => Math.abs((await now(horizontal)) - 300) <= 1 || (await now(horizontal)))
  await page.mouse.wheel(0, 400)
  await until('the wheel down moves the vertical scrollbar', async () => Math.abs((await now(vertical)) - 400) <= 1 || (await now(vertical)))
  assert(Math.abs((await now(horizontal)) - 300) <= 1, 'the wheel down keeps the horizontal position')

  // They move the view: the keyboard on the horizontal one, a drag of the vertical one's thumb.
  await horizontal.focus()
  await page.keyboard.press('Home')
  await until('Home on the horizontal scrollbar goes to the first column', async () => (await now(horizontal)) === 0 || (await now(horizontal)))
  await page.keyboard.press('PageDown')
  await until('Page Down on the horizontal scrollbar moves one view to the right', async () => (await now(horizontal)) > grid.width / 2 || (await now(horizontal)))
  assert(Math.abs((await now(vertical)) - 400) <= 1, 'the horizontal scrollbar keeps the vertical position')
  const thumb = await vertical.locator('.kui-scrollbar-thumb').boundingBox()
  const before = await now(vertical)
  await page.mouse.move(thumb.x + thumb.width / 2, thumb.y + thumb.height / 2)
  await page.mouse.down()
  await page.mouse.move(thumb.x + thumb.width / 2, thumb.y + thumb.height / 2 + 40, { steps: 4 })
  await page.mouse.up()
  await until('dragging the vertical thumb down scrolls down', async () => (await now(vertical)) > before + 10 || (await now(vertical)))

  // The sheet grows: at the end of the horizontal scrollbar there is more sheet to scroll to.
  const end = await number(horizontal, 'aria-valuemax')
  await horizontal.focus()
  await page.keyboard.press('End')
  await until('at its end the sheet grows to the right', async () => (await number(horizontal, 'aria-valuemax')) > end || { end, max: await number(horizontal, 'aria-valuemax'), now: await now(horizontal) })

  assert.equal(await kit.getByTestId('sheet-unsaved').count(), 0, 'scrolling is not an edit')
  assert.deepEqual(errors, [], 'no page errors')
  console.log(`PASS kit.sheet grid scrollbars (${which})`)
} finally {
  await browser.close()
  await server.close()
}
