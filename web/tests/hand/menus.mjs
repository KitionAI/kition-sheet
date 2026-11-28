// The ribbon in the default Single line mode (plan stage 0A, section 2 rows 21, 26, 27;
// walkthrough G1, V2, F6; WPS W-M1, W-L1, W-Q1): each of the seven menus a row button opens shows
// its items and choosing one does what it says; Home's Sort sorts at once; Freeze Panes is in View's
// row; a choice in a group's panel (Freeze Panes, Format Painter) closes the panel and gives the
// grid the keyboard back.
// node tests/hand/menus.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** Opens the menu of the row button `testId` on tab `tab`; answers it after checking its rows show. */
async function openMenu(e, tab, testId) {
  const { kit, wait } = e
  await kit.getByTestId(`ribbon-tab-${tab}`).click()
  await wait(250)
  assert.equal(await kit.locator('.kui-app').getAttribute('data-mode'), 'line', 'the editor is in Single line')
  const trigger = kit.locator('.kui-line-inline').getByTestId(testId)
  assert.equal(await trigger.count(), 1, `${testId} is in the row itself`)
  await trigger.click()
  const menu = kit.locator('.kui-line-inline .kui-popover').first()
  await e.until(() => menu.isVisible())
  const box = await menu.boundingBox()
  assert(box && box.height > 20, `${testId}: the menu has a height (${box?.height})`)
  const rows = await menu.locator('button').evaluateAll((items) => items.map((item) => item.getBoundingClientRect().height))
  assert(rows.length > 0 && rows.every((height) => height > 0), `${testId}: every row shows (${rows.length} rows, heights ${[...new Set(rows)].join('/')})`)
  return menu
}
const closed = (e) => e.until(async () => (await e.kit.locator('.kui-popover').count()) === 0, 30)
const sheetOf = async (e) => {
  const name = (await e.details()).sheet
  return (await e.ask('file.inspect')).sheets.find((sheet) => sheet.name === name)
}
/** The numbers in O1:O5, a column of its own (clear of the sample's chart) for sorting. */
const column = async (e) => Promise.all(['O1', 'O2', 'O3', 'O4', 'O5'].map((ref) => e.value(ref)))

const list = [
  ['g1-single-line-menus', async (e) => {
    const { kit, until, agent, page } = e
    await agent([
      { command: 'range.setValues', params: { range: 'O1:O5', values: [[30], [10], [50], [20], [40]] } },
      { command: 'range.setValues', params: { range: 'Q1:R2', values: [['a', 'b'], ['c', 'd']] } },
    ])

    // Home › Paste: with cells copied, Paste Special opens its dialog.
    await e.click('O1')
    await page.keyboard.press('ControlOrMeta+c')
    await e.wait(300)
    await e.click('O8')
    let menu = await openMenu(e, 'home', 'sheet-paste')
    await menu.getByRole('menuitem').filter({ hasText: /Paste Special/ }).click()
    const dialog = kit.locator('[role=dialog]').first()
    await until(async () => (await dialog.isVisible()) || (await e.value('O8')) === 30, 30)
    if (await dialog.isVisible()) await dialog.getByTestId('command-dialog-run').click()
    await until(async () => (await e.value('O8')) === 30, 30)
    assert.equal(await e.value('O8'), 30, 'Paste › Paste Special pastes O1 into O8')
    await page.keyboard.press('Delete')
    await until(async () => (await e.value('O8')) == null)

    // Home › Merge & Center: merges Q1:R1.
    await e.click('Q1')
    await page.keyboard.down('Shift')
    await e.click('R1')
    await page.keyboard.up('Shift')
    menu = await openMenu(e, 'home', 'sheet-merge')
    await menu.getByRole('menuitem').filter({ hasText: /^Merge & Center$/ }).click()
    await until(async () => (await sheetOf(e)).merges?.includes('Q1:R1'))
    assert((await sheetOf(e)).merges?.includes('Q1:R1'), `Merge & Center merges Q1:R1 (${JSON.stringify((await sheetOf(e)).merges)})`)
    await closed(e)

    // Home › AutoSum › Sum under O1:O5.
    await e.click('O6')
    menu = await openMenu(e, 'home', 'sheet-autosum')
    await menu.getByRole('menuitem').filter({ hasText: /^Sum/ }).click()
    // The guess is typed for Enter to confirm (plan stage 6, walkthrough E8).
    await until(async () => (await e.details()).editText === '=SUM(O1:O5)', 30)
    await page.keyboard.press('Enter')
    await until(async () => (await e.value('O6')) === 150)
    assert.equal(await e.value('O6'), 150, 'AutoSum › Sum adds O1:O5')
    await e.click('O6')
    await page.keyboard.press('Delete')
    await until(async () => (await e.value('O6')) == null)

    // Home › Sort's choices: Sort Z to A on O1:O5.
    await e.click('O2')
    menu = await openMenu(e, 'home', 'sheet-sort')
    await menu.getByRole('menuitem').filter({ hasText: /Z to A/ }).click()
    await until(async () => (await e.value('O1')) === 50)
    assert.deepEqual(await column(e), [50, 40, 30, 20, 10], 'Sort › Sort Z to A sorts O1:O5 descending')

    // Insert › Shapes: picks a shape to draw.
    await e.click('A9')
    menu = await openMenu(e, 'insert', 'sheet-drawing-shapes')
    await menu.locator('button').first().click()
    await closed(e)
    await page.keyboard.press('Escape')
    await e.wait(200)

    // Insert › Chart: inserts a chart of O1:O5.
    await e.click('O1')
    await page.keyboard.down('Shift')
    await e.click('O5')
    await page.keyboard.up('Shift')
    const charts = (await sheetOf(e)).charts.length
    menu = await openMenu(e, 'insert', 'sheet-chart-insert')
    await menu.locator('button').first().click()
    await until(async () => (await sheetOf(e)).charts.length > charts, 60)
    assert.equal((await sheetOf(e)).charts.length, charts + 1, 'Insert › Chart inserts a chart')
    await page.keyboard.press('Escape')
    await e.wait(200)
    await page.keyboard.press('Escape')
    await e.wait(200)

    // Insert › Symbol: types a symbol into the active cell.
    await e.click('S1')
    menu = await openMenu(e, 'insert', 'sheet-symbol')
    const symbol = menu.locator('.sheet-symbol-cell').first()
    const text = (await symbol.textContent()).trim()
    await symbol.click()
    await e.wait(300)
    const shown = await e.details()
    assert(shown.editing ? shown.editText?.includes(text) : (await e.value('S1')) === text, `Insert › Symbol types ${text} into S1`)
    await page.keyboard.press('Enter')
    await closed(e)
  }],
  ['sort-button', async (e) => {
    await e.agent([{ command: 'range.setValues', params: { range: 'O1:O5', values: [[3], [5], [1], [4], [2]] } }])
    await e.click('O3')
    await e.kit.getByTestId('ribbon-tab-home').click()
    await e.kit.locator('.kui-line-inline').getByTestId('sheet-sort-apply').click()
    await e.until(async () => (await e.value('O1')) === 1)
    assert.deepEqual(await column(e), [1, 2, 3, 4, 5], 'Home › Sort sorts the active cell\'s column ascending at once')
  }],
  ['v2-freeze-closes', async (e) => {
    await e.click('B3')
    // View's row: Freeze Panes itself.
    let menu = await openMenu(e, 'view', 'sheet-freeze')
    await menu.getByTestId('sheet-freeze-top-row').click()
    await e.until(async () => (await e.details()).freeze[0] === 1)
    assert.deepEqual((await e.details()).freeze, [1, 0], 'View › Freeze Panes › Freeze Top Row')
    await closed(e)
    await e.wait(200)
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the grid has the keyboard back')
    // Home's Editing panel: Freeze Panes there closes the panel.
    await e.kit.getByTestId('ribbon-tab-home').click()
    await e.kit.getByTestId('kui-line-more-Editing').click()
    const panel = e.kit.locator('.kui-drop-panel')
    await e.until(() => panel.isVisible())
    await panel.getByTestId('sheet-freeze').click()
    await e.kit.getByTestId('sheet-freeze-off').click()
    await e.until(async () => (await e.details()).freeze[0] === 0)
    assert.deepEqual((await e.details()).freeze, [0, 0], 'Unfreeze Panes from the Editing panel')
    await closed(e)
    assert.equal(await e.kit.locator('.kui-drop-panel').count(), 0, 'the panel closed after the choice')
    await e.wait(200)
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the grid has the keyboard back')
  }],
  ['f6-painter-closes', async (e) => {
    await e.click('B3')
    await e.kit.getByTestId('ribbon-tab-home').click()
    await e.kit.getByTestId('kui-line-more-Clipboard').click()
    const panel = e.kit.locator('.kui-drop-panel')
    await e.until(() => panel.isVisible())
    await panel.getByTestId('sheet-format-painter').click()
    await e.until(async () => (await e.kit.locator('.kui-drop-panel').count()) === 0, 30)
    assert.equal(await e.kit.locator('.kui-drop-panel').count(), 0, 'Format Painter closes the panel')
    assert.equal((await e.details()).formatPainter, 'once', 'and is picked up for one paint')
    await e.page.keyboard.press('Escape')
    await e.until(async () => (await e.details()).formatPainter == null, 30)
    assert.equal((await e.details()).formatPainter, null, 'Escape puts it down')
  }],
]

await segments('menus', list, (which) => open({ which }))
