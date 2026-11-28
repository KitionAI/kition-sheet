// Custom Sort in the kit's own dialog (plan stage 4B; walkthrough S2; Excel for Mac's Sort, WPS's
// 自定义排序): Data › Sort…, Home › Sort › Custom Sort and the cell menu's Sort ▸ Custom Sort open
// one dialog with the range and the header guess the engine chose and the active cell's column as
// the first level; a second level added, OK sorts by both in one undo step, the header row stays;
// Cancel changes nothing; no English in the Chinese dialog.
// A person's keys and clicks; the engine's state read back.
// node tests/hand/sort.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const ROWS = [
  ['Name', 'Region', 'Units', 'Price'],
  ['a', 'East', 5, 30],
  ['b', 'North', 3, 20],
  ['c', 'East', 2, 50],
  ['d', 'North', 8, 10],
  ['e', 'East', 1, 40],
  ['f', 'North', 6, 60],
]
const table = (e) => e.agent([{ command: 'range.setValues', params: { range: 'V1:Y7', values: ROWS } }])
const names = (e) => Promise.all(['V2', 'V3', 'V4', 'V5', 'V6', 'V7'].map((ref) => e.value(ref)))
const dialog = (e) => e.kit.getByTestId('sheet-sort-dialog')
const opened = async (e) => {
  await e.until(() => dialog(e).isVisible(), 50)
  assert(await dialog(e).isVisible(), 'the Sort dialog is open')
}
/** Home › Sort's arrow › Custom Sort. */
const fromHome = async (e) => {
  await e.kit.getByTestId('ribbon-tab-home').click()
  await e.wait(250)
  const arrow = e.kit.locator('.kui-line-inline').getByTestId('sheet-sort')
  if (!(await arrow.isVisible().catch(() => false))) await e.kit.getByTestId('kui-line-more-Editing').click()
  await e.kit.getByTestId('sheet-sort').first().click()
  await e.kit.getByTestId('sheet-sort-custom').click()
}

const list = [
  ['s2-sort-dialog', async (e) => {
    await table(e)
    await e.click('W3')
    // Data › Sort… (the ribbon's command button), else Home's Custom Sort.
    await e.kit.getByTestId('ribbon-tab-data').click()
    await e.wait(300)
    const button = e.kit.getByTestId('sheet-cmd-data.sort').first()
    if (await button.isVisible().catch(() => false)) await button.click()
    else await fromHome(e)
    await opened(e)
    assert.equal(await e.kit.getByTestId('command-dialog-run').count(), 0, 'no parameters dialog')
    assert.equal(await e.kit.getByTestId('sheet-sort-range').inputValue(), 'V1:Y7', 'the range is the data round the active cell')
    assert.equal(await e.kit.getByTestId('sheet-sort-header').isChecked(), true, 'its first row is taken as headers')
    await e.until(async () => (await e.kit.getByTestId('sheet-sort-column-0').locator('option').allInnerTexts()).includes('Region'), 30)
    assert.deepEqual(await e.kit.getByTestId('sheet-sort-column-0').locator('option').allInnerTexts(), ['Name', 'Region', 'Units', 'Price'], 'the columns are named by their headers')
    assert.equal(await e.kit.getByTestId('sheet-sort-column-0').inputValue(), 'W', 'the first level is the active cell\'s column')
    // Then by Price, descending.
    await e.kit.getByTestId('sheet-sort-add').click()
    await e.kit.getByTestId('sheet-sort-column-1').selectOption('Y')
    await e.kit.getByTestId('sheet-sort-order-1').selectOption('desc')
    await e.kit.getByTestId('sheet-sort-ok').click()
    await e.until(async () => (await e.value('V2')) === 'c', 40)
    assert.deepEqual(await names(e), ['c', 'e', 'a', 'f', 'b', 'd'], 'sorted by Region, then by Price from the highest')
    assert.equal(await e.value('V1'), 'Name', 'the header row stays')
    assert.equal(await dialog(e).count(), 0, 'the dialog closed')
    await e.until(async () => (await e.active()) === 'textarea.kui-text-sink', 30)
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the grid has the keyboard')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await e.value('V2')) === 'a', 40)
    assert.deepEqual(await names(e), ['a', 'b', 'c', 'd', 'e', 'f'], 'one undo takes the sort back')
  }],
  ['s2-sort-context-menu', async (e) => {
    await table(e)
    await e.click('W3')
    const at = await e.cellPoint('W3')
    await e.page.mouse.click(at.x, at.y, { button: 'right' })
    await e.kit.getByTestId('sheet-grid-menu-sort').click()
    await e.kit.getByTestId('sheet-grid-sort-custom').click()
    await opened(e)
    assert.equal(await e.kit.getByTestId('sheet-sort-column-0').inputValue(), 'W', 'the first level is Region')
    await e.page.keyboard.press('Escape')
    await e.until(async () => (await dialog(e).count()) === 0, 30)
  }],
  ['s2-sort-cancel', async (e) => {
    await table(e)
    await e.click('X4')
    await fromHome(e)
    await opened(e)
    await e.kit.getByTestId('sheet-sort-order-0').selectOption('desc')
    await e.page.keyboard.press('Escape')
    await e.until(async () => (await dialog(e).count()) === 0, 30)
    assert.equal(await dialog(e).count(), 0, 'Escape cancels')
    assert.deepEqual(await names(e), ['a', 'b', 'c', 'd', 'e', 'f'], 'nothing was sorted')
    await fromHome(e)
    await opened(e)
    await e.kit.getByTestId('sheet-sort-cancel').click()
    await e.until(async () => (await dialog(e).count()) === 0, 30)
    assert.deepEqual(await names(e), ['a', 'b', 'c', 'd', 'e', 'f'], 'Cancel sorts nothing')
  }],
  ['s2-same-column', async (e) => {
    await table(e)
    await e.click('W3')
    await fromHome(e)
    await opened(e)
    await e.kit.getByTestId('sheet-sort-add').click()
    await e.kit.getByTestId('sheet-sort-column-1').selectOption('W')
    assert(await e.kit.getByTestId('sheet-sort-twice').isVisible(), 'a column used twice is pointed out')
    assert.equal(await e.kit.getByTestId('sheet-sort-ok').isDisabled(), true, 'and OK waits')
    await e.kit.getByTestId('sheet-sort-remove-1').click()
    assert.equal(await e.kit.getByTestId('sheet-sort-ok').isDisabled(), false)
    await e.kit.getByTestId('sheet-sort-cancel').click()
    await e.until(async () => (await dialog(e).count()) === 0, 30)
  }],
  ['zh-labels', async (e) => {
    const z = await open({ which: e.which, query: 'locale=zh-CN', width: 2000 })
    try {
      await table(z)
      await z.click('W3')
      await fromHome(z)
      await opened(z)
      const text = await dialog(z).innerText()
      assert(!/\b(header|keys|matchCase|orientation|range|Sort by|Then by|Order|Column|Ascending|OK|Cancel|Add Level)\b/.test(text), `no English in the Chinese dialog (${text.replace(/\s+/g, ' ')})`)
      await z.page.keyboard.press('Escape')
    } finally {
      await z.close()
    }
  }],
]

await segments('sort', list, (which) => open({ which, width: 2000 }))
