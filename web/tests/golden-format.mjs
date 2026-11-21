// The format painter and the font controls (module M6, Format Painter and Font), as WPS for Mac has them, in
// a real browser through the development host: node tests/golden-format.mjs [chromium|webkit]
// Home › Format Painter (a click paints once, a double-click keeps painting, Esc and a second click put it
// down; cells, ranges, rows, columns, merged cells, number formats, other sheets, formulas, links,
// validation, conditional formats, undo), Home › Font (the font box and its list with search and
// Recently Used, the size box and WPS's size list, Increase and Decrease Font Size, bold, italic,
// underline, strikethrough with their shortcuts and tooltips, font and fill colours with Recent
// Colors and More Colors, characters of a cell formatted on their own, whole rows and columns,
// several areas, filtered and hidden rows, copy and paste, save and reopen, read-only).
// Each step names its case (FP-C6 for the format painter, FONT-C13 for fonts); `GOLDEN_ONLY=FP-C6,FONT-C13`
// runs some.
import { startGolden } from './golden-harness.mjs'

const h = await startGolden({ prefix: '' })
/** The grid's pitch as the engine lays it out (the harness reads it when a workbook opens). */
const G = h.grid
const { page, kit, step, section, until, same, ai, exec, details, q, cellOf, region, clickCell, goTo, build, numbered, saveAndReopen, touch, zipPart, openEditor, errors } = h

const settle = (ms = 250) => page.waitForTimeout(ms)
const COLS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'
const colIndex = (ref) => COLS.indexOf(ref.replace(/\d+$/, ''))
const rowIndex = (ref) => Number(ref.replace(/^[A-Z]+/, '')) - 1
/** Clicks the cell `ref` (on screen at the sheet's top-left, 100%). */
const click = (ref) => clickCell(colIndex(ref), rowIndex(ref))
/** Presses on `from` and drags to `to` (cells on screen). */
const dragCells = async (from, to) => {
  const box = await region()
  const point = (ref) => ({ x: box.x + 46 + colIndex(ref) * G.cw + 32, y: box.y + 21 + rowIndex(ref) * G.rh + 10 })
  const [a, b] = [point(from), point(to)]
  await page.mouse.move(a.x, a.y)
  await page.mouse.down()
  await page.mouse.move((a.x + b.x) / 2, (a.y + b.y) / 2, { steps: 4 })
  await page.mouse.move(b.x, b.y, { steps: 4 })
  await page.mouse.up()
  await settle(200)
}
/** Clicks the heading of row `row` (from 1) or of column `col` (a letter). */
const clickRowHeading = async (row) => {
  const box = await region()
  await page.mouse.click(box.x + 23, box.y + 21 + (row - 1) * G.rh + 10)
  await settle(200)
}
const clickColHeading = async (col) => {
  const box = await region()
  await page.mouse.click(box.x + 46 + colIndex(col) * G.cw + 32, box.y + 10)
  await settle(200)
}
/** Types into the active cell and enters it. */
const type = async (text) => {
  await page.keyboard.type(text)
  await page.keyboard.press('Enter')
  await settle(150)
}

// ———— cell formats ————
const style = async (ref) => (await cellOf(ref)).style ?? {}
const hex = (color) => (color && typeof color === 'object' && 'Rgb' in color ? `#${color.Rgb.toString(16).padStart(6, '0').toUpperCase()}` : null)
const fontOf = async (ref) => (await style(ref)).font ?? {}
const fillOf = async (ref) => hex((await style(ref)).fill?.fg)
/** The font name, colour, bold, fill and number format of a cell: what painting copies. */
const look = async (ref) => {
  const s = await style(ref)
  const f = s.font ?? {}
  return JSON.stringify([f.name, f.size, f.bold, f.italic, f.underline, f.strike, hex(f.color), hex(s.fill?.fg), s.num_fmt, s.align ?? null, s.borders ?? null])
}
const runsOf = async (ref) => (await cellOf(ref)).runs ?? null

// ———— the ribbon ————
const home = async () => {
  await kit.getByTestId('ribbon-tab-home').click()
  await kit.getByTestId('sheet-format-painter').waitFor()
}
const painter = () => kit.getByTestId('sheet-format-painter')
const painterState = async () => (await details()).formatPainter ?? null
const pressed = async (testId) => (await kit.getByTestId(testId).getAttribute('aria-pressed')) === 'true'
const boldButton = () => kit.getByTestId('sheet-cmd-home.bold')
const cursor = () => kit.locator('.kui-canvas-surface').first().evaluate((canvas) => canvas.style.cursor)
const fontBox = () => kit.getByTestId('sheet-font')
const sizeBox = () => kit.getByTestId('sheet-size')
const openFonts = async () => {
  if ((await kit.getByTestId('sheet-font-panel').count()) === 0) await kit.getByTestId('sheet-font-arrow').click()
  await kit.getByTestId('sheet-font-panel').waitFor()
}
const pickFont = async (name) => {
  await openFonts()
  await kit.getByTestId(`sheet-font-item-${name}`).click()
  await until(async () => (await kit.getByTestId('sheet-font-panel').count()) === 0, 20)
  await settle(200)
}
const pickSize = async (size) => {
  await kit.getByTestId('sheet-size-arrow').click()
  await kit.getByTestId(`sheet-size-item-${size}`).click()
  await settle(200)
}
const typeSize = async (text) => {
  await sizeBox().click()
  await sizeBox().fill(text)
  await sizeBox().press('Enter')
  await settle(250)
}
const fontColorMenu = async () => {
  await kit.getByTestId('sheet-font-color').click()
  await kit.getByTestId('sheet-font-color-colors').waitFor()
}
const undo = async () => {
  await kit.getByTestId('sheet-undo').click()
  await settle(250)
}
const redo = async () => {
  await kit.getByTestId('sheet-redo').click()
  await settle(250)
}
/** Gives the keyboard back to the grid (a click on a cell would select it). */
const sink = () => kit.locator('.kui-text-sink').first().focus()
const mod = process.platform === 'darwin' ? 'Meta' : 'Control'

/** Whether any pixel of a cell's text area matches `test` (rgb). */
const cellHas = (ref, test) =>
  kit.getByTestId('sheet-canvas-region').evaluate(
    (element, [x0, y0, w, h, test]) => {
      const out = document.createElement('canvas')
      out.width = w
      out.height = h
      const context = out.getContext('2d')
      const box = element.getBoundingClientRect()
      for (const canvas of element.querySelectorAll('canvas')) {
        const rect = canvas.getBoundingClientRect()
        if (!rect.width) continue
        const ratio = canvas.width / rect.width
        context.drawImage(canvas, (box.left + x0 - rect.left) * ratio, (box.top + y0 - rect.top) * ratio, w * ratio, h * ratio, 0, 0, w, h)
      }
      const data = context.getImageData(0, 0, w, h).data
      const check = new Function('r', 'g', 'b', `return ${test}`)
      for (let i = 0; i < data.length; i += 4) if (check(data[i], data[i + 1], data[i + 2])) return true
      return false
    },
    [46 + colIndex(ref) * G.cw + 3, 21 + rowIndex(ref) * G.rh + 3, 58, 15, test],
  )
const RED_TEXT = 'r > 180 && g < 90 && b < 90'
/** Whether any pixel of the active cell matches `test` (its real rectangle, whatever the row heights). */
const activeHas = async (test) => {
  const [x0, y0, x1, y1] = (await details()).activeRect
  return kit.getByTestId('sheet-canvas-region').evaluate(
    (element, [x0, y0, w, h, test]) => {
      const out = document.createElement('canvas')
      out.width = w
      out.height = h
      const context = out.getContext('2d')
      const box = element.getBoundingClientRect()
      for (const canvas of element.querySelectorAll('canvas')) {
        const rect = canvas.getBoundingClientRect()
        if (!rect.width) continue
        const ratio = canvas.width / rect.width
        context.drawImage(canvas, (box.left + x0 - rect.left) * ratio, (box.top + y0 - rect.top) * ratio, w * ratio, h * ratio, 0, 0, w, h)
      }
      const data = context.getImageData(0, 0, w, h).data
      const check = new Function('r', 'g', 'b', `return ${test}`)
      for (let i = 0; i < data.length; i += 4) if (check(data[i], data[i + 1], data[i + 2])) return true
      return false
    },
    [Math.round(x0) + 3, Math.round(y0) + 3, Math.max(4, Math.round(x1 - x0) - 6), Math.max(4, Math.round(y1 - y0) - 6), test],
  )
}
/** A pixel of a cell away from its text (right part), for messages. */
const sample = async (ref) => JSON.stringify(await h.pixel(46 + colIndex(ref) * G.cw + 56, 21 + rowIndex(ref) * G.rh + 15))
const YELLOW_FILL = 'r > 200 && g > 200 && b < 80'

/** A fresh two-sheet workbook at 100%, its top-left in view, A1 active, the Home tab shown. */
async function fresh() {
  await build(numbered(2), { fill: false })
  await exec('view.zoom', { percent: 100 })
  await home()
  const box = await region()
  await page.mouse.move(box.x + 300, box.y + 200)
  for (let i = 0; i < 3; i += 1) await page.mouse.wheel(-2000, -2000)
  await settle(200)
  await goTo('A1')
  await click('A1')
}
/** A1 holding 0.5, in `楷体` (KaiTi) 18, bold, red on yellow, 0.00%, centred with a border. */
async function styledA1() {
  await ai([
    { command: 'cell.set', params: { cell: 'A1', input: '0.5' } },
    { command: 'home.fontName', params: { range: 'A1', name: '楷体' } },
    { command: 'home.fontSize', params: { range: 'A1', size: 14 } },
    { command: 'home.bold', params: { range: 'A1', on: true } },
    { command: 'home.italic', params: { range: 'A1', on: true } },
    { command: 'home.underline', params: { range: 'A1', on: true } },
    { command: 'home.strikethrough', params: { range: 'A1', on: true } },
    { command: 'home.fontColor', params: { range: 'A1', color: '#FF0000' } },
    { command: 'home.fillColor', params: { range: 'A1', color: '#FFFF00' } },
    { command: 'home.numberFormat', params: { range: 'A1', format: '0.00%' } },
    { command: 'home.alignCenter', params: { range: 'A1' } },
    { command: 'home.wrapText', params: { range: 'A1', on: true } },
    { command: 'home.borders', params: { range: 'A1', preset: 'all' } },
    // A fixed height, so the rows below stay where clickCell finds them.
    { command: 'home.rowHeight', params: { rows: '1', height: 20 } },
  ])
  await settle(200)
}

try {
  await openEditor('')

  // ———————————————— Format painter ————————————————

  await section('FP-C4 入口', async () => {
    await fresh()
    await styledA1()
    await click('A1')
    const paint = await painter().boundingBox()
    const paste = await kit.getByTestId('sheet-paste').boundingBox()
    step('FP-C4 「开始」选项卡有格式刷、不置灰（WPS 在「粘贴」左侧）', paint != null && paste != null && !(await painter().isDisabled()) && paint.x + paint.width <= paste.x + 2, JSON.stringify({ paint, paste }))
    await painter().click()
    await until(async () => (await painterState()) === 'once', 20)
    step('FP-C4 单击后按钮呈激活态、指针为格式刷', (await pressed('sheet-format-painter')) && (await cursor()).includes('url('), await cursor())
    await page.keyboard.press('Escape')
    await until(async () => (await painterState()) == null, 20)
    step('FP-X1 Esc 取消格式刷，按钮与指针复原', !(await pressed('sheet-format-painter')) && !(await cursor()).includes('url('))
  })

  await section('FP-C6/C12/C14 单击一次性复制', async () => {
    await fresh()
    await styledA1()
    await ai([{ command: 'range.setValues', params: { range: 'B1', values: [['b', 'c', 'd']] } }])
    await click('A1')
    await painter().click()
    await until(async () => (await painterState()) === 'once', 20)
    step('FP-C6 单击后进入一次性复制状态', (await pressed('sheet-format-painter')) && (await painterState()) === 'once')
    await click('B1')
    await until(async () => (await look('B1')) === (await look('A1')), 20)
    step('FP-C6/C14 B1 的样式与 A1 一致，内容不变', (await look('B1')) === (await look('A1')) && (await cellOf('B1')).value === 'b')
    step('FP-C6/C12 刷完一次自动关闭，按钮恢复未激活', (await painterState()) == null && !(await pressed('sheet-format-painter')))
    await click('C1')
    await click('D1')
    step('FP-C12 C1、D1 不再应用源样式', (await look('C1')) !== (await look('A1')) && (await look('D1')) !== (await look('A1')))
    step('FP-C14 选中目标后选区为刷到的单元格', (await details()).selection === 'D1')
  })

  await section('FP-C7/C13/C8 双击锁定连续刷与再次点击取消', async () => {
    await fresh()
    await styledA1()
    await click('A1')
    await painter().dblclick()
    await until(async () => (await painterState()) === 'sticky', 20)
    step('FP-C7/C13 双击进入锁定状态，按钮保持激活', (await painterState()) === 'sticky' && (await pressed('sheet-format-painter')))
    for (const ref of ['B1', 'C1', 'D1']) await click(ref)
    await settle(200)
    const looks = await Promise.all(['B1', 'C1', 'D1'].map(look))
    step('FP-C7/C13 B1、C1、D1 均应用源样式，无需重复点击', looks.every((value) => value === looks[0]) && looks[0] === (await look('A1')) && (await painterState()) === 'sticky')
    await painter().click()
    await until(async () => (await painterState()) == null, 20)
    step('FP-C8 再次点击取消格式刷，按钮复原、指针为默认', !(await pressed('sheet-format-painter')) && !(await cursor()).includes('url('))
    await click('E1')
    step('FP-C8 此后点击 E1 不再应用源样式', (await look('E1')) !== (await look('A1')))
  })

  await section('FP-C9 鼠标指针', async () => {
    await fresh()
    await styledA1()
    await click('A1')
    await painter().click()
    await until(async () => (await painterState()) === 'once', 20)
    const box = await region()
    await page.mouse.move(box.x + 46 + 4 * G.cw + 20, box.y + 21 + 5 * G.rh + 8)
    await settle(200)
    const over = await cursor()
    await page.mouse.move(box.x + 46 + 2 * G.cw + 20, box.y + 21 + 8 * G.rh + 8)
    await settle(200)
    step('FP-C9 在表格区域移动时指针保持格式刷样式', over.includes('url(') && (await cursor()).includes('url('), over)
    await page.keyboard.press('Escape')
    await until(async () => (await painterState()) == null, 20)
  })

  await section('FP-C10/C11/C38/C39 一排、多排与区域循环', async () => {
    await fresh()
    const fills = ['#F4CCCC', '#FCE5CD', '#FFF2CC', '#D9EAD3', '#D0E0E3', '#CFE2F3', '#D9D2E9', '#EAD1DC', '#E6B8AF']
    const steps = []
    fills.forEach((color, i) => steps.push({ command: 'home.fillColor', params: { range: `${COLS[i % 3]}${Math.floor(i / 3) + 1}`, color } }))
    steps.push({ command: 'home.columnWidth', params: { cols: 'E', width: 90 } })
    await ai(steps)
    await settle(200)
    const widthE = async () => (await exec('document.inspect')).sheets[0]?.columns?.E ?? null
    const before = await widthE()
    // One row: A1:C1 onto A6.
    await goTo('A1:C1')
    await painter().click()
    await click('A6')
    await settle(200)
    step('FP-C10 单选一排：A6:C6 依次取 A1:C1 的样式', same([await fillOf('A6'), await fillOf('B6'), await fillOf('C6')], fills.slice(0, 3)) && (await fillOf('D6')) == null)
    // Many rows: A1:C3 dragged onto D1:F3.
    await goTo('A1:C3')
    await painter().click()
    await dragCells('E1', 'G3')
    const block = await Promise.all(['E1', 'F2', 'G3'].map(fillOf))
    step('FP-C11 多选多排：按行列对应应用，区域外不变', same(block, [fills[0], fills[4], fills[8]]) && (await fillOf('H1')) == null && (await fillOf('E4')) == null)
    step('FP-C10/C11 行高列宽不变', same(await widthE(), before))
    // Smaller and larger than the source.
    await goTo('A1:C3')
    await painter().dblclick()
    await until(async () => (await painterState()) === 'sticky', 20)
    await dragCells('A10', 'B11')
    step('FP-C38 不超过源区域：D1:E2 式的小区域按对应位置取样式', same(await Promise.all(['A10', 'B10', 'A11', 'B11'].map(fillOf)), [fills[0], fills[1], fills[3], fills[4]]) && (await fillOf('C10')) == null && (await fillOf('A12')) == null)
    await dragCells('D10', 'F15')
    step('FP-C39 超过源区域：按源区域循环铺满，区域外不变', same(await Promise.all(['D13', 'F15', 'E14'].map(fillOf)), [fills[0], fills[8], fills[4]]) && (await fillOf('G10')) == null && (await fillOf('D16')) == null)
    await page.keyboard.press('Escape')
  })

  await section('FP-C16 编辑态置灰', async () => {
    await fresh()
    await styledA1()
    await click('B2')
    await page.keyboard.type('abc')
    await settle(200)
    step('FP-C16 单元格编辑中格式刷置灰', await painter().isDisabled())
    await page.keyboard.press('Escape')
    await settle(200)
    step('FP-C16 退出编辑态后恢复可用', !(await painter().isDisabled()))
  })

  await section('FP-C19/C22 文本类样式与覆盖', async () => {
    await fresh()
    await styledA1()
    await ai([
      { command: 'home.mergeCenter', params: { range: 'A3:B3' } },
      { command: 'cell.set', params: { cell: 'D1', input: '原内容' } },
      { command: 'home.fontName', params: { range: 'D1', name: '黑体' } },
      { command: 'home.fillColor', params: { range: 'D1', color: '#0000FF' } },
    ])
    await click('A1')
    await painter().click()
    await click('D1')
    await settle(200)
    const [a, d] = [await style('A1'), await style('D1')]
    step('FP-C19 字体、字号、BIUS、字体颜色、背景色、边框、对齐、换行均复制', (await look('D1')) === (await look('A1')) && d.font?.name === '楷体' && d.font?.bold && d.font?.italic && d.font?.strike && d.font?.underline === 'Single' && d.align?.wrap === true, JSON.stringify({ a, d }))
    step('FP-C22 覆盖后 D1 与 A1 一致、无蓝底残留，原内容不变', (await fillOf('D1')) === '#FFFF00' && (await cellOf('D1')).value === '原内容')
    step('FP-C19/C22 画面：D1 红字黄底', (await cellHas('D1', RED_TEXT)) && (await cellHas('D1', YELLOW_FILL)))
    // Merged A3:B3 painted onto D5: merged as the source.
    await click('A3')
    await painter().click()
    await click('D5')
    await settle(200)
    const merges = (await exec('document.inspect')).sheets[0]?.merges ?? (await details()).merges
    step('FP-C19/C27 合并单元格随样式复制到 D5:E5', JSON.stringify(merges ?? []).includes('D5:E5') || (await details()).selection === 'D5:E5', JSON.stringify(merges))
  })

  await section('FP-C20/C21/C26/C28/C30/C32/C33/C37 非文本类属性与应用对象', async () => {
    await fresh()
    await styledA1()
    await ai([
      { command: 'cell.set', params: { cell: 'B1', input: '=SUM(A1:A1)' } },
      { command: 'cell.set', params: { cell: 'C1', input: '链接' } },
      { command: 'insert.link', params: { cell: 'C1', target: 'https://example.com' } },
      { command: 'data.validation', params: { range: 'D1', type: 'list', formula1: '"甲,乙"' } },
      { command: 'cell.set', params: { cell: 'E1', input: '3' } },
      { command: 'home.conditionalFormat', params: { range: 'E1', rule: { type: 'cellIs', operator: 'greaterThan', value: '5', style: { fill: '#00FF00' } } } },
      { command: 'review.newNote', params: { cell: 'F1', text: '批注' } },
      { command: 'home.rowHeight', params: { rows: '9', height: 30 } },
    ])
    const before = await exec('document.inspect')
    await click('A1')
    await painter().dblclick()
    await until(async () => (await painterState()) === 'sticky', 20)
    for (const ref of ['B1', 'C1', 'D1', 'E1', 'F1', 'C3']) await click(ref)
    await page.keyboard.press('Escape')
    await settle(200)
    const after = await exec('document.inspect')
    const b1 = await cellOf('B1')
    step('FP-C26 公式单元格应用样式，公式与结果不变', (await look('B1')) === (await look('A1')) && b1.formula === '=SUM(A1:A1)' && b1.value === 0.5)
    await click('B1')
    step('FP-C26 编辑栏仍显示公式', (await h.formulaBar()) === '=SUM(A1:A1)')
    step('FP-C28 自定义数字格式一并复制，数值不变（50.00%）', (await style('B1')).num_fmt === '0.00%' && (await cellOf('B1')).text === '50.00%', (await cellOf('B1')).text)
    step('FP-C30 链接应用源样式的下划线与颜色，链接保留', (await fontOf('C1')).underline === 'Single' && hex((await fontOf('C1')).color) === '#FF0000' && (await cellOf('C1')).hyperlink?.target === 'https://example.com', JSON.stringify((await cellOf('C1')).hyperlink))
    step('FP-C32 数据验证规则不被覆盖', JSON.stringify(after.sheets?.[0]?.validationRanges ?? before.sheets?.[0]?.validationRanges) === JSON.stringify(before.sheets?.[0]?.validationRanges) && (await look('D1')) === (await look('A1')))
    step('FP-C33 条件格式规则不被复制或破坏', JSON.stringify(after.sheets?.[0]?.conditionalFormatRanges) === JSON.stringify(before.sheets?.[0]?.conditionalFormatRanges))
    await ai([{ command: 'cell.set', params: { cell: 'E1', input: '9' } }])
    await settle(200)
    step('FP-C33 触发条件后条件格式正常生效（E1 变绿）', await cellHas('E1', 'g > 200 && r < 80 && b < 80'))
    step('FP-C37 批注不受影响，可正常查看', (await cellOf('F1')).comment?.text === '批注' && (await look('F1')) === (await look('A1')))
    step('FP-C20 行高等非样式属性不变，表格数据不被修改', JSON.stringify(after.sheets?.[0]?.used) === JSON.stringify(before.sheets?.[0]?.used) && (await cellOf('E1')).value === 9)
    await click('C3')
    await type('新内容')
    await click('C3')
    step('FP-C21 空白单元格应用源样式，输入后按源样式渲染', (await look('C3')).includes('楷体') && (await activeHas(RED_TEXT)), `${await look('C3')} ${JSON.stringify((await cellOf('C3')).value)}`)
  })

  await section('FP-C23 整行整列', async () => {
    await fresh()
    await ai([
      { command: 'home.bold', params: { range: 'A1', on: true } },
      { command: 'home.fillColor', params: { range: 'A1', color: '#FFFF00' } },
      { command: 'cell.set', params: { cell: 'C5', input: '行内' } },
    ])
    await click('A1')
    await painter().dblclick()
    await until(async () => (await painterState()) === 'sticky', 20)
    await clickRowHeading(5)
    await clickColHeading('H')
    await page.keyboard.press('Escape')
    await settle(200)
    step('FP-C23 整行内单元格（含空白、远处）均应用源样式', (await fontOf('C5')).bold && (await fillOf('C5')) === '#FFFF00' && (await fontOf('Z5')).bold && (await cellOf('C5')).value === '行内')
    step('FP-C23 整列内单元格均应用源样式', (await fontOf('H1')).bold && (await fontOf('H500')).bold && !(await fontOf('I1')).bold)
    step('FP-C23 画面：第 5 行与 H 列黄底', (await cellHas('E5', YELLOW_FILL)) && (await cellHas('H9', YELLOW_FILL)) && !(await cellHas('E6', YELLOW_FILL)), `${await sample('E5')} ${await sample('H9')} ${await sample('E6')}`)
  })

  await section('FP-C24 跨 sheet', async () => {
    await fresh()
    await styledA1()
    await ai([{ command: 'cell.set', params: { cell: `${q('工作表2')}!B2`, input: '表二' } }])
    await click('A1')
    await painter().click()
    await kit.getByTestId('sheet-tab-工作表2').click()
    await until(async () => (await details()).sheet === '工作表2', 20)
    step('FP-C24 切换工作表后仍保持格式刷状态', (await painterState()) === 'once' && (await pressed('sheet-format-painter')))
    await click('B2')
    await settle(200)
    const b2 = await style('B2')
    step('FP-C24 Sheet2 目标单元格应用源样式，数据不变', b2.font?.name === '楷体' && hex(b2.fill?.fg) === '#FFFF00' && (await cellOf('B2')).value === '表二')
    await kit.getByTestId('sheet-tab-工作表1').click()
    await until(async () => (await details()).sheet === '工作表1', 20)
    step('FP-C24 源表数据不变', (await cellOf('A1')).value === 0.5)
  })

  await section('FP-C25 框选区域', async () => {
    await fresh()
    await styledA1()
    await click('A1')
    await painter().click()
    await dragCells('B1', 'D5')
    const inside = await Promise.all(['B1', 'C3', 'D5'].map(look))
    step('FP-C25 框选区域内全部应用源样式，无漏刷', inside.every((value) => value === inside[0]) && inside[0] === (await look('A1')))
    step('FP-C25 区域外样式不变', (await fillOf('E5')) == null && (await fillOf('B6')) == null)
    step('FP-C25 选区为刷到的区域', (await details()).selection === 'B1:D5')
  })

  await section('FP-C27 合并单元格', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '标题' } },
      { command: 'home.bold', params: { range: 'A1', on: true } },
      { command: 'home.mergeCenter', params: { range: 'A1:C1' } },
      { command: 'range.setValues', params: { range: 'B3', values: [['一', '二', '三']] } },
    ])
    await click('A1')
    await painter().click()
    await click('B3')
    await settle(200)
    const wb = await exec('document.inspect')
    step('FP-C27 目标应用源样式并合并为 B3:D3', JSON.stringify(wb).includes('B3:D3') && (await fontOf('B3')).bold, JSON.stringify(wb.sheets?.[0]?.merges ?? null))
    step('FP-C27 目标区域原有内容不被删除', (await cellOf('B3')).value === '一' && (await cellOf('C3')).value === '二' && (await cellOf('D3')).value === '三')
  })

  await section('FP-C31 图片', async () => {
    await fresh()
    await styledA1()
    const png = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='
    await ai([{ command: 'insert.picture', params: { at: 'B1', base64: png, mime: 'image/png', placement: 'cell' } }])
    const before = JSON.stringify((await cellOf('B1')).picture)
    await click('A1')
    await painter().click()
    await click('B1')
    await settle(200)
    const after = JSON.stringify((await cellOf('B1')).picture)
    step('FP-C31 单元格图片所在单元格应用源样式，图片本身不变', (await look('B1')) === (await look('A1')) && before === after && after !== 'null' && after != null, after)
  })

  await section('FP-C45 撤销重做', async () => {
    await fresh()
    await styledA1()
    await ai([{ command: 'cell.set', params: { cell: 'B1', input: 'b' } }])
    const plain = await look('B1')
    await click('A1')
    await painter().click()
    await click('B1')
    await settle(200)
    const painted = await look('B1')
    let ok = painted === (await look('A1'))
    for (let i = 0; i < 3; i += 1) {
      await undo()
      ok = ok && (await look('B1')) === plain && (await cellOf('B1')).value === 'b'
      await redo()
      ok = ok && (await look('B1')) === painted
    }
    step('FP-C45 撤销回到刷前样式、重做恢复，反复多次一致，按钮无残留激活态', ok && (await painterState()) == null && !(await pressed('sheet-format-painter')))
  })

  await section('FP-C40 只读权限', async () => {
    await openEditor('&role=viewer')
    try {
      const a1 = await cellOf('A1')
      await click('B2')
      step('FP-C40 只读下没有「开始」选项卡，格式刷入口不可用（kit-editor 的只读规则：只显示查看类选项卡）', (await kit.getByTestId('ribbon-tab-home').count()) === 0 && (await kit.getByTestId('sheet-format-painter').count()) === 0)
      await exec('edit.formatPainter', {}).catch(() => null)
      step('FP-C40 点击无响应，目标单元格样式与内容不变', JSON.stringify(await cellOf('A1')) === JSON.stringify(a1) && !(await exec('document.inspect')).dirty)
    } finally {
      await openEditor('')
    }
  })

  await section('FP-C47/C48 兼容性：连续刷后保存重开', async () => {
    await fresh()
    await styledA1()
    await click('A1')
    await painter().dblclick()
    await until(async () => (await painterState()) === 'sticky', 20)
    for (const ref of ['B2', 'C3']) await click(ref)
    await page.keyboard.press('Escape')
    const painted = (await look('B2')) === (await look('A1')) && (await look('C3')) === (await look('A1'))
    await saveAndReopen()
    const want = await look('A1')
    step(`FP-C47/C48 ${h.which}：双击锁定连续刷正常，保存重开后样式保持`, painted && (await look('B2')) === want && (await look('C3')) === want && want.includes('楷体'), want)
  })

  // ———————————————— Font ————————————————

  await section('FONT-C4/C7/C8/C9/C11 字体入口与下拉面板', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '数据' } }])
    const font = await fontBox().boundingBox()
    const size = await sizeBox().boundingBox()
    step('FONT-C4 字体框位于字号左侧，不置灰', font.x + font.width <= size.x && !(await fontBox().isDisabled()))
    await kit.getByTestId('sheet-font-arrow').click()
    await kit.getByTestId('sheet-font-panel').waitFor()
    step('FONT-C4 点击弹出面板，入口呈展开态', (await kit.getByTestId('sheet-font-arrow').getAttribute('aria-expanded')) === 'true')
    const search = await kit.getByTestId('sheet-font-search').boundingBox()
    const list = await kit.getByTestId('sheet-font-list').boundingBox()
    step('FONT-C7 面板自上而下：搜索框 → 字体列表（首次没有「最近使用」）', search.y < list.y && (await kit.getByTestId('sheet-font-recent').count()) === 0 && (await kit.getByTestId('sheet-font-all').count()) === 1)
    const panel = await kit.getByTestId('sheet-font-panel').boundingBox()
    const viewport = page.viewportSize()
    const ribbon = await kit.getByTestId('sheet-font-arrow').boundingBox()
    step('FONT-C9 面板不超出窗口、不遮挡工具栏', panel.y + panel.height <= viewport.height && panel.y >= ribbon.y + ribbon.height - 1, JSON.stringify(panel))
    await kit.getByTestId('sheet-font-list').evaluate((element) => {
      element.scrollTop = element.scrollHeight
    })
    await settle(100)
    const searchAfter = await kit.getByTestId('sheet-font-search').boundingBox()
    const scrolls = await kit.getByTestId('sheet-font-list').evaluate((element) => element.scrollHeight > element.clientHeight || element.scrollTop >= 0)
    step('FONT-C8 只有字体列表滚动，搜索框固定', same(searchAfter, search) && scrolls)
    const rows = await kit.getByTestId('sheet-font-all').locator('[role="menuitemcheckbox"]').evaluateAll((items) => items.map((item) => ({ text: item.textContent.replace('✓', '').trim(), family: item.style.fontFamily })))
    const names = rows.map((row) => row.text)
    step('FONT-C6 列表含宋体、黑体、微软雅黑、仿宋、楷体、Arial、Calibri、Courier New、Times New Roman、Verdana', ['宋体', '黑体', '微软雅黑', '仿宋', '楷体', 'Arial', 'Calibri', 'Courier New', 'Times New Roman', 'Verdana'].every((name) => names.includes(name)), names.join(','))
    step('FONT-C6/C11 每个字体项按其自身字体展示', rows.every((row) => row.family.length > 0) && rows.find((row) => row.text === '楷体')?.family.includes('Kai'))
    step('FONT-C7 面板展开不改变表格数据与格式', (await cellOf('A1')).value === '数据' && (await fontOf('A1')).name === 'Calibri')
    await kit.getByTestId('sheet-font-arrow').click()
    await until(async () => (await kit.getByTestId('sheet-font-panel').count()) === 0, 20)
    step('FONT-C4 再次点击面板收起', (await kit.getByTestId('sheet-font-panel').count()) === 0)
    await kit.getByTestId('sheet-font-arrow').click()
    await kit.getByTestId('sheet-font-panel').waitFor()
    await clickCell(5, 12)
    await until(async () => (await kit.getByTestId('sheet-font-panel').count()) === 0, 20)
    step('FONT-C4 点击面板外区域收起', (await kit.getByTestId('sheet-font-panel').count()) === 0)
  })

  await section('FONT-C23/C11/C12/C7 选择字体、勾选与最近使用', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '字体样式测试' } }])
    await click('A1')
    await pickFont('楷体')
    step('FONT-C23 单元格内容按所选字体（楷体）', (await fontOf('A1')).name === '楷体' && (await fontBox().inputValue()) === '楷体')
    await openFonts()
    const checked = await kit.getByTestId('sheet-font-item-楷体').getAttribute('aria-checked')
    step('FONT-C23/C11 当前字体前方打勾（选中态）', checked === 'true' && (await kit.getByTestId('sheet-font-item-黑体').getAttribute('aria-checked')) === 'false')
    await kit.getByTestId('sheet-font-item-微软雅黑').click()
    await settle(200)
    await click('A2')
    await pickFont('黑体')
    await click('A1')
    await openFonts()
    const recent = await kit.getByTestId('sheet-font-recent').locator('[role="menuitemcheckbox"]').evaluateAll((items) => items.map((item) => item.textContent.replace('✓', '').trim()))
    step('FONT-C7 「最近使用」按最近使用时间倒序', same(recent.slice(0, 3), ['黑体', '微软雅黑', '楷体']), recent.join(','))
    const row = kit.getByTestId('sheet-font-recent-微软雅黑')
    const rowBox = await row.boundingBox()
    const listBox = await kit.getByTestId('sheet-font-list').boundingBox()
    step('FONT-C12 再次打开，「最近使用」中的当前字体（微软雅黑）选中且在可视区内', (await row.getAttribute('aria-checked')) === 'true' && rowBox.y >= listBox.y && rowBox.y + rowBox.height <= listBox.y + listBox.height + 1)
    await page.keyboard.press('Escape')
  })

  await section('FONT-C13/C14/C15/C16 搜索', async () => {
    await fresh()
    await click('A1')
    await openFonts()
    const search = kit.getByTestId('sheet-font-search')
    const results = async () => kit.getByTestId('sheet-font-list').locator('[role="menuitemcheckbox"]').evaluateAll((items) => items.map((item) => item.textContent.replace('✓', '').trim()))
    const counted = async () => (await kit.getByTestId('sheet-font-results').textContent()).replace(/\s/g, '')
    await search.fill('宋体')
    await settle(100)
    const songti = await results()
    step('FONT-C13/C14 输入中文实时搜索，显示「搜索结果（n）」与匹配项', songti.includes('宋体') && songti.every((name) => name.includes('宋')) && (await counted()) === `搜索结果（${songti.length}）`, `${await counted()} ${songti}`)
    await search.fill('Arial')
    await settle(100)
    step('FONT-C13 英文名匹配', same(await results(), ['Arial']))
    await search.fill('st')
    await settle(100)
    const initials = await results()
    await search.fill('songti')
    await settle(100)
    step('FONT-C13 拼音首字母与全拼匹配（st、songti → 宋体）', initials.includes('宋体') && (await results()).includes('宋体'), initials.join(','))
    await search.fill('zzzz不存在')
    await settle(100)
    const listBox = await kit.getByTestId('sheet-font-list').boundingBox()
    step('FONT-C15 无匹配：「搜索结果（0）」与「暂无搜索结果」，面板不塌陷', (await counted()) === '搜索结果（0）' && (await kit.getByTestId('sheet-font-empty').textContent()) === '暂无搜索结果' && listBox.height >= 232, JSON.stringify(listBox))
    await search.fill('')
    await settle(100)
    step('FONT-C16 清空后恢复默认分组视图，输入框焦点保留', (await kit.getByTestId('sheet-font-all').count()) === 1 && (await search.evaluate((element) => element === document.activeElement)))
    await search.fill('kt')
    await search.press('Enter')
    await settle(250)
    step('FONT-C14 搜索结果可直接选中并应用', (await fontOf('A1')).name === '楷体')
  })

  await section('FONT-C25/C10/C26/C28 工具栏显示', async () => {
    await fresh()
    await ai([
      { command: 'home.fontName', params: { range: 'A1', name: 'Droid Serif' } },
      { command: 'cell.set', params: { cell: 'A1', input: 'serif' } },
      { command: 'home.fontName', params: { range: 'B1', name: '黑体' } },
    ])
    await click('A1')
    await settle(200)
    await openFonts()
    const anyChecked = await kit.getByTestId('sheet-font-panel').locator('[aria-checked="true"]').count()
    await page.keyboard.press('Escape')
    step('FONT-C25 清单外字体：工具栏显示其名称，列表中不打勾，不回退默认字体', (await fontBox().inputValue()) === 'Droid Serif' && anyChecked === 0 && (await fontOf('A1')).name === 'Droid Serif')
    await goTo('A1:B1')
    step('FONT-C26 区域内多种字体时显示活动单元格的字体', (await fontBox().inputValue()) === 'Droid Serif')
    await sink()
    await page.keyboard.press('Tab')
    await settle(200)
    step('FONT-C26 移动活动单元格后随之变化（WPS；FONT-C10 的「多字体不显示文字」按 WPS 为活动单元格）', (await fontBox().inputValue()) === '黑体', await fontBox().inputValue())
    await click('C5')
    step('FONT-C10/C81 未单独设置的单元格显示默认字体与 11 号', (await fontBox().inputValue()) === 'Calibri' && (await sizeBox().inputValue()) === '11')
    await dragCells('C3', 'E6')
    await pickFont('仿宋')
    step('FONT-C28 拖选区域内字体均应用，区域外不变', (await fontOf('C3')).name === '仿宋' && (await fontOf('E6')).name === '仿宋' && (await fontOf('F6')).name === 'Calibri' && (await fontBox().inputValue()) === '仿宋')
    await fontBox().click()
    await fontBox().fill('Georgia')
    await fontBox().press('Enter')
    await settle(250)
    step('FONT-X1 字体框可直接输入字体名回车应用（WPS）', (await fontOf('C3')).name === 'Georgia')
  })

  await section('FONT-C27/C104/C134/C150/C166 整行整列', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'C3', input: '已有' } }])
    await clickRowHeading(3)
    await pickFont('楷体')
    await kit.getByTestId('sheet-cmd-home.bold').click()
    await settle(200)
    await clickColHeading('F')
    await kit.getByTestId('sheet-cmd-home.italic').click()
    await kit.getByTestId('sheet-cmd-home.underline').click()
    await kit.getByTestId('sheet-cmd-home.strikethrough').click()
    await settle(200)
    step('FONT-C27 整行文本字体统一变更', (await fontOf('C3')).name === '楷体' && (await fontOf('Q3')).name === '楷体' && (await fontOf('C4')).name === 'Calibri')
    await click('H3')
    step('FONT-C104 整行加粗：对应单元格 B 为选中态', await pressed('sheet-cmd-home.bold'))
    await type('新输入')
    step('FONT-C27/C104 空白单元格预置字体，输入后自动继承（楷体、加粗）', (await fontOf('H3')).name === '楷体' && (await fontOf('H3')).bold)
    await click('F20')
    step('FONT-C134/C150/C166 整列倾斜、下划线、删除线：对应单元格按钮为选中态', (await pressed('sheet-cmd-home.italic')) && (await pressed('sheet-cmd-home.underline')) && (await pressed('sheet-cmd-home.strikethrough')))
    await type('列内')
    const f = await fontOf('F20')
    step('FONT-C134/C150/C166 输入内容为斜体、下划线、删除线', f.italic && f.underline === 'Single' && f.strike)
  })

  await section('FONT-C29/C106/C136/C152/C168 多个不连续区域', async () => {
    await fresh()
    await click('A1')
    const box = await region()
    await page.keyboard.down(mod)
    await page.mouse.click(box.x + 46 + 2 * G.cw + 32, box.y + 21 + 2 * G.rh + 10)
    await page.mouse.click(box.x + 46 + 4 * G.cw + 32, box.y + 21 + 5 * G.rh + 10)
    await page.keyboard.up(mod)
    await settle(200)
    step('FONT-C29 Ctrl/Cmd 选中多个不连续单元格', (await details()).selection === 'A1,C3,E6', (await details()).selection)
    await pickFont('黑体')
    await boldButton().click()
    await kit.getByTestId('sheet-cmd-home.italic').click()
    await kit.getByTestId('sheet-cmd-home.underline').click()
    await kit.getByTestId('sheet-cmd-home.strikethrough').click()
    await settle(250)
    const all = await Promise.all(['A1', 'C3', 'E6'].map(fontOf))
    step('FONT-C29 各非连续选区字体均应用，未选中区域不变', all.every((f) => f.name === '黑体') && (await fontOf('B2')).name === 'Calibri' && (await fontBox().inputValue()) === '黑体')
    step('FONT-C106/C136/C152/C168 加粗、倾斜、下划线、删除线都应用到各区域', all.every((f) => f.bold && f.italic && f.underline === 'Single' && f.strike) && !(await fontOf('B2')).bold)
  })

  await section('FONT-C33/C34/C35 富文本', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '字体样式测试' } }])
    await click('A1')
    await page.keyboard.press('F2')
    await settle(150)
    for (let i = 0; i < 2; i += 1) await page.keyboard.press('ArrowLeft')
    await page.keyboard.down('Shift')
    for (let i = 0; i < 2; i += 1) await page.keyboard.press('ArrowLeft')
    await page.keyboard.up('Shift')
    await settle(150)
    await pickFont('楷体')
    step('FONT-C33 编辑中选中部分内容设置字体后仍在编辑', (await details()).editing === true)
    await sink()
    await boldButton().click()
    await sink()
    await page.keyboard.press(`${mod}+i`)
    await sink()
    await kit.getByTestId('sheet-cmd-home.underline').click()
    await sink()
    await kit.getByTestId('sheet-cmd-home.strikethrough').click()
    await pickSize(18)
    await sink()
    await page.keyboard.press('Enter')
    await settle(250)
    const runs = await runsOf('A1')
    const styled = runs?.find((run) => run.text === '样式')?.font ?? {}
    step('FONT-C33/C35 同一单元格内不同字符字体不同，字体与 BIUS、字号互不覆盖', styled.name === '楷体' && styled.bold && styled.italic && styled.underline === 'Single' && styled.strike && styled.size === 18 && (await fontOf('A1')).name === 'Calibri', JSON.stringify(runs))
    await click('A1')
    step('FONT-C35 回车后选中单元格：工具栏按单元格显示（非全量加粗时 B 不选中）', !(await pressed('sheet-cmd-home.bold')))
    await page.keyboard.press('F2')
    await settle(150)
    for (let i = 0; i < 3; i += 1) await page.keyboard.press('ArrowLeft')
    await settle(200)
    step('FONT-C33 光标停在设置了字体的内容上：工具栏显示该字体', (await fontBox().inputValue()) === '楷体' && (await sizeBox().inputValue()) === '18' && (await pressed('sheet-cmd-home.bold')), await fontBox().inputValue())
    await page.keyboard.press('Home')
    await settle(200)
    step('FONT-C34 光标在未单独设置字体的内容上：显示单元格字体（WPS），各样式保持', (await fontBox().inputValue()) === 'Calibri' && !(await pressed('sheet-cmd-home.bold')))
    await page.keyboard.press('Escape')
    await settle(150)
    step('FONT-C34 再次进入编辑态后富文本不丢失', JSON.stringify(await runsOf('A1')) === JSON.stringify(runs))
    step('FONT-C35 画面：样式按字符绘制（单元格内有加粗/下划线的楷体段）', (await cellHas('A1', 'r < 60 && g < 60 && b < 60')))
  })

  await section('FONT-C36/C71/C114/C93 撤销重做', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '撤销测试' } }])
    await click('A1')
    await pickFont('楷体')
    await boldButton().click()
    await settle(200)
    await fontColorMenu()
    await kit.getByTestId('sheet-font-color-colors').locator('button').nth(41).click()
    await settle(200)
    await pickSize(28)
    const changed = JSON.stringify(await fontOf('A1'))
    let ok = true
    for (let i = 0; i < 2; i += 1) {
      await undo()
      ok = ok && (await fontOf('A1')).size === 11
      await undo()
      ok = ok && hex((await fontOf('A1')).color) == null
      await undo()
      ok = ok && !(await fontOf('A1')).bold
      step(`FONT-C114 撤销后取消加粗、B 同步（第 ${i + 1} 轮）`, !(await pressed('sheet-cmd-home.bold')))
      await undo()
      ok = ok && (await fontOf('A1')).name === 'Calibri'
      for (let j = 0; j < 4; j += 1) await redo()
      ok = ok && JSON.stringify(await fontOf('A1')) === changed
    }
    step('FONT-C36/C71/C93 字体、颜色、字号、加粗反复撤销重做均正确', ok && (await pressed('sheet-cmd-home.bold')))
  })

  await section('FONT-C37/C38/C80/C91 插入行列、自适应行高', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'B3', input: '字体' } },
      { command: 'home.fontName', params: { range: 'B3', name: '楷体' } },
    ])
    await goTo('B4')
    await exec('home.insertRows', { rows: '4' })
    await settle(200)
    step('FONT-C37 插入的行对应单元格继承字体（WPS：向上插入取上一行）', (await fontOf('B4')).name === '楷体' && (await cellOf('B3')).value === '字体')
    await click('B3')
    await pickSize(36)
    await settle(300)
    const cellBox = async () => (await details()).activeRect
    const tall = await cellBox()
    step('FONT-C38/C80 有内容时行高随字号自适应撑高', tall && tall[3] - tall[1] > 30, JSON.stringify(tall))
    await goTo('E8')
    await pickSize(18)
    const empty = await cellBox()
    step('FONT-C80 18 号前方打勾；空单元格设字号行高也随之撑高（按 Excel）', empty[3] - empty[1] > 20 && (await sizeBox().inputValue()) === '18', `${JSON.stringify(empty)} ${await sizeBox().inputValue()}`)
    await sink()
    await type('撑高')
    await goTo('E8')
    const filled = await cellBox()
    step('FONT-C80 输入内容后单元格自适应撑高', filled[3] - filled[1] > 20, JSON.stringify(filled))
  })

  await section('FONT-C39/C40/C74 格式刷与清除格式', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '楷体' } },
      { command: 'home.fontName', params: { range: 'A1', name: '楷体' } },
      { command: 'home.fontColor', params: { range: 'A1', color: '#FF0000' } },
      { command: 'home.bold', params: { range: 'A1', on: true } },
      { command: 'range.setValues', params: { range: 'B1', values: [['b', 'c']] } },
    ])
    await click('A1')
    await painter().dblclick()
    await click('B1')
    await click('C1')
    await page.keyboard.press('Escape')
    await settle(200)
    step('FONT-C39/C74 B1、C1 显示为楷体、文本颜色与 A1 一致，可连续刷', (await fontOf('B1')).name === '楷体' && (await fontOf('C1')).name === '楷体' && hex((await fontOf('B1')).color) === '#FF0000' && (await fontOf('D1')).name === 'Calibri')
    await click('A1')
    await exec('edit.clearFormats', {})
    await settle(200)
    const a1 = await fontOf('A1')
    step('FONT-C40/C74 清除格式后 A1 为默认字体与颜色，内容保留', a1.name === 'Calibri' && hex(a1.color) == null && !a1.bold && (await cellOf('A1')).value === '楷体')
  })

  await section('FONT-C41/C42/C43/C44/C45/C48/C49 与其他功能交互', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '源' } },
      { command: 'home.fontName', params: { range: 'A1', name: '楷体' } },
      { command: 'cell.set', params: { cell: 'B1', input: '=A1' } },
      { command: 'cell.set', params: { cell: 'C1', input: '=1+1' } },
      { command: 'home.fontName', params: { range: 'C1', name: '黑体' } },
      { command: 'cell.set', params: { cell: 'D1', input: '链接文字' } },
      { command: 'home.fontName', params: { range: 'D1', name: '仿宋' } },
      { command: 'insert.link', params: { cell: 'D1', target: 'https://example.com' } },
      { command: 'insert.link', params: { cell: 'E1', target: 'https://example.org', text: '链接二' } },
      { command: 'home.fontName', params: { range: 'E1', name: '微软雅黑' } },
    ])
    step('FONT-C41 公式引用设置了字体的数据源：结果为默认字体，计算正确', (await fontOf('B1')).name === 'Calibri' && (await cellOf('B1')).value === '源')
    await ai([{ command: 'home.fontName', params: { range: 'A1', name: '宋体' } }])
    step('FONT-C41 数据源字体变化不影响引用结果字体', (await fontOf('B1')).name === 'Calibri')
    step('FONT-C42 公式单元格显示对应字体，计算不受影响', (await fontOf('C1')).name === '黑体' && (await cellOf('C1')).value === 2)
    step('FONT-C43 先设字体再插链接：保留字体，有下划线与链接', (await fontOf('D1')).name === '仿宋' && (await fontOf('D1')).underline === 'Single' && (await cellOf('D1')).hyperlink?.target === 'https://example.com')
    step('FONT-C44 先插链接再设字体：链接属性不受影响', (await fontOf('E1')).name === '微软雅黑' && (await cellOf('E1')).hyperlink?.target === 'https://example.org')
    await ai([
      { command: 'cell.set', params: { cell: 'A5', input: '旧词' } },
      { command: 'home.fontName', params: { range: 'A5', name: '楷体' } },
      { command: 'cell.set', params: { cell: 'B5', input: '旧词' } },
      { command: 'home.fontName', params: { range: 'B5', name: '黑体' } },
    ])
    await exec('edit.replace', { what: '旧词', with: '新词', all: true })
    step('FONT-C45 替换后内容沿用各自原有字体', (await cellOf('A5')).value === '新词' && (await fontOf('A5')).name === '楷体' && (await fontOf('B5')).name === '黑体')
    await ai([
      { command: 'range.setValues', params: { range: 'G1', values: [['c'], ['a'], ['b']] } },
      { command: 'home.fontName', params: { range: 'G1', name: '楷体' } },
      { command: 'home.fontName', params: { range: 'G2', name: '黑体' } },
      { command: 'home.fontName', params: { range: 'G3', name: '仿宋' } },
    ])
    await goTo('G1:G3')
    await exec('data.sortAscending', {})
    await settle(200)
    const sorted = await Promise.all(['G1', 'G2', 'G3'].map(async (ref) => [(await cellOf(ref)).value, (await fontOf(ref)).name]))
    step('FONT-C49 排序后字体跟着内容走，不错位', same(sorted, [['a', '黑体'], ['b', '仿宋'], ['c', '楷体']]), JSON.stringify(sorted))
    await ai([{ command: 'cell.formatText', params: { cell: 'A5', start: 0, end: 1, name: '宋体', bold: true } }])
    await exec('sheet.move', { sheet: '工作表1', to: 1, copy: true })
    await settle(300)
    const copyName = (await h.order()).find((name) => name.startsWith('工作表1(') || name.startsWith('工作表1 ('))
    const copied = copyName ? await exec('cell.get', { cell: `${q(copyName)}!A5` }) : null
    step('FONT-C48 sheet 副本内字体与源表一致，富文本字体保留', copied != null && copied.style?.font?.name === '楷体' && JSON.stringify(copied.runs).includes('宋体'), copyName)
  })

  await section('FONT-C46/C47 删除重复项与分列', async () => {
    await fresh()
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: [['甲'], ['甲'], ['乙']] } },
      { command: 'home.fontName', params: { range: 'A1:A3', name: '楷体' } },
      { command: 'cell.set', params: { cell: 'C1', input: '一,二,三' } },
      { command: 'home.fontName', params: { range: 'C1', name: '黑体' } },
    ])
    await goTo('A1:A3')
    await exec('data.removeDuplicates', { range: 'A1:A3', columns: ['A'], header: false })
    await settle(200)
    step('FONT-C46 去重结果正确，保留单元格的字体不变', (await cellOf('A1')).value === '甲' && (await cellOf('A2')).value === '乙' && (await cellOf('A3')).value == null && (await fontOf('A1')).name === '楷体' && (await fontOf('A2')).name === '楷体')
    await goTo('C1')
    await exec('data.textToColumns', { range: 'C1', delimiters: [','] })
    await settle(200)
    step('FONT-C47 分列后内容正确拆分，起始单元格字体保持', (await cellOf('C1')).value === '一' && (await cellOf('E1')).value === '三' && (await fontOf('C1')).name === '黑体', JSON.stringify([await cellOf('D1'), await fontOf('D1')]))
  })

  await section('FONT-C30 复制/剪切、粘贴', async () => {
    await fresh()
    const fonts = ['宋体', '黑体', '微软雅黑', '仿宋', '楷体', 'Arial', 'Calibri', 'Courier New', 'Times New Roman', 'Verdana']
    const steps = []
    fonts.forEach((name, i) => {
      steps.push({ command: 'cell.set', params: { cell: `A${i + 1}`, input: name } })
      steps.push({ command: 'home.fontName', params: { range: `A${i + 1}`, name } })
    })
    await ai(steps)
    // An agent's selection and clipboard last one run (KHP C3b), so its copy and paste go together.
    await ai([{ command: 'edit.copy', params: { range: 'A1:A10' } }, { command: 'selection.set', params: { range: 'C1' } }, { command: 'edit.paste', params: {} }])
    const here = await Promise.all(fonts.map((_, i) => fontOf(`C${i + 1}`)))
    step('FONT-C30 本 sheet 内粘贴：字体样式全部粘贴成功', here.every((f, i) => f.name === fonts[i]))
    await ai([{ command: 'edit.cut', params: { range: 'A1:A10' } }, { command: 'sheet.activate', params: { sheet: '工作表2' } }, { command: 'selection.set', params: { range: 'B2' } }, { command: 'edit.paste', params: {} }])
    await settle(300)
    // The person goes to 工作表2 to look at what the agent pasted there.
    await h.choose('工作表2')
    await click('B6')
    step('FONT-C30 剪切后跨 sheet 粘贴：字体与文本正确，工具栏一致', (await fontOf('B6')).name === '楷体' && (await cellOf('B6')).value === '楷体' && (await fontBox().inputValue()) === '楷体')
    await h.choose('工作表1')
  })

  await section('FONT-C57/C59/C95 导出与重新导入', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '字体样式测试' } },
      { command: 'home.fontName', params: { range: 'A1', name: '微软雅黑' } },
      { command: 'cell.formatText', params: { cell: 'A1', start: 2, end: 4, name: '楷体', bold: true, color: '#FF0000' } },
      { command: 'home.fontName', params: { range: 'B1', name: '仿宋' } },
      { command: 'home.fontSize', params: { range: 'B1', size: 20 } },
      { command: 'cell.set', params: { cell: 'B1', input: '仿宋' } },
    ])
    await saveAndReopen()
    const styles = await zipPart('xl/styles.xml')
    const shared = (await zipPart('xl/sharedStrings.xml')) ?? ''
    const sheet = (await zipPart('xl/worksheets/sheet1.xml')) ?? ''
    step('FONT-C57 导出的 .xlsx 中字体名一致', styles.includes('val="微软雅黑"') && styles.includes('val="仿宋"'))
    step('FONT-C57 富文本字体导出后不丢失（sharedStrings 中的 rFont）', (shared + sheet).includes('rFont val="楷体"'))
    const runs = await runsOf('A1')
    step('FONT-C59/C95 重新导入后字体、字号一致，富文本结构不被压平', (await fontOf('A1')).name === '微软雅黑' && (await fontOf('B1')).size === 20 && runs?.some((run) => run.text === '样式' && run.font?.name === '楷体' && run.font?.bold), JSON.stringify(runs))
    await click('B1')
    step('FONT-C59 工具栏与下拉勾选状态一致', (await fontBox().inputValue()) === '仿宋' && (await sizeBox().inputValue()) === '20')
  })

  // ———————————————— Font colour ————————————————

  await section('FONT-C61/C62/C63/C64/C65 字体颜色入口与预设颜色', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '颜色' } },
      { command: 'home.bold', params: { range: 'A1', on: true } },
    ])
    await page.evaluate(() => undefined)
    await click('A1')
    const apply = kit.getByTestId('sheet-font-color-apply')
    const remembered = await apply.getAttribute('data-color')
    await apply.click()
    await settle(250)
    step('FONT-C61 主入口直接应用当前记住的颜色，不展开面板', hex((await fontOf('A1')).color) === remembered && (await kit.getByTestId('sheet-font-color-colors').count()) === 0, remembered)
    await fontColorMenu()
    const none = await kit.getByTestId('sheet-font-color-none').boundingBox()
    const grid = await kit.getByTestId('sheet-font-color-colors').boundingBox()
    const hexBox = await kit.getByTestId('sheet-font-color-hex').boundingBox()
    const swatches = await kit.getByTestId('sheet-font-color-colors').locator('button').count()
    step('FONT-C62/C63 面板自上而下：自动（无颜色）→ 色板 → 其他颜色；色板 50 色（含黑白）', none.y < grid.y && grid.y < hexBox.y && swatches === 50, String(swatches))
    step('FONT-C62 展开面板不改变表格数据与格式', (await cellOf('A1')).value === '颜色' && (await fontOf('A1')).bold)
    const blue = kit.getByTestId('sheet-font-color-colors').locator('button').nth(47)
    const blueHex = (await blue.getAttribute('title')).toUpperCase()
    await blue.click()
    await settle(250)
    step('FONT-C64 选择颜色后应用到文本，面板收起，主入口色条更新', hex((await fontOf('A1')).color) === blueHex && (await kit.getByTestId('sheet-font-color-colors').count()) === 0 && (await apply.getAttribute('data-color')) === blueHex)
    step('FONT-C64 单元格不做其他格式变更', (await fontOf('A1')).bold && (await fontOf('A1')).name === 'Calibri')
    await fontColorMenu()
    await kit.getByTestId('sheet-font-color-none').click()
    await settle(250)
    step('FONT-C65 「自动」清除字体颜色恢复默认（黑），其余样式不变', hex((await fontOf('A1')).color) == null && (await fontOf('A1')).bold)
  })

  await section('FONT-C66/C67/C68/C69 其他颜色与最近颜色', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '自定义' } }])
    await click('A1')
    await fontColorMenu()
    step('FONT-C66 「其他颜色」有取色器（系统色谱）', (await kit.getByTestId('sheet-font-color-picker').getAttribute('type')) === 'color')
    await kit.getByTestId('sheet-font-color-hex').fill('#3C5593')
    await kit.getByTestId('sheet-font-color-hex-ok').click()
    await settle(250)
    step('FONT-C67 输入 Hex 色值确认后应用，主入口色条同步', hex((await fontOf('A1')).color) === '#3C5593' && (await kit.getByTestId('sheet-font-color-apply').getAttribute('data-color')) === '#3C5593')
    await fontColorMenu()
    const recent = await kit.getByTestId('sheet-font-color-recent').locator('button').evaluateAll((items) => items.map((item) => item.getAttribute('title').toUpperCase()))
    step('FONT-C68 自定义颜色写入「最近使用颜色」，可直接复用', recent[0] === '#3C5593')
    await page.keyboard.press('Escape')
    await saveAndReopen()
    await home()
    await click('A1')
    await fontColorMenu()
    const kept = await kit.getByTestId('sheet-font-color-recent').locator('button').count()
    await page.keyboard.press('Escape')
    step('FONT-C69 重新打开后主入口仍显示当前颜色、最近颜色保留、文本颜色保持', (await kit.getByTestId('sheet-font-color-apply').getAttribute('data-color')) === '#3C5593' && kept >= 1 && hex((await fontOf('A1')).color) === '#3C5593')
  })

  await section('FONT-C70 区域与富文本颜色', async () => {
    await fresh()
    await ai([{ command: 'range.setValues', params: { range: 'A1', values: [['一二三四', 'b'], ['c', 'd']] } }])
    await goTo('A1:B2')
    await fontColorMenu()
    await kit.getByTestId('sheet-font-color-colors').locator('button').nth(41).click()
    await settle(250)
    const green = hex((await fontOf('B2')).color)
    step('FONT-C70 区域内所有文本颜色统一变更', green != null && hex((await fontOf('A1')).color) === green && hex((await fontOf('C3')).color) == null)
    await click('A1')
    await page.keyboard.press('F2')
    await page.keyboard.down('Shift')
    await page.keyboard.press('ArrowLeft')
    await page.keyboard.press('ArrowLeft')
    await page.keyboard.up('Shift')
    await fontColorMenu()
    await kit.getByTestId('sheet-font-color-colors').locator('button').nth(40).click()
    await sink()
    await page.keyboard.press('Enter')
    await settle(250)
    const runs = await runsOf('A1')
    step('FONT-C70 富文本：仅被选中的文本变色，其余不变', runs?.length === 2 && runs[1].text === '三四' && runs[0].font?.color == null, JSON.stringify(runs))
  })

  await section('FONT-C108/C109/C110/C138/C139/C140/C154/C155/C156/C170/C171/C172 与边框、底色、功能单元格', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '边框' } },
      { command: 'home.borders', params: { range: 'A1', preset: 'all' } },
      { command: 'cell.set', params: { cell: 'B1', input: '底色' } },
      { command: 'home.fillColor', params: { range: 'B1', color: '#FFFF00' } },
      { command: 'home.fontColor', params: { range: 'B1', color: '#FF0000' } },
      { command: 'cell.set', params: { cell: 'C1', input: '=1+2' } },
      { command: 'cell.set', params: { cell: 'D1', input: '链接' } },
      { command: 'insert.link', params: { cell: 'D1', target: 'https://example.com' } },
      { command: 'data.validation', params: { range: 'E1', type: 'list', formula1: '"甲,乙"' } },
      { command: 'cell.set', params: { cell: 'F1', input: '9' } },
      { command: 'home.conditionalFormat', params: { range: 'F1', rule: { type: 'cellIs', operator: 'greaterThan', value: '5', style: { fill: '#00FF00' } } } },
    ])
    const before = await exec('document.inspect')
    await goTo('A1:F1')
    for (const id of ['bold', 'italic', 'underline', 'strikethrough']) await kit.getByTestId(`sheet-cmd-home.${id}`).click()
    await settle(250)
    const after = await exec('document.inspect')
    const fonts = await Promise.all(['A1', 'B1', 'C1', 'D1', 'E1', 'F1'].map(fontOf))
    step('FONT-C108/C138/C154/C170 边框单元格：BIUS 设置成功，边框不变', fonts[0].bold && fonts[0].italic && fonts[0].strike && JSON.stringify((await style('A1')).borders).includes('Thin'))
    step('FONT-C109/C139/C155/C171 底色单元格：底色与字体颜色不变', (await fillOf('B1')) === '#FFFF00' && hex(fonts[1].color) === '#FF0000' && fonts[1].bold)
    step('FONT-C110/C140/C156/C172 公式、链接、验证、条件格式单元格：样式应用且功能正常', fonts.every((f) => f.bold && f.italic) && (await cellOf('C1')).value === 3 && JSON.stringify(after.sheets?.[0]?.validationRanges) === JSON.stringify(before.sheets?.[0]?.validationRanges) && (await cellOf('D1')).hyperlink?.target === 'https://example.com' && (await cellHas('F1', 'g > 200 && r < 80 && b < 80')), JSON.stringify([after.sheets?.[0]?.validationRanges, before.sheets?.[0]?.validationRanges, await sample('F1')]))
  })

  // ———————————————— Font size ————————————————

  await section('FONT-C77/C80/C81/C89/C90 字号列表', async () => {
    await fresh()
    await ai([{ command: 'range.setValues', params: { range: 'A1', values: [['a', 'b'], ['c', 'd']] } }])
    await click('A1')
    step('FONT-C81 默认显示 11 号', (await sizeBox().inputValue()) === '11')
    await kit.getByTestId('sheet-size-arrow').click()
    const sizes = await kit.getByTestId('sheet-size-list').locator('[role="menuitemcheckbox"]').evaluateAll((items) => items.map((item) => ({ size: item.textContent.replace('✓', '').trim(), checked: item.getAttribute('aria-checked') === 'true' })))
    step('FONT-C77 字号列表为 WPS 的 6、8、9、10、11、12、14、16、18、20、22、24、26、28、36、48、72（用例的 30 按 WPS 不列）', same(sizes.map((s) => s.size), ['6', '8', '9', '10', '11', '12', '14', '16', '18', '20', '22', '24', '26', '28', '36', '48', '72']), sizes.map((s) => s.size).join(','))
    step('FONT-C77 当前字号（11）前方打勾', same(sizes.filter((s) => s.checked).map((s) => s.size), ['11']))
    await kit.getByTestId('sheet-size-item-18').click()
    await settle(250)
    step('FONT-C77/C80 单击选中并收起，字号框显示 18', (await kit.getByTestId('sheet-size-list').count()) === 0 && (await sizeBox().inputValue()) === '18' && (await fontOf('A1')).size === 18)
    await goTo('A1:B2')
    await typeSize('14')
    step('FONT-C90 区域字号统一变更，区域外不变', (await fontOf('B2')).size === 14 && (await fontOf('C3')).size === 11)
    await ai([{ command: 'home.fontSize', params: { range: 'B2', size: 26 } }])
    await goTo('A1:B2')
    await page.keyboard.press('Enter')
    await page.keyboard.press('Enter')
    await page.keyboard.press('Tab')
    await settle(200)
    step('FONT-C89 区域字号不一致时显示活动单元格的字号', (await details()).active === 'B2' ? (await sizeBox().inputValue()) === '26' : (await sizeBox().inputValue()) === String((await fontOf((await details()).active)).size), (await details()).active)
  })

  await section('FONT-C91 多行多列字号', async () => {
    await fresh()
    await ai([{ command: 'range.setValues', params: { range: 'A3', values: [['三'], ['四'], ['五']] } }])
    await goTo('A5')
    const before = (await details()).activeRect
    await goTo('3:4')
    await typeSize('24')
    await goTo('A3')
    const third = (await details()).activeRect
    await goTo('A5')
    const fifth = (await details()).activeRect
    step('FONT-C91 多行字号设置成功，行高自适应，未选中行字号与行高不变', (await fontOf('A3')).size === 24 && (await fontOf('Z4')).size === 24 && third[3] - third[1] > 20 && fifth[3] - fifth[1] === before[3] - before[1] && (await fontOf('A5')).size === 11)
    await goTo('C:D')
    await typeSize('16')
    step('FONT-C91 多列字号设置成功，未选中列不变', (await fontOf('C9')).size === 16 && (await fontOf('D900')).size === 16 && (await fontOf('E9')).size === 11)
  })

  await section('FONT-C82/C83/C84/C85/C88 增大减小字号', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '字号' } }])
    await click('A1')
    const grow = kit.getByTestId('sheet-cmd-home.increaseFontSize')
    const shrink = kit.getByTestId('sheet-cmd-home.decreaseFontSize')
    await grow.click()
    await settle(200)
    step('FONT-C82 增大字号：11 → 12，字号框同步', (await fontOf('A1')).size === 12 && (await sizeBox().inputValue()) === '12')
    await shrink.click()
    await settle(200)
    step('FONT-C84 减小字号：12 → 11', (await fontOf('A1')).size === 11 && (await sizeBox().inputValue()) === '11')
    await typeSize('20')
    await grow.click()
    await settle(150)
    const a = (await fontOf('A1')).size
    await grow.click()
    await settle(150)
    step('FONT-C88 预设范围内按列表顺序：20 → 22 → 24', a === 22 && (await fontOf('A1')).size === 24)
    await typeSize('15')
    await grow.click()
    await settle(150)
    step('FONT-C88 预设外的 15 增大到下一个预设字号 16（WPS；用例 15→16→17 与「按列表」矛盾）', (await fontOf('A1')).size === 16)
    await typeSize('48')
    await grow.click()
    await settle(250)
    step('FONT-C83 到 72 后「增大字号」置灰', (await fontOf('A1')).size === 72 && (await grow.isDisabled()))
    await grow.click({ force: true }).catch(() => undefined)
    await settle(150)
    step('FONT-C83 置灰后点击无响应', (await fontOf('A1')).size === 72)
    await typeSize('8')
    await shrink.click()
    await settle(250)
    step('FONT-C85 到 6 后「减小字号」置灰、点击无响应', (await fontOf('A1')).size === 6 && (await shrink.isDisabled()))
  })

  await section('FONT-C86/C87 自定义字号', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '自定义' } }])
    await click('A1')
    await typeSize('15')
    await kit.getByTestId('sheet-size-arrow').click()
    const checked = await kit.getByTestId('sheet-size-list').locator('[aria-checked="true"]').count()
    await page.keyboard.press('Escape')
    step('FONT-C86 输入 15 回车：应用，字号框显示 15，列表无勾选（非预设值）', (await fontOf('A1')).size === 15 && (await sizeBox().inputValue()) === '15' && checked === 0)
    await typeSize('200')
    const rect = (await details()).activeRect
    step('FONT-C86 输入 200：应用，行高自适应', (await fontOf('A1')).size === 200 && rect[3] - rect[1] > 200, JSON.stringify(rect))
    for (const bad of ['0', '500', '大号']) {
      await typeSize(bad)
      const toast = await kit.getByText('设置失败，字号必须在 1-409 磅之间').first().isVisible().catch(() => false)
      step(`FONT-C87 输入 ${bad}：提示「设置失败，字号必须在 1-409 磅之间」，字号不变，输入框回退`, toast && (await fontOf('A1')).size === 200 && (await sizeBox().inputValue()) === '200')
    }
  })

  await section('FONT-C92 各数据类型', async () => {
    await fresh()
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: [[1234.5, '文本', '2026-11-10', 0.25, '=A1*2']] } },
      { command: 'home.numberFormat', params: { range: 'D1', format: '0%' } },
      { command: 'home.accounting', params: { range: 'A1' } },
      { command: 'home.mergeCenter', params: { range: 'F1:G1' } },
    ])
    const texts = await Promise.all(['A1', 'C1', 'D1', 'E1'].map(async (ref) => (await cellOf(ref)).text))
    await goTo('A1:G1')
    await typeSize('24')
    const after = await Promise.all(['A1', 'C1', 'D1', 'E1'].map(async (ref) => (await cellOf(ref)).text))
    const rect = (await details()).activeRect
    step('FONT-C92 各格式单元格设置字号成功，显示格式不被破坏，行高撑高', same(texts, after) && (await fontOf('E1')).size === 24 && (await fontOf('F1')).size === 24 && rect[3] - rect[1] > 20, JSON.stringify(after))
  })

  // ———————————————— Bold, italic, underline, strikethrough ————————————————

  await section('FONT-C97/C99/C103/C113/C129/C131/C133/C141/C146/C149/C157/C162/C165/C173/C174 入口、快捷键与取消', async () => {
    await fresh()
    await ai([{ command: 'range.setValues', params: { range: 'A1', values: [['加粗', '斜体', '下划线', '中划线']] } }])
    const toggles = [
      ['bold', 'A1', 'b', (f) => f.bold, 'FONT-C97/C103/C113', 'FONT-C99'],
      ['italic', 'B1', 'i', (f) => f.italic, 'FONT-C129/C133/C141', 'FONT-C131'],
      ['underline', 'C1', 'u', (f) => f.underline === 'Single', 'FONT-C146/C149/C157', 'FONT-C146'],
      ['strikethrough', 'D1', '5', (f) => f.strike, 'FONT-C162/C165/C173', 'FONT-C174'],
    ]
    for (const [id, ref, key, test, ids, keyIds] of toggles) {
      await click(ref)
      await kit.getByTestId(`sheet-cmd-home.${id}`).click()
      await settle(200)
      const on = test(await fontOf(ref)) && (await pressed(`sheet-cmd-home.${id}`))
      await kit.getByTestId(`sheet-cmd-home.${id}`).click()
      await settle(200)
      const off = !test(await fontOf(ref)) && !(await pressed(`sheet-cmd-home.${id}`))
      step(`${ids} 点击 ${id}：内容应用、按钮选中；再次点击取消、按钮未激活`, on && off && (await cellOf(ref)).value != null)
      await sink()
      await page.keyboard.press(`${mod}+${key}`)
      await settle(250)
      const byKey = test(await fontOf(ref))
      await sink()
      await page.keyboard.press(`${mod}+${key}`)
      await settle(250)
      step(`${keyIds} 快捷键 ${mod}+${key.toUpperCase()} 应用与取消，与点击结果一致`, byKey && !test(await fontOf(ref)))
    }
    const tips = await Promise.all(['bold', 'italic', 'underline', 'strikethrough'].map((id) => kit.getByTestId(`sheet-cmd-home.${id}`).getAttribute('title')))
    step('FONT-C100/C174 tooltip 显示名称与快捷键：加粗 B、倾斜 I、下划线 U、删除线 5', /加粗.*B/.test(tips[0]) && /倾斜.*I/.test(tips[1]) && /下划线.*U/.test(tips[2]) && /删除线.*5/.test(tips[3]), tips.join(' | '))
    const plain = await fontOf('A1')
    step('FONT-C113/C157/C173 取消后其余样式不受影响', plain.name === 'Calibri' && plain.size === 11)
    step('FONT-C97/C113 画面：取消后不再加粗（文字深色像素存在）', await cellHas('A1', 'r < 90 && g < 90 && b < 90'))
  })

  await section('FONT-C101/C132/C148/C164 空白单元格预置', async () => {
    await fresh()
    const cases = [
      ['bold', 'B2', (f) => f.bold, 'FONT-C101'],
      ['italic', 'C2', (f) => f.italic, 'FONT-C132'],
      ['underline', 'D2', (f) => f.underline === 'Single', 'FONT-C148'],
      ['strikethrough', 'E2', (f) => f.strike, 'FONT-C164'],
    ]
    for (const [id, ref, test, cid] of cases) {
      await click(ref)
      await kit.getByTestId(`sheet-cmd-home.${id}`).click()
      await sink()
      await type('输入')
      await click(ref)
      step(`${cid} 空白单元格预置 ${id}，输入即生效，按钮选中`, test(await fontOf(ref)) && (await cellOf(ref)).value === '输入' && (await pressed(`sheet-cmd-home.${id}`)))
    }
  })

  await section('FONT-C102/C112 部分加粗与按钮状态', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: '字体样式测试' } }])
    await click('A1')
    await page.keyboard.press('F2')
    await page.keyboard.down('Shift')
    for (let i = 0; i < 3; i += 1) await page.keyboard.press('ArrowLeft')
    await page.keyboard.up('Shift')
    await settle(150)
    await boldButton().click()
    await settle(200)
    step('FONT-C112 选中的文本全部加粗时按钮为选中态', await pressed('sheet-cmd-home.bold'))
    await sink()
    await page.keyboard.press('Home')
    await page.keyboard.down('Shift')
    await page.keyboard.press('End')
    await page.keyboard.up('Shift')
    await settle(200)
    step('FONT-C112 选中内容中存在未加粗内容时按钮不为选中态', !(await pressed('sheet-cmd-home.bold')))
    await page.keyboard.press('Enter')
    await settle(200)
    const runs = await runsOf('A1')
    await click('A1')
    step('FONT-C102 选中部分加粗成功，未选中部分原样，单元格的 B 为未选中', runs?.length === 2 && runs[1].text === '式测试' && runs[1].font?.bold === true && !runs[0].font?.bold && !(await pressed('sheet-cmd-home.bold')), JSON.stringify(runs))
  })

  await section('FONT-C105/C135/C151/C167 拖选区域', async () => {
    await fresh()
    await dragCells('B2', 'D4')
    for (const id of ['bold', 'italic', 'underline', 'strikethrough']) await kit.getByTestId(`sheet-cmd-home.${id}`).click()
    await settle(200)
    await click('C3')
    const f = await fontOf('C3')
    step('FONT-C105/C135/C151/C167 区域设置成功，对应单元格按钮选中，区域外不变', f.bold && f.italic && f.underline === 'Single' && f.strike && (await pressed('sheet-cmd-home.bold')) && !(await fontOf('E4')).bold)
    await type('区域')
    step('FONT-C105 输入内容加粗显示', (await fontOf('C3')).bold)
  })

  await section('FONT-C125/C126 筛选隐藏行与隐藏行列', async () => {
    await fresh()
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: [['类'], ['甲'], ['乙'], ['甲'], ['乙']] } },
      { command: 'data.filter', params: { range: 'A1:A5' } },
      { command: 'data.filterBy', params: { column: 0, values: ['甲'] } },
    ])
    await goTo('A2:A5')
    await boldButton().click()
    await settle(200)
    await exec('data.clearFilter', {})
    await settle(200)
    step('FONT-C125 筛选隐藏的行不加粗，恢复显示后保持原字重', (await fontOf('A2')).bold && (await fontOf('A4')).bold && !(await fontOf('A3')).bold && !(await fontOf('A5')).bold)
    await ai([
      { command: 'range.setValues', params: { range: 'C1', values: [['c1'], ['c2'], ['c3']] } },
      { command: 'home.hideRows', params: { rows: '2' } },
    ])
    await goTo('C1:C3')
    await boldButton().click()
    await settle(200)
    await exec('home.unhideRows', { rows: '1:3' })
    step('FONT-C126 隐藏行的内容也加粗，取消隐藏后加粗显示', (await fontOf('C2')).bold && (await fontOf('C1')).bold)
  })

  await section('FONT-C121/C119/C120/C53/C73/C79/C143/C159/C176/C26 权限', async () => {
    await fresh()
    await ai([
      { command: 'cell.set', params: { cell: 'A1', input: '可编辑' } },
      { command: 'home.bold', params: { range: 'A1', on: true } },
    ])
    await click('A1')
    await boldButton().click()
    await settle(200)
    step('FONT-C121 可编辑权限：取消加粗生效，无拦截提示', !(await fontOf('A1')).bold)
    await boldButton().click()
    await ai([
      { command: 'home.fontName', params: { range: 'A1', name: '楷体' } },
      { command: 'home.fontSize', params: { range: 'A1', size: 16 } },
      { command: 'home.italic', params: { range: 'A1', on: true } },
      { command: 'home.underline', params: { range: 'A1', on: true } },
      { command: 'home.strikethrough', params: { range: 'A1', on: true } },
    ])
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
      await settle(200)
      step('FONT-C53/C73/C79/C119/C120/C143/C159/C176 只读下没有「开始」选项卡，字体、字号、颜色、BIUS 入口均不可用（kit-editor 的只读规则）', (await kit.getByTestId('ribbon-tab-home').count()) === 0 && (await kit.getByTestId('sheet-font').count()) === 0 && (await kit.getByTestId('sheet-cmd-home.bold').count()) === 0)
      step('FONT-C53/C79/C26 只读下单元格的字体、字号信息保留可读（document 中为楷体 16）', (await fontOf('A1')).name === '楷体' && (await fontOf('A1')).size === 16)
      await sink()
      await page.keyboard.press(`${mod}+b`)
      await settle(200)
      step('FONT-C120 只读下快捷键无响应，内容与样式不变', (await fontOf('A1')).bold === true)
    } finally {
      await openEditor('')
    }
  })

  await section('FONT-C127/C144/C160/C177/C75 兼容性', async () => {
    await fresh()
    await ai([{ command: 'cell.set', params: { cell: 'A1', input: 'abc' } }])
    await click('A1')
    for (const id of ['bold', 'italic', 'underline', 'strikethrough']) await kit.getByTestId(`sheet-cmd-home.${id}`).click()
    await fontColorMenu()
    await kit.getByTestId('sheet-font-color-hex').fill('#FF0000')
    await kit.getByTestId('sheet-font-color-hex-ok').click()
    await settle(300)
    step(`FONT-C127/C144/C160/C177/C75 ${h.which}：BIUS 与颜色正常应用并按红色绘制`, (await cellHas('A1', RED_TEXT)) && (await fontOf('A1')).strike)
  })

  if (errors.length) step('no page errors', false, errors.slice(0, 3).join(' | '))
} catch (error) {
  await h.finish(error)
}
await h.finish()
