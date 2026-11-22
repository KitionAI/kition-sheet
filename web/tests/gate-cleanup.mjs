// The final cleanup round (module M10), as WPS for Mac has it, in a real browser through the
// development host: node tests/gate-cleanup.mjs [chromium|webkit]
// Dragging the selection's border (move, Cmd/Ctrl copy, Shift insert, whole rows, the replace
// question, undo), a table's and a range's filter buttons (values, search, conditions, sort,
// clear), data validation in the editor (a list's arrow and entries, the input message, the
// Stop/Warning/Information messages, the Data Validation dialog), a new sheet's WPS sizes, the
// engine's messages in Chinese, and `楷体` (KaiTi) drawn in a Kai face. Each step names its case (CLN-X1 …);
// `GATE_ONLY=X3` runs one.
import { startGate } from './gate-harness.mjs'

const h = await startGate({ prefix: 'CLN-' })
/** The grid's pitch as the engine lays it out (the harness reads it when a workbook opens). */
const G = h.grid
const { page, kit, step, section, until, same, ai, exec, details, cellOf, region, clickCell, build, numbered, saveAndReopen, openEditor, errors } = h

const settle = (ms = 250) => page.waitForTimeout(ms)
const COLS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'
const colIndex = (ref) => COLS.indexOf(ref.replace(/\d+$/, ''))
const rowIndex = (ref) => Number(ref.replace(/^[A-Z]+/, '')) - 1
const mod = process.platform === 'darwin' ? 'Meta' : 'Control'
const val = async (ref) => (await cellOf(ref)).value ?? null
const input = async (ref) => {
  const cell = await cellOf(ref)
  return cell.formula ?? cell.input ?? ''
}
/** A cell's rectangle on the page (100%, the sheet at its top-left, default sizes). */
const rect = async (ref) => {
  const box = await region()
  const x = box.x + 46 + colIndex(ref) * G.cw
  const y = box.y + 21 + rowIndex(ref) * G.rh
  return { x, y, w: G.cw, h: G.rh }
}
/** A point on a cell's bottom border, just inside it: where a drag of the selection grabs it. */
const bottomBorder = async (ref) => {
  const r = await rect(ref)
  return { x: r.x + r.w / 2, y: r.y + r.h - 1 }
}
const cursor = () => kit.locator('.kui-canvas-surface').evaluate((element) => element.style.cursor)
/** Drags from `from` by (`dx`, `dy`) page pixels, holding `keys` (Shift, Meta…) throughout. */
async function dragBy(from, dx, dy, keys = []) {
  for (const key of keys) await page.keyboard.down(key)
  await page.mouse.move(from.x, from.y)
  await page.mouse.down()
  await page.mouse.move(from.x + dx / 3, from.y + dy / 3, { steps: 3 })
  await page.mouse.move(from.x + dx, from.y + dy, { steps: 6 })
  await settle(120)
  await page.mouse.up()
  for (const key of keys) await page.keyboard.up(key)
  await settle(300)
}
/** A fresh sheet at the top-left (no filter, nothing selected but A1). */
async function fresh() {
  await build(numbered(1), { fill: false })
  await settle(200)
}
/** Whether row `row` (from 1) of the active sheet is hidden. */
async function rowHidden(row) {
  const inspected = await exec('document.inspect')
  const active = (await details()).sheet
  const sheet = inspected.sheets?.find((s) => s.name === active) ?? inspected.sheets?.[0]
  return (sheet?.hiddenRows ?? []).includes(row)
}
const hiddenRows = async (rows) => {
  const out = []
  for (const row of rows) if (await rowHidden(row)) out.push(row)
  return out
}
const toasts = () => kit.locator('.kui-toast').allTextContents().then((all) => all.join(' | '))
const menu = () => kit.getByTestId('sheet-filter-menu')
/** Presses the filter button of a header cell (at the cell's right edge). */
async function filterButton(ref) {
  const r = await rect(ref)
  await page.mouse.click(r.x + r.w - 9, r.y + r.h - 8)
  await menu().waitFor({ timeout: 5000 })
  await until(async () => (await kit.getByTestId('sheet-filter-all').count()) === 1, 30)
}
/** A hash of what the grid shows in a cell. */
const ink = (ref) =>
  kit.getByTestId('sheet-canvas-region').evaluate(
    (element, [x, y, w, hgt]) => {
      const out = document.createElement('canvas')
      out.width = w
      out.height = hgt
      const context = out.getContext('2d')
      const box = element.getBoundingClientRect()
      for (const canvas of element.querySelectorAll('canvas')) {
        const r = canvas.getBoundingClientRect()
        if (!r.width) continue
        const ratio = canvas.width / r.width
        context.drawImage(canvas, Math.round((box.left + x - r.left) * ratio), Math.round((box.top + y - r.top) * ratio), Math.round(w * ratio), Math.round(hgt * ratio), 0, 0, w, hgt)
      }
      const data = context.getImageData(0, 0, w, hgt).data
      let dark = 0
      let hash = 0
      for (let i = 0; i < data.length; i += 4) {
        if (data[i] < 128) dark += 1
        hash = (hash * 31 + (data[i] < 128 ? 1 : 0)) | 0
      }
      return { dark, hash }
    },
    [46 + colIndex(ref) * G.cw + 2, 21 + rowIndex(ref) * G.rh + 2, G.cw - 4, G.rh * 2 - 4],
  )

try {
  await openEditor('')

  // ———————————————— Dragging the selection border (WPS moves the cells) ————————————————

  await section('CLN-X1 拖动选区边框移动单元格，公式引用随之更新，可撤销', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '5' } },
      { command: 'cell.set', params: { cell: 'B1', input: '=A1*2' } },
    ])
    await clickCell(0, 0)
    const from = await bottomBorder('A1')
    await page.mouse.move(from.x, from.y)
    await until(async () => (await cursor()) === 'move', 20)
    step('CLN-X1 指针移到选区边框上显示移动指针', (await cursor()) === 'move', await cursor())
    const inside = await rect('A1')
    await page.mouse.move(inside.x + inside.w / 2, inside.y + inside.h / 2)
    await until(async () => (await cursor()) !== 'move', 20)
    step('CLN-X1 单元格内部为普通指针', (await cursor()) !== 'move', await cursor())
    await dragBy(from, 2 * G.cw, 3 * G.rh)
    await until(async () => (await val('C4')) === 5)
    step('CLN-X1 A1 移到 C4：值移动、原位置清空、选区落在 C4', (await val('C4')) === 5 && (await val('A1')) == null && (await details()).selection === 'C4', JSON.stringify([await val('C4'), await val('A1'), (await details()).selection]))
    step('CLN-X1 引用 A1 的公式随之改为 C4', (await input('B1')) === '=C4*2' && (await val('B1')) === 10, await input('B1'))
    await page.keyboard.press(`${mod}+z`)
    await until(async () => (await val('A1')) === 5)
    step('CLN-X1 撤销一步恢复原状', (await val('A1')) === 5 && (await val('C4')) == null && (await input('B1')) === '=A1*2', await input('B1'))
  })

  await section('CLN-X2 拖到有内容的单元格先询问「是否替换目标单元格内容？」', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '新' } },
      { command: 'cell.set', params: { cell: 'A3', input: '旧' } },
    ])
    await clickCell(0, 0)
    await dragBy(await bottomBorder('A1'), 0, 2 * G.rh)
    const dialog = kit.getByTestId('sheet-move-confirm')
    await dialog.waitFor({ timeout: 5000 })
    step('CLN-X2 弹出替换确认，文字为「是否替换目标单元格内容？」', (await kit.getByTestId('sheet-move-confirm-message').textContent()).includes('是否替换目标单元格内容'))
    await kit.getByTestId('sheet-move-confirm-cancel').click()
    await settle()
    step('CLN-X2 取消：两格不变', (await val('A1')) === '新' && (await val('A3')) === '旧')
    await clickCell(0, 0)
    await dragBy(await bottomBorder('A1'), 0, 2 * G.rh)
    await dialog.waitFor({ timeout: 5000 })
    await kit.getByTestId('sheet-move-confirm-ok').click()
    await until(async () => (await val('A3')) === '新')
    step('CLN-X2 确定：A3 被替换为「新」，A1 清空', (await val('A3')) === '新' && (await val('A1')) == null)
  })

  await section('CLN-X3 按住 Cmd/Ctrl 拖动为复制', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'B2', input: '复制我' } }])
    await clickCell(1, 1)
    const from = await bottomBorder('B2')
    await page.keyboard.down(mod)
    await page.mouse.move(from.x, from.y)
    await page.mouse.move(from.x + 1, from.y)
    await until(async () => (await cursor()) === 'copy', 20)
    const shownCursor = await cursor()
    await page.keyboard.up(mod)
    step('CLN-X3 按住 Cmd/Ctrl 时边框上为复制指针', shownCursor === 'copy', shownCursor)
    await dragBy(from, 2 * G.cw, 0, [mod])
    await until(async () => (await val('D2')) === '复制我')
    step('CLN-X3 复制到 D2，B2 保留', (await val('D2')) === '复制我' && (await val('B2')) === '复制我')
  })

  await section('CLN-X4 按住 Shift 拖动为插入（其他单元格让位）', async () => {
    await fresh()
    await ai([{ command: 'range.setValues', params: { range: 'A1', values: [['a'], ['b'], ['c'], ['d']] } }])
    await clickCell(0, 0)
    const from = await bottomBorder('A1')
    // Just above the line between rows 3 and 4.
    const target = (await rect('A3')).y + G.rh - 3
    await dragBy(from, 0, target - from.y, ['Shift'])
    await until(async () => (await val('A3')) === 'a')
    const column = [await val('A1'), await val('A2'), await val('A3'), await val('A4')]
    step('CLN-X4 a 插到 c 与 d 之间：b、c、a、d', same(column, ['b', 'c', 'a', 'd']), JSON.stringify(column))
  })

  await section('CLN-X5 拖动整行（行号选中）移动整行，Shift 插入', async () => {
    await fresh()
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: [['r1', 1], ['r2', 2], ['r3', 3], ['r4', 4]] } },
      { command: 'cell.set', params: { cell: 'D1', input: '=B2' } },
    ])
    const box = await region()
    await page.mouse.click(box.x + 23, box.y + 21 + G.rh + G.rh / 2)
    await until(async () => (await details()).selection === '2:2')
    step('CLN-X5 点行号选中第 2 行', (await details()).selection === '2:2', (await details()).selection)
    // The row's top border, among the cells.
    const from = { x: box.x + 46 + 2 * G.cw + 10, y: box.y + 21 + G.rh + 1 }
    const below = box.y + 21 + 4 * G.rh - 3
    await dragBy(from, 0, below - from.y, ['Shift'])
    await until(async () => (await val('A4')) === 'r2')
    const column = [await val('A1'), await val('A2'), await val('A3'), await val('A4')]
    step('CLN-X5 第 2 行插到第 4 行之后：r1、r3、r4、r2，整行选中', same(column, ['r1', 'r3', 'r4', 'r2']) && (await details()).selection === '4:4', JSON.stringify([column, (await details()).selection]))
    step('CLN-X5 引用该行的公式跟随（=B4）', (await input('D1')) === '=B4', await input('D1'))
  })

  await section('CLN-X6 合并单元格、批注随拖动一起移动', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '合并' } },
      { command: 'home.mergeCells', params: { range: 'A1:B1' } },
      { command: 'review.newNote', params: { cell: 'A1', text: '备注' } },
      { command: 'selection.set', params: { range: 'A1:B1' } },
    ])
    await h.goTo('A1:B1')
    await settle()
    await dragBy(await bottomBorder('A1'), 0, 3 * G.rh)
    await until(async () => (await val('A4')) === '合并')
    const moved = await cellOf('A4')
    step('CLN-X6 合并区域移到 A4:B4，原合并取消', moved.merge === 'A4:B4' && (await cellOf('A1')).merge == null, JSON.stringify([moved.merge, (await cellOf('A1')).merge]))
    step('CLN-X6 批注随之移动', JSON.stringify(moved).includes('备注') && !JSON.stringify(await cellOf('A1')).includes('备注'))
  })

  // ———————— Drag cases left over from M7 and M8 (table styles, inserting content) ————————

  const tableAt = async () => {
    const active = (await details()).sheet
    return ((await h.workbook()).sheets.find((sheet) => sheet.name === active)?.tables ?? []).map((t) => t.range)
  }

  await section('TS-C65 拖拽单元格、行到表格右/下侧：不扩充表格，内容按插入落地', async () => {
    await fresh()
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: [['名称', '数量'], ['甲', 1], ['乙', 2]] } },
      { command: 'insert.table', params: { range: 'A1:B3' } },
      { command: 'cell.set', params: { cell: 'E1', input: '外' } },
      { command: 'cell.set', params: { cell: 'A6', input: '行' } },
      { command: 'selection.set', params: { range: 'E1' } },
    ])
    await h.goTo('E1')
    await settle()
    await dragBy(await bottomBorder('E1'), -2 * G.cw, 0)
    await until(async () => (await val('C1')) === '外')
    step('TS-C65 单元格拖到表格右侧 C1：内容落地，表格仍为 A1:B3', (await val('C1')) === '外' && same(await tableAt(), ['A1:B3']), JSON.stringify(await tableAt()))
    const box = await region()
    await page.mouse.click(box.x + 23, box.y + 21 + 5 * G.rh + G.rh / 2)
    await until(async () => (await details()).selection === '6:6')
    const from = { x: box.x + 46 + 3 * G.cw, y: box.y + 21 + 5 * G.rh + 1 }
    const target = box.y + 21 + 3 * G.rh + 2
    await dragBy(from, 0, target - from.y, ['Shift'])
    await until(async () => (await val('A4')) === '行')
    step('TS-C65 行拖插到表格下方第 4 行：不扩充表格，样式范围不变', (await val('A4')) === '行' && same(await tableAt(), ['A1:B3']), JSON.stringify([await val('A4'), await tableAt()]))
  })

  await section('TS-C66 拖拽列替换表格右侧有内容的列：询问后替换，表格不扩充', async () => {
    await fresh()
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: [['名称', '数量', '旁'], ['甲', 1, 'x'], ['乙', 2, 'y']] } },
      { command: 'insert.table', params: { range: 'A1:B3' } },
      { command: 'range.setValues', params: { range: 'E1', values: [['新'], ['n1'], ['n2']] } },
    ])
    const box = await region()
    await page.mouse.click(box.x + 46 + 4 * G.cw + G.cw / 2, box.y + 10)
    await until(async () => (await details()).selection === 'E:E')
    const from = { x: box.x + 46 + 4 * G.cw + 1, y: box.y + 21 + 2 * G.rh }
    await dragBy(from, -2 * G.cw, 0)
    const dialog = kit.getByTestId('sheet-move-confirm')
    await dialog.waitFor({ timeout: 5000 })
    await kit.getByTestId('sheet-move-confirm-ok').click()
    await until(async () => (await val('C2')) === 'n1')
    step('TS-C66 C 列被替换为拖入的内容，表格仍为 A1:B3', (await val('C1')) === '新' && (await val('C2')) === 'n1' && (await val('E1')) == null && same(await tableAt(), ['A1:B3']), JSON.stringify([await val('C1'), await tableAt()]))
  })

  await section('TS-C102 表格内筛选隐藏行：交替底纹按可见行连续计算，清除后恢复', async () => {
    await fresh()
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: [['名称', '数量'], ['甲', 1], ['乙', 2], ['丙', 3], ['丁', 4], ['戊', 5]] } },
      { command: 'insert.table', params: { range: 'A1:B6', style: 'TableStyleMedium2' } },
      { command: 'selection.set', params: { range: 'D1' } },
    ])
    await h.goTo('D1')
    await settle(300)
    await filterButton('A1')
    await kit.getByTestId('sheet-filter-item-乙').uncheck()
    await kit.getByTestId('sheet-filter-ok').click()
    await until(async () => same(await hiddenRows([2, 3, 4, 5, 6]), [3]))
    const shade = async (shownIndex) => JSON.stringify(await h.pixel(46 + G.cw - 20, 21 + shownIndex * G.rh + 3))
    // Shown: header, 甲 (row 2), 丙 (row 4, hidden row 3 skipped), 丁 (row 5).
    const [first, second, third] = [await shade(1), await shade(2), await shade(3)]
    step('TS-C102 筛掉第 3 行后，第 2、4 行底纹交替，第 2、5 行相同', same(await hiddenRows([2, 3, 4, 5, 6]), [3]) && first !== second && first === third, JSON.stringify([first, second, third]))
    await filterButton('A1')
    await kit.getByTestId('sheet-filter-clear').click()
    await until(async () => (await hiddenRows([2, 3, 4, 5, 6])).length === 0)
    step('TS-C102 清除筛选后全部恢复，表格样式不变', (await hiddenRows([2, 3, 4, 5, 6])).length === 0 && same(await tableAt(), ['A1:B6']))
  })

  await section('INSC-C282 拖拽勾选的复选框单元格到 C3', async () => {
    await fresh()
    await ai([
      { command: 'insert.checkbox', params: { range: 'A1' } },
      { command: 'cell.set', params: { cell: 'A1', input: 'TRUE' } },
      { command: 'selection.set', params: { range: 'A1' } },
    ])
    await h.goTo('A1')
    await settle()
    await dragBy(await bottomBorder('A1'), 2 * G.cw, 2 * G.rh)
    await until(async () => (await val('C3')) === true)
    const c3 = await cellOf('C3')
    const a1 = await cellOf('A1')
    step('INSC-C282 A1 回到空白不呈方框，C3 呈勾选的方框', c3.style?.checkbox === true && c3.value === true && a1.value == null && a1.style?.checkbox !== true, JSON.stringify([c3.value, c3.style?.checkbox, a1.value, a1.style?.checkbox]))
  })

  await section('INSC-C283 删除复选框后拖拽数值单元格覆盖', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '1' } },
      { command: 'insert.checkbox', params: { range: 'B1' } },
      { command: 'edit.clearContents', params: { range: 'B1' } },
      { command: 'selection.set', params: { range: 'A1' } },
    ])
    await h.goTo('A1')
    await settle()
    await dragBy(await bottomBorder('A1'), G.cw, 0)
    await until(async () => (await val('B1')) === 1)
    const b1 = await cellOf('B1')
    step('INSC-C283 B1 呈数值 1、不呈方框，A1 回到空白', b1.value === 1 && b1.style?.checkbox !== true && (await val('A1')) == null, JSON.stringify([b1.value, b1.style?.checkbox, await val('A1')]))
  })

  // ———————————————— Filter buttons (tables and plain ranges) ————————————————

  const fruit = async (asTable) => {
    await fresh()
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: [['名称', '数量'], ['苹果', 3], ['香蕉', 8], ['苹果', 5], ['橙子', 1], [null, 9]] } },
      asTable ? { command: 'insert.table', params: { range: 'A1:B6' } } : { command: 'data.filter', params: { range: 'A1:B6' } },
      { command: 'selection.set', params: { range: 'D1' } },
    ])
    await h.goTo('D1')
    await settle(300)
  }

  await section('CLN-X7 表格表头筛选按钮：列出值与个数，取消勾选后确定即筛选', async () => {
    await fruit(true)
    await filterButton('A1')
    const items = await kit.locator('[data-testid^="sheet-filter-item-"]').evaluateAll((all) => all.map((e) => e.getAttribute('data-testid').slice('sheet-filter-item-'.length)))
    step('CLN-X7 下拉列出 橙子、苹果、香蕉 与（空白），带个数', same(items, ['橙子', '苹果', '香蕉']) && (await kit.getByTestId('sheet-filter-blanks').count()) === 1 && (await menu().textContent()).includes('(2)'), JSON.stringify(items))
    const menuText = await menu().textContent()
    step('CLN-X7 菜单有升序、降序、内容筛选、文本筛选、清空条件', ['升序', '降序', '内容筛选', '文本筛选', '清空条件'].every((label) => menuText.includes(label)), menuText)
    step('CLN-X7 未选单元格（D1 不变）', (await details()).selection === 'D1')
    await kit.getByTestId('sheet-filter-item-香蕉').uncheck()
    await kit.getByTestId('sheet-filter-blanks').uncheck()
    await kit.getByTestId('sheet-filter-ok').click()
    await until(async () => same(await hiddenRows([2, 3, 4, 5, 6]), [3, 6]))
    step('CLN-X7 香蕉与空白行被隐藏（第 3、6 行）', same(await hiddenRows([2, 3, 4, 5, 6]), [3, 6]), JSON.stringify(await hiddenRows([2, 3, 4, 5, 6])))
    await filterButton('A1')
    step('CLN-X7 再次打开：香蕉未勾选', !(await kit.getByTestId('sheet-filter-item-香蕉').isChecked()) && (await kit.getByTestId('sheet-filter-item-苹果').isChecked()))
    await kit.getByTestId('sheet-filter-search').fill('苹')
    step('CLN-X7 搜索「苹」只列苹果', (await kit.locator('[data-testid^="sheet-filter-item-"]').count()) === 1)
    await kit.getByTestId('sheet-filter-clear').click()
    await until(async () => (await hiddenRows([2, 3, 4, 5, 6])).length === 0)
    step('CLN-X7 清空条件：全部行显示', (await hiddenRows([2, 3, 4, 5, 6])).length === 0)
  })

  await section('CLN-X8 表格筛选：数字条件、排序、Esc 关闭', async () => {
    await fruit(true)
    await filterButton('B1')
    step('CLN-X8 数字列显示「数字筛选」', (await kit.getByTestId('sheet-filter-conditions').textContent()).includes('数字筛选'))
    await kit.getByTestId('sheet-filter-conditions').click()
    await kit.getByTestId('sheet-filter-cond-filterGreater').click()
    await kit.getByTestId('sheet-filter-custom').waitFor()
    await kit.getByTestId('sheet-filter-custom-value').fill('4')
    await kit.getByTestId('sheet-filter-custom-ok').click()
    await until(async () => same(await hiddenRows([2, 3, 4, 5, 6]), [2, 5]))
    step('CLN-X8 大于 4：3 和 1 所在行隐藏', same(await hiddenRows([2, 3, 4, 5, 6]), [2, 5]), JSON.stringify(await hiddenRows([2, 3, 4, 5, 6])))
    await filterButton('B1')
    await kit.getByTestId('sheet-filter-desc').click()
    await until(async () => (await val('B2')) === 9)
    const numbers = [await val('B2'), await val('B3'), await val('B4'), await val('B5'), await val('B6')]
    step('CLN-X8 降序：9 8 5 3 1，筛选仍按条件隐藏 3 和 1', same(numbers, [9, 8, 5, 3, 1]) && same(await hiddenRows([2, 3, 4, 5, 6]), [5, 6]), JSON.stringify([numbers, await hiddenRows([2, 3, 4, 5, 6])]))
    await filterButton('A1')
    await page.keyboard.press('Escape')
    await until(async () => (await menu().count()) === 0, 20)
    step('CLN-X8 Esc 关闭筛选菜单', (await menu().count()) === 0)
  })

  await section('CLN-X9 普通区域的自动筛选按钮同样可用（文本条件、清除）', async () => {
    await fruit(false)
    await filterButton('A1')
    await kit.getByTestId('sheet-filter-conditions').click()
    await kit.getByTestId('sheet-filter-cond-filterContains').click()
    await kit.getByTestId('sheet-filter-custom').waitFor()
    await kit.getByTestId('sheet-filter-custom-value').fill('果')
    await kit.getByTestId('sheet-filter-custom-ok').click()
    await until(async () => same(await hiddenRows([2, 3, 4, 5, 6]), [3, 5, 6]))
    step('CLN-X9 包含「果」：只剩两行苹果', same(await hiddenRows([2, 3, 4, 5, 6]), [3, 5, 6]), JSON.stringify(await hiddenRows([2, 3, 4, 5, 6])))
    await filterButton('A1')
    await kit.getByTestId('sheet-filter-asc').click()
    await settle(400)
    step('CLN-X9 升序后首行为 橙子 且被筛掉', (await val('A2')) === '橙子' && (await rowHidden(2)))
    await filterButton('A1')
    await kit.getByTestId('sheet-filter-clear').click()
    await until(async () => (await hiddenRows([2, 3, 4, 5, 6])).length === 0)
    step('CLN-X9 清空条件后全部显示', (await hiddenRows([2, 3, 4, 5, 6])).length === 0)
  })

  // ———————————————— 数据验证 ————————————————

  await section('CLN-X10 序列验证：单元格右侧下拉箭头、选取、Alt+↓、输入信息', async () => {
    await fresh()
    await ai([{ command: 'data.validation', params: { range: 'B2', type: 'list', formula1: '"甲,乙,丙"', inputTitle: '提示', inputMessage: '请选择甲乙丙' } }])
    await clickCell(1, 1)
    await kit.getByTestId('sheet-validation-arrow').waitFor({ timeout: 5000 })
    step('CLN-X10 选中 B2 显示下拉箭头', (await kit.getByTestId('sheet-validation-arrow').count()) === 1)
    const prompt = kit.getByTestId('sheet-validation-prompt')
    step('CLN-X10 显示输入信息（标题与内容）', (await prompt.count()) === 1 && (await prompt.textContent()).includes('请选择甲乙丙') && (await prompt.textContent()).includes('提示'))
    await kit.getByTestId('sheet-validation-arrow').click()
    await kit.getByTestId('sheet-validation-list').waitFor()
    const options = await kit.locator('[data-testid^="sheet-validation-option-"]').allTextContents()
    step('CLN-X10 下拉列出 甲、乙、丙', same(options, ['甲', '乙', '丙']), JSON.stringify(options))
    await kit.getByTestId('sheet-validation-option-乙').click()
    await until(async () => (await val('B2')) === '乙')
    step('CLN-X10 选取「乙」写入 B2，列表关闭', (await val('B2')) === '乙' && (await kit.getByTestId('sheet-validation-list').count()) === 0)
    await page.keyboard.press('Alt+ArrowDown')
    await kit.getByTestId('sheet-validation-list').waitFor({ timeout: 3000 }).catch(() => undefined)
    step('CLN-X10 Alt+↓ 打开下拉列表', (await kit.getByTestId('sheet-validation-list').count()) === 1)
    await page.keyboard.press('Escape')
    await until(async () => (await kit.getByTestId('sheet-validation-list').count()) === 0, 20)
    step('CLN-X10 Esc 关闭列表', (await kit.getByTestId('sheet-validation-list').count()) === 0)
    await clickCell(3, 3)
    step('CLN-X10 选中其他单元格：箭头与输入信息消失', (await kit.getByTestId('sheet-validation-arrow').count()) === 0 && (await prompt.count()) === 0)
  })

  await section('CLN-X11 出错警告：停止（重试/取消）、警告（是/否/取消）、信息（确定/取消）', async () => {
    await fresh()
    await ai([
      { command: 'data.validation', params: { range: 'A1', type: 'whole', operator: 'between', formula1: '1', formula2: '10', errorTitle: '输入有误', errorMessage: '请输入 1 到 10 的整数' } },
      { command: 'data.validation', params: { range: 'A2', type: 'whole', operator: 'between', formula1: '1', formula2: '10', errorStyle: 'warning' } },
      { command: 'data.validation', params: { range: 'A3', type: 'whole', operator: 'between', formula1: '1', formula2: '10', errorStyle: 'information' } },
    ])
    await clickCell(0, 0)
    await page.keyboard.type('99')
    await page.keyboard.press('Enter')
    const alert = kit.getByTestId('sheet-validation-alert')
    await alert.waitFor({ timeout: 5000 })
    step('CLN-X11 停止：标题与信息为规则所设，按钮为 重试、取消', (await alert.textContent()).includes('输入有误') && (await kit.getByTestId('sheet-validation-message').textContent()).includes('请输入 1 到 10 的整数') && (await kit.getByTestId('sheet-validation-retry').count()) === 1, await alert.textContent())
    step('CLN-X11 不再另弹英文/重复的提示条', !(await toasts()).includes('请输入 1 到 10 的整数'), await toasts())
    await kit.getByTestId('sheet-validation-retry').click()
    await settle()
    step('CLN-X11 重试：仍在编辑 A1，值未写入', (await details()).editing === true && (await details()).active === 'A1' && (await val('A1')) == null)
    await page.keyboard.type('7')
    await page.keyboard.press('Enter')
    await until(async () => (await val('A1')) === 7)
    step('CLN-X11 改为 7 后写入并下移', (await val('A1')) === 7 && (await details()).active === 'A2')
    await page.keyboard.type('50')
    await page.keyboard.press('Enter')
    await alert.waitFor({ timeout: 5000 })
    step('CLN-X11 警告：询问「是否继续？」，按钮 是、否、取消', (await alert.textContent()).includes('是否继续') && (await kit.getByTestId('sheet-validation-yes').count()) === 1 && (await kit.getByTestId('sheet-validation-no').count()) === 1)
    await kit.getByTestId('sheet-validation-yes').click()
    await until(async () => (await val('A2')) === 50)
    step('CLN-X11 警告选「是」：50 写入并下移到 A3', (await val('A2')) === 50 && (await details()).active === 'A3')
    await page.keyboard.type('60')
    await page.keyboard.press('Enter')
    await alert.waitFor({ timeout: 5000 })
    step('CLN-X11 信息：按钮 确定、取消', (await kit.getByTestId('sheet-validation-ok').count()) === 1 && (await kit.getByTestId('sheet-validation-cancel').count()) === 1)
    await kit.getByTestId('sheet-validation-cancel').click()
    await settle()
    step('CLN-X11 信息选「取消」：不写入、结束编辑', (await val('A3')) == null && (await details()).editing === false)
  })

  await section('CLN-X12 数据验证对话框：设置序列与来源、出错警告样式、再次打开回显、全部清除', async () => {
    await fresh()
    await clickCell(2, 2)
    await kit.getByTestId('ribbon-tab-data').click()
    await kit.getByTestId('sheet-data-validation-button').click()
    const dialog = kit.getByTestId('sheet-validation-dialog')
    await dialog.waitFor()
    const tabs = await kit.locator('[data-testid^="sheet-validation-tab-"]').allTextContents()
    step('CLN-X12 对话框有 设置、输入信息、出错警告 三页', same(tabs, ['设置', '输入信息', '出错警告']), JSON.stringify(tabs))
    await kit.getByTestId('sheet-validation-type').selectOption('list')
    await kit.getByTestId('sheet-validation-source').fill('东,西,南,北')
    await kit.getByTestId('sheet-validation-tab-error').click()
    await kit.getByTestId('sheet-validation-style').selectOption('warning')
    await kit.getByTestId('sheet-validation-error-message').fill('只能选方向')
    await kit.getByTestId('sheet-validation-ok').click()
    await until(async () => same((await details()).validation?.list, ['东', '西', '南', '北']))
    const rule = (await details()).validation?.rule
    step('CLN-X12 确定后 C3 有序列验证与警告样式', same((await details()).validation?.list, ['东', '西', '南', '北']) && rule?.errorStyle === 'warning' && rule?.errorMessage === '只能选方向', JSON.stringify(rule))
    await kit.getByTestId('sheet-data-validation-button').click()
    await dialog.waitFor()
    step('CLN-X12 再次打开：允许为序列、来源回显', (await kit.getByTestId('sheet-validation-type').inputValue()) === 'list' && (await kit.getByTestId('sheet-validation-source').inputValue()) === '东,西,南,北')
    await kit.getByTestId('sheet-validation-clear-all').click()
    await kit.getByTestId('sheet-validation-ok').click()
    await until(async () => (await details()).validation == null)
    step('CLN-X12 全部清除后确定：验证移除', (await details()).validation == null)
  })

  // ———————————————— A new workbook's default column widths and row heights (WPS) ————————————————

  await section('CLN-X13 新建工作表列宽 69 px、行高 19 px（WPS for Mac 新建工作簿：51.75 磅 × 14.25 磅）', async () => {
    await fresh()
    const size = (await details()).defaultSize
    step('CLN-X13 默认列宽 69、行高 19', same(size, [69, 19]), JSON.stringify(size))
    const line = await h.pixel(46 + 69, 30)
    const inside = await h.pixel(46 + 64, 30)
    step('CLN-X13 网格线在 A、B 列之间的 69 px 处', line[0] < 240 && inside[0] > 245, JSON.stringify([line, inside]))
    await saveAndReopen()
    step('CLN-X13 保存后重新打开仍为 69 × 19', same((await details()).defaultSize, [69, 19]), JSON.stringify((await details()).defaultSize))
  })

  // ———————————————— Messages in Chinese ————————————————

  await section('CLN-X14 引擎的提示以中文显示（表格重叠）', async () => {
    await fresh()
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: [['甲', '乙'], [1, 2]] } },
      { command: 'insert.table', params: { range: 'A1:B2' } },
    ])
    await clickCell(0, 1)
    await kit.getByTestId('ribbon-tab-insert').click()
    await kit.getByTestId('sheet-cmd-insert.table').click()
    await until(async () => (await toasts()).includes('表格不能与另一个表格重叠'), 40)
    step('CLN-X14 提示「表格不能与另一个表格重叠。」而非英文', (await toasts()).includes('表格不能与另一个表格重叠') && !(await toasts()).includes("can't overlap"), await toasts())
  })

  await section('CLN-X18 导出为 CSV UTF-8（带 BOM）：Excel for Windows 能正确读中文（EXP-C69）', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '中文' } }])
    const before = new Set(await page.evaluate(() => [...window.devHost.files.keys()]))
    await kit.getByTestId('ribbon-tab-file').click()
    await kit.getByTestId('sheet-export-csv-bom').click()
    await until(async () => (await page.evaluate(() => [...window.devHost.files.keys()])).some((n) => !before.has(n) && n.endsWith('.csv')), 100)
    const name = (await page.evaluate(() => [...window.devHost.files.keys()])).find((n) => !before.has(n) && n.endsWith('.csv'))
    const head = name ? await page.evaluate((file) => [...new Uint8Array(window.devHost.files.get(file).bytes).slice(0, 9)], name) : []
    step('CLN-X18 文件以 UTF-8 BOM（EF BB BF）开头，后接「中文」', same(head.slice(0, 3), [0xef, 0xbb, 0xbf]) && new TextDecoder().decode(new Uint8Array(head.slice(3))) === '中文', JSON.stringify([name, head]))
    await kit.getByTestId('ribbon-tab-home').click()
  })

  // ———————————————— Font: `楷体` (KaiTi) ————————————————

  await section('CLN-X15 楷体以开源楷书字形（Ma Shan Zheng）绘制，与黑体不同', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '永和九年' } },
      { command: 'cell.set', params: { cell: 'C1', input: '永和九年' } },
      { command: 'home.fontName', params: { range: 'A1', name: '楷体' } },
      { command: 'home.fontName', params: { range: 'C1', name: '黑体' } },
    ])
    await settle(500)
    const kai = await ink('A1')
    const hei = await ink('C1')
    step('CLN-X15 两格都有字，楷体与黑体字形不同', kai.dark > 20 && hei.dark > 20 && kai.hash !== hei.hash, JSON.stringify([kai, hei]))
  })

  // ———————————————— Circular references (WPS warns once, when one is made) ————————————————

  await section('CLN-X17 输入造成新的循环引用：弹出循环引用警告，单元格显示 0', async () => {
    await fresh()
    await clickCell(0, 2)
    await page.keyboard.type('=A3*2')
    await page.keyboard.press('Enter')
    const warning = kit.getByTestId('sheet-circular')
    await warning.waitFor({ timeout: 5000 }).catch(() => undefined)
    step('CLN-X17 弹出「循环引用警告」，只有确定', (await warning.count()) === 1 && (await warning.textContent()).includes('循环引用') && (await kit.getByTestId('sheet-circular-cancel').count()) === 0, (await warning.count()) ? await warning.textContent() : 'none')
    if (await warning.count()) await kit.getByTestId('sheet-circular-ok').click()
    step('CLN-X17 A3 显示 0', (await val('A3')) === 0, JSON.stringify(await val('A3')))
    await clickCell(1, 0)
    await page.keyboard.type('5')
    await page.keyboard.press('Enter')
    await settle(300)
    step('CLN-X17 其他输入不再提示', (await warning.count()) === 0)
  })

  // ———————————————— The message when a file fails to open (in Chinese) ————————————————

  await section('CLN-X16 打开空文件：编辑器与宿主都显示中文原因', async () => {
    await page.evaluate(async () => {
      document.getElementById('status').textContent = ''
      await window.devHost.open('空白.xlsx', new ArrayBuffer(0)).catch(() => undefined)
    })
    await until(async () => (await page.locator('#status').textContent()).includes('could not open'), 100)
    const status = await page.locator('#status').textContent()
    const message = kit.locator('.kui-canvas-region .kui-message')
    await message.waitFor({ timeout: 10000 }).catch(() => undefined)
    const shown = (await message.count()) ? await message.textContent() : ''
    step('CLN-X16 宿主收到的原因为中文「是空文件」', status.includes('是空文件'), status)
    step('CLN-X16 编辑器内显示的原因为中文', shown.includes('是空文件（0 字节）'), shown)
  })

  if (errors.length) step('no page errors', false, errors.slice(0, 3).join(' | '))
} catch (error) {
  await h.finish(error)
}
await h.finish()
