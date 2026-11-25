// Inserting content (module M8, Insert, its generic part), as WPS for Mac and Excel have it, in a
// real browser through the development host: node tests/gate-insert.mjs [chromium|webkit]
// Insert › Symbol (the panel of recent and common symbols, More Symbols… opening the Symbol dialog with its
// Symbols and Special Characters tabs, a search by name, the font and the subset; a symbol going in at the caret
// of what is typed, after the cell's text with the cell open, into a text box, a merged area;
// undo, save and reopen) and Insert › Checkbox (Excel's checkbox cells: a click on the box or Space
// ticks it, a click beside it selects, typing, a double-click and the formula bar leave it; fonts,
// fills, alignment and Text format; clear, copy, cut, paste special, the format painter, merge
// and unmerge, sort, filter, remove duplicates, text to columns, find and replace, inserted rows
// and columns, array formulas, save and reopen, a viewer); the cell menu's new rows (Clear,
// Sort, Merge, Paste Special, Insert Comment, Hyperlink); and the cases M6 and M7 left here
// (fonts in a text box, a picture cell's format, inserting beside a table, a picture in a
// table's header row). Each step names its case (INSC-C96 for inserting content, FONT-C111 for fonts,
// TS-C76 for table styles); `GATE_ONLY=INSC-C96` runs some.
import { startGate } from './gate-harness.mjs'

const h = await startGate({ prefix: '' })
/** The grid's pitch as the engine lays it out (the harness reads it when a workbook opens). */
const G = h.grid
const { page, kit, step, section, until, same, ai, exec, details, cellOf, region, clickCell, build, numbered, saveAndReopen, touch, zipPart, formulaBar, openEditor, errors } = h

const settle = (ms = 250) => page.waitForTimeout(ms)
const COLS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'
const colIndex = (ref) => COLS.indexOf(ref.replace(/\d+$/, ''))
const rowIndex = (ref) => Number(ref.replace(/^[A-Z]+/, '')) - 1
const mod = process.platform === 'darwin' ? 'Meta' : 'Control'
const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='

// ———— the grid (100%, the sheet at its top-left, rows 20 px) ————
/** A cell's centre on the page (a checkbox's box is there). */
const centre = async (ref) => {
  const box = await region()
  return { x: box.x + 46 + colIndex(ref) * G.cw + G.cw / 2, y: box.y + 21 + rowIndex(ref) * G.rh + G.rh / 2 }
}
/** Clicks a cell's centre (on a checkbox: its box). */
const click = (ref) => clickCell(colIndex(ref), rowIndex(ref))
/** Selects a cell with a click near its left edge (beside a checkbox's box). */
const select = async (ref) => {
  const box = await region()
  await page.mouse.click(box.x + 46 + colIndex(ref) * G.cw + 6, box.y + 21 + rowIndex(ref) * G.rh + 10)
  await settle(150)
}
const right = async (ref) => {
  const box = await region()
  await page.mouse.click(box.x + 46 + colIndex(ref) * G.cw + 6, box.y + 21 + rowIndex(ref) * G.rh + 10, { button: 'right' })
  await kit.getByTestId('sheet-grid-menu').waitFor()
}
/** The cell menu › a submenu › an item. */
const cellMenu = async (ref, sub, item) => {
  await right(ref)
  if (sub) await kit.getByTestId(`sheet-grid-menu-${sub}`).hover()
  await kit.getByTestId(item).click()
  await settle(300)
}
const type = async (text) => {
  await page.keyboard.type(text)
  await page.keyboard.press('Enter')
  await settle(200)
}
const val = async (ref) => (await cellOf(ref)).value
const styleOf = async (ref) => (await cellOf(ref)).style ?? {}
/** Whether a cell holds a checkbox (its style has one and it holds TRUE or FALSE). */
const isBox = async (ref) => {
  const c = await cellOf(ref)
  return c.style?.checkbox === true && typeof c.value === 'boolean'
}
const dark = (p) => p[0] < 140 && p[1] < 140 && p[2] < 140
const white = (p) => p[0] > 235 && p[1] > 235 && p[2] > 235
/** What the grid shows of a checkbox centred at canvas point (`cx`, `cy`): checked, empty, or none. */
const boxAt = async (cx, cy) => {
  const inner = await h.pixel(cx - 3, cy - 3)
  const edge = await h.pixel(cx - 6, cy)
  if (dark(inner) && dark(edge)) return 'checked'
  const grey = edge[0] > 130 && edge[0] < 205 && Math.abs(edge[0] - edge[2]) < 16
  if (grey && white(inner)) return 'empty'
  return 'none'
}
/** A cell's middle in whole pixels, as the engine centres a checkbox's box (rounded down). */
const mid = (size) => Math.floor(size / 2)
const shown = (ref) => boxAt(46 + colIndex(ref) * G.cw + mid(G.cw), 21 + rowIndex(ref) * G.rh + mid(G.rh))
const noticeShown = (text) => until(async () => (await kit.locator('.kui-toast').allTextContents()).some((t) => t.includes(text)), 30)
const dismiss = async () => {
  for (const close of await kit.locator('.kui-toast-close').all()) await close.click().catch(() => undefined)
}
const editing = async () => (await details()).editing === true

/** A fresh workbook: 工作表1–3 (zh-CN Sheet1–3), empty, rows 20 px, 100%, its top-left in view, A1 selected. */
async function fresh(names = numbered(3)) {
  await build(names, { fill: false })
  await ai([
    { command: 'row.setHeight', params: { rows: '1:40', height: G.rh } },
    { command: 'view.zoom', params: { percent: 100 } },
  ])
  const box = await region()
  await page.mouse.move(box.x + 300, box.y + 200)
  for (let i = 0; i < 3; i += 1) await page.mouse.wheel(-2000, -2000)
  await settle(200)
  await dismiss()
  await select('A1')
}

// ———— Insert › Symbol ————
const insertTab = async () => {
  await kit.getByTestId('ribbon-tab-insert').click()
  await kit.getByTestId('sheet-symbol').waitFor()
}
const panel = () => kit.getByTestId('sheet-symbol-panel')
const openSymbols = async () => {
  await insertTab()
  if ((await panel().count()) === 0) await kit.getByTestId('sheet-symbol').click()
  await panel().waitFor()
}
const symbolsOf = (testId) => kit.getByTestId(testId).locator('[data-symbol]').evaluateAll((cells) => cells.map((c) => c.getAttribute('data-symbol')))
/** Opens the panel and clicks a common symbol. */
const pick = async (symbol) => {
  await openSymbols()
  await kit.locator(`[data-testid="sheet-symbol-item"][data-symbol="${symbol}"]`).click()
  await settle(200)
}
const forgetSymbols = () => kit.locator('body').evaluate(() => window.localStorage.removeItem('kit:sheet:recentSymbols'))
const dialog = () => kit.getByTestId('sheet-symbol-dialog')
const openDialog = async () => {
  await openSymbols()
  await kit.getByTestId('sheet-symbol-more').click()
  await dialog().waitFor()
  await until(async () => (await kit.getByTestId('sheet-symbol-grid').locator('[data-symbol]').count()) > 0)
}
const gridSymbols = () => symbolsOf('sheet-symbol-grid')
const closeDialog = async () => {
  if ((await dialog().count()) > 0) await kit.getByTestId('sheet-symbol-close').click()
  await settle(200)
}

// ———— Insert › Checkbox ————
const insertCheckbox = async () => {
  await insertTab()
  await kit.getByTestId('sheet-cmd-checkbox.insert').click()
  await settle(300)
}
/** Checkboxes in `range`, ticked as `values` say (row by row), by command. */
const boxes = async (range, values = []) => {
  const steps = [{ command: 'checkbox.insert', params: { range } }]
  for (const [ref, on] of values) steps.push({ command: 'cell.set', params: { cell: ref, input: on ? 'TRUE' : 'FALSE' } })
  await ai(steps)
  await settle(200)
}

try {
  await openEditor('')

  // ———————————————— The Insert menu (WPS desktop: the Insert tab) ————————————————

  await section('INSC-C9 再次单击收起', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '温度' } }])
    await select('A1')
    await openSymbols()
    await kit.getByTestId('sheet-symbol').click()
    await settle(200)
    step('INSC-C9 再次单击「符号」，面板收起，面板中的项不再呈现', (await panel().count()) === 0 && (await kit.getByTestId('sheet-symbol-item').count()) === 0)
    step('INSC-C9 单元格内容与选区不变', (await val('A1')) === '温度' && (await details()).selection === 'A1')
  })

  await section('INSC-C10/C94 单击面板外收起', async () => {
    await fresh()
    await openSymbols()
    await click('B2')
    await settle(200)
    step('INSC-C10/C94 单击 B2，面板收起', (await panel().count()) === 0)
    step('INSC-C10/C94 B2 呈选中状态，内容不变', (await details()).selection === 'B2' && (await val('B2')) == null)
  })

  await section('INSC-C11 悬浮样式', async () => {
    await fresh()
    await openSymbols()
    const item = kit.locator('[data-testid="sheet-symbol-item"][data-symbol="α"]')
    const other = kit.locator('[data-testid="sheet-symbol-item"][data-symbol="β"]')
    const before = await item.evaluate((e) => getComputedStyle(e).backgroundColor)
    await item.hover()
    await settle(150)
    const after = await item.evaluate((e) => getComputedStyle(e).backgroundColor)
    const rest = await other.evaluate((e) => getComputedStyle(e).backgroundColor)
    step('INSC-C11 鼠标所在项底色变化、面板不收起，其余项样式不变', after !== before && rest === before && (await panel().count()) === 1, `${before} → ${after}; ${rest}`)
    await page.keyboard.press('Escape')
  })

  // ———————————————— Symbols ————————————————

  await section('INSC-C89/C90/C91/C92 符号面板', async () => {
    await fresh()
    await forgetSymbols()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '温度' } }])
    await openSymbols()
    const p = await panel().boundingBox()
    const button = await kit.getByTestId('sheet-symbol').boundingBox()
    step('INSC-C89 单击「符号」，在按钮旁弹出符号面板，A1 内容不变', p != null && button != null && p.y >= button.y + button.height - 2 && Math.abs(p.x - button.x) < 400 && (await val('A1')) === '温度')
    const text = await panel().textContent()
    const headings = await panel().locator('.sheet-symbol-heading').allTextContents()
    step('INSC-C90 自上而下：最近使用符号、常用符号、底部「更多符号…」，共 2 个分组标题', same(headings, ['最近使用符号', '常用符号']) && text.indexOf('常用符号') < text.indexOf('更多符号'), JSON.stringify(headings))
    const common = await symbolsOf('sheet-symbol-common')
    step('INSC-C91 常用符号 5 行 8 列 40 个，第 1 行 ＋−×÷±≡≠≈，第 2 行 ≤≥‰¼½αβθ', common.length === 40 && same(common.slice(0, 8), ['＋', '−', '×', '÷', '±', '≡', '≠', '≈']) && same(common.slice(8, 16), ['≤', '≥', '‰', '¼', '½', 'α', 'β', 'θ']), common.slice(0, 16).join(''))
    const recentBox = await kit.getByTestId('sheet-symbol-recent').boundingBox()
    step('INSC-C92 最近使用符号初始为空，占位保持（常用符号不上移）', (await symbolsOf('sheet-symbol-recent')).length === 0 && recentBox != null && recentBox.height >= 28)
    await page.keyboard.press('Escape')
  })

  await section('INSC-C95 反复打开关闭', async () => {
    await fresh()
    await openSymbols()
    const first = JSON.stringify([await symbolsOf('sheet-symbol-recent'), await symbolsOf('sheet-symbol-common')])
    await click('B2')
    await openSymbols()
    const again = JSON.stringify([await symbolsOf('sheet-symbol-recent'), await symbolsOf('sheet-symbol-common')])
    step('INSC-C95 再次打开构成与取值不变（最近使用、常用 5×8、更多符号）', first === again && (await kit.getByTestId('sheet-symbol-more').count()) === 1)
    await page.keyboard.press('Escape')
  })

  await section('INSC-C96/C97 点选符号插入', async () => {
    await fresh()
    await forgetSymbols()
    await pick('＋')
    step('INSC-C96 面板自动收起，A1 呈「＋」（WPS：单元格进入编辑，光标在末尾）', (await panel().count()) === 0 && (await details()).editText === '＋')
    await page.keyboard.press('Enter')
    await settle(200)
    step('INSC-C96 回车后 A1 的值为「＋」', (await val('A1')) === '＋')
    await openSymbols()
    step('INSC-C97 最近使用符号呈「＋」，常用符号中的位置不变', same(await symbolsOf('sheet-symbol-recent'), ['＋']) && (await symbolsOf('sheet-symbol-common'))[0] === '＋')
    await page.keyboard.press('Escape')
  })

  await section('INSC-C98 最近使用的次序', async () => {
    await fresh()
    await forgetSymbols()
    for (const symbol of ['≤', '≠', '℃']) await pick(symbol)
    await page.keyboard.press('Escape')
    await openSymbols()
    const recent = await symbolsOf('sheet-symbol-recent')
    // WPS for Mac keeps more than 8 (two rows and more); we keep 16.
    step('INSC-C98 最近使用自左向右 ℃ ≠ ≤，至多 16 个（WPS 桌面多于 8 个）', same(recent.slice(0, 3), ['℃', '≠', '≤']) && recent.length <= 16, recent.join(''))
    for (const symbol of ['＋', '−', '×', '÷', '±', '≡', '≈', '‰', '¼', '½', 'α', 'β', 'θ', '°', '√', '∞']) await pick(symbol)
    await page.keyboard.press('Escape')
    await openSymbols()
    step('INSC-C98 插入 19 个不同符号后最近使用仍为 16 个，最新的在最前', (await symbolsOf('sheet-symbol-recent')).length === 16 && (await symbolsOf('sheet-symbol-recent'))[0] === '∞')
    await page.keyboard.press('Escape')
  })

  await section('INSC-C99 与既有文字共存', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '温度' } }])
    await select('A1')
    await pick('℃')
    step('INSC-C99 A1 呈「温度℃」，原文字仍在，符号追加在后', (await details()).editText === '温度℃')
    await page.keyboard.press('Enter')
    await settle(200)
    step('INSC-C99 回车后 A1 的值为「温度℃」', (await val('A1')) === '温度℃')
  })

  await section('INSC-C100 重复插入不重复登记', async () => {
    await fresh()
    await forgetSymbols()
    await pick('＋')
    await pick('＋')
    await page.keyboard.press('Enter')
    await openSymbols()
    step('INSC-C100 最近使用只呈 1 个「＋」，A1 呈两个「＋」', same(await symbolsOf('sheet-symbol-recent'), ['＋']) && (await val('A1')) === '＋＋')
    await page.keyboard.press('Escape')
  })

  await section('INSC-C101/C102/C103/C104/C105 更多符号', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '温度' } }])
    await select('A1')
    await openDialog()
    step('INSC-C101 点「更多符号…」打开「符号」对话框（WPS 桌面为对话框），面板收起，A1 不变', (await dialog().textContent()).includes('符号') && (await panel().count()) === 0 && (await val('A1')) === '温度')
    const parts = await Promise.all(['sheet-symbol-tab-symbols', 'sheet-symbol-tab-special', 'sheet-symbol-search', 'sheet-symbol-font', 'sheet-symbol-subset', 'sheet-symbol-grid'].map((id) => kit.getByTestId(id).count()))
    step('INSC-C102 「符号」「特殊字符」两个页签、搜索框、字体与子集下拉、符号网格', parts.every((n) => n === 1), JSON.stringify(parts))
    step('INSC-C102 左下角「来自：Unicode（十六进制）」，右上角「×」（「?」帮助入口未做）', (await kit.getByTestId('sheet-symbol-from').textContent()).includes('来自：Unicode（十六进制）') && (await dialog().locator('.kui-modal-close').count()) === 1)
    step('INSC-C103 搜索框提示「输入符号名称，回车搜索」，框内无其它文字', (await kit.getByTestId('sheet-symbol-search').getAttribute('placeholder')) === '输入符号名称，回车搜索' && (await kit.getByTestId('sheet-symbol-search').inputValue()) === '')
    await kit.getByTestId('sheet-symbol-tab-special').click()
    await settle(150)
    const rows = await kit.getByTestId('sheet-symbol-special').locator('[data-symbol]').allTextContents()
    step('INSC-C104 切到「特殊字符」，「符号」页签不再选中，呈带名称的特殊字符清单，搜索与字体、子集仍在', (await kit.getByTestId('sheet-symbol-tab-special').getAttribute('aria-selected')) === 'true' && (await kit.getByTestId('sheet-symbol-tab-symbols').getAttribute('aria-selected')) === 'false' && rows.some((r) => r.includes('版权所有')) && (await kit.getByTestId('sheet-symbol-search').count()) === 1 && (await kit.getByTestId('sheet-symbol-font').count()) === 1, rows.slice(0, 3).join('|'))
    await dialog().locator('.kui-modal-close').click()
    await settle(200)
    step('INSC-C105 点「×」关闭，对话框中的控件不再呈现，A1 内容与选区不变', (await dialog().count()) === 0 && (await val('A1')) === '温度' && (await details()).selection === 'A1')
  })

  await section('INSC-C106/C165 按名称检索', async () => {
    await fresh()
    await openDialog()
    const all = (await gridSymbols()).length
    await kit.getByTestId('sheet-symbol-search').fill('plus')
    await kit.getByTestId('sheet-symbol-search').press('Enter')
    await settle(200)
    const found = await kit.getByTestId('sheet-symbol-grid').locator('[data-symbol]').evaluateAll((cells) => cells.map((c) => c.getAttribute('title')))
    step('INSC-C106 输入 plus 回车，网格只呈名称匹配的符号，数量少于检索前', found.length > 0 && found.length < all && found.every((name) => /PLUS/.test(name)) && found.length < 200, `${found.length} of ${all}`)
    await kit.getByTestId('sheet-symbol-search').fill('zzzzzz')
    await kit.getByTestId('sheet-symbol-search').press('Enter')
    await settle(200)
    step('INSC-C165 无匹配时网格不呈任何符号，对话框保持，搜索框仍为 zzzzzz', (await gridSymbols()).length === 0 && (await dialog().count()) === 1 && (await kit.getByTestId('sheet-symbol-search').inputValue()) === 'zzzzzz')
    await closeDialog()
  })

  await section('INSC-C107 编辑中插入', async () => {
    await fresh()
    await forgetSymbols()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '温度' } }])
    await select('A1')
    await page.keyboard.press('F2')
    await settle(200)
    await openDialog()
    await kit.getByTestId('sheet-symbol-subset').selectOption('letterlike')
    await settle(150)
    await kit.locator('[data-testid="sheet-symbol-grid-item"][data-symbol="℃"]').click()
    step('INSC-C107 选中符号后底部呈其字符代码 2103', (await kit.getByTestId('sheet-symbol-code').textContent()) === '2103')
    await kit.getByTestId('sheet-symbol-insert').click()
    await settle(250)
    step('INSC-C107 「℃」插入光标处，A1 呈「温度℃」且仍在编辑', (await details()).editText === '温度℃' && (await editing()))
    step('INSC-C107 对话框的最近使用符号呈「℃」', (await symbolsOf('sheet-symbol-dialog-recent'))[0] === '℃')
    await closeDialog()
    await page.keyboard.press('Enter')
    await settle(200)
    step('INSC-C107 回车后 A1 为「温度℃」', (await val('A1')) === '温度℃')
  })

  await section('INSC-C108 选中未编辑时插入', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '温度' } }])
    await select('A1')
    await openDialog()
    await kit.getByTestId('sheet-symbol-subset').selectOption('letterlike')
    await settle(150)
    await kit.locator('[data-testid="sheet-symbol-grid-item"][data-symbol="℃"]').dblclick()
    await settle(250)
    // WPS for Mac: the cell opens with the symbol after its text.
    step('INSC-C108 双击符号插入；按 WPS 桌面追加在原文字后、单元格进入编辑', (await details()).editText === '温度℃')
    step('INSC-C108 最近使用符号呈「℃」', (await symbolsOf('sheet-symbol-dialog-recent'))[0] === '℃')
    await closeDialog()
    await page.keyboard.press('Escape')
  })

  await section('INSC-C109 连续插入 20 个', async () => {
    await fresh()
    for (let i = 0; i < 20; i += 1) await pick('＋')
    await page.keyboard.press('Enter')
    await settle(200)
    step('INSC-C109 A1 呈 20 个连续的「＋」，内容完整保留', (await val('A1')) === '＋'.repeat(20))
  })

  await section('INSC-C195 撤销', async () => {
    await fresh()
    await pick('＋')
    await page.keyboard.press('Enter')
    await settle(200)
    await select('A1')
    await page.keyboard.press(`${mod}+z`)
    await settle(300)
    step('INSC-C195 插入后 Ctrl+Z，A1 的「＋」消失，回到空白', (await val('A1')) == null)
  })

  await section('INSC-C196/C197/C198/C199 字体与子集', async () => {
    await fresh()
    await openDialog()
    step('INSC-C196 字体下拉默认（普通文本）——单元格的字体（WPS）', (await kit.getByTestId('sheet-symbol-font').inputValue()) === '' && (await kit.getByTestId('sheet-symbol-font').locator('option:checked').textContent()) === '（普通文本）')
    const latin = await gridSymbols()
    step('INSC-C197 子集默认「基本拉丁字母」，网格呈该子集的字符', (await kit.getByTestId('sheet-symbol-subset').locator('option:checked').textContent()) === '基本拉丁字母' && latin.includes('A') && latin.includes('~'))
    await kit.getByTestId('sheet-symbol-font').selectOption('宋体')
    await settle(150)
    const family = await kit.getByTestId('sheet-symbol-grid').locator('[data-symbol]').first().evaluate((e) => e.style.fontFamily)
    step('INSC-C198 字体选「宋体」，网格字形随之变化，个数不变', /SimSun|Songti/.test(family) && (await gridSymbols()).length === latin.length, family)
    await kit.getByTestId('sheet-symbol-subset').selectOption('geometric')
    await settle(150)
    const shapes = await gridSymbols()
    step('INSC-C199 子集选「几何图形」，网格换为几何图形', (await kit.getByTestId('sheet-symbol-subset').locator('option:checked').textContent()) === '几何图形' && shapes.includes('■') && shapes.includes('▲') && !shapes.includes('A'))
    await closeDialog()
  })

  await section('INSC-C200 合并单元格', async () => {
    await fresh()
    await ai([{ command: 'cell.merge', params: { range: 'A1:B2' } }])
    await select('A1')
    await pick('＋')
    await page.keyboard.press('Enter')
    await settle(200)
    const merge = (await cellOf('A1')).merge
    step('INSC-C200 「＋」落在合并区左上角 A1，B2 无字符，合并范围不变', (await val('A1')) === '＋' && (await val('B2')) == null && merge === 'A1:B2', String(merge))
  })

  await section('INSC-C93 文本框激活时插入', async () => {
    await fresh()
    const id = (await exec('drawing.insert', { kind: 'textBox', text: 'Box', frame: { x: 300, y: 60, w: 120, h: 40 } })).id
    const box = await region()
    await page.mouse.click(box.x + 46 + 360, box.y + 21 + 80)
    await settle(200)
    await page.keyboard.press('Enter')
    await settle(150)
    step('INSC-C93 文本框处于激活（输入中）', (await details()).shapeEditing === true)
    await pick('＋')
    await page.keyboard.press('Escape')
    await settle(250)
    const text = (await exec('drawing.describe', { ids: [id] })).objects?.[0]?.text
    const plain = typeof text === 'string' ? text : (text?.paragraphs ?? []).map((p) => p.runs.map((r) => r.text).join('')).join('\n')
    step('INSC-C93 符号插入到文本框的光标处，原有文字仍在', plain === 'Box＋' && (await val('A1')) == null, JSON.stringify(text))
  })

  await section('INSC-C201/C322/C323 保存、导入、导出', async () => {
    await fresh(numbered(2))
    await pick('±')
    await page.keyboard.press('Enter')
    await touch()
    await saveAndReopen()
    step('INSC-C201 关闭后重新打开，A1 仍为「±」', (await val('A1')) === '±')
    const sheet = (await zipPart('xl/sharedStrings.xml')) ?? ''
    step('INSC-C323 导出的 .xlsx 中 A1 为「±」（Excel 打开呈同一字符）', sheet.includes('±'))
    // The reopened file stands for a third-party one: inserting works the same.
    await select('B1')
    await pick('≈')
    await page.keyboard.press('Enter')
    await settle(200)
    step('INSC-C322 导入的表格中插入符号：B1 呈「≈」，工作表 1、2 的内容与数量不变', (await val('B1')) === '≈' && (await val('A1')) === '±' && (await h.order()).length === 2)
  })

  await section('INSC-C166 只读', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '温度' } }])
    await touch()
    const version = await page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, h.docPath())
    await kit.getByTestId('sheet-save').click()
    await until(async () => (await page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, h.docPath())) > version)
    const bytes = await page.evaluate((path) => [...new Uint8Array(window.devHost.files.get(path).bytes)], h.docPath())
    await openEditor('&role=viewer')
    try {
      await page.evaluate(async (bytes) => {
        document.getElementById('status').textContent = ''
        await window.devHost.open('Viewer.xlsx', new Uint8Array(bytes).buffer)
      }, bytes)
      h.setDocPath('Viewer.xlsx')
      await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
      await kit.getByTestId('sheet-strip').waitFor()
      await settle(600)
      step('INSC-C166 仅可查看：没有「插入」选项卡，符号面板无从弹出，A1 不变', (await kit.getByTestId('ribbon-tab-insert').count()) === 0 && (await kit.getByTestId('sheet-symbol').count()) === 0 && (await val('A1')) === '温度')
    } finally {
      await openEditor('')
    }
  })

  // ———————————————— Checkboxes ————————————————

  await section('INSC-C50/C52/C54/C55 插入复选框', async () => {
    await fresh()
    await insertCheckbox()
    step('INSC-C50 A1 呈方形复选框，不呈其它文字', (await isBox('A1')) && (await shown('A1')) === 'empty' && white(await h.pixel(46 + 56, 21 + 10)))
    step('INSC-C52 默认未勾选：浅灰描边空心方框，取值 FALSE', (await val('A1')) === false)
    step('INSC-C54 方框只在 A1，B1、C1、A2 不变', !(await isBox('B1')) && !(await isBox('C1')) && !(await isBox('A2')) && (await shown('B1')) === 'none' && (await shown('A2')) === 'none')
    const left = await h.pixel(46 + mid(G.cw) - 6, 21 + mid(G.rh))
    const rightEdge = await h.pixel(46 + mid(G.cw) + 6, 21 + mid(G.rh))
    step('INSC-C55 默认行高列宽下方框完整呈现（左右两边都在格内）', !white(left) && !white(rightEdge) && white(await h.pixel(46 + 4, 21 + 10)))
  })

  await section('INSC-C51 选区多个单元格', async () => {
    await fresh()
    // The person's selection (an agent's select.set moves only its own editor state, KHP C3b).
    await h.goTo('A1:A3')
    await insertCheckbox()
    // The selection's tint off the cells looked at.
    await select('D6')
    const all = [await shown('A1'), await shown('A2'), await shown('A3')]
    step('INSC-C51 A1、A2、A3 各一个浅灰描边空心方框', same(all, ['empty', 'empty', 'empty']) && (await isBox('A3')), JSON.stringify(all))
  })

  await section('INSC-C53/C59/C60/C250/C249 勾选与取消', async () => {
    await fresh()
    await boxes('A1')
    step('INSC-C53 未勾选为浅灰描边的空心方框', (await shown('A1')) === 'empty')
    await click('A1')
    // The box is drawn checked a frame or more after the value turns TRUE. Wait on what the
    // person sees (the formula bar, the grid's pixels), then read the value through the agent.
    // A box never drawn (the click's press and release taken in one React render repainted
    // nothing, regressions/issue-checkbox-paint-lost) fails here, within about 3 s.
    await until(async () => (await formulaBar()) === 'TRUE' && (await shown('A1')) === 'checked', 30)
    step('INSC-C53/C59 单击方框：深灰实底与白色对勾，取值 TRUE', (await shown('A1')) === 'checked' && (await val('A1')) === true)
    step('INSC-C59/C250 fx 栏呈「TRUE」', (await formulaBar()) === 'TRUE')
    await click('A1')
    await until(async () => (await formulaBar()) === 'FALSE' && (await shown('A1')) === 'empty', 30)
    step('INSC-C60 再次单击取消勾选，回到空心方框', (await shown('A1')) === 'empty' && (await val('A1')) === false)
    // Excel and WPS show FALSE in capitals.
    step('INSC-C60/C249 fx 栏呈「FALSE」（WPS/Excel 大写）', (await formulaBar()) === 'FALSE')
  })

  await section('INSC-C61/C62 空格键与方框外单击', async () => {
    await fresh()
    await boxes('A1')
    await select('A1')
    step('INSC-C62 单击方框以外的区域：A1 选中，方框不变', (await details()).selection === 'A1' && (await shown('A1')) === 'empty' && (await val('A1')) === false)
    await page.keyboard.press(' ')
    await settle(200)
    step('INSC-C61 空格键勾选', (await shown('A1')) === 'checked' && (await val('A1')) === true)
    await page.keyboard.press(' ')
    await settle(200)
    step('INSC-C61 再按空格取消勾选', (await shown('A1')) === 'empty' && (await val('A1')) === false)
  })

  await section('INSC-C63 框选后单击其中一个', async () => {
    await fresh()
    await boxes('A1:A3', [['A1', true], ['A3', true]])
    await ai([{ command: 'select.set', params: { range: 'A1:A3' } }])
    await click('A2')
    await settle(200)
    step('INSC-C63 只有 A2 转为勾选，A1、A3 仍勾选', (await val('A2')) === true && (await val('A1')) === true && (await val('A3')) === true && (await shown('A2')) === 'checked')
  })

  await section('INSC-C64/C173/C65 不可直接编辑', async () => {
    await fresh()
    await boxes('A1')
    const box = await region()
    await page.mouse.dblclick(box.x + 46 + 6, box.y + 21 + 10)
    await settle(250)
    step('INSC-C64 双击不进入编辑，fx 栏仍为 FALSE', !(await editing()) && (await formulaBar()) === 'FALSE' && (await val('A1')) === false)
    await select('A1')
    await page.keyboard.type('已完成')
    await settle(250)
    step('INSC-C173 直接输入被禁止：仍为复选框、未勾选，不呈文字', (await isBox('A1')) && (await val('A1')) === false && !(await editing()))
    step('INSC-C173 页面提示「复选框不可直接编辑，请删除后再插入」', await noticeShown('复选框不可直接编辑，请删除后再插入'))
    await dismiss()
    await settle(900)
    await page.keyboard.press('Control+;')
    await settle(250)
    step('INSC-C65 按插入日期快捷键 Ctrl+;：提示「复选框不可直接编辑，请删除后再插入」，取值不变', (await noticeShown('复选框不可直接编辑')) && (await isBox('A1')) && (await val('A1')) === false)
    await dismiss()
    await select('B1')
    await page.keyboard.press('Control+;')
    await settle(200)
    const date = new Date()
    step('INSC-C65 （对照）普通单元格 Ctrl+; 输入当天日期', ((await details()).editText ?? '') === `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()}`, String((await details()).editText))
    await page.keyboard.press('Escape')
  })

  await section('INSC-C66/C268 清除内容', async () => {
    await fresh()
    await boxes('A1', [['A1', true]])
    await cellMenu('A1', 'clear', 'sheet-grid-clear-contents')
    step('INSC-C66/C268 右键 清除内容 › 内容：方框消失，单元格回到空白', !(await isBox('A1')) && (await val('A1')) == null && (await shown('A1')) === 'none')
    await select('A1')
    await type('已完成')
    step('INSC-C66 之后可直接输入「已完成」', (await val('A1')) === '已完成')
  })

  await section('INSC-C67 清除全部', async () => {
    await fresh()
    await boxes('A1', [['A1', true]])
    await cellMenu('A1', 'clear', 'sheet-grid-clear-all')
    await touch()
    await saveAndReopen()
    // WPS and Excel: Clear All leaves the cell empty, as clearing the
    // contents does in INSC-C268.
    step('INSC-C67 清除 › 全部后刷新：单元格为空、不呈方框（WPS/Excel；与 C268 一致）', !(await isBox('A1')) && (await val('A1')) == null)
  })

  await section('INSC-C267 清除格式', async () => {
    await fresh()
    await boxes('A1', [['A1', true]])
    await ai([{ command: 'format.fillColor', params: { range: 'A1', color: '#FFFF00' } }])
    await cellMenu('A1', 'clear', 'sheet-grid-clear-formats')
    await touch()
    await saveAndReopen()
    step('INSC-C267 清除 › 格式后刷新：仍为勾选的复选框（复选框是内容），填充色去掉', (await isBox('A1')) && (await val('A1')) === true && (await shown('A1')) === 'checked' && (await styleOf('A1')).fill?.pattern !== 'Solid')
  })

  await section('INSC-C68 字号与字体属性', async () => {
    await fresh()
    await boxes('A1', [['A1', true]])
    await ai([
      { command: 'format.bold', params: { range: 'A1', on: true } },
      { command: 'format.italic', params: { range: 'A1', on: true } },
      { command: 'format.underline', params: { range: 'A1', on: true } },
      { command: 'format.strikethrough', params: { range: 'A1', on: true } },
      { command: 'format.color', params: { range: 'A1', color: '#FF0000' } },
      { command: 'format.fontSize', params: { range: 'A1', size: 16 } },
      { command: 'row.setHeight', params: { rows: '1', height: 30 } },
    ])
    await settle(300)
    // Row 1 is 30 px now: the box is centred at y = 21 + 15.
    const fill = await h.pixel(46 + mid(G.cw) - 4, 21 + 15 - 4)
    const outer = await h.pixel(46 + mid(G.cw) - 9, 21 + 15)
    const red = (p) => p[0] > 180 && p[1] < 90 && p[2] < 90
    step('INSC-C68 方框颜色变为字体颜色（红），尺寸随字号 16 放大（19 px，大于默认 13 px）', red(fill) && red(outer), `${fill} ${outer}`)
    step('INSC-C68 仍为勾选的复选框，不呈文字', (await isBox('A1')) && (await val('A1')) === true)
  })

  await section('INSC-C69/C70 背景色、边框、对齐', async () => {
    await fresh()
    await boxes('A1', [['A1', true]])
    await ai([
      { command: 'format.fillColor', params: { range: 'A1', color: '#FFFF00' } },
      { command: 'format.borders', params: { range: 'A1', preset: 'all' } },
      { command: 'format.alignCenter', params: { range: 'A1' } },
      { command: 'format.wrapText', params: { range: 'A1', on: true } },
    ])
    await settle(250)
    const yellow = (p) => p[0] > 230 && p[1] > 230 && p[2] < 80
    step('INSC-C69 A1 呈黄色背景与四边细边框，方框水平居中', yellow(await h.pixel(46 + 10, 21 + 10)) && (await styleOf('A1')).borders?.top?.style === 'Thin' && (await shown('A1')) === 'checked', JSON.stringify((await styleOf('A1')).borders?.top))
    await ai([{ command: 'format.fillColor', params: { range: 'B1', color: '#FFFF00' } }])
    await select('B1')
    await insertCheckbox()
    step('INSC-C70 先设黄色背景再插入：单元格仍为黄色，方框内部为方框本身的白底', yellow(await h.pixel(46 + G.cw + 10, 21 + 10)) && (await shown('B1')) === 'empty')
  })

  await section('INSC-C71 纯文本格式', async () => {
    await fresh()
    await boxes('A1')
    await cellMenu('A1', null, 'sheet-cmd-format.cells')
    await kit.getByTestId('sheet-format-cells').waitFor()
    await kit.getByTestId('fc-cat-text').click()
    await kit.getByTestId('fc-ok').click()
    await settle(300)
    await kit.getByTestId('ribbon-tab-home').click()
    await settle(200)
    step('INSC-C71 设为「文本」后工具栏水平对齐呈居中，仍为方形复选框', (await styleOf('A1')).num_fmt === '@' && (await kit.getByTestId('sheet-cmd-format.alignCenter').getAttribute('aria-pressed')) === 'true' && (await isBox('A1')) && (await shown('A1')) === 'empty')
  })

  await section('INSC-C251/C252 撤销与切换工作表', async () => {
    await fresh()
    await boxes('A1')
    await click('A1')
    await settle(200)
    await page.keyboard.press(`${mod}+z`)
    await settle(300)
    step('INSC-C251 勾选后 Ctrl+Z：回到空心方框，未勾选', (await val('A1')) === false && (await shown('A1')) === 'empty')
    await click('A1')
    await settle(200)
    await h.choose('工作表2')
    await h.choose('工作表1')
    await settle(300)
    step('INSC-C252 切换工作表再切回，A1 仍为勾选', (await val('A1')) === true && (await shown('A1')) === 'checked')
  })

  await section('INSC-C253 fx 栏不被激活', async () => {
    await fresh()
    await boxes('A1')
    await select('A1')
    await kit.getByTestId('sheet-formula').click()
    await page.keyboard.type('已完成')
    await settle(250)
    const focused = await kit.getByTestId('sheet-formula').evaluate((e) => e.ownerDocument.activeElement === e)
    step('INSC-C253 单击 fx 栏不被激活，仍呈 FALSE，取值不变', !focused && (await formulaBar()) === 'FALSE' && (await isBox('A1')) && (await val('A1')) === false)
    await dismiss()
  })

  await section('INSC-C254 链接与公式不可用', async () => {
    await fresh()
    await boxes('A1')
    await select('A1')
    await insertTab()
    step('INSC-C254 插入选项卡的「链接」置灰', await kit.getByTestId('sheet-cmd-link.insert').isDisabled())
    step('INSC-C254 fx（插入函数）置灰', await kit.getByTestId('sheet-formula-fx').isDisabled())
    await right('A1')
    step('INSC-C254 右键菜单的「超链接…」置灰', await kit.getByTestId('sheet-grid-menu-link').isDisabled())
    await page.keyboard.press('Escape')
    await settle(150)
    await select('A1')
    await page.keyboard.press(`${mod}+k`)
    await settle(300)
    step('INSC-C254 按 Ctrl+K 不弹出插入链接，A1 不变', (await kit.locator('[data-testid="command-dialog-run"]').count()) === 0 && (await isBox('A1')) && (await cellOf('A1')).hyperlink == null)
    await page.keyboard.press('Escape')
  })

  await section('INSC-C255/C256 数组公式', async () => {
    await fresh()
    await ai([
      { command: 'range.setValues', params: { range: 'B2', values: [[1], [2]] } },
      { command: 'select.set', params: { range: 'A2:A3' } },
      { command: 'cell.set', params: { cell: 'A2', input: '=B2:B3*2', array: true } },
    ])
    await select('A1')
    await insertCheckbox()
    step('INSC-C255 A1 可插入', await isBox('A1'))
    await select('A2')
    await insertCheckbox()
    step('INSC-C255 A2（数组的一部分）不可插入：内容不变、不呈方框，提示不能更改数组的某一部分', !(await isBox('A2')) && (await val('A2')) === 2 && (await noticeShown('不能更改数组的某一部分')))
    await dismiss()
    await ai([{ command: 'select.set', params: { range: 'A2:A3' } }])
    await insertCheckbox()
    step('INSC-C256 选中整个数组也不可插入：A2、A3 取值保持，数组范围不变', !(await isBox('A2')) && !(await isBox('A3')) && (await val('A2')) === 2 && (await val('A3')) === 4 && (await cellOf('A2')).formula != null)
    await dismiss()
  })

  await section('INSC-C257 公式、链接、图片处可插入', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '=1+1' } },
      { command: 'link.insert', params: { cell: 'B1', target: 'https://example.com', text: 'link' } },
      { command: 'image.insert', params: { at: 'C1', base64: PNG, mime: 'image/png', placement: 'cell' } },
    ])
    for (const ref of ['A1', 'B1', 'C1']) {
      await select(ref)
      await insertCheckbox()
    }
    step('INSC-C257 A1（公式）、B1（链接）、C1（单元格图片）各呈未勾选的复选框（透视表区域未测）', (await isBox('A1')) && (await isBox('B1')) && (await isBox('C1')) && (await val('A1')) === false && (await cellOf('C1')).picture == null)
  })

  await section('INSC-C258/C259/C260 复制、剪切、跨表粘贴', async () => {
    await fresh()
    await boxes('A1', [['A1', true]])
    await exec('edit.copy', { range: 'A1' })
    await exec('edit.paste', { at: 'B1' })
    await settle(200)
    step('INSC-C258 复制后粘贴：B1 为勾选的复选框，A1 不变', (await isBox('B1')) && (await val('B1')) === true && (await shown('B1')) === 'checked' && (await val('A1')) === true)
    await exec('edit.cut', { range: 'A1' })
    await exec('edit.paste', { at: 'C1' })
    await settle(200)
    step('INSC-C259 剪切后粘贴：C1 为勾选的复选框，A1 回到空白', (await isBox('C1')) && (await val('C1')) === true && !(await isBox('A1')) && (await val('A1')) == null)
    await exec('edit.copy', { range: 'B1' })
    await h.choose('工作表2')
    await exec('edit.paste', { at: 'B2' })
    await settle(200)
    step('INSC-C260 跨工作表粘贴：工作表2 的 B2 为勾选的复选框', (await isBox('B2')) && (await val('B2')) === true)
    await h.choose('工作表1')
  })

  await section('INSC-C262/C263 选择性粘贴', async () => {
    await fresh()
    await boxes('A1:B1', [['A1', true]])
    // The person copies A1:B1 (the cell menu's Copy) and pastes it with Paste Special.
    await h.goTo('A1:B1')
    await cellMenu('A1', null, 'sheet-grid-menu-copy')
    await right('A3')
    await kit.getByTestId('sheet-grid-menu-paste-special').click()
    // kit-editor's command dialog keeps the parameters the command does not list up front under
    // its More options.
    await kit.locator('[role=dialog]').waitFor()
    if (!(await kit.locator('#param-what').isVisible())) await kit.locator('[role=dialog] details > summary').click()
    await kit.locator('#param-what').selectOption('values')
    await kit.getByTestId('command-dialog-run').click()
    await settle(300)
    await select('D6')
    step('INSC-C262 选择性粘贴为值：A3 呈 TRUE、B3 呈 FALSE（WPS/Excel 大写），均不呈方框', (await val('A3')) === true && (await val('B3')) === false && !(await isBox('A3')) && !(await isBox('B3')) && (await cellOf('A3')).text === 'TRUE', JSON.stringify([await val('A3'), await val('B3'), (await cellOf('A3')).text, await shown('A3')]))
    // An agent's selection and clipboard last one run (KHP C3b), so its copy and paste go together.
    await ai([{ command: 'edit.copy', params: { range: 'A1' } }, { command: 'select.set', params: { range: 'D1' } }, { command: 'edit.pasteSpecial', params: { what: 'formats' } }])
    await settle(200)
    step('INSC-C263 选择性粘贴为格式：D1 不呈方框，仍为空', !(await isBox('D1')) && (await val('D1')) == null && (await shown('D1')) === 'none')
  })

  await section('INSC-C264/C265/C266 格式刷', async () => {
    await fresh()
    await boxes('A1', [['A1', true]])
    await ai([{ command: 'format.fillColor', params: { range: 'A1', color: '#FFFF00' } }])
    await ai([{ command: 'select.set', params: { range: 'A1' } }, { command: 'edit.formatPainter', params: {} }, { command: 'edit.formatPainter', params: { apply: 'B1' } }])
    await settle(200)
    step('INSC-C264 复选框单元格刷到 B1：B1 得到 A1 的格式（黄色），不呈方框', (await styleOf('B1')).fill?.pattern === 'Solid' && !(await isBox('B1')) && (await val('B1')) == null)
    await ai([
      { command: 'cell.set', params: { cell: 'C1', input: 'x' } },
      { command: 'format.bold', params: { range: 'C1', on: true } },
      { command: 'format.fillColor', params: { range: 'C1', color: '#FFFF00' } },
      { command: 'checkbox.insert', params: { range: 'D1' } },
      { command: 'select.set', params: { range: 'C1' } },
      { command: 'edit.formatPainter', params: {} },
      { command: 'edit.formatPainter', params: { apply: 'D1' } },
    ])
    await settle(200)
    step('INSC-C265 已设样式的单元格刷到复选框：D1 加粗、黄色，仍为复选框，取值不变', (await styleOf('D1')).font?.bold === true && (await styleOf('D1')).fill?.pattern === 'Solid' && (await isBox('D1')) && (await val('D1')) === false)
    await ai([
      { command: 'select.set', params: { range: 'E5' } },
      { command: 'edit.formatPainter', params: {} },
      { command: 'edit.formatPainter', params: { apply: 'A1' } },
    ])
    await touch()
    await saveAndReopen()
    step('INSC-C266 空单元格刷到复选框后刷新：A1 仍为勾选的复选框', (await isBox('A1')) && (await val('A1')) === true && (await shown('A1')) === 'checked')
  })

  await section('INSC-C269/C273 合并后插入', async () => {
    await fresh()
    await ai([{ command: 'cell.merge', params: { range: 'A1:B1' } }])
    await select('A1')
    await insertCheckbox()
    await select('D6')
    step('INSC-C273 方框落在合并区（左上角 A1 的值），B1 不另呈方框', (await isBox('A1')) && !(await isBox('B1')) && (await boxAt(46 + G.cw, 21 + 10)) === 'empty', JSON.stringify([await isBox('A1'), await isBox('B1'), await boxAt(46 + G.cw, 21 + 10)]))
    await cellMenu('A1', null, 'sheet-grid-menu-unmerge')
    await select('D6')
    step('INSC-C269 取消合并：A1 呈方框，B1 回到空白', (await isBox('A1')) && !(await isBox('B1')) && (await val('B1')) == null && (await shown('A1')) === 'empty')
    await fresh()
    await ai([{ command: 'cell.merge', params: { range: 'A1:B2' } }])
    await select('A1')
    await insertCheckbox()
    await select('D6')
    step('INSC-C273 合并 A1:B2 插入：方框在合并区，B2 不另呈方框，合并范围不变', (await isBox('A1')) && !(await isBox('B2')) && (await cellOf('A1')).merge === 'A1:B2' && (await boxAt(46 + G.cw, 21 + G.rh)) === 'empty')
  })

  await section('INSC-C270/C271/C272 合并与取消合并', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '1' } }, { command: 'checkbox.insert', params: { range: 'B1' } }, { command: 'select.set', params: { range: 'A1:B1' } }])
    await h.goTo('A1:B1')
    await right('A1')
    await kit.getByTestId('sheet-grid-menu-merge').click()
    await kit.getByTestId('sheet-merge-confirm').waitFor()
    step('INSC-C270 弹出提示框：标题「合并单元格」，「合并后的单元格将只保留左/上单元格中的值，确定合并吗？」，确定与取消', (await kit.getByTestId('sheet-merge-confirm').textContent()).includes('合并单元格') && (await kit.getByTestId('sheet-merge-confirm-message').textContent()) === '合并后的单元格将只保留左/上单元格中的值，确定合并吗？' && (await kit.getByTestId('sheet-merge-confirm-cancel').count()) === 1)
    await kit.getByTestId('sheet-merge-confirm-ok').click()
    await settle(300)
    step('INSC-C270 合并后呈数值 1', (await val('A1')) === 1 && (await cellOf('A1')).merge === 'A1:B1')
    await cellMenu('A1', null, 'sheet-grid-menu-unmerge')
    await select('D6')
    step('INSC-C270 取消合并：A1 为 1，B1 回到空白（不呈方框）', (await val('A1')) === 1 && (await val('B1')) == null && !(await isBox('B1')) && (await shown('B1')) === 'none')

    await fresh()
    await ai([{ command: 'checkbox.insert', params: { range: 'A1' } }, { command: 'cell.set', params: { cell: 'B1', input: '1' } }, { command: 'select.set', params: { range: 'A1:B1' } }])
    await h.goTo('A1:B1')
    await right('A1')
    await kit.getByTestId('sheet-grid-menu-merge').click()
    await kit.getByTestId('sheet-merge-confirm-ok').click()
    await settle(300)
    await select('D6')
    step('INSC-C271 复选框与数值合并：合并单元格呈方形复选框', (await isBox('A1')) && (await boxAt(46 + G.cw, 21 + 10)) === 'empty')
    await cellMenu('A1', null, 'sheet-grid-menu-unmerge')
    await select('D6')
    step('INSC-C271 取消合并：A1 呈复选框，B1 回到空白', (await isBox('A1')) && (await val('B1')) == null && (await shown('A1')) === 'empty')

    await fresh()
    await ai([{ command: 'checkbox.insert', params: { range: 'A1:B1' } }, { command: 'select.set', params: { range: 'A1:B1' } }])
    await h.goTo('A1:B1')
    await right('A1')
    await kit.getByTestId('sheet-grid-menu-merge').click()
    await kit.getByTestId('sheet-merge-confirm-ok').click()
    await settle(300)
    await cellMenu('A1', null, 'sheet-grid-menu-unmerge')
    await select('B1')
    await page.keyboard.press(' ')
    await settle(250)
    step('INSC-C272 两个复选框合并再取消：A1 呈复选框；B1 可选中，按空格不出现已勾选的复选框', (await isBox('A1')) && !(await isBox('B1')) && (await shown('B1')) !== 'checked' && (await details()).active === 'B1')
    await page.keyboard.press('Escape')
  })

  await section('INSC-C274 排序', async () => {
    await fresh()
    await ai([
      { command: 'checkbox.insert', params: { range: 'A1:A3' } },
      { command: 'cell.set', params: { cell: 'A1', input: 'TRUE' } },
      { command: 'cell.set', params: { cell: 'A3', input: 'TRUE' } },
      { command: 'range.setValues', params: { range: 'B1', values: [['张一'], ['张二'], ['张三']] } },
      { command: 'select.set', params: { range: 'A1:B3' } },
    ])
    await right('A1')
    await kit.getByTestId('sheet-grid-menu-sort').hover()
    await kit.getByTestId('sheet-grid-sort-ascending').click()
    await settle(300)
    await touch()
    await saveAndReopen()
    const rows = [[await val('A1'), await val('B1')], [await val('A2'), await val('B2')], [await val('A3'), await val('B3')]]
    step('INSC-C274 升序后刷新：各行仍为复选框，状态随同行文字移动', (await isBox('A1')) && (await isBox('A2')) && (await isBox('A3')) && same(rows, [[false, '张二'], [true, '张一'], [true, '张三']]), JSON.stringify(rows))
  })

  await section('INSC-C275/C276 筛选', async () => {
    await fresh()
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: [['完成', '姓名']] } },
      { command: 'checkbox.insert', params: { range: 'A2:A4' } },
      { command: 'cell.set', params: { cell: 'A2', input: 'TRUE' } },
      { command: 'cell.set', params: { cell: 'A4', input: 'TRUE' } },
      { command: 'range.setValues', params: { range: 'B2', values: [['张一'], ['张二'], ['张三']] } },
      { command: 'select.set', params: { range: 'A1:B4' } },
      { command: 'data.filter', params: {} },
    ])
    const shownValues = [...new Set([await cellOf('A2'), await cellOf('A3'), await cellOf('A4')].map((c) => c.text))]
    step('INSC-C275 筛选取值项为 TRUE 与 FALSE（值的显示文字），不呈方框图标', same(shownValues.sort(), ['FALSE', 'TRUE']), JSON.stringify(shownValues))
    await exec('data.filterBy', { column: 'A', values: ['TRUE'] })
    await settle(250)
    step('INSC-C276 只勾选 TRUE：只呈第 1、第 3 条（张一、张三），第 2 条（张二）隐藏', (await val('B3')) === '张二' && (await rowHidden(3)) && !(await rowHidden(2)) && !(await rowHidden(4)))
  })

  await section('INSC-C277 删除重复项', async () => {
    await fresh()
    await ai([
      { command: 'checkbox.insert', params: { range: 'A1:A4' } },
      { command: 'cell.set', params: { cell: 'A3', input: 'TRUE' } },
      { command: 'cell.set', params: { cell: 'A4', input: 'TRUE' } },
      { command: 'data.removeDuplicates', params: { range: 'A1:A4', header: false } },
    ])
    await settle(250)
    step('INSC-C277 只剩 2 个：一个未勾选、一个已勾选，均为复选框', (await isBox('A1')) && (await isBox('A2')) && (await val('A1')) === false && (await val('A2')) === true && !(await isBox('A3')) && (await shown('A3')) === 'none' && (await shown('A4')) === 'none')
  })

  await section('INSC-C278 分列', async () => {
    await fresh()
    await ai([
      { command: 'checkbox.insert', params: { range: 'A1' } },
      { command: 'cell.set', params: { cell: 'A1', input: 'TRUE' } },
      { command: 'cell.set', params: { cell: 'B1', input: '已完成' } },
      { command: 'data.textToColumns', params: { range: 'A1', delimiters: [' '] } },
    ])
    await settle(200)
    step('INSC-C278 分列后未被拆开的复选框仍呈方形复选框（已勾选），B1 不变', (await isBox('A1')) && (await val('A1')) === true && (await val('B1')) === '已完成')
  })

  await section('INSC-C279/C280/C281 查找替换', async () => {
    await fresh()
    await boxes('A1', [['A1', true]])
    await exec('edit.replace', { what: 'true', with: 'false', all: true })
    await settle(200)
    step('INSC-C279 true 替换为 false：取值未勾选，仍为空心方框', (await isBox('A1')) && (await val('A1')) === false && (await shown('A1')) === 'empty')
    await exec('edit.replace', { what: 'false', with: 'true', all: true })
    await settle(200)
    step('INSC-C280 false 替换为 true：取值勾选，深灰实底白勾', (await isBox('A1')) && (await val('A1')) === true && (await shown('A1')) === 'checked')
    await ai([{ command: 'format.numberFormat', params: { range: 'B1', format: 'Text' } }, { command: 'checkbox.insert', params: { range: 'B1' } }])
    await exec('edit.replace', { what: 'false', with: 'true', all: true })
    await touch()
    await saveAndReopen()
    step('INSC-C281 文本格式下替换后刷新：B1 仍为复选框，已勾选', (await isBox('B1')) && (await val('B1')) === true)
  })

  await section('INSC-C284 插入行列', async () => {
    await fresh()
    await boxes('A1:A3')
    await ai([
      { command: 'row.insert', params: { rows: '2:2' } },
      { command: 'column.insert', params: { cols: 'B:B' } },
    ])
    await settle(250)
    step('INSC-C284 新插入的第 2 行与 B 列不呈复选框，为空白', !(await isBox('A2')) && (await val('A2')) == null && (await shown('A2')) === 'none' && !(await isBox('B1')) && (await shown('B1')) === 'none')
    step('INSC-C284 原 3 个复选框（A1、A3、A4）取值与状态不变', (await isBox('A1')) && (await isBox('A3')) && (await isBox('A4')) && (await val('A4')) === false)
  })

  await section('INSC-C286/C328/C330 保存与导出', async () => {
    await fresh()
    await select('A1')
    await insertCheckbox()
    step('INSC-C330 （导入的表格中同样可插入）A1 呈复选框', await isBox('A1'))
    await ai([{ command: 'checkbox.insert', params: { range: 'A2:A3' } }, { command: 'cell.set', params: { cell: 'A1', input: 'TRUE' } }, { command: 'cell.set', params: { cell: 'A3', input: 'TRUE' } }])
    await touch()
    await saveAndReopen()
    step('INSC-C286 关闭后重新打开：A1 仍为勾选的复选框（深灰实底白勾）', (await isBox('A1')) && (await val('A1')) === true && (await shown('A1')) === 'checked')
    const sheetXml = (await zipPart('xl/worksheets/sheet1.xml')) ?? ''
    const bags = (await zipPart('xl/featurePropertyBag/featurePropertyBag.xml')) ?? ''
    const cells = [...sheetXml.matchAll(/<c r="(A[123])"[^>]*t="b"[^>]*><v>([01])<\/v>/g)].map((m) => `${m[1]}=${m[2]}`)
    // Excel opens them as checkboxes (its feature property bags).
    step('INSC-C328 导出的 .xlsx：A1、A3 为 TRUE，A2 为 FALSE，并写入 Excel 的复选框（featurePropertyBag）', same(cells, ['A1=1', 'A2=0', 'A3=1']) && bags.includes('Checkbox'), JSON.stringify(cells))
  })

  await section('INSC-C177 只读下单击方框', async () => {
    await fresh()
    await boxes('A1')
    await touch()
    const version = await page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, h.docPath())
    await kit.getByTestId('sheet-save').click()
    await until(async () => (await page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, h.docPath())) > version)
    const bytes = await page.evaluate((path) => [...new Uint8Array(window.devHost.files.get(path).bytes)], h.docPath())
    await openEditor('&role=viewer')
    try {
      await page.evaluate(async (bytes) => {
        document.getElementById('status').textContent = ''
        await window.devHost.open('Viewer.xlsx', new Uint8Array(bytes).buffer)
      }, bytes)
      h.setDocPath('Viewer.xlsx')
      await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
      await kit.getByTestId('sheet-strip').waitFor()
      await settle(600)
      await click('A1')
      await settle(300)
      step('INSC-C177 仅可查看：单击方框不切换，仍为空心方框、未勾选', (await val('A1')) === false && (await shown('A1')) === 'empty')
    } finally {
      await openEditor('')
    }
  })

  // ———————————————— Fonts (left over from M6): text boxes, pictures in cells ————————————————

  await section('FONT-C111/C123/C54 文本框内文字', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: 'abc' } }])
    const id = (await exec('drawing.insert', { kind: 'textBox', text: 'Box', frame: { x: 300, y: 60, w: 120, h: 40 } })).id
    const box = await region()
    await page.mouse.click(box.x + 46 + 360, box.y + 21 + 80)
    await settle(200)
    await kit.getByTestId('ribbon-tab-home').click()
    await kit.getByTestId('sheet-cmd-format.bold').click()
    await settle(250)
    const runs = async () => ((await exec('drawing.describe', { ids: [id] })).objects?.[0]?.text?.paragraphs ?? []).flatMap((p) => p.runs)
    step('FONT-C111/C123 选中文本框点「加粗」，文本框内文字按加粗字重渲染', (await runs()).length > 0 && (await runs()).every((r) => r.bold === true))
    step('FONT-C123 与单元格加粗互不影响（A1 未加粗）', (await styleOf('A1')).font?.bold !== true)
    await kit.getByTestId('sheet-font').fill('楷体')
    await kit.getByTestId('sheet-font').press('Enter')
    await settle(250)
    step('FONT-C54 文本框内文字设为楷体，A1 的字体不变', (await runs()).every((r) => r.font === '楷体') && (await styleOf('A1')).font?.name !== '楷体')
    await touch()
    await saveAndReopen()
    const after = ((await exec('drawing.describe', {})).objects ?? []).flatMap((o) => o.text?.paragraphs ?? []).flatMap((p) => p.runs)
    step('FONT-C54 保存后再次打开，文本框字体与加粗保持', after.length > 0 && after.every((r) => r.bold === true && r.font === '楷体'))
  })

  await section('FONT-C107/C137/C153/C169 单元格图片', async () => {
    await fresh()
    await ai([{ command: 'image.insert', params: { at: 'B1', base64: PNG, mime: 'image/png', placement: 'cell' } }])
    const before = JSON.stringify((await cellOf('B1')).picture)
    await select('B1')
    await kit.getByTestId('ribbon-tab-home').click()
    for (const id of ['bold', 'italic', 'underline', 'strikethrough']) await kit.getByTestId(`sheet-cmd-home.${id}`).click()
    await settle(250)
    const pressed = async (id) => (await kit.getByTestId(`sheet-cmd-home.${id}`).getAttribute('aria-pressed')) === 'true'
    step('FONT-C107/C137/C153/C169 设置成功：图片单元格的 B、I、U、S 按钮为选中态', (await pressed('bold')) && (await pressed('italic')) && (await pressed('underline')) && (await pressed('strikethrough')))
    step('FONT-C107/C137/C153/C169 图片无变化', JSON.stringify((await cellOf('B1')).picture) === before && before !== 'null')
    await page.keyboard.press('Delete')
    await settle(200)
    await select('B1')
    await type('abc')
    const font = (await styleOf('B1')).font ?? {}
    step('FONT-C107/C137/C153/C169 删除图片再输入内容：应用加粗、斜体、下划线、删除线', (await val('B1')) === 'abc' && font.bold && font.italic && font.underline !== 'None' && font.strike)
  })

  // ———————————————— Table styles (left over from M7): inserting from the toolbar, pictures in the header row ————————————————

  await section('TS-C60 工具栏插入不扩充表格', async () => {
    await fresh()
    const rows = [['名称', '地区', '一季度', '二季度']]
    for (let r = 2; r <= 5; r += 1) rows.push([`n${r}`, '东', r * 10, r * 7])
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: rows } },
      { command: 'table.insert', params: { range: 'A1:D5', style: 'TableStyleMedium2' } },
    ])
    // Right of the table: a picture, a link and a formula through Insert Function; below it, a symbol.
    await ai([{ command: 'image.insert', params: { at: 'E2', base64: PNG, mime: 'image/png', placement: 'cell' } }])
    await ai([{ command: 'select.set', params: { range: 'E3' } }, { command: 'link.insert', params: { cell: 'E3', target: 'https://example.com', text: 'link' } }])
    await select('E4')
    await kit.getByTestId('sheet-formula-fx').click()
    await kit.getByTestId('sheet-insert-function').waitFor()
    await kit.getByTestId('sheet-insert-function-search').fill('SUM')
    await settle(200)
    await kit.getByTestId('sheet-insert-function-list').locator('[role="option"]').first().click()
    await kit.getByTestId('sheet-insert-function-ok').click()
    await settle(250)
    await page.keyboard.type('C2:C3')
    await page.keyboard.press('Enter')
    await settle(200)
    await select('A6')
    await pick('±')
    await page.keyboard.press('Escape')
    await settle(250)
    const range = (await h.workbook()).sheets[0].tables?.[0]?.range
    step('TS-C60 在表格右/下侧经工具栏插入图片、链接、公式、符号：表格范围不变（A1:D5）', range === 'A1:D5' && (await cellOf('E2')).picture != null && (await cellOf('E3')).hyperlink != null, String(range))
    step('TS-C60 插入的内容正常落地', (await cellOf('E4')).formula != null || (await cellOf('E4')).input?.startsWith('='))
  })

  await section('TS-C76 标题行为图片单元格', async () => {
    await fresh()
    const rows = [['', '地区', '一季度', '二季度']]
    for (let r = 2; r <= 5; r += 1) rows.push([`n${r}`, '东', r * 10, r * 7])
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: rows } },
      { command: 'image.insert', params: { at: 'A1', base64: PNG, mime: 'image/png', placement: 'cell' } },
    ])
    await select('B2')
    await kit.getByTestId('ribbon-tab-home').click()
    await kit.getByTestId('sheet-format-table').click()
    await kit.getByTestId('sheet-table-gallery').waitFor()
    await kit.getByTestId('sheet-table-TableStyleMedium2').click()
    await kit.getByTestId('sheet-fat').waitFor()
    await kit.getByTestId('sheet-fat-table').check()
    await kit.getByTestId('sheet-fat-ok').click()
    await kit.getByTestId('sheet-table-pictures').waitFor()
    step('TS-C76 提示标题行不能有图片', (await kit.getByTestId('sheet-table-pictures-message').textContent()).includes('标题行不能包含图片'))
    await kit.getByTestId('sheet-table-pictures-ok').click()
    await settle(400)
    const tables = (await h.workbook()).sheets[0].tables ?? []
    step('TS-C76 确认后清除图片，标题行为「列1」', tables.length === 1 && (await cellOf('A1')).picture == null && (await val('A1')) === '列1', JSON.stringify([tables[0]?.range, await val('A1')]))
  })

  if (errors.length) step('no page errors', false, errors.slice(0, 3).join(' | '))
} catch (error) {
  await h.finish(error)
}
await h.finish()

/** Whether row `row` (from 1) of the active sheet is hidden. */
async function rowHidden(row) {
  const inspected = await exec('file.inspect')
  const active = (await details()).sheet
  const sheet = inspected.sheets?.find((s) => s.name === active) ?? inspected.sheets?.[0]
  return (sheet?.hiddenRows ?? []).includes(row)
}
