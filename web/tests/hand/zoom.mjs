// Zooming as WPS does (plan stage 0A, section 2 row 11; walkthrough A7, A8, A9, A10, A11; WPS
// W-S1, W-S2, W-R4): a mouse wheel notch with Ctrl or Cmd adds or takes 15 points about the
// pointer, the status bar's − and + go 10 points to a multiple of 10, Cmd+= as +, the percentage
// opens the choices (presets, a typed value, Zoom to Selection), View › Zoom the same, no Fit, and
// every way stays within 10%–400%. A click after zooming hits the cell under the pointer.
// node tests/hand/zoom.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

async function settled(e, value) {
  await e.until(async () => (await e.zoom()) === value, 30)
  return e.zoom()
}

async function wheelAt(e, modifier, deltaY) {
  const box = await e.surface()
  await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
  await e.page.keyboard.down(modifier)
  await e.page.mouse.wheel(0, deltaY)
  await e.page.keyboard.up(modifier)
  await e.wait(200)
}

/** Types a zoom into the status bar's choices. */
async function typeZoom(e, percent) {
  await e.kit.getByTestId('kit-zoom').click()
  await e.kit.getByTestId('sheet-zoom-menu-field').fill(String(percent))
  await e.page.keyboard.press('Enter')
  await e.wait(200)
}

const list = [
  ['a7-wheel-zoom-step', async (e) => {
    assert.equal(await e.zoom(), 100, 'the sheet opens at 100%')
    await wheelAt(e, 'Control', -100)
    assert.equal(await settled(e, 115), 115, 'Ctrl and one wheel notch: 100% → 115%')
    await wheelAt(e, 'Control', -100)
    assert.equal(await settled(e, 130), 130, 'the next notch: 130%')
    await wheelAt(e, 'Control', 100)
    assert.equal(await settled(e, 115), 115, 'a notch back: 115%')
    await wheelAt(e, 'Meta', 100)
    assert.equal(await settled(e, 100), 100, 'Cmd and a notch back: 100%')
    await wheelAt(e, 'Meta', 100)
    assert.equal(await settled(e, 85), 85, 'and 85%')
    await wheelAt(e, 'Meta', -100)
    assert.equal(await settled(e, 100), 100, 'Cmd and a notch: 100%')
  }],
  ['a10-zoom-steps', async (e) => {
    await typeZoom(e, 129)
    assert.equal(await settled(e, 129), 129, 'a typed 129%')
    await e.kit.getByTestId('sheet-zoom-in').click()
    assert.equal(await settled(e, 130), 130, '+ from 129%: 130%')
    await e.kit.getByTestId('sheet-zoom-in').click()
    assert.equal(await settled(e, 140), 140, '+ again: 140%')
    await e.kit.getByTestId('sheet-zoom-out').click()
    assert.equal(await settled(e, 130), 130, '−: 130%')
    await e.page.keyboard.press('ControlOrMeta+Equal')
    assert.equal(await settled(e, 140), 140, 'Cmd+=: 140%')
    await e.kit.getByTestId('kit-zoom').click()
    await e.kit.getByTestId('sheet-zoom-menu-100').click()
    assert.equal(await settled(e, 100), 100, 'the choices\' 100% gives 100% at once')
    await e.kit.getByTestId('kit-zoom').click()
    await e.kit.getByTestId('sheet-zoom-menu-75').click()
    assert.equal(await settled(e, 75), 75, 'and 75%')
    assert.equal(await e.kit.locator('.kui-status-bar').getByText(/^Fit$/).count(), 0, 'the status bar has no Fit')
    await e.page.keyboard.press('ControlOrMeta+0')
    await e.wait(300)
    assert.equal(await e.zoom(), 75, 'Cmd+0 does not zoom')
    await typeZoom(e, 100)
    assert.equal(await settled(e, 100), 100)
  }],
  ['a11-zoom-limits', async (e) => {
    await typeZoom(e, 5)
    assert.equal(await settled(e, 10), 10, 'a typed 5% is 10%')
    await e.kit.getByTestId('sheet-zoom-out').click()
    await wheelAt(e, 'Control', 100)
    assert.equal(await settled(e, 10), 10, 'nothing goes below 10%')
    await typeZoom(e, 1000)
    assert.equal(await settled(e, 400), 400, 'a typed 1000% is 400%')
    await e.kit.getByTestId('sheet-zoom-in').click()
    await wheelAt(e, 'Control', -100)
    assert.equal(await settled(e, 400), 400, 'nothing goes above 400%')
    await typeZoom(e, 100)
    assert.equal(await settled(e, 100), 100)
  }],
  ['a9-view-zoom', async (e) => {
    await e.kit.getByTestId('ribbon-tab-view').click()
    await e.wait(250)
    const trigger = e.kit.locator('.kui-line-inline').getByTestId('sheet-view-zoom')
    assert.equal(await trigger.count(), 1, 'View › Zoom is in Single line\'s row')
    await trigger.click()
    await e.kit.getByTestId('sheet-view-zoom-menu-field').fill('85')
    await e.page.keyboard.press('Enter')
    assert.equal(await settled(e, 85), 85, 'View › Zoom takes a typed 85%')
    assert.equal(await e.kit.locator('[role=dialog]').count(), 0, 'and opens no parameters dialog')
    assert.equal(await e.kit.getByText(/missing `percent`/).count(), 0, 'and reports no error')
    await e.kit.locator('.kui-line-inline').getByTestId('sheet-view-zoom').click()
    await e.kit.getByTestId('sheet-view-zoom-menu-150').click()
    assert.equal(await settled(e, 150), 150, 'View › Zoom › 150%')
    await e.kit.getByTestId('kit-zoom').click()
    await e.kit.getByTestId('sheet-zoom-menu-100').click()
    assert.equal(await settled(e, 100), 100, 'the status bar\'s choices give 100% again')
    await e.kit.getByTestId('ribbon-tab-home').click()
  }],
  ['click-after-zoom', async (e) => {
    await e.home()
    await e.click('A1')
    await wheelAt(e, 'Control', 100)
    await wheelAt(e, 'Control', 100)
    assert.equal(await settled(e, 70), 70, 'zoomed to 70%')
    await e.click('D10')
    assert.equal((await e.details()).active, 'D10', 'a click at 70% hits D10')
    await e.page.keyboard.press('ControlOrMeta+Equal')
    await e.page.keyboard.press('ControlOrMeta+Equal')
    assert.equal(await settled(e, 90), 90)
    const box = await e.surface()
    await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    await e.page.mouse.wheel(0, 600)
    await e.wait(300)
    await e.click('F40')
    assert.equal((await e.details()).active, 'F40', 'and after scrolling at 90%, F40')
  }],
]

await segments('zoom', list, (which) => open({ which }))
