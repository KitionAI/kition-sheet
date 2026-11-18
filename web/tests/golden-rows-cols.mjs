// Inserting and deleting rows and columns (module M4, insert and delete rows and columns), as WPS for Mac has
// them, in a real browser through the development host: node tests/golden-rows-cols.mjs [chromium|webkit]
// Home › Insert and Home › Delete (rows above and below, columns left and right, each with its
// count; cells; the sheet), the grid's right-click menus (a cell's, a row heading's, a column
// heading's), Ctrl+Shift+= and Ctrl+-, undo and redo, save and reopen, and what moves with the
// lines: formats, conditional formats, validation, links, pictures, merged cells, hidden lines,
// filters, frozen panes, formulas and charts.
// Each step names its case (INS-C7 for inserting, DEL-C8 for deleting); `GOLDEN_ONLY=INS-C7,DEL-C8` runs some.
import { startGolden } from './golden-harness.mjs'

const h = await startGolden({ prefix: '' })
/** The grid's pitch as the engine lays it out (the harness reads it when a workbook opens). */
const G = h.grid
const { page, kit, step, section, until, same, ai, exec, details, q, cellOf, region, goTo, build, numbered, saveAndReopen, openEditor } = h

const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAFklEQVR4nGP4z8DAwMDAxMDAwMDAAAANHQEDasKb6QAAAABJRU5ErkJggg=='
const letters = (index) => {
  let n = index + 1
  let out = ''
  while (n > 0) {
    const r = (n - 1) % 26
    out = String.fromCharCode(65 + r) + out
    n = Math.floor((n - 1) / 26)
  }
  return out
}
const val = async (ref) => {
  const v = (await cellOf(ref)).value
  return v == null ? '' : String(v)
}
/** The active sheet as `document.inspect` lists it. */
const info = async () => {
  const wb = await h.workbook()
  return wb.sheets[wb.activeSheet]
}
/** The used range's last row and last column (`A1:J20` → [20, 'J']). */
const used = async () => {
  const range = (await info()).usedRange ?? 'A1'
  const end = range.split(':').pop()
  return [Number(end.replace(/^[A-Z]+/, '')), end.replace(/\d+$/, '')]
}
const settle = () => page.waitForTimeout(250)

/** A sheet of `rows` × `cols` cells, each holding its own address, at the top-left with `at` selected. */
async function table(rows = 20, cols = 10, at = 'A1') {
  await build(numbered(2), { fill: false })
  const values = Array.from({ length: rows }, (_, row) => Array.from({ length: cols }, (_, col) => `${letters(col)}${row + 1}`))
  await ai([{ command: 'range.setValues', params: { range: `${q('工作表1')}!A1`, values } }])
  await exec('view.zoom', { percent: 100 })
  // Back to the top-left (a sheet of the same name keeps where it was scrolled).
  const box = await region()
  await page.mouse.move(box.x + 300, box.y + 200)
  await page.mouse.wheel(-20000, -20000)
  await page.waitForTimeout(300)
  await goTo(at)
}
/** Selects `range` through the Name Box (`B3`, `A3:E5`, `3:5`, `C:E`). */
const select = async (range) => {
  await kit.getByTestId('sheet-name-box').fill(range)
  await kit.getByTestId('sheet-name-box').press('Enter')
  await settle()
}

// ———— the menus ————
const home = async () => {
  await kit.getByTestId('ribbon-tab-home').click()
}
/** Runs a menu row; with `count`, types it in the row's count box first. */
async function pick(id, count) {
  const row = kit.getByTestId(id)
  await row.waitFor()
  if (count != null) {
    const box = kit.getByTestId(`${id}-count`)
    await box.fill('')
    await box.fill(String(count))
  }
  await row.click({ position: { x: 24, y: 10 } })
  await settle()
}
/** Home › Insert or Home › Delete, then one of its rows (`rows-above`, `cols-right`, `cells-down`, `rows`…). */
async function viaRibbon(menu, item, count) {
  await home()
  await kit.getByTestId(`sheet-${menu}-menu`).click()
  await pick(`sheet-${menu}-${item}`, count)
}
const openRibbon = async (menu) => {
  await home()
  await kit.getByTestId(`sheet-${menu}-menu`).click()
  await kit.getByTestId(`sheet-${menu}-${menu === 'insert' ? 'sheet' : 'rows'}`).waitFor()
}
const centre = async (col, row) => {
  const box = await region()
  return { x: box.x + 46 + col * G.cw + 32, y: box.y + 21 + row * G.rh + 10 }
}
const menuShown = () => kit.getByTestId('sheet-grid-menu').waitFor()
const rightCell = async (col, row) => {
  const p = await centre(col, row)
  await page.mouse.click(p.x, p.y, { button: 'right' })
  await menuShown()
}
const rightRow = async (row) => {
  const box = await region()
  await page.mouse.click(box.x + 20, box.y + 21 + row * G.rh + 10, { button: 'right' })
  await menuShown()
}
const rightCol = async (col) => {
  const box = await region()
  await page.mouse.click(box.x + 46 + col * G.cw + 32, box.y + 10, { button: 'right' })
  await menuShown()
}
/** A cell's right-click menu › Insert or Delete › a row. */
async function viaCellMenu(col, row, sub, item, count) {
  await rightCell(col, row)
  await kit.getByTestId(`sheet-grid-menu-${sub}`).hover()
  await pick(`sheet-grid-${sub}-${item}`, count)
}
/** Clicks a row or column heading; `shift` holds Shift (extending the selection). */
const clickAt = async (x, y, shift) => {
  if (shift) await page.keyboard.down('Shift')
  await page.mouse.click(x, y)
  if (shift) await page.keyboard.up('Shift')
  await settle()
}
const clickRowHeading = async (row, shift = false) => {
  const box = await region()
  await clickAt(box.x + 20, box.y + 21 + row * G.rh + 10, shift)
}
const clickColHeading = async (col, shift = false) => {
  const box = await region()
  await clickAt(box.x + 46 + col * G.cw + 32, box.y + 10, shift)
}
const closeMenus = async () => {
  await page.keyboard.press('Escape')
  await page.keyboard.press('Escape')
  await settle()
}
const texts = (locator) => locator.evaluateAll((rows) => rows.map((row) => row.textContent.trim()))
/** The rows of the open popover holding `id`. */
const popoverRows = (id) => kit.locator(`.kui-popover:has([data-testid="${id}"])`).last().locator(':scope > [role^="menuitem"], :scope > div > [role^="menuitem"]')
const undo = async () => {
  await kit.getByTestId('sheet-undo').click()
  await settle()
}
const redo = async () => {
  await kit.getByTestId('sheet-redo').click()
  await settle()
}
/** Tries each refused value in a count box: it stays empty and the row does nothing. */
async function refuses(menu, item, cell) {
  const results = []
  for (const bad of ['汉字', '-3', '1.5', 'abc', '@#']) {
    await home()
    await kit.getByTestId(`sheet-${menu}-menu`).click()
    const box = kit.getByTestId(`sheet-${menu}-${item}-count`)
    await box.fill('')
    await box.fill(bad)
    const shown = await box.inputValue()
    await kit.getByTestId(`sheet-${menu}-${item}`).click({ position: { x: 24, y: 10 } })
    await settle()
    await closeMenus()
    results.push({ bad, shown, kept: (await val(cell)) === cell })
  }
  return results
}

try {
  await openEditor('')
  await table()

  // ———————————————————— Insert: entry points and menu ————————————————————
  await section('INS-C4 开始-插入下拉的菜单项与数量框', async () => {
    await table(20, 10, 'C5')
    await openRibbon('insert')
    const rows = await texts(popoverRows('sheet-insert-sheet'))
    step('INS-C4 菜单项依次为 向上插入行、向下插入行、向左插入列、向右插入列、插入单元格-右移、插入单元格-下移、插入工作表', same(rows, ['向上插入行', '向下插入行', '向左插入列', '向右插入列', '插入单元格-右移', '插入单元格-下移', '插入工作表']), JSON.stringify(rows))
    const counts = []
    for (const id of ['rows-above', 'rows-below', 'cols-left', 'cols-right']) counts.push(await kit.getByTestId(`sheet-insert-${id}-count`).inputValue())
    step('INS-C4 四个插入行列项各带数量框，默认 1', same(counts, ['1', '1', '1', '1']), JSON.stringify(counts))
    await closeMenus()
  })

  await section('INS-C5 右键菜单「插入」的子菜单', async () => {
    await table(20, 10, 'C5')
    await rightCell(2, 4)
    await kit.getByTestId('sheet-grid-menu-insert').hover()
    await kit.getByTestId('sheet-grid-insert-sheet').waitFor()
    const rows = await texts(popoverRows('sheet-grid-insert-sheet'))
    // WPS for Mac's order: the cells first (the case sheet lists them after the rows and columns).
    step('INS-C5 右键「插入」展开：插入单元格-右移/下移、向上/向下插入行、向左/向右插入列、插入工作表', same(rows, ['插入单元格-右移', '插入单元格-下移', '向上插入行', '向下插入行', '向左插入列', '向右插入列', '插入工作表']), JSON.stringify(rows))
    await closeMenus()
  })

  // ———————————————————— Insert N rows above ————————————————————
  await section('INS-C7 向上插入 1 行', async () => {
    await table(20, 10, 'B3')
    await viaRibbon('insert', 'rows-above')
    const [rows] = await used()
    step('INS-C7 第 3 行上方新增空行，原 B3 移到 B4，总行数加 1', (await val('B4')) === 'B3' && (await val('B3')) === '' && rows === 21)
    // The same from the cell's right-click menu.
    await undo()
    await viaCellMenu(1, 2, 'insert', 'rows-above')
    step('INS-C7 右键-插入-向上插入行 结果相同', (await val('B4')) === 'B3' && (await val('B3')) === '')
  })

  await section('INS-C8 向上插入 5 行（手动输入数量）', async () => {
    await table(20, 10, 'B5')
    await viaRibbon('insert', 'rows-above', 5)
    step('INS-C8 B5 上方一次插入 5 行，原 B5 到 B10，总行数加 5', (await val('B10')) === 'B5' && (await val('B5')) === '' && (await used())[0] === 25)
  })

  await section('INS-C9 选中多个单元格向上插入 1 行', async () => {
    await table(20, 10)
    await select('B3:D5')
    await rightCell(2, 3)
    await kit.getByTestId('sheet-grid-menu-insert').hover()
    const shown = await kit.getByTestId('sheet-grid-insert-rows-above-count').inputValue()
    step('INS-C9 数量框默认是选区的行数（WPS）', shown === '3', shown)
    await pick('sheet-grid-insert-rows-above', 1)
    step('INS-C9 选区首行上方新增 1 行，原第 3 行下移 1 行', (await val('B4')) === 'B3' && (await val('B3')) === '' && (await val('B6')) === 'B5')
  })

  await section('INS-C10 向上插入数量 1234 显示为 1000', async () => {
    await table(20, 10, 'B5')
    await home()
    await kit.getByTestId('sheet-insert-menu').click()
    const box = kit.getByTestId('sheet-insert-rows-above-count')
    await box.fill('')
    await box.fill('1234')
    step('INS-C10 输入 1234 显示为 1000', (await box.inputValue()) === '1000', await box.inputValue())
    await kit.getByTestId('sheet-insert-rows-above').click({ position: { x: 24, y: 10 } })
    await until(async () => (await val('B1005')) === 'B5')
    step('INS-C10 插入 1000 行，原 B5 到 B1005', (await val('B1005')) === 'B5' && (await used())[0] === 1020)
  })

  await section('INS-C11 向上插入 3 行后保存重开', async () => {
    await table(20, 10, 'B5')
    await viaRibbon('insert', 'rows-above', 3)
    await saveAndReopen()
    step('INS-C11 重开后总行数多 3，原 B5 在 B8，其余位置不变', (await val('B8')) === 'B5' && (await val('B4')) === 'B4' && (await used())[0] === 23)
  })

  await section('INS-C12 向上插入数量框拒绝 汉字、负数、小数、字母、特殊字符', async () => {
    await table(20, 10, 'B5')
    const results = await refuses('insert', 'rows-above', 'B5')
    step('INS-C12 均无法输入，行数与内容不变', results.every((r) => r.shown === '' && r.kept) && (await used())[0] === 20, JSON.stringify(results))
  })

  // ———————————————————— Insert N rows below ————————————————————
  await section('INS-C14 向下插入 1 行', async () => {
    await table(20, 10, 'A5')
    await viaRibbon('insert', 'rows-below')
    step('INS-C14 第 5 行下方新增空行，原 A6 到 A7，总行数加 1', (await val('A5')) === 'A5' && (await val('A6')) === '' && (await val('A7')) === 'A6' && (await used())[0] === 21)
    step('INS-C14 新行被选中（WPS）', (await details()).selection === 'A6')
  })

  await section('INS-C15 向下插入 5 行', async () => {
    await table(20, 10, 'A5')
    await viaRibbon('insert', 'rows-below', 5)
    step('INS-C15 原 A6 到 A11，总行数加 5', (await val('A11')) === 'A6' && (await val('A6')) === '' && (await used())[0] === 25)
  })

  await section('INS-C16 选中多个单元格向下插入 1 行', async () => {
    await table(20, 10)
    await select('A4:C6')
    await viaCellMenu(1, 4, 'insert', 'rows-below', 1)
    step('INS-C16 选区末行下方新增 1 行，原第 7 行下移 1 行', (await val('A6')) === 'A6' && (await val('A7')) === '' && (await val('A8')) === 'A7')
  })

  await section('INS-C17 向下插入数量 1234 显示为 1000', async () => {
    await table(20, 10, 'A5')
    await home()
    await kit.getByTestId('sheet-insert-menu').click()
    const box = kit.getByTestId('sheet-insert-rows-below-count')
    await box.fill('')
    await box.fill('1234')
    const shown = await box.inputValue()
    await kit.getByTestId('sheet-insert-rows-below').click({ position: { x: 24, y: 10 } })
    await until(async () => (await val('A1006')) === 'A6')
    step('INS-C17 显示 1000，插入 1000 行，原 A6 到 A1006', shown === '1000' && (await val('A1006')) === 'A6')
  })

  await section('INS-C18 向下插入 3 行后保存重开', async () => {
    await table(20, 10, 'A5')
    await viaRibbon('insert', 'rows-below', 3)
    await saveAndReopen()
    step('INS-C18 重开后原 A6 在 A9，总行数多 3', (await val('A9')) === 'A6' && (await val('A5')) === 'A5' && (await used())[0] === 23)
  })

  await section('INS-C19 向下插入数量框拒绝非法输入', async () => {
    await table(20, 10, 'A5')
    const results = await refuses('insert', 'rows-below', 'A6')
    step('INS-C19 均无法输入，行数与内容不变', results.every((r) => r.shown === '' && r.kept) && (await used())[0] === 20, JSON.stringify(results))
  })

  // ———————————————————— Insert N columns to the left ————————————————————
  await section('INS-C21 向左插入 1 列', async () => {
    await table(20, 10, 'C3')
    await viaRibbon('insert', 'cols-left')
    step('INS-C21 C 列左侧新增空列，原 C3 到 D3，总列数加 1', (await val('D3')) === 'C3' && (await val('C3')) === '' && (await used())[1] === 'K')
  })

  await section('INS-C22 向左插入 5 列', async () => {
    await table(20, 10, 'C3')
    await viaRibbon('insert', 'cols-left', 5)
    step('INS-C22 原 C3 到 H3，总列数加 5', (await val('H3')) === 'C3' && (await val('C3')) === '' && (await used())[1] === 'O')
  })

  await section('INS-C23 选中多个单元格向左插入 1 列', async () => {
    await table(20, 10)
    await select('C3:E5')
    await viaCellMenu(3, 3, 'insert', 'cols-left', 1)
    step('INS-C23 选区首列左侧新增 1 列，原 C 列右移 1 列', (await val('D3')) === 'C3' && (await val('C3')) === '' && (await val('F5')) === 'E5')
  })

  await section('INS-C24 向左插入数量 123 显示为 100', async () => {
    await table(20, 10, 'C3')
    await home()
    await kit.getByTestId('sheet-insert-menu').click()
    const box = kit.getByTestId('sheet-insert-cols-left-count')
    await box.fill('')
    await box.fill('123')
    const shown = await box.inputValue()
    await kit.getByTestId('sheet-insert-cols-left').click({ position: { x: 24, y: 10 } })
    const to = `${letters(102)}3`
    await until(async () => (await val(to)) === 'C3')
    step(`INS-C24 显示 100，插入 100 列，原 C3 到 ${to}`, shown === '100' && (await val(to)) === 'C3', shown)
  })

  await section('INS-C25 向左插入 3 列后保存重开', async () => {
    await table(20, 10, 'C3')
    await viaRibbon('insert', 'cols-left', 3)
    await saveAndReopen()
    step('INS-C25 重开后原 C3 在 F3，总列数多 3', (await val('F3')) === 'C3' && (await val('B3')) === 'B3' && (await used())[1] === 'M')
  })

  await section('INS-C26 向左插入数量框拒绝非法输入', async () => {
    await table(20, 10, 'C3')
    const results = await refuses('insert', 'cols-left', 'C3')
    step('INS-C26 均无法输入，列数与内容不变', results.every((r) => r.shown === '' && r.kept) && (await used())[1] === 'J', JSON.stringify(results))
  })

  // ———————————————————— Insert N columns to the right ————————————————————
  await section('INS-C28 向右插入 1 列', async () => {
    await table(20, 10, 'C3')
    await viaRibbon('insert', 'cols-right')
    step('INS-C28 C 列右侧新增空列，原 D3 到 E3，总列数加 1', (await val('C3')) === 'C3' && (await val('D3')) === '' && (await val('E3')) === 'D3' && (await used())[1] === 'K')
  })

  await section('INS-C29 向右插入 5 列', async () => {
    await table(20, 10, 'C3')
    await viaRibbon('insert', 'cols-right', 5)
    step('INS-C29 原 D3 到 I3，总列数加 5', (await val('I3')) === 'D3' && (await val('D3')) === '' && (await used())[1] === 'O')
  })

  await section('INS-C30 选中多个单元格向右插入 1 列', async () => {
    await table(20, 10)
    await select('C3:E5')
    await viaCellMenu(3, 3, 'insert', 'cols-right', 1)
    step('INS-C30 选区末列右侧新增 1 列，原 F 列右移 1 列', (await val('E3')) === 'E3' && (await val('F3')) === '' && (await val('G3')) === 'F3')
  })

  await section('INS-C31 向右插入数量 123 显示为 100', async () => {
    await table(20, 10, 'C3')
    await home()
    await kit.getByTestId('sheet-insert-menu').click()
    const box = kit.getByTestId('sheet-insert-cols-right-count')
    await box.fill('')
    await box.fill('123')
    const shown = await box.inputValue()
    await kit.getByTestId('sheet-insert-cols-right').click({ position: { x: 24, y: 10 } })
    const to = `${letters(103)}3`
    await until(async () => (await val(to)) === 'D3')
    step(`INS-C31 显示 100，插入 100 列，原 D3 到 ${to}`, shown === '100' && (await val(to)) === 'D3', shown)
  })

  await section('INS-C32 向右插入 3 列后保存重开', async () => {
    await table(20, 10, 'C3')
    await viaRibbon('insert', 'cols-right', 3)
    await saveAndReopen()
    step('INS-C32 重开后原 D3 在 G3，总列数多 3', (await val('G3')) === 'D3' && (await val('C3')) === 'C3' && (await used())[1] === 'M')
  })

  await section('INS-C33 向右插入数量框拒绝非法输入', async () => {
    await table(20, 10, 'C3')
    const results = await refuses('insert', 'cols-right', 'D3')
    step('INS-C33 均无法输入，列数与内容不变', results.every((r) => r.shown === '' && r.kept) && (await used())[1] === 'J', JSON.stringify(results))
  })

  // ———————————————————— Undo and redo ————————————————————
  await section('INS-C45/C46 四个方向插入 3 行列后撤销、重做', async () => {
    const cases = [
      ['rows-above', 'B8', 'B5'],
      ['rows-below', 'B9', 'B6'],
      ['cols-left', 'E5', 'B5'],
      ['cols-right', 'F5', 'C5'],
    ]
    for (const [item, moved, original] of cases) {
      await table(20, 10, 'B5')
      await viaRibbon('insert', item, 3)
      const after = (await val(moved)) === original
      await undo()
      const back = (await val(original)) === original && same(await used(), [20, 'J'])
      step(`INS-C45 ${item} 插入 3 后撤销：位置还原，无残留空行/列`, after && back)
      await redo()
      step(`INS-C46 ${item} 重做：重新插入 3，位置与撤销前一致`, (await val(moved)) === original)
    }
  })

  // ———————————————————— Import and export ————————————————————
  await section('INS-C49 导入的 xlsx（合并、隐藏列、条件格式）四个方向插入', async () => {
    await table(10, 8, 'A1')
    await ai([
      { command: 'home.mergeCells', params: { range: 'E2:F3' } },
      { command: 'home.hideColumns', params: { cols: 'H:H' } },
      { command: 'home.conditionalFormat', params: { range: 'A1:A5', rule: { type: 'expression', formula: 'TRUE', preset: 'lightRedFill' } } },
    ])
    await saveAndReopen('Imported.xlsx')
    await select('B3')
    await viaRibbon('insert', 'rows-above')
    await viaRibbon('insert', 'cols-left')
    const sheet = await info()
    // The rule on A grows over the column inserted after it (WPS: lines inserted after a rule's range take it).
    step('INS-C49 插入位置与新建表格一致，合并、隐藏列、条件格式随之移动且无丢失', (await val('C4')) === 'B3' && same(sheet.merges, ['F2:G4']) && same(sheet.hiddenColumns, ['I']) && same(sheet.conditionalFormatRanges, ['A1:B6']), JSON.stringify([sheet.merges, sheet.hiddenColumns, sheet.conditionalFormatRanges]))
  })

  await section('INS-C52/C53 插入并填写后导出 xlsx 再打开', async () => {
    await table(10, 6, 'B3')
    await viaRibbon('insert', 'rows-above', 3)
    await ai([{ command: 'range.setValues', params: { range: 'B3', values: [['新1'], ['新2'], ['新3']] } }])
    await select('C3')
    await viaRibbon('insert', 'cols-right', 2)
    await saveAndReopen('Exported.xlsx')
    step('INS-C52 重开后插入的 3 行与填写的内容完整保留', (await val('B3')) === '新1' && (await val('B5')) === '新3' && (await val('B6')) === 'B3')
    step('INS-C53 重开后插入的 2 列保留，行列数与内容位置一致', (await val('F6')) === 'D3' && (await val('D6')) === '' && same(await used(), [13, 'H']))
  })

  // ———————————————————— Errors and limits ————————————————————
  const edge = [
    ['INS-C55 首行 A1 向上插入 1 行', 'A1', 'rows-above', async () => (await val('A2')) === 'A1' && (await val('A1')) === ''],
    ['INS-C56 末行 A10 向上插入 1 行', 'A10', 'rows-above', async () => (await val('A11')) === 'A10' && (await val('A10')) === ''],
    ['INS-C57 首行 A1 向下插入 1 行', 'A1', 'rows-below', async () => (await val('A1')) === 'A1' && (await val('A2')) === '' && (await val('A3')) === 'A2'],
    ['INS-C58 末行 A10 向下插入 1 行', 'A10', 'rows-below', async () => (await val('A10')) === 'A10' && (await val('A11')) === '' && (await details()).selection === 'A11'],
    ['INS-C59 首列 A1 向左插入 1 列', 'A1', 'cols-left', async () => (await val('B1')) === 'A1' && (await val('A1')) === ''],
    ['INS-C60 末列 F1 向左插入 1 列', 'F1', 'cols-left', async () => (await val('G1')) === 'F1' && (await val('F1')) === ''],
    ['INS-C61 首列 A1 向右插入 1 列', 'A1', 'cols-right', async () => (await val('A1')) === 'A1' && (await val('B1')) === '' && (await val('C1')) === 'B1'],
    ['INS-C62 末列 F1 向右插入 1 列', 'F1', 'cols-right', async () => (await val('F1')) === 'F1' && (await val('G1')) === '' && (await details()).selection === 'G1'],
  ]
  for (const [name, at, item, check] of edge) {
    await section(name, async () => {
      await table(10, 6, at)
      await viaRibbon('insert', item)
      step(name, await check())
    })
  }

  // ———————————————————— Interaction with other features ————————————————————
  const rule = (range) => ({ command: 'home.conditionalFormat', params: { range, rule: { type: 'expression', formula: 'TRUE', preset: 'lightRedFill' } } })
  await section('INS-C67 A5 规则 1、B5 规则 2，选中 A 列向右插入 1 列', async () => {
    await table(10, 6, 'A1')
    await ai([rule('A5'), rule('B5')])
    await clickColHeading(0)
    await viaRibbon('insert', 'cols-right')
    const ranges = (await info()).conditionalFormatRanges
    step('INS-C67 A、B 列为规则 1，C 列为规则 2', ranges.includes('A5:B5') && ranges.includes('C5') && ranges.length === 2, JSON.stringify(ranges))
  })

  await section('INS-C68 A1:A200 规则 1，在第 100 行插入 1 行', async () => {
    await table(10, 6, 'A1')
    await ai([rule('A1:A200')])
    await select('100:100')
    await viaRibbon('insert', 'rows-above')
    step('INS-C68 范围变为 A1:A201', same((await info()).conditionalFormatRanges, ['A1:A201']), JSON.stringify((await info()).conditionalFormatRanges))
  })

  await section('INS-C69 在规则所在的行末/列末增加行列', async () => {
    await table(10, 6, 'A5')
    await ai([rule('A5')])
    await viaRibbon('insert', 'rows-below')
    const rows = (await info()).conditionalFormatRanges
    await select('A5')
    await viaRibbon('insert', 'cols-right')
    const cols = (await info()).conditionalFormatRanges
    step('INS-C69 新增的行与列继承条件格式 2', same(rows, ['A5:A6']) && same(cols, ['A5:B6']), JSON.stringify([rows, cols]))
  })

  await section('INS-C70 A1 有图片，四个方向插入不继承图片', async () => {
    await table(10, 6, 'A1')
    await exec('insert.picture', { base64: PNG, at: 'A1', width: 40, height: 16 })
    for (const item of ['rows-above', 'rows-below', 'cols-left', 'cols-right']) {
      await select('A1')
      await viaRibbon('insert', item)
    }
    step('INS-C70 插入成功，图片仍只有 1 张', (await info()).images === 1 && (await val('C3')) === 'A1')
  })

  await section('INS-C71 单元格格式的继承（按 WPS：向上/向左取前一行列，向下/向右取后一行列）', async () => {
    await table(10, 6, 'A1')
    await ai([{ command: 'home.bold', params: { range: 'B3', on: true } }, { command: 'home.bold', params: { range: 'A1', on: true } }])
    const bold = async (ref) => (await cellOf(ref)).style?.font?.bold === true
    await select('B4')
    await viaRibbon('insert', 'rows-above')
    const above = await bold('B4')
    await undo()
    await select('B2')
    await viaRibbon('insert', 'rows-below')
    const below = await bold('B3')
    await undo()
    step('INS-C71 B3 加粗：在 B4 上方插入的新行继承，在 B2 下方插入的新行（取原第 3 行）继承', above && below)
    await select('A1')
    await viaRibbon('insert', 'rows-above')
    const top = !(await bold('A1')) && (await bold('A2'))
    step('INS-C71 A1 上方没有可取的行：新行不带格式，A1 原格式随内容到 A2（WPS）', top)
  })

  await section('INS-C72 A1 有超链接，四个方向插入不继承链接', async () => {
    await table(10, 6, 'A1')
    await exec('insert.link', { cell: 'A1', target: 'https://example.com' })
    let fine = true
    for (const [item, fresh, moved] of [['rows-above', 'A1', 'A2'], ['rows-below', 'A2', 'A1'], ['cols-left', 'A1', 'B1'], ['cols-right', 'B1', 'A1']]) {
      await select('A1')
      await viaRibbon('insert', item)
      fine = fine && (await cellOf(fresh)).hyperlink == null && (await cellOf(moved)).hyperlink != null
      await undo()
    }
    step('INS-C72 新行列没有链接，原链接跟随内容', fine)
  })

  await section('INS-C75 筛选按钮在第 1 行，向上/向下插入', async () => {
    await table(10, 4, 'A1')
    await exec('data.filter', { range: 'A1:D10' })
    await clickRowHeading(0)
    await viaRibbon('insert', 'rows-above')
    const up = (await info()).autofilter
    await undo()
    await clickRowHeading(0)
    await viaRibbon('insert', 'rows-below')
    const down = (await info()).autofilter
    step('INS-C75 向上插入筛选范围下移，向下插入筛选范围扩充', up === 'A2:D11' && down === 'A1:D11', JSON.stringify([up, down]))
  })

  await section('INS-C76 某列有合并单元格，插入 1 列不继承合并', async () => {
    await table(10, 6, 'A1')
    await exec('home.mergeCells', { range: 'B2:B3' })
    await clickColHeading(1)
    await viaRibbon('insert', 'cols-right')
    const right = (await info()).merges
    await undo()
    await clickColHeading(1)
    await viaRibbon('insert', 'cols-left')
    const left = (await info()).merges
    step('INS-C76 插入成功，新列没有合并', same(right, ['B2:B3']) && same(left, ['C2:C3']), JSON.stringify([right, left]))
  })

  const hiddenRow = async () => {
    await table(10, 4, 'A1')
    await exec('home.hideRows', { rows: '5:5' })
  }
  await section('INS-C77 第 5 行隐藏，选中第 4 行向下插入 1 行', async () => {
    await hiddenRow()
    await clickRowHeading(3)
    await viaRibbon('insert', 'rows-below')
    step('INS-C77 第 6 行为隐藏行', same((await info()).hiddenRows, [6]))
  })
  await section('INS-C78 第 5 行隐藏，选中第 6 行向上插入 1 行', async () => {
    await hiddenRow()
    await select('6:6')
    await viaRibbon('insert', 'rows-above')
    step('INS-C78 第 5 行仍为隐藏行', same((await info()).hiddenRows, [5]))
  })
  await section('INS-C79 第 5 行隐藏，选中 4-6 行向上插入 3 行', async () => {
    await hiddenRow()
    await select('4:6')
    await viaRibbon('insert', 'rows-above')
    step('INS-C79 插入 3 行，第 8 行为隐藏行', same((await info()).hiddenRows, [8]) && (await val('A7')) === 'A4')
  })
  await section('INS-C80 第 5 行隐藏，选中 4-6 行向下插入 3 行', async () => {
    await hiddenRow()
    await select('4:6')
    await viaRibbon('insert', 'rows-below')
    step('INS-C80 插入 3 行，第 5 行为隐藏行', same((await info()).hiddenRows, [5]) && (await val('A10')) === 'A7')
  })
  const hiddenCol = async () => {
    await table(10, 6, 'A1')
    await exec('home.hideColumns', { cols: 'C:C' })
  }
  await section('INS-C81 C 列隐藏，选中 B 列向右插入 1 列', async () => {
    await hiddenCol()
    await clickColHeading(1)
    await viaRibbon('insert', 'cols-right')
    step('INS-C81 D 列变为隐藏列', same((await info()).hiddenColumns, ['D']))
  })
  await section('INS-C82 C 列隐藏，选中 D 列向左插入 1 列', async () => {
    await hiddenCol()
    await select('D:D')
    await viaRibbon('insert', 'cols-left')
    step('INS-C82 C 列仍为隐藏列', same((await info()).hiddenColumns, ['C']))
  })
  await section('INS-C83 C 列隐藏，选中 B-D 列向右插入', async () => {
    await hiddenCol()
    await select('B:D')
    await viaRibbon('insert', 'cols-right', 1)
    step('INS-C83 C 列仍为隐藏列', same((await info()).hiddenColumns, ['C']))
  })
  await section('INS-C84 C 列隐藏，选中 B-D 列向左插入（数量默认为选中的 3 列）', async () => {
    await hiddenCol()
    await select('B:D')
    await viaRibbon('insert', 'cols-left')
    step('INS-C84 F 列变为隐藏列', same((await info()).hiddenColumns, ['F']))
  })

  const frozen = async (cell) => {
    await table(10, 6, 'A1')
    await exec('view.freezePanes', { cell })
  }
  await section('INS-C85 冻结第 1 行，选中第 1 行向上插入', async () => {
    await frozen('A2')
    await clickRowHeading(0)
    await viaRibbon('insert', 'rows-above')
    step('INS-C85 冻结至第 2 行', same((await info()).freeze, [2, 0]))
  })
  await section('INS-C86 冻结第 1 行，选中第 1 行向下插入', async () => {
    await frozen('A2')
    await clickRowHeading(0)
    await viaRibbon('insert', 'rows-below')
    step('INS-C86 仍冻结第 1 行', same((await info()).freeze, [1, 0]))
  })
  await section('INS-C87 冻结第 1 列，选中第 1 列向左插入', async () => {
    await frozen('B1')
    await clickColHeading(0)
    await viaRibbon('insert', 'cols-left')
    step('INS-C87 冻结至第 2 列', same((await info()).freeze, [0, 2]))
  })
  await section('INS-C88 冻结第 1 列，选中第 1 列向右插入', async () => {
    await frozen('B1')
    await clickColHeading(0)
    await viaRibbon('insert', 'cols-right')
    step('INS-C88 仍冻结第 1 列', same((await info()).freeze, [0, 1]))
  })

  const validated = async () => {
    await table(10, 4, 'A1')
    await exec('data.validation', { range: 'A2', type: 'whole', operator: 'between', formula1: '8', formula2: '12' })
  }
  await section('INS-C89 A2 有数据验证，选中第 2、3 行向下插入 2 行', async () => {
    await validated()
    await select('2:3')
    await viaRibbon('insert', 'rows-below')
    step('INS-C89 插入的 2 行没有数据验证', same((await info()).validationRanges, ['A2']) && (await val('A6')) === 'A4')
  })
  await section('INS-C90 A2 有数据验证，选中第 1、2 行向上插入 2 行', async () => {
    await validated()
    await select('1:2')
    await viaRibbon('insert', 'rows-above')
    step('INS-C90 插入的 2 行没有数据验证（规则随原 A2 到 A4）', same((await info()).validationRanges, ['A4']))
  })

  await section('INS-C91 选中合并单元格，右键插入的行列数与合并所占行列数一致', async () => {
    await table(10, 6, 'A1')
    await exec('home.mergeCells', { range: 'B2:C4' })
    await rightCell(1, 1)
    await kit.getByTestId('sheet-grid-menu-insert').hover()
    const counts = []
    for (const id of ['rows-above', 'rows-below', 'cols-left', 'cols-right']) counts.push(await kit.getByTestId(`sheet-grid-insert-${id}-count`).inputValue())
    step('INS-C91 行数 3、列数 2', same(counts, ['3', '3', '2', '2']), JSON.stringify(counts))
    await closeMenus()
  })

  // Row and column headings' menus, and the shortcuts (WPS for Mac's heading menus).
  await section('INS-X1 行号、列标的右键菜单', async () => {
    await table(10, 6, 'A1')
    await rightRow(2)
    const rows = await texts(popoverRows('sheet-grid-insert-rows-above'))
    step('INS-X1 行号右键：删除、隐藏、取消隐藏、向上/向下插入行（带数量）、行高…', ['删除', '隐藏', '取消隐藏', '向上插入行', '向下插入行', '行高…'].every((label) => rows.some((row) => row === label || row.startsWith(`${label}Cmd`))) && (await details()).selection === '3:3', JSON.stringify(rows))
    await pick('sheet-grid-insert-rows-below', 2)
    step('INS-X1 行号右键向下插入 2 行', (await val('A6')) === 'A4' && (await val('A4')) === '')
    await rightCol(1)
    const cols = await texts(popoverRows('sheet-grid-insert-cols-left'))
    step('INS-X1 列标右键：删除、隐藏、取消隐藏、向左/向右插入列、列宽…', ['删除', '隐藏', '取消隐藏', '向左插入列', '向右插入列', '列宽…'].every((label) => cols.some((row) => row === label || row.startsWith(`${label}Cmd`))) && (await details()).selection === 'B:B', JSON.stringify(cols))
    await kit.getByTestId('sheet-grid-menu-delete').click()
    await settle()
    step('INS-X1 列标右键删除 B 列', (await val('B1')) === 'C1')
  })

  await section('INS-X2 Ctrl+Shift+= 插入、Ctrl+- 删除；编辑中菜单置灰', async () => {
    await table(10, 6, 'A1')
    await clickRowHeading(2)
    await page.keyboard.press('Control+Shift+Equal')
    await settle()
    const inserted = (await val('A4')) === 'A3' && (await val('A3')) === ''
    await clickColHeading(1)
    await page.keyboard.press('Control+Minus')
    await settle()
    step('INS-X2 选中整行 Ctrl+Shift+= 插入行，选中整列 Ctrl+- 删除列', inserted && (await val('B1')) === 'C1')
    const p = await centre(3, 4)
    await page.mouse.click(p.x, p.y)
    await page.waitForTimeout(150)
    await page.keyboard.type('x')
    await until(async () => (await details()).editing)
    await home()
    await kit.getByTestId('sheet-insert-menu').click()
    const disabled = (await kit.getByTestId('sheet-insert-rows-above').getAttribute('aria-disabled')) === 'true'
    await closeMenus()
    step('INS-X2 单元格编辑中「插入」各项置灰', disabled)
  })

  // ———————————————————— Delete: entry points ————————————————————
  await section('DEL-C4 开始-删除下拉', async () => {
    await table(10, 6, 'C5')
    await openRibbon('delete')
    const rows = await texts(popoverRows('sheet-delete-rows'))
    step('DEL-C4 包含 删除行、删除列、删除单元格-左移、删除单元格-上移', ['删除行', '删除列', '删除单元格-左移', '删除单元格-上移'].every((label) => rows.includes(label)), JSON.stringify(rows))
    await closeMenus()
  })
  await section('DEL-C5 右键菜单「删除」', async () => {
    await table(10, 6, 'C5')
    await rightCell(2, 4)
    await kit.getByTestId('sheet-grid-menu-delete').hover()
    await kit.getByTestId('sheet-grid-delete-rows').waitFor()
    const rows = await texts(popoverRows('sheet-grid-delete-rows'))
    step('DEL-C5 右键「删除」含 删除单元格-左移、删除单元格-上移、删除行、删除列（WPS 的顺序）', same(rows, ['删除单元格-左移', '删除单元格-上移', '删除行', '删除列']), JSON.stringify(rows))
    await closeMenus()
  })

  // ———————————————————— Delete rows ————————————————————
  await section('DEL-C7 选中 B3 删除行（开始-删除与右键）', async () => {
    await table(20, 10, 'B3')
    await viaRibbon('delete', 'rows')
    const ribbon = (await val('B3')) === 'B4' && (await used())[0] === 19
    await undo()
    await viaCellMenu(1, 2, 'delete', 'rows')
    step('DEL-C7 两个入口都删除第 3 行，原 B4 上移到 B3，总行数减 1', ribbon && (await val('B3')) === 'B4' && (await used())[0] === 19)
  })
  await section('DEL-C8 选中 A3:E5 删除行', async () => {
    await table(20, 10)
    await select('A3:E5')
    await viaRibbon('delete', 'rows')
    step('DEL-C8 第 3~5 行删除，原第 6 行上移 3 行（F 列整行一起），第 1~2 行不变，总行数减 3', (await val('A3')) === 'A6' && (await val('F3')) === 'F6' && (await val('F2')) === 'F2' && (await used())[0] === 17)
  })
  await section('DEL-C9 选中 A3:A7 删除行', async () => {
    await table(20, 10)
    await select('A3:A7')
    await viaRibbon('delete', 'rows')
    step('DEL-C9 第 3~7 行一次删除，原第 8 行上移 5 行', (await val('A3')) === 'A8' && (await used())[0] === 15)
  })
  await section('DEL-C10 单击行号 1 删除首行', async () => {
    await table(20, 10)
    await clickRowHeading(0)
    await viaRibbon('delete', 'rows')
    step('DEL-C10 原第 2 行成为第 1 行，总行数减 1', (await val('A1')) === 'A2' && (await used())[0] === 19)
  })
  await section('DEL-C11 B5 连续删除 3 行后保存重开', async () => {
    await table(20, 10, 'B5')
    for (let i = 0; i < 3; i += 1) await viaRibbon('delete', 'rows')
    await saveAndReopen()
    step('DEL-C11 重开后总行数少 3，位置与删除后一致', (await val('B5')) === 'B8' && (await used())[0] === 17)
  })

  // ———————————————————— Delete columns ————————————————————
  await section('DEL-C13 选中 C3 删除列（开始-删除与右键）', async () => {
    await table(20, 10, 'C3')
    await viaRibbon('delete', 'cols')
    const ribbon = (await val('C3')) === 'D3' && (await used())[1] === 'I'
    await undo()
    await viaCellMenu(2, 2, 'delete', 'cols')
    step('DEL-C13 两个入口都删除 C 列，原 D3 左移到 C3，总列数减 1', ribbon && (await val('C3')) === 'D3' && (await used())[1] === 'I')
  })
  await section('DEL-C14 选中 A3:C5 删除列', async () => {
    await table(20, 10)
    await select('A3:C5')
    await viaRibbon('delete', 'cols')
    step('DEL-C14 A~C 列删除，原 D 列左移 3 列，总列数减 3', (await val('A1')) === 'D1' && (await val('A3')) === 'D3' && (await used())[1] === 'G')
  })
  await section('DEL-C15 选中 C~E 三列删除', async () => {
    await table(20, 10)
    await clickColHeading(2)
    await clickColHeading(4, true)
    await viaRibbon('delete', 'cols')
    step('DEL-C15 C~E 一次删除，原 F 列左移 3 列', (await val('C1')) === 'F1' && (await used())[1] === 'G')
  })
  await section('DEL-C16 单击 A 列列标删除首列', async () => {
    await table(20, 10)
    await clickColHeading(0)
    await viaRibbon('delete', 'cols')
    step('DEL-C16 原 B 列成为 A 列，总列数减 1', (await val('A1')) === 'B1' && (await used())[1] === 'I')
  })
  await section('DEL-C17 C3 连续删除 3 列后保存重开', async () => {
    await table(20, 10, 'C3')
    for (let i = 0; i < 3; i += 1) await viaRibbon('delete', 'cols')
    await saveAndReopen()
    step('DEL-C17 重开后总列数少 3，位置与删除后一致', (await val('C3')) === 'F3' && (await used())[1] === 'G')
  })

  // ———————————————————— Undo and redo ————————————————————
  await section('DEL-C19/C20 删除第 3 行与 C 列后撤销、重做', async () => {
    await table(20, 10)
    await clickRowHeading(2)
    await viaRibbon('delete', 'rows')
    await undo()
    const rowBack = (await val('A3')) === 'A3' && (await used())[0] === 20
    await redo()
    const rowAgain = (await val('A3')) === 'A4'
    await undo()
    await clickColHeading(2)
    await viaRibbon('delete', 'cols')
    await undo()
    const colBack = (await val('C1')) === 'C1' && (await used())[1] === 'J'
    await redo()
    step('DEL-C19 两次撤销都生效：第 3 行、C 列回到原位，行列数恢复', rowBack && colBack)
    step('DEL-C20 两个维度的重做都生效', rowAgain && (await val('C1')) === 'D1')
  })
  await section('DEL-C21 删除 3 行、3 列后各自撤销', async () => {
    await table(20, 10)
    await select('A3:A5')
    await viaRibbon('delete', 'rows')
    await undo()
    const rows = (await val('A3')) === 'A3' && (await used())[0] === 20
    await select('C:E')
    await viaRibbon('delete', 'cols')
    await undo()
    step('DEL-C21 第 3~5 行、C~E 列都回到原位', rows && (await val('E1')) === 'E1' && same(await used(), [20, 'J']))
  })
  await section('DEL-C22 删除第 3 行、C 列后连续撤销 2 次', async () => {
    await table(20, 10)
    await clickRowHeading(2)
    await viaRibbon('delete', 'rows')
    await clickColHeading(2)
    await viaRibbon('delete', 'cols')
    await undo()
    const first = (await val('C3')) === 'C4'
    await undo()
    step('DEL-C22 先回退删除列，再回退删除行，最终回到初始状态', first && (await val('C3')) === 'C3' && same(await used(), [20, 'J']))
  })
  await section('DEL-C23 删除后连续撤销、重做各 5 次', async () => {
    await table(20, 10)
    await select('A3:A5')
    await viaRibbon('delete', 'rows')
    let fine = true
    for (let i = 0; i < 5; i += 1) {
      await undo()
      fine = fine && (await val('A3')) === 'A3'
      await redo()
      fine = fine && (await val('A3')) === 'A6'
    }
    await select('C:E')
    await viaRibbon('delete', 'cols')
    for (let i = 0; i < 5; i += 1) {
      await undo()
      fine = fine && (await val('C1')) === 'C1'
      await redo()
      fine = fine && (await val('C1')) === 'F1'
    }
    step('DEL-C23 每次撤销回到删除前、每次重做回到删除后', fine && same(await used(), [17, 'G']))
  })

  // ———————————————————— Save, reopen and export ————————————————————
  await section('DEL-C25 导入的 xlsx（合并、隐藏列、条件格式）删除行', async () => {
    await table(10, 8, 'A1')
    await ai([
      { command: 'home.mergeCells', params: { range: 'E5:F6' } },
      { command: 'home.hideColumns', params: { cols: 'H:H' } },
      { command: 'home.conditionalFormat', params: { range: 'A1:A5', rule: { type: 'expression', formula: 'TRUE', preset: 'lightRedFill' } } },
    ])
    await saveAndReopen('ImportedDelete.xlsx')
    await select('B2')
    await viaRibbon('delete', 'rows')
    const sheet = await info()
    step('DEL-C25 删除位置与新建一致，合并、隐藏列、条件格式无丢失', (await val('B2')) === 'B3' && same(sheet.merges, ['E4:F5']) && same(sheet.hiddenColumns, ['H']) && same(sheet.conditionalFormatRanges, ['A1:A4']), JSON.stringify([sheet.merges, sheet.hiddenColumns, sheet.conditionalFormatRanges]))
  })
  await section('DEL-C27/C28 删除后导出 xlsx 再打开', async () => {
    await table(20, 10)
    await select('A3:A5')
    await viaRibbon('delete', 'rows')
    await select('C:D')
    await viaRibbon('delete', 'cols')
    await saveAndReopen('DeleteExport.xlsx')
    step('DEL-C27 重开后行结构与编辑区一致，删除的 3 行未重新出现', (await val('A3')) === 'A6' && (await used())[0] === 17)
    step('DEL-C28 重开后总列数与内容位置与导出前一致', (await val('C1')) === 'E1' && (await used())[1] === 'H')
  })

  // ———————————————————— Alongside the other sheet features ————————————————————
  await section('DEL-C30 全选后删除行、删除列置灰', async () => {
    await table(10, 6)
    const box = await region()
    await page.mouse.click(box.x + 10, box.y + 8)
    await settle()
    await openRibbon('delete')
    const rowsOff = await kit.getByTestId('sheet-delete-rows').isDisabled()
    const colsOff = await kit.getByTestId('sheet-delete-cols').isDisabled()
    await kit.getByTestId('sheet-delete-rows').click({ force: true }).catch(() => undefined)
    await closeMenus()
    step('DEL-C30 两项置灰，点击无删除，行列数不变', rowsOff && colsOff && same(await used(), [10, 'F']) && (await details()).selection === 'A1:XFD1048576', (await details()).selection)
  })
  await section('DEL-C31 第 2 行隐藏，选中 1~3 行删除', async () => {
    await table(10, 4)
    await exec('home.hideRows', { rows: '2:2' })
    await select('1:3')
    await viaRibbon('delete', 'rows')
    step('DEL-C31 第 1~3 行（含隐藏的第 2 行）删除，原第 4 行到第 1 行', (await val('A1')) === 'A4' && same((await info()).hiddenRows, []) && (await used())[0] === 7)
  })
  await section('DEL-C32 第 3、5 行隐藏，删除第 4 行后取消隐藏', async () => {
    await table(10, 4)
    await exec('home.hideRows', { rows: '3:3' })
    await exec('home.hideRows', { rows: '5:5' })
    await select('4:4')
    await viaRibbon('delete', 'rows')
    const hidden = (await info()).hiddenRows
    await exec('home.unhideRows', { rows: '1:10' })
    step('DEL-C32 隐藏标记仍对应原第 3、5 行（现第 3、4 行），取消隐藏后位置正确', same(hidden, [3, 4]) && (await val('A3')) === 'A3' && (await val('A4')) === 'A5' && (await val('A5')) === 'A6', JSON.stringify(hidden))
  })
  await section('DEL-C33 C 列隐藏，删除 D 列', async () => {
    await table(10, 6)
    await exec('home.hideColumns', { cols: 'C:C' })
    await select('D:D')
    await viaRibbon('delete', 'cols')
    step('DEL-C33 C 列仍为隐藏', same((await info()).hiddenColumns, ['C']) && (await val('D1')) === 'E1')
  })
  await section('DEL-C34 筛选后删除筛选结果的行（按 WPS：只删可见行，筛选保留）', async () => {
    await build(numbered(2), { fill: false })
    await ai([{ command: 'range.setValues', params: { range: `${q('工作表1')}!A1`, values: [['类'], ['a'], ['b'], ['a'], ['b'], ['a'], ['b'], ['a'], ['b'], ['a']] } }])
    await exec('data.filter', { range: 'A1:A10' })
    await exec('data.filterBy', { column: 'A', values: ['a'] })
    await select('2:10')
    await viaRibbon('delete', 'rows')
    const sheet = await info()
    const left = [await val('A1'), await val('A2'), await val('A5')]
    step('DEL-C34 筛选出的行一次删除，被筛掉的行保留（仍隐藏），筛选仍在', same(left, ['类', 'b', 'b']) && same(sheet.filteredColumns, [0]) && sheet.hiddenRows.length === 4, JSON.stringify([left, sheet.filteredColumns, sheet.hiddenRows]))
  })
  await section('DEL-C35 冻结第 1 行和 A 列，删除冻结内的行与列', async () => {
    await table(10, 6)
    await exec('view.freezePanes', { cell: 'B2' })
    await clickRowHeading(0)
    await viaRibbon('delete', 'rows')
    const afterRow = (await info()).freeze
    await clickColHeading(0)
    await viaRibbon('delete', 'cols')
    step('DEL-C35 冻结收缩到新的第 1 行和 A 列，冻结不丢失', same(afterRow, [1, 1]) && same((await info()).freeze, [1, 1]) && (await val('A1')) === 'B2')
  })
  await section('DEL-C36 A1:B2 合并，删除第 1 行、撤销后删除 A 列', async () => {
    await table(10, 6)
    await exec('home.mergeCells', { range: 'A1:B2' })
    await clickRowHeading(0)
    await viaRibbon('delete', 'rows')
    const rows = (await info()).merges
    await undo()
    await clickColHeading(0)
    await viaRibbon('delete', 'cols')
    const cols = (await info()).merges
    step('DEL-C36 合并区域随删除收缩，关系不丢失、不错位', same(rows, ['A1:B1']) && same(cols, ['A1:A2']), JSON.stringify([rows, cols]))
  })
  await section('DEL-C37 B1 = SUM(B2:B10)，删除第 5 行', async () => {
    await build(numbered(2), { fill: false })
    await ai([{ command: 'range.setValues', params: { range: `${q('工作表1')}!B2`, values: [[1], [2], [3], [4], [5], [6], [7], [8], [9]] } }, { command: 'cell.set', params: { cell: `${q('工作表1')}!B1`, input: '=SUM(B2:B10)' } }])
    await clickRowHeading(4)
    await viaRibbon('delete', 'rows')
    const cell = await cellOf('B1')
    step('DEL-C37 引用收缩为 B2:B9，结果随数据更新', cell.formula === '=SUM(B2:B9)' && cell.value === 41, JSON.stringify([cell.formula, cell.value]))
  })
  await section('DEL-C38 图表引用 A1:B5，删除第 3 行', async () => {
    await build(numbered(2), { fill: false })
    await ai([{ command: 'range.setValues', params: { range: `${q('工作表1')}!A1`, values: [['名', '值'], ['a', 1], ['b', 2], ['c', 3], ['d', 4]] } }])
    await exec('insert.chart', { range: 'A1:B5', type: 'column', at: 'D2' })
    await clickRowHeading(2)
    await viaRibbon('delete', 'rows')
    const sheet = await info()
    const values = sheet.charts[0]?.values ?? []
    step('DEL-C38 图表仍在，引用收缩 1 行', sheet.charts.length === 1 && values.length === 1 && values[0].includes('$B$2:$B$4'), JSON.stringify(values))
  })
  await section('DEL-C39 A 列整数验证（8~12），删除 B 列', async () => {
    await table(10, 4)
    await exec('data.validation', { range: 'A:A', type: 'whole', operator: 'between', formula1: '8', formula2: '12' })
    await clickColHeading(1)
    await viaRibbon('delete', 'cols')
    const check = await exec('data.validate', { cell: 'A3', input: '20' })
    step('DEL-C39 规则仍作用于 A 列并生效', ['A:A', 'A1:A1048576'].includes((await info()).validationRanges.join()) && check.ok === false, JSON.stringify((await info()).validationRanges))
  })
  await section('DEL-C40 A1:A100 条件格式公式规则，删除 D 列', async () => {
    await table(10, 6)
    await ai([{ command: 'home.conditionalFormat', params: { range: 'A1:A100', rule: { type: 'expression', formula: '$A1<>""', preset: 'lightRedFill' } } }])
    await clickColHeading(3)
    await viaRibbon('delete', 'cols')
    step('DEL-C40 规则不丢失，作用范围仍为 A1:A100', same((await info()).conditionalFormatRanges, ['A1:A100']))
  })

  // ———————————————————— Errors and limits ————————————————————
  await section('DEL-C42/C45 删除后关闭重开（桌面版无刷新）', async () => {
    await table(20, 10)
    await select('A3:A5')
    await viaRibbon('delete', 'rows')
    await select('C:C')
    await viaRibbon('delete', 'cols')
    await saveAndReopen('Refresh.xlsx')
    step('DEL-C42 重开后行数与内容位置不变，无回滚', (await val('A3')) === 'A6' && (await used())[0] === 17)
    step('DEL-C45 重开后列数与内容位置不变', (await val('C1')) === 'D1' && (await used())[1] === 'I')
  })
  await section('DEL-C43 选中 B3 连续快速点击删除行 10 次', async () => {
    await table(20, 10, 'B3')
    for (let i = 0; i < 10; i += 1) {
      await home()
      await kit.getByTestId('sheet-delete-menu').click()
      await kit.getByTestId('sheet-delete-rows').click()
    }
    await until(async () => (await val('B3')) === 'B13')
    step('DEL-C43 累计删除 10 行，无重删漏删', (await val('B3')) === 'B13' && (await val('B2')) === 'B2' && (await used())[0] === 10)
  })
  await section('DEL-C44 500 行数据中删除前 200 行', async () => {
    await build(numbered(2), { fill: false })
    await ai([{ command: 'range.setValues', params: { range: `${q('工作表1')}!A1`, values: Array.from({ length: 500 }, (_, i) => [`A${i + 1}`, i + 1]) } }])
    await select('1:200')
    const started = Date.now()
    await viaRibbon('delete', 'rows')
    await until(async () => (await val('A1')) === 'A201')
    const took = Date.now() - started
    step('DEL-C44 一次删除 200 行，剩余 300 行上移，不卡顿', (await val('A1')) === 'A201' && (await val('A300')) === 'A500' && (await used())[0] === 300 && took < 5000, `${took} ms`)
  })
  await section('DEL-C46 删除 C 列、撤销、再删除 D 列', async () => {
    await table(20, 10)
    await clickColHeading(2)
    await viaRibbon('delete', 'cols')
    await page.keyboard.press('ControlOrMeta+z')
    await settle()
    const back = (await val('C1')) === 'C1'
    await clickColHeading(3)
    await viaRibbon('delete', 'cols')
    step('DEL-C46 撤销后 C 列还原，再删 D 列，最终少 1 列', back && (await val('C1')) === 'C1' && (await val('D1')) === 'E1' && (await used())[1] === 'I')
  })

  // ———————————————————— Mixed scenarios ————————————————————
  await section('DEL-C48/C49 删除 2 行、3 列后撤销 2 次', async () => {
    await table(20, 10)
    await select('3:4')
    await viaRibbon('delete', 'rows')
    await select('B:D')
    await viaRibbon('delete', 'cols')
    step('DEL-C48 总行数减 2、总列数减 3，位置符合语义', (await val('B3')) === 'E5' && same(await used(), [18, 'G']))
    await undo()
    await undo()
    step('DEL-C49 两次撤销回到初始', (await val('B3')) === 'B3' && same(await used(), [20, 'J']))
  })
  await section('DEL-C50 开始-删除删 2 行，右键删 1 列', async () => {
    await table(20, 10)
    await select('A3:A4')
    await viaRibbon('delete', 'rows')
    await rightCol(1)
    await kit.getByTestId('sheet-grid-menu-delete').click()
    await settle()
    step('DEL-C50 总行数减 2、总列数减 1', (await val('B3')) === 'C5' && same(await used(), [18, 'I']))
  })
  await section('DEL-C51 先删第 3 行再删第 5 行，与先删第 5 行再删第 3 行', async () => {
    await table(20, 10)
    await clickRowHeading(2)
    await viaRibbon('delete', 'rows')
    await clickRowHeading(4)
    await viaRibbon('delete', 'rows')
    const pathA = [await val('A3'), await val('A4'), await val('A5')]
    await undo()
    await undo()
    await clickRowHeading(4)
    await viaRibbon('delete', 'rows')
    await clickRowHeading(2)
    await viaRibbon('delete', 'rows')
    const pathB = [await val('A3'), await val('A4'), await val('A5')]
    step('DEL-C51 路径 A 删原第 3、6 行；路径 B 删原第 5、3 行', same(pathA, ['A4', 'A5', 'A7']) && same(pathB, ['A4', 'A6', 'A7']), JSON.stringify([pathA, pathB]))
  })
  await section('DEL-C52 先隐藏 C 列再删 B 列，与先删 B 列再隐藏原 C 列', async () => {
    await table(10, 6)
    await exec('home.hideColumns', { cols: 'C:C' })
    await clickColHeading(1)
    await viaRibbon('delete', 'cols')
    const pathA = (await info()).hiddenColumns
    const shownA = await val('B1')
    await table(10, 6)
    await clickColHeading(1)
    await viaRibbon('delete', 'cols')
    await exec('home.hideColumns', { cols: 'B:B' })
    const pathB = (await info()).hiddenColumns
    step('DEL-C52 两种顺序都是原 C 列（现 B 列）隐藏，不错位', same(pathA, ['B']) && same(pathB, ['B']) && shownA === 'C1', JSON.stringify([pathA, pathB]))
  })
} catch (error) {
  await h.finish(error)
}
await h.finish()
