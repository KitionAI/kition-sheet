// Paste Special in the kit's own dialog (plan stage 4B; walkthrough J5; Excel's Paste Special, the
// items of WPS's paste options W-F9): ⌃⌘V (Ctrl+Alt+V), the cell menu and Home › Paste open one
// dialog; Values pastes a formula's result, Transpose turns a column into a row; Home › Paste has
// Paste Values and Transpose at once; with nothing copied in the sheet the key opens nothing; no
// English in the Chinese dialog.
// A person's keys and clicks; the engine's state read back.
// node tests/hand/paste-special.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const dialog = (e) => e.kit.getByTestId('sheet-paste-special')
const opened = async (e) => {
  await e.until(() => dialog(e).isVisible(), 40)
  assert(await dialog(e).isVisible(), 'the Paste Special dialog is open')
}
const gone = (e) => e.until(async () => (await dialog(e).count()) === 0, 30)
const cell = (e, ref) => e.ask('cell.get', { cell: ref })
const data = (e) =>
  e.agent([
    { command: 'cell.set', params: { cell: 'X1', input: '2' } },
    { command: 'cell.set', params: { cell: 'X2', input: '=X1*3' } },
    { command: 'range.setValues', params: { range: 'X4:X5', values: [[10], [20]] } },
  ])
/** Selects `from`:`to` with the mouse (a click, a Shift+click) and copies it with the keys. */
const copy = async (e, from, to) => {
  await e.click(from)
  await e.page.keyboard.down('Shift')
  await e.click(to)
  await e.page.keyboard.up('Shift')
  await e.page.keyboard.press('ControlOrMeta+c')
  await e.until(async () => (await e.details()).copied != null, 30)
}
/** Home › Paste's menu. */
const pasteMenu = async (e) => {
  await e.kit.getByTestId('ribbon-tab-home').click()
  await e.wait(250)
  const trigger = e.kit.locator('.kui-line-inline').getByTestId('sheet-paste')
  if (!(await trigger.isVisible().catch(() => false))) await e.kit.getByTestId('kui-line-more-Clipboard').click()
  await e.kit.getByTestId('sheet-paste').first().click()
}

const list = [
  ['j5-nothing-copied', async (e) => {
    await e.click('Z8')
    await e.page.keyboard.press('Control+Alt+v')
    await e.wait(500)
    assert.equal(await dialog(e).count(), 0, 'with nothing copied the key opens nothing')
    assert.equal(await e.value('Z8'), null, 'and pastes nothing')
  }],
  ['j5-paste-special', async (e) => {
    await data(e)
    await copy(e, 'X1', 'X2')
    await e.click('Z1')
    await e.page.keyboard.press('Control+Alt+v')
    await opened(e)
    assert.equal(await e.kit.locator('#param-what').isVisible(), true, 'Paste is a list in view')
    await e.kit.locator('#param-what').selectOption('values')
    await dialog(e).getByTestId('command-dialog-run').click()
    await gone(e)
    await e.until(async () => (await e.value('Z2')) === 6, 30)
    const pasted = await cell(e, 'Z2')
    assert.equal(pasted.value, 6, 'Values pastes the result')
    assert(!pasted.formula, `and no formula (${JSON.stringify(pasted.formula)})`)
    assert.equal(await e.value('Z1'), 2)
    assert((await cell(e, 'X2')).formula, 'the copied formula stays a formula')
    await e.until(async () => (await e.active()) === 'textarea.kui-text-sink', 30)
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await e.value('Z2')) == null, 30)
    assert.equal(await e.value('Z1'), null, 'one undo takes the paste back')
  }],
  ['j5-cmd-ctrl-v', async (e) => {
    // ⌃⌘V, the Mac's key (kit-editor's shortcut table is asked after the kit's).
    if (e.which !== 'chromium' || process.platform !== 'darwin') return
    await data(e)
    await copy(e, 'X1', 'X2')
    await e.click('AA1')
    await e.page.keyboard.press('Meta+Control+v')
    await opened(e)
    await dialog(e).getByTestId('sheet-paste-special-cancel').click()
    await gone(e)
    assert.equal(await e.value('AA1'), null, 'Cancel pastes nothing')
  }],
  ['j5-transpose', async (e) => {
    await data(e)
    await copy(e, 'X4', 'X5')
    await e.click('AB1')
    await pasteMenu(e)
    await e.kit.getByTestId('sheet-paste-transpose').click()
    await e.until(async () => (await e.value('AC1')) === 20, 30)
    assert.equal(await e.value('AB1'), 10, 'Transpose turns the column into a row')
    assert.equal(await e.value('AC1'), 20)
    assert.equal(await e.value('AB2'), null)
  }],
  ['j5-values-item', async (e) => {
    await data(e)
    await copy(e, 'X1', 'X2')
    await e.click('Z4')
    await pasteMenu(e)
    await e.kit.getByTestId('sheet-paste-values').click()
    await e.until(async () => (await e.value('Z5')) === 6, 30)
    assert.equal(await dialog(e).count(), 0, 'Paste Values asks nothing')
    assert(!(await cell(e, 'Z5')).formula, 'and pastes the result, not the formula')
  }],
  ['j5-context-menu', async (e) => {
    await data(e)
    await copy(e, 'X1', 'X2')
    const at = await e.cellPoint('AA4')
    await e.page.mouse.click(at.x, at.y, { button: 'right' })
    await e.kit.getByTestId('sheet-grid-menu-paste-special').click()
    await opened(e)
    await dialog(e).getByTestId('sheet-paste-transpose').check()
    await dialog(e).getByTestId('command-dialog-run').click()
    await gone(e)
    await e.until(async () => (await e.value('AB4')) != null, 30)
    assert.equal(await e.value('AA4'), 2, 'the cell menu opens the same dialog; transposed into a row')
    assert.equal(await e.value('AA5'), null)
  }],
  ['zh-labels', async (e) => {
    const z = await open({ which: e.which, query: 'locale=zh-CN', width: 2200 })
    try {
      await data(z)
      await copy(z, 'X1', 'X2')
      await z.click('Z1')
      await z.page.keyboard.press('Control+Alt+v')
      await opened(z)
      const text = await dialog(z).innerText()
      assert(!/\b(what|operation|skipBlanks|transpose|link|More options|Run|Paste|Values|Cancel|OK)\b/.test(text), `no English in the Chinese dialog (${text.replace(/\s+/g, ' ')})`)
      await z.page.keyboard.press('Escape')
    } finally {
      await z.close()
    }
  }],
]

await segments('paste-special', list, (which) => open({ which, width: 2200 }))
