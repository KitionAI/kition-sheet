// Every key the menus show works (plan stage 6, item 3; walkthrough R6, G6): each command of the
// engine's catalogue that has a key is pressed as a person presses it, on a small table with B2:C3
// selected, and something must change (the editor's state, the workbook, its cells, an open
// dialog or window, the zoom) — or the key is in EXEMPT with the reason. Then Cmd+E centres, Cmd+R fills
// right, and the number-format keys (Ctrl+Shift+1, 3, 4, 5) format the selection.
// A person's keys; the engine's state read back. Keys named `Cmd` are pressed with Cmd on a Mac and
// with Ctrl elsewhere (`Ctrl` is Control on both).
// node tests/hand/shortcuts.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const MAC = process.platform === 'darwin' && process.env.PLAYWRIGHT_NO_UA_PLATFORM !== '1'

/** Keys that change nothing a test can read here, and why; each is covered elsewhere or is the host's. */
const EXEMPT = {
  'file.new': 'the host makes workbooks (the development host ignores it)',
  'file.open': "the host's file dialog",
  'file.save': "the host saves; nothing in the sheet changes",
  'file.saveAs': "the host's file dialog",
  'file.close': 'the host closes the document',
  'file.print': "the host's print dialog",
  'formula.calculateNow': 'a workbook without volatile functions shows no change (formula tests cover it)',
  'formula.calculateSheet': 'a workbook without volatile functions shows no change (formula tests cover it)',
  'pivot.refresh': 'needs a pivot table (pivot tests cover it)',
  'data.refreshAll': 'needs a pivot table or a query (pivot tests cover it)',
  'cell.toggleCheckbox': 'needs a checkbox cell (gate-insert covers it)',
  'range.fill': 'acts on a cell being typed (tests/hand/commit.mjs covers it)',
  'edit.paste': 'reads the system clipboard (clipboard-keys.mjs covers it)',
  'edit.pasteSpecial': 'needs a copy in the sheet (tests/hand/paste-special.mjs covers it)',
  'edit.clearClipboard': 'needs a copy in the sheet (tests/hand/clipboard-marks.mjs covers it)',
  'edit.redo': 'needs an undone step (tests/hand/keys.mjs covers Cmd+Shift+Z and Cmd+Y)',
  'edit.undo': 'needs a step of the person (tests/hand/keys.mjs covers it)',
  'name.addFromSelection': 'needs labels beside the cells (formula tests cover it)',
}

/** A catalogue key (`Cmd+Shift+>`) as Playwright presses it on this platform. */
function playwrightKey(shortcut) {
  const parts = shortcut.split('+')
  const name = parts.length > 1 && parts[parts.length - 1] === '' ? '+' : parts.pop()
  const mods = parts.filter(Boolean)
  const shifted = { '<': 'Comma', '>': 'Period', '%': 'Digit5', '+': 'Equal' }
  const keys = { '-': 'Minus', '=': 'Equal', '`': 'Backquote', ';': 'Semicolon', Space: 'Space' }
  const out = []
  // ⌃⌘V elsewhere is Ctrl+Alt+V (keys.ts).
  if (mods.includes('Cmd') && mods.includes('Ctrl')) return MAC ? `Control+Meta+${name}` : `Control+Alt+${name}`
  for (const mod of mods) out.push(mod === 'Cmd' ? (MAC ? 'Meta' : 'Control') : mod === 'Ctrl' ? 'Control' : mod)
  const key = shifted[name] ?? keys[name] ?? (/^[a-z]$/i.test(name) ? name.toUpperCase() : /^[0-9]$/.test(name) ? `Digit${name}` : name)
  return [...out, key].join('+')
}

/** What a person can see change: the editor's state, the workbook, the table's cells and the windows open. */
async function snapshot(e) {
  const details = await e.details()
  const book = await e.ask('file.inspect')
  const cells = await e.ask('sheet.read', { range: 'A1:F12', formulas: true })
  const windows = await e.frame().evaluate(() => document.querySelectorAll('[role=dialog], [role=alertdialog], [data-testid=sheet-find], [data-testid=sheet-gridmenu]').length)
  const zoom = await e.zoom()
  return JSON.stringify({ details, book, cells, windows, zoom })
}

/** A small table, B2:C3 selected, nothing open; the person's undo steps taken back. */
async function reset(e) {
  for (let index = 0; index < 3; index += 1) {
    await e.page.keyboard.press('Escape')
    await e.wait(80)
  }
  for (let index = 0; index < 6 && ((await e.ask('history.list'))?.undo?.length ?? 0) > 0; index += 1) await e.agent([{ command: 'edit.undo', params: { scope: 'person' } }]).catch(() => undefined)
  await e.agent([
    { command: 'sheet.activate', params: { sheet: 0 } },
    { command: 'row.show', params: { rows: '1:20' } },
    { command: 'column.show', params: { cols: 'A:H' } },
    { command: 'cell.set', params: { cell: 'A1', input: 'Name' } },
    { command: 'cell.set', params: { cell: 'B1', input: 'Q1' } },
    { command: 'cell.set', params: { cell: 'C1', input: 'Q2' } },
    { command: 'cell.set', params: { cell: 'A2', input: 'north' } },
    { command: 'cell.set', params: { cell: 'B2', input: '12.5' } },
    { command: 'cell.set', params: { cell: 'C2', input: '7' } },
    { command: 'cell.set', params: { cell: 'A3', input: 'south' } },
    { command: 'cell.set', params: { cell: 'B3', input: '3' } },
    { command: 'cell.set', params: { cell: 'C3', input: '4.25' } },
  ])
  await e.click('B2')
  await e.page.keyboard.press('Shift+ArrowDown')
  await e.page.keyboard.press('Shift+ArrowRight')
  await e.until(async () => (await e.details()).selection === 'B2:C3', 30)
}

const list = [
  ['every-key-works', async (e) => {
    await e.agent([{ command: 'sheet.insert', params: {} }, { command: 'sheet.activate', params: { sheet: 0 } }])
    const commands = (await e.ask('app.commands')).filter((command) => command.shortcut)
    assert(commands.length >= 55, `the catalogue has ${commands.length} keys`)
    const dead = []
    for (const command of commands) {
      if (EXEMPT[command.id]) continue
      await reset(e)
      const before = await snapshot(e)
      await e.page.keyboard.press(playwrightKey(command.shortcut))
      let changed = false
      for (let tries = 0; tries < 12 && !changed; tries += 1) {
        await e.wait(100)
        changed = (await snapshot(e)) !== before
      }
      if (!changed) dead.push(`${command.id} (${command.shortcut} → ${playwrightKey(command.shortcut)})`)
    }
    await reset(e)
    assert.deepEqual(dead, [], `keys that did nothing: ${dead.join(', ')}`)
  }],
  ['g6-cmd-e-centres-cmd-r-fills-right', async (e) => {
    await reset(e)
    await e.page.keyboard.press(playwrightKey('Cmd+E'))
    await e.until(async () => (await e.details()).style?.align?.h === 'Center', 30)
    assert.equal((await e.details()).style?.align?.h, 'Center', 'Cmd+E centres')
    await e.page.keyboard.press(playwrightKey('Cmd+R'))
    await e.until(async () => (await e.value('C2')) === 12.5, 30)
    assert.equal(await e.value('C2'), 12.5, 'Cmd+R fills right')
  }],
  ['g6-number-format-keys', async (e) => {
    const format = async () => (await e.details()).style?.num_fmt
    for (const [key, check] of [
      ['Control+Shift+Digit1', (code) => code === '#,##0.00'],
      ['Control+Shift+Digit4', (code) => /[$¥€£]/.test(code ?? '')],
      ['Control+Shift+Digit5', (code) => code === '0%'],
      ['Control+Shift+Digit3', (code) => /[dy]/i.test(code ?? '') && !/[$%]/.test(code ?? '')],
    ]) {
      await reset(e)
      await e.page.keyboard.press(key)
      await e.until(async () => check(await format()), 30)
      assert(check(await format()), `${key} gave ${await format()}`)
    }
  }],
  ['g8-format-cells-currency', async (e) => {
    // Format Cells on a dollar cell shows $, every tab whole and every category without scrolling;
    // OK with nothing changed leaves the format as it was (walkthrough G8).
    await reset(e)
    await e.agent([{ command: 'cell.set', params: { cell: 'B5', input: '$8' } }])
    await e.click('B5')
    const code = (await e.details()).style?.num_fmt
    assert.match(code ?? '', /\$/, `a dollar format (${code})`)
    await e.page.keyboard.press(playwrightKey('Cmd+1'))
    await e.until(() => e.kit.getByTestId('fc-ok').isVisible(), 40)
    await e.until(async () => (await e.kit.getByTestId('fc-symbol').inputValue().catch(() => '')) === '$', 30)
    assert.equal(await e.kit.getByTestId('fc-symbol').inputValue(), '$', 'the cell\'s own symbol')
    const layout = await e.kit.getByTestId('sheet-format-cells').evaluate((dialog) => {
      const box = dialog.getBoundingClientRect()
      const tabs = [...dialog.querySelectorAll('[role=tab]')].map((tab) => ({ clipped: tab.scrollWidth > tab.clientWidth + 1, right: tab.getBoundingClientRect().right }))
      const list = dialog.querySelector('[data-testid=fc-categories]')
      return { tabs, right: box.right, scrolls: list ? list.scrollHeight > list.clientHeight + 1 : true }
    })
    assert(layout.tabs.length >= 6 && layout.tabs.every((tab) => !tab.clipped && tab.right <= layout.right), `every tab whole: ${JSON.stringify(layout)}`)
    assert.equal(layout.scrolls, false, 'the categories show without scrolling')
    await e.kit.getByTestId('fc-ok').click()
    await e.wait(400)
    assert.equal((await e.details()).style?.num_fmt, code, 'OK changed nothing')
  }],
]

await segments('shortcuts', list, (which) => open({ which }))
