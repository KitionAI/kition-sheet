// Table styles (module M7, Format as Table) and the Format Cells dialog, as WPS for Mac has them, in a
// real browser through the development host: node tests/gate-table-style.mjs [chromium|webkit]
// Home › Format as Table (Preset Styles, the theme colour row, light/medium/dark groups, the workbook's
// own styles, New Table Style), the Format As Table dialog (the range taken from one cell, Only
// Format As Table with title rows, Convert to a table), the Table Design tab (the style options,
// Resize Table, Convert to Range), the look composed part by part, a cell's own format over it,
// tables growing as entries are typed or pasted beside them, rows and columns inserted and
// deleted, hidden, filtered and grouped rows, sorting, clearing, merging, the format painter,
// header names, styles saved and reopened; Format Cells (Cmd/Ctrl+1, the cell menu, Home ›
// Format) with its Number, Alignment, Font, Border, Patterns and Protection tabs.
// Each step names its case (TS-C10 for table styles, FC-X1 for Format Cells); `GATE_ONLY=TS-C10`
// runs some.
import { startGate } from './gate-harness.mjs'

const h = await startGate({ prefix: '' })
/** The grid's pitch as the engine lays it out (the harness reads it when a workbook opens). */
const G = h.grid
const { page, kit, step, section, until, same, ai, exec, details, workbook, cellOf, region, clickCell, goTo, build, numbered, saveAndReopen, openEditor, errors } = h

const settle = (ms = 250) => page.waitForTimeout(ms)
const COLS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'
const colIndex = (ref) => COLS.indexOf(ref.replace(/\d+$/, ''))
const rowIndex = (ref) => Number(ref.replace(/^[A-Z]+/, '')) - 1
const click = (ref) => clickCell(colIndex(ref), rowIndex(ref))
const mod = process.platform === 'darwin' ? 'Meta' : 'Control'
const type = async (text) => {
  await page.keyboard.type(text)
  await page.keyboard.press('Enter')
  await settle(200)
}

// ———— reading the sheet ————
const style = async (ref) => (await cellOf(ref)).style ?? {}
const hex = (color) => (color && typeof color === 'object' && 'Rgb' in color ? `#${color.Rgb.toString(16).padStart(6, '0').toUpperCase()}` : null)
const ownFill = async (ref) => {
  const fill = (await style(ref)).fill
  return fill?.pattern && fill.pattern !== 'None' ? (hex(fill.fg) ?? 'theme') : null
}
const value = async (ref) => (await cellOf(ref)).value
/** The active sheet's tables, as document.inspect lists them. */
const tables = async () => {
  const sheet = (await details()).sheet
  return (await workbook()).sheets.find((s) => s.name === sheet)?.tables ?? []
}
const tableRange = async () => (await tables())[0]?.range ?? null
/** The colour the grid shows inside a cell, off its text (2 px in from its left edge, mid-height). */
const shownFill = async (ref, screenRow = rowIndex(ref), dx = 2) => {
  const [r, g, b] = await h.pixel(46 + colIndex(ref) * G.cw + dx, 21 + screenRow * G.rh + 10)
  return `#${[r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('').toUpperCase()}`
}
const near = (a, b, by = 12) => {
  if (!a || !b) return false
  const [x, y] = [a, b].map((c) => [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16)))
  return x.every((v, i) => Math.abs(v - y[i]) <= by)
}
const WHITE = '#FFFFFF'

// ———— the gallery and its dialogs ————
const home = async () => {
  await kit.getByTestId('ribbon-tab-home').click()
  await kit.getByTestId('sheet-format-table').waitFor()
}
const openGallery = async () => {
  await home()
  if ((await kit.getByTestId('sheet-table-gallery').count()) === 0) await kit.getByTestId('sheet-format-table').click()
  await kit.getByTestId('sheet-table-gallery').waitFor()
  await until(async () => (await kit.getByTestId('sheet-table-group-light').count()) === 1)
}
const closeMenus = async () => {
  await page.keyboard.press('Escape')
  await settle(150)
}
/** The look of a style in a colour (header fill, band fill) from the gallery's own preview. */
const looks = async (name, slot = 1) => {
  const g = await exec('table.styles', { slot })
  const all = [...g.groups.flatMap((group) => group.styles), ...g.custom]
  const found = all.find((s) => s.name === name)
  return { header: found?.preview[0][0][0] ?? null, band: found?.preview[1][0][0] ?? null, plain: found?.preview[2][0][0] ?? null }
}
/** Chooses `name` in the gallery and, in Format As Table's dialog, `mode` (format or table). */
const apply = async (name, { mode = 'table', titleRows, header } = {}) => {
  await openGallery()
  await kit.getByTestId(`sheet-table-${name}`).click()
  await kit.getByTestId('sheet-fat').waitFor()
  if (mode === 'table') await kit.getByTestId('sheet-fat-table').check()
  if (titleRows != null) await kit.getByTestId('sheet-fat-title-rows').fill(String(titleRows))
  if (header === false) await kit.getByTestId('sheet-fat-header').uncheck()
  await kit.getByTestId('sheet-fat-ok').click()
  await settle(300)
}
const tableTab = async () => {
  await until(async () => (await kit.getByTestId('ribbon-tab-tableDesign').count()) === 1)
  await kit.getByTestId('ribbon-tab-tableDesign').click()
  await kit.getByTestId('sheet-table-opt-header').waitFor()
}
const noticeShown = (text) => until(async () => (await kit.locator('.kui-toast').allTextContents()).some((shown) => shown.includes(text)), 30)
const dismiss = async () => {
  for (const close of await kit.locator('.kui-toast-close').all()) await close.click().catch(() => undefined)
}

/** A fresh sheet at 100%, its top-left in view: A1:D10 holds a header and nine rows; rows stay at the default height. */
async function dataset() {
  await build(numbered(2), { fill: false })
  // Each case starts without the styles an earlier one made, and with Kition Sheet's default.
  const own = await exec('table.styles', {})
  for (const style of own.custom) await exec('table.deleteStyle', { name: style.name })
  if (own.default !== 'TableStyleMedium2') await exec('table.setDefaultStyle', { name: '' })
  const rows = [['名称', '地区', '一季度', '二季度']]
  for (let r = 2; r <= 10; r += 1) rows.push([`n${r}`, r % 2 ? '西' : '东', r * 10, r * 7])
  await ai([
    { command: 'range.setValues', params: { range: 'A1', values: rows } },
    { command: 'home.rowHeight', params: { rows: '1:40', height: G.rh } },
    { command: 'view.zoom', params: { percent: 100 } },
  ])
  await home()
  const box = await region()
  await page.mouse.move(box.x + 300, box.y + 200)
  for (let i = 0; i < 3; i += 1) await page.mouse.wheel(-2000, -2000)
  await settle(200)
  await click('B3')
}
/** The data as a table in Medium2 (Accent 1), through the gallery. */
async function asTable(name = 'TableStyleMedium2') {
  await dataset()
  await apply(name, { mode: 'table' })
  await until(async () => (await tableRange()) === 'A1:D10')
}

try {
  await openEditor('')

  // ———————————————— Entry points ————————————————

  await section('TS-C4 下拉面板结构', async () => {
    await dataset()
    await openGallery()
    const g = kit.getByTestId('sheet-table-gallery')
    const text = await g.textContent()
    step('TS-C4 面板自上而下：预设样式、主题颜色、浅色系/中色系/深色系样式网格，底部「新建表格样式…」', text.indexOf('预设样式') < text.indexOf('主题颜色') && text.indexOf('主题颜色') < text.indexOf('浅色系') && text.indexOf('深色系') < text.indexOf('新建表格样式'), text.slice(0, 80))
    const counts = [await kit.getByTestId('sheet-table-group-light').locator('.sheet-table-style').count(), await kit.getByTestId('sheet-table-group-medium').locator('.sheet-table-style').count(), await kit.getByTestId('sheet-table-group-dark').locator('.sheet-table-style').count()]
    step('TS-C4 主题颜色 7 个色块（WPS 桌面：文本色 + 6 个强调色），浅/中/深 3/4/5 个样式', (await g.locator('[role="radio"]').count()) === 7 && same(counts, [3, 4, 5]), JSON.stringify(counts))
    step('TS-C4 面板展开时按钮呈展开态，面板内无置灰项', (await kit.getByTestId('sheet-format-table').getAttribute('aria-pressed')) === 'true' && (await g.locator('button[disabled]').count()) === 0)
    const fixed = await g.evaluate((element) => {
      const scroll = element.querySelector('.sheet-table-scroll')
      const actions = element.querySelector('.sheet-table-actions')
      return scroll != null && actions != null && !scroll.contains(actions) && getComputedStyle(scroll).overflowY === 'auto'
    })
    step('TS-C117 样式列表单独滚动，底部操作区固定在滚动区之外始终可见', fixed && (await kit.getByTestId('sheet-table-new').isVisible()))
    step('TS-C4 打开面板不影响表格数据与格式', (await value('A1')) === '名称' && (await ownFill('A1')) === null && (await tables()).length === 0)
    await closeMenus()
  })

  await section('TS-C5 主题颜色切换时预设样式动态刷新', async () => {
    await dataset()
    await openGallery()
    const firstMedium = () => kit.getByTestId('sheet-table-group-medium').locator('.sheet-table-style').first().getAttribute('data-testid')
    const before = await firstMedium()
    const thumbBefore = await kit.getByTestId('sheet-table-group-medium').locator('.sheet-table-thumb').first().innerHTML()
    await kit.getByTestId('sheet-table-color-3').click()
    await until(async () => (await firstMedium()) !== before)
    const after = await firstMedium()
    step('TS-C5 切换主题颜色后预设样式即时刷新（中色系第一个由 Medium2 变为 Medium4，缩略图配色变化）', before === 'sheet-table-TableStyleMedium2' && after === 'sheet-table-TableStyleMedium4' && (await kit.getByTestId('sheet-table-group-medium').locator('.sheet-table-thumb').first().innerHTML()) !== thumbBefore, `${before} → ${after}`)
    step('TS-C5 选中色块有选中态（aria-checked + 描边），面板仍展开', (await kit.getByTestId('sheet-table-color-3').getAttribute('aria-checked')) === 'true' && (await kit.getByTestId('sheet-table-color-1').getAttribute('aria-checked')) === 'false' && (await kit.getByTestId('sheet-table-gallery').count()) === 1)
    for (let k = 0; k < 7; k += 1) await kit.getByTestId(`sheet-table-color-${k}`).click()
    step('TS-C5 依次点击 7 个色块，表格数据不被改写', (await value('C3')) === 30 && (await tables()).length === 0)
    await closeMenus()
  })

  await section('TS-C6/C10/TS-X1 单个单元格自动识别周边区域，仅套用表格样式', async () => {
    await dataset()
    await click('B3')
    await openGallery()
    await kit.getByTestId('sheet-table-TableStyleMedium2').click()
    await kit.getByTestId('sheet-fat').waitFor()
    step('TS-C6 对话框「表数据的来源」自动扩充为 =$A$1:$D$10', (await kit.getByTestId('sheet-fat-range').inputValue()) === '=$A$1:$D$10', await kit.getByTestId('sheet-fat-range').inputValue())
    step('TS-X1 默认「仅套用表格样式」、标题行 1 行，「表包含标题」「筛选按钮」在转换为表格时才可用（WPS）', (await kit.getByTestId('sheet-fat-format').isChecked()) && (await kit.getByTestId('sheet-fat-title-rows').inputValue()) === '1' && (await kit.getByTestId('sheet-fat-header').isDisabled()))
    await kit.getByTestId('sheet-fat-ok').click()
    await settle(300)
    const look = await looks('TableStyleMedium2')
    step('TS-C6/C10 样式应用到 A1:D10：标题行与条带写入单元格格式（第 3 行无填充）', (await ownFill('A1')) !== null && (await ownFill('A2')) !== null && (await ownFill('A3')) === null && (await ownFill('D10')) !== null, `${await ownFill('A1')} ${await ownFill('A3')}`)
    step('TS-X1 仅套用样式不生成表格（无「表格工具」选项卡）', (await tables()).length === 0 && (await kit.getByTestId('ribbon-tab-tableDesign').count()) === 0)
    step('TS-C6 数据区域外单元格格式不变', (await ownFill('E1')) === null && (await ownFill('A11')) === null)
    step('TS-C10 单元格原有数据不被改写', (await value('C5')) === 50 && (await value('A1')) === '名称')
    step('TS-C10 画面按样式渲染（标题行着色、交替行）', near(await shownFill('A1'), look.header) && near(await shownFill('A2'), look.band) && near(await shownFill('A3'), WHITE))
  })

  await section('TS-C7 整行/整列/多行/连续范围不扩充', async () => {
    await dataset()
    await goTo('A1:B5')
    await click('A1')
    await page.keyboard.down('Shift')
    await click('B5')
    await page.keyboard.up('Shift')
    await apply('TableStyleLight9')
    step('TS-C7 连续范围 A1:B5 原样套用，不扩充', (await tableRange()) === 'A1:B5', await tableRange())
    await dataset()
    const box = await region()
    await page.mouse.click(box.x + 46 + 2 * G.cw + 32, box.y + 10)
    await page.keyboard.down('Shift')
    await page.mouse.click(box.x + 46 + 3 * G.cw + 32, box.y + 10)
    await page.keyboard.up('Shift')
    await settle(200)
    await apply('TableStyleLight9')
    step('TS-C7 选中整列 C:D：作用范围为这两列中有内容的部分 C1:D10', (await tableRange()) === 'C1:D10', await tableRange())
    await dataset()
    await page.mouse.click(box.x + 20, box.y + 21 + 1 * G.rh + 10)
    await page.keyboard.down('Shift')
    await page.mouse.click(box.x + 20, box.y + 21 + 3 * G.rh + 10)
    await page.keyboard.up('Shift')
    await settle(200)
    await apply('TableStyleLight9', { mode: 'format' })
    step('TS-C7 选中整行 2:4（仅套用样式）：只有 A2:D4 被格式化，第 1 行与第 5 行不变', (await ownFill('A1')) === null && (await ownFill('A5')) === null && (await style('A2')).font?.bold === true, JSON.stringify((await style('A2')).font))
  })

  await section('TS-C8/C18/C20 选中表格样式单元格时入口切换与表格工具', async () => {
    await asTable()
    await click('F5')
    await openGallery()
    step('TS-C8 选中不含表格样式的单元格：底部只有「新建表格样式…」', (await kit.getByTestId('sheet-table-new').count()) === 1 && (await kit.getByTestId('sheet-table-convert').count()) === 0)
    await closeMenus()
    await click('B3')
    await openGallery()
    step('TS-C8 选中已套用表格样式的单元格：底部增加「转换为区域」「清除表格样式」（WPS：「新建表格样式…」仍在）', (await kit.getByTestId('sheet-table-convert').count()) === 1 && (await kit.getByTestId('sheet-table-clear').count()) === 1)
    await closeMenus()
    await tableTab()
    const text = await kit.locator('[data-testid^="sheet-table-opt-"]').count()
    step('TS-C18/C20 「表格工具」选项卡：标题行/汇总行/镶边行/第一列/最后一列/镶边列（+筛选按钮），当前勾选与表格一致', text === 7 && (await kit.getByTestId('sheet-table-opt-header').isChecked()) && (await kit.getByTestId('sheet-table-opt-banded-rows').isChecked()) && !(await kit.getByTestId('sheet-table-opt-first-col').isChecked()))
    step('TS-C18 选项卡不遮挡选区，选区仍为 B3', (await details()).active === 'B3')
    await home()
  })

  await section('TS-C9/C15 多区域、受保护工作表时入口置灰', async () => {
    await dataset()
    await click('A1')
    await page.keyboard.down(mod)
    await click('C3')
    await page.keyboard.up(mod)
    await settle(200)
    step('TS-C9 多个非连续区域：「套用表格样式」置灰', await kit.getByTestId('sheet-format-table').isDisabled())
    await kit.getByTestId('sheet-format-table').click({ force: true }).catch(() => undefined)
    step('TS-C9 点击无响应，不弹出面板', (await kit.getByTestId('sheet-table-gallery').count()) === 0)
    await click('B3')
    await exec('review.protectSheet', {})
    await settle(200)
    step('TS-C9/C15 受保护（锁定）的工作表：入口置灰，数据与格式不变', (await kit.getByTestId('sheet-format-table').isDisabled()) && (await tables()).length === 0)
    await exec('review.protectSheet', {})
  })

  await section('TS-C13/C14/C96 含合并单元格的范围', async () => {
    await dataset()
    await exec('home.mergeCells', { range: 'A2:B3' })
    await click('C5')
    await apply('TableStyleMedium2')
    const sheet = (await workbook()).sheets.find((s) => s.name === '工作表1')
    step('TS-C13/C14/C96 自动解除合并，以完整矩形 A1:D10 生成表格', (await tableRange()) === 'A1:D10' && (sheet?.merges ?? []).length === 0, JSON.stringify(sheet?.merges ?? null))
    step('TS-C96 原合并内容保留在左上角单元格', (await value('A2')) === 'n2')
  })

  await section('TS-C16 扩充区域已有表格', async () => {
    await asTable()
    await click('E3')
    await openGallery()
    await kit.getByTestId('sheet-table-TableStyleLight9').click()
    await kit.getByTestId('sheet-fat').waitFor()
    await kit.getByTestId('sheet-fat-table').check()
    await kit.getByTestId('sheet-fat-ok').click()
    step('TS-C16 提示目标区域已有表，不生成新的表格样式，原样式不被覆盖', (await noticeShown('表格不能与另一个表格重叠')) && (await tables()).length === 1 && (await tables())[0].style === 'TableStyleMedium2')
    await dismiss()
  })

  // ———————————————— Managing table styles (Table Tools and New Table Style) ————————————————

  await section('TS-C21/C31/C32 选择其他样式，当前样式选中态', async () => {
    await asTable()
    await tableTab()
    await kit.getByTestId('sheet-table-design-styles').click()
    await kit.getByTestId('sheet-table-design-TableStyleLight9').click()
    await settle(300)
    step('TS-C21 应用新样式，作用范围与数据不变', (await tables())[0].style === 'TableStyleLight9' && (await tableRange()) === 'A1:D10' && (await value('C3')) === 30)
    await kit.getByTestId('sheet-table-design-styles').click()
    await kit.getByTestId('sheet-table-design-gallery').waitFor()
    step('TS-C21/C31 再次打开，当前样式为最后应用的样式（选中态）', (await kit.getByTestId('sheet-table-design-TableStyleLight9').getAttribute('aria-pressed')) === 'true')
    await closeMenus()
    await exec('table.newStyle', { name: '临时样式', elements: { headerRow: { fill: '#334455' } } })
    await exec('table.style', { style: '临时样式' })
    await kit.getByTestId('sheet-table-design-styles').click()
    await kit.getByTestId('sheet-table-design-gallery').waitFor()
    step('TS-C32 不在预设中的样式：预设列表无选中态', (await kit.getByTestId('sheet-table-design-gallery').locator('.sheet-table-group:not([data-testid$="custom"]) [aria-pressed="true"]').count()) === 0)
    await closeMenus()
    await home()
  })

  await section('TS-C23/C24/C25/C26/C27/C38/C114/C116 样式选项勾选与优先级', async () => {
    await asTable()
    const look = await looks('TableStyleMedium2')
    await tableTab()
    await kit.getByTestId('sheet-table-opt-banded-rows').uncheck()
    await settle(250)
    step('TS-C24/C114 去勾选「镶边行」即时生效（无需确认）：第 2 行不再着色', near(await shownFill('B2'), WHITE) && (await details()).tableInfo.bandedRows === false)
    await kit.getByTestId('sheet-table-opt-banded-cols').check()
    await settle(250)
    step('TS-C24 仅「镶边列」：A 列着色、B 列不着色', near(await shownFill('A5'), look.band) && near(await shownFill('B5'), WHITE))
    await kit.getByTestId('sheet-table-opt-banded-rows').check()
    await settle(250)
    step('TS-C25 镶边行+镶边列：同一单元格两者都着色时颜色不叠加（交替行优先）', near(await shownFill('B2'), look.band) && near(await shownFill('A3'), look.band) && near(await shownFill('B3'), WHITE))
    await kit.getByTestId('sheet-table-opt-first-col').check()
    await settle(250)
    step('TS-C26/C27 标题行 > 第一列：A1 仍为标题行颜色', near(await shownFill('A1'), look.header) && (await details()).tableInfo.firstCol === true)
    await kit.getByTestId('sheet-table-opt-last-col').check()
    await kit.getByTestId('sheet-table-opt-totals').check()
    await settle(300)
    step('TS-C38 勾选「最后一列」「汇总行」：汇总行加在数据下方（A1:D11）', (await tableRange()) === 'A1:D11' && (await details()).tableInfo.lastCol === true && (await value('A11')) === 'Total')
    await kit.getByTestId('sheet-table-opt-totals').uncheck()
    await kit.getByTestId('sheet-table-opt-last-col').uncheck()
    await settle(300)
    step('TS-C38 取消勾选后恢复', (await tableRange()) === 'A1:D10' && (await details()).tableInfo.lastCol === false)
    for (const id of ['header', 'banded-rows', 'banded-cols', 'first-col']) await kit.getByTestId(`sheet-table-opt-${id}`).uncheck()
    await settle(300)
    step('TS-C23 全部去勾选：行列不再渲染条纹，标题行移出表格（A2:D10）', near(await shownFill('B3'), WHITE) && near(await shownFill('B4'), WHITE) && (await tableRange()) === 'A2:D10', await tableRange())
    step('TS-C116 状态变化只通过控件自身反馈，无引导浮层或对话框', (await kit.locator('[role="dialog"]').count()) === 0)
    await home()
  })

  await section('TS-C11/C22/C33/C34/C115/C29/C30/C35 新建表格样式、自定义、设为默认', async () => {
    await dataset()
    await openGallery()
    await kit.getByTestId('sheet-table-new').click()
    await kit.getByTestId('sheet-nts').waitFor()
    step('TS-C11 「新建表格样式…」收起面板并打开对话框：名称、9 个表元素、格式、预览、设为默认，底部取消/确定', (await kit.getByTestId('sheet-table-gallery').count()) === 0 && (await kit.locator('[data-testid^="sheet-nts-el-"]').count()) === 9 && (await kit.getByTestId('sheet-nts-default').count()) === 1)
    await kit.getByTestId('sheet-nts-name').fill('我的样式')
    await kit.getByTestId('sheet-nts-el-headerRow').click()
    await kit.getByTestId('sheet-nts-fill').click()
    step('TS-C34 颜色选择：「无颜色」、色板、「其他颜色」', (await kit.getByTestId('sheet-nts-fill-none').count()) === 1 && (await kit.getByTestId('sheet-nts-fill-colors').locator('button').count()) > 20 && (await kit.getByTestId('sheet-nts-fill-hex').count()) === 1)
    await kit.getByTestId('sheet-nts-fill-hex').fill('12345')
    await kit.getByTestId('sheet-nts-fill-hex-ok').click()
    step('TS-C33 非法色值有提示', await noticeShown('#'))
    await kit.getByTestId('sheet-nts-fill-hex').fill('#123456')
    await kit.getByTestId('sheet-nts-fill-hex-ok').click()
    await kit.getByTestId('sheet-nts-bold').check()
    await kit.getByTestId('sheet-nts-el-firstRowStripe').click()
    await kit.getByTestId('sheet-nts-fill').click()
    await kit.getByTestId('sheet-nts-fill-hex').fill('#EEDDCC')
    await kit.getByTestId('sheet-nts-fill-hex-ok').click()
    step('TS-C34 选色后预览同步刷新', (await kit.getByTestId('sheet-nts-preview').innerHTML()).includes('rgb(18, 52, 86)'))
    await kit.getByTestId('sheet-nts-ok').click()
    await settle(300)
    await openGallery()
    step('TS-C22/C115 添加成功：「自定义」区出现新样式缩略图，可直接被选中', (await kit.getByTestId('sheet-table-我的样式').count()) === 1)
    step('TS-C29 未设置默认样式时默认为备选样式的第一个推荐（Medium2，★）', (await kit.locator('[data-testid="sheet-table-TableStyleMedium2"]').locator('xpath=..').getAttribute('data-default')) === 'true')
    await kit.getByTestId('sheet-table-我的样式').click()
    await kit.getByTestId('sheet-fat').waitFor()
    await kit.getByTestId('sheet-fat-table').check()
    await kit.getByTestId('sheet-fat-ok').click()
    await settle(300)
    step('TS-C22/C33 点击自定义缩略图复用：表格按输入色值渲染', (await tables())[0]?.style === '我的样式' && near(await shownFill('A1'), '#123456') && near(await shownFill('A2'), '#EEDDCC'))
    await click('F2')
    await openGallery()
    await kit.getByTestId('sheet-table-我的样式').click({ button: 'right' })
    await kit.getByTestId('sheet-table-menu-default').click()
    await settle(250)
    const g = await exec('table.styles', {})
    step('TS-C30/C35 右键「设为默认值」：该样式成为默认（★）', g.default === '我的样式')
    await ai([{ command: 'range.setValues', params: { range: 'F1', values: [['x', 'y'], [1, 2]] } }])
    await exec('home.formatAsTable', { range: 'F1:G2' })
    step('TS-C35 之后新建表格默认应用该样式', (await tables()).some((t) => t.range === 'F1:G2' && t.style === '我的样式'))
    await saveAndReopen()
    step('TS-C35 重新打开文档后默认样式与自定义样式保持（.xlsx 的 tableStyles）', (await exec('table.styles', {})).default === '我的样式' && (await exec('table.styles', {})).custom.some((s) => s.name === '我的样式'))
    step('TS-C22 重新打开后表格仍按自定义样式渲染', near(await shownFill('A1'), '#123456'))
  })

  await section('TS-C37 修改自定义样式后回显', async () => {
    await dataset()
    await exec('table.newStyle', { name: '可改样式', elements: { headerRow: { fill: '#336699' } } })
    await exec('home.formatAsTable', { range: 'A1:D10', style: '可改样式' })
    await openGallery()
    await kit.getByTestId('sheet-table-可改样式').click({ button: 'right' })
    await kit.getByTestId('sheet-table-menu-modify').click()
    await kit.getByTestId('sheet-nts').waitFor()
    step('TS-C37 再次打开时名称与各部分配置按上次回显', (await kit.getByTestId('sheet-nts-name').inputValue()) === '可改样式' && (await kit.getByTestId('sheet-nts-el-headerRow').getAttribute('data-set')) === 'true')
    await kit.getByTestId('sheet-nts-el-headerRow').click()
    await kit.getByTestId('sheet-nts-fill').click()
    await kit.getByTestId('sheet-nts-fill-hex').fill('#993300')
    await kit.getByTestId('sheet-nts-fill-hex-ok').click()
    await kit.getByTestId('sheet-nts-ok').click()
    await settle(300)
    step('TS-C37 「确定」后表格渲染与配置一致', near(await shownFill('A1'), '#993300'))
  })

  await section('TS-C28/C118 自定义样式 hover 出现 × 删除', async () => {
    await dataset()
    await exec('table.newStyle', { name: '删我', elements: { headerRow: { fill: '#AA0000' } } })
    await exec('home.formatAsTable', { range: 'A1:D10', style: '删我' })
    await openGallery()
    await kit.getByTestId('sheet-table-删我').hover()
    step('TS-C28/C118 hover 自定义样式右上角显示 ×', await kit.getByTestId('sheet-table-delete-删我').isVisible())
    await kit.getByTestId('sheet-table-delete-删我').click()
    await until(async () => (await kit.getByTestId('sheet-table-删我').count()) === 0)
    step('TS-C28 点击 × 删除该自定义样式，使用它的表格改用默认样式', !(await exec('table.styles', {})).custom.some((c) => c.name === '删我') && (await tables())[0].style === 'TableStyleMedium2')
    await closeMenus()
  })

  await section('TS-C36/C118/C121 清除表格样式', async () => {
    await asTable()
    await click('B3')
    await openGallery()
    await kit.getByTestId('sheet-table-clear').hover()
    const red = await kit.getByTestId('sheet-table-clear').evaluate((element) => getComputedStyle(element).color)
    step('TS-C36/C118 hover「清除表格样式」文字变红', /rgb\(21[0-9], 4[0-9], 3[0-9]\)/.test(red), red)
    await kit.getByTestId('sheet-table-clear').click()
    await settle(300)
    step('TS-C36/C121 清除后区域不再显示表格样式，数据不丢失', near(await shownFill('A1'), WHITE) && near(await shownFill('A2'), WHITE) && (await value('C3')) === 30)
    step('TS-C36 默认样式不受影响', (await exec('table.styles', {})).default === 'TableStyleMedium2')
  })

  // ———————————————— The table's range (Resize Table) ————————————————

  await section('TS-C39/C48/C49/C113 重新选择作用范围', async () => {
    await asTable()
    await tableTab()
    await kit.getByTestId('sheet-table-resize').click()
    await kit.getByTestId('sheet-resize').waitFor()
    step('TS-C39 作用范围输入框显示当前范围', (await kit.getByTestId('sheet-resize-range').inputValue()) === '=$A$1:$D$10')
    await kit.getByTestId('sheet-resize-range').fill('a1:c5')
    await kit.getByTestId('sheet-resize-ok').click()
    await until(async () => (await tableRange()) === 'A1:C5')
    step('TS-C39/C48 手动输入 a1:c5 识别为 A1:C5，范围更新，无报错', (await tableRange()) === 'A1:C5' && (await kit.getByTestId('sheet-resize').count()) === 0)
    step('TS-C39/C113 新范围即时渲染表格样式，范围外恢复无样式', near(await shownFill('D3'), WHITE) && !near(await shownFill('A2'), WHITE))
    await kit.getByTestId('sheet-table-resize').click()
    await kit.getByTestId('sheet-resize-range').fill('$A$1:$D$1048999')
    await kit.getByTestId('sheet-resize-ok').click()
    await until(async () => (await tableRange()) === 'A:D')
    step('TS-C49 超过最大行自动截取到最大范围（A1:D1048576，即 A:D）', (await tableRange()) === 'A:D', await tableRange())
    await exec('table.resize', { range: 'A1:D10' })
    await home()
  })

  await section('TS-C40/C44/C45/C46/C47/C50/C51 非法作用范围', async () => {
    await asTable()
    await ai([
      { command: 'range.setValues', params: { range: 'H1', values: [['h1', 'h2'], [1, 2]] } },
      { command: 'home.formatAsTable', params: { range: 'H1:I2', style: 'TableStyleLight1' } },
      { command: 'selection.set', params: { range: 'K1:K3' } },
      { command: 'cell.set', params: { cell: 'K1', input: '=ROW(K1:K3)', array: true } },
    ])
    await click('B3')
    await tableTab()
    for (const [id, bad] of [['C40', '工作表2!A1:B4'], ['C44/C45', 'A1:I10'], ['C46/C47', 'A1:K10'], ['C50', 'B1:A0'], ['C50', '1:2'], ['C51', '']]) {
      await kit.getByTestId('sheet-table-resize').click()
      await kit.getByTestId('sheet-resize').waitFor()
      await kit.getByTestId('sheet-resize-range').fill(bad)
      await kit.getByTestId('sheet-resize-ok').click()
      await settle(300)
      const shown = (await kit.locator('.kui-toast').allTextContents()).join(' ')
      step(`TS-${id} 输入「${bad}」：提示并恢复原区域，表格样式不被破坏`, shown.length > 0 && (await kit.getByTestId('sheet-resize-range').inputValue()) === '=$A$1:$D$10' && (await tableRange()) === 'A1:D10', shown.slice(0, 80))
      await dismiss()
      await kit.getByTestId('sheet-resize-cancel').click()
    }
    await home()
  })

  // ———————————————— Changing the range's content ————————————————

  await section('TS-C53/C56/C122 单元格样式优先于表格样式', async () => {
    await asTable()
    const look = await looks('TableStyleMedium2')
    await click('B2')
    await kit.getByTestId('sheet-fill-apply').click()
    await settle(250)
    const own = await ownFill('B2')
    step('TS-C53/C56 表样式单元格再加填充色：单元格样式优先', own !== null && near(await shownFill('B2'), own))
    await exec('edit.clearFormats', { range: 'B2' })
    await settle(250)
    step('TS-C53/C122 清掉单元格样式后显示表样式，其余区域不变，表格仍在', near(await shownFill('B2'), look.band) && (await tables()).length === 1)
  })

  await section('TS-C54/C55 格式刷与复制粘贴带样式单元格到表样式单元格', async () => {
    await asTable()
    await ai([
      { command: 'cell.set', params: { cell: 'H1', input: '源' } },
      { command: 'home.fillColor', params: { range: 'H1', color: '#FF0000' } },
      { command: 'home.bold', params: { range: 'H1', on: true } },
    ])
    await click('H1')
    await kit.getByTestId('sheet-format-painter').click()
    await click('B3')
    await settle(250)
    step('TS-C54 格式刷刷到表样式单元格：应用被刷入的样式，表格范围不变', near(await shownFill('B3'), '#FF0000') && (await tableRange()) === 'A1:D10')
    await exec('edit.copy', { range: 'H1' })
    await exec('edit.paste', { at: 'C4' })
    await settle(250)
    step('TS-C55 复制粘贴带样式单元格：应用被粘贴的样式，范围不扩充', near(await shownFill('C4'), '#FF0000') && (await tableRange()) === 'A1:D10')
    await click('A1')
    await kit.getByTestId('sheet-format-painter').click()
    await click('F1')
    await settle(250)
    const header = (await looks('TableStyleMedium2')).header
    step('TS-X2 从表格单元格刷到普通单元格：带上表格样式的外观（WPS/Excel）', (await ownFill('F1')) !== null && near(await shownFill('F1'), header))
  })

  await section('TS-C57/C62/C63 右/下侧输入扩充，上/左侧与已有数据不扩充', async () => {
    await asTable()
    await click('B11')
    await type('新行')
    step('TS-C57 表样式下方空白单元格输入：向下扩充（A1:D11）', (await tableRange()) === 'A1:D11', await tableRange())
    await click('E4')
    await type('备注')
    step('TS-C57 右侧空白单元格输入：向右扩充（A1:E11），新列标题自动命名「列5」', (await tableRange()) === 'A1:E11' && (await value('E1')) === '列5', `${await tableRange()} ${await value('E1')}`)
    const look = await looks('TableStyleMedium2')
    step('TS-C57 新增单元格继承交替渲染', near(await shownFill('E2'), look.band) && near(await shownFill('E3'), WHITE))
    await ai([{ command: 'cell.set', params: { cell: 'A13', input: '已有' } }])
    await click('A13')
    await type('改')
    step('TS-C63 已有数据的单元格再输入不扩充', (await tableRange()) === 'A1:E11')
    await ai([{ command: 'home.insertRows', params: { rows: '1' } }])
    await click('B1')
    await type('上方')
    await click('G5')
    await type('隔一列')
    step('TS-C62 上方与不相邻处输入不扩充，表格上/左边界不变', (await tableRange()) === 'A2:E12', await tableRange())
  })

  await section('TS-C58/C64/C61 粘贴扩充、粘贴超出范围与填充', async () => {
    await asTable()
    await exec('edit.paste', { at: 'A11', text: 'p\t1\nq\t2\n' })
    await settle(200)
    step('TS-C58 下方粘贴一片范围：向下扩充，内容完整', (await tableRange()) === 'A1:D12' && (await value('A12')) === 'q')
    await exec('edit.paste', { at: 'B12', text: 'x\ny\nz\n' })
    step('TS-C64 粘贴在表内并超出：不扩充，超出部分正常落地', (await tableRange()) === 'A1:D12' && (await value('B14')) === 'z')
    await exec('edit.fillDown', { range: 'C12:C16' })
    step('TS-C61 下拉填充不扩充，填充内容正常', (await tableRange()) === 'A1:D12' && (await value('C16')) === (await value('C12')))
  })

  await section('TS-C59/C67/C69/C70/C71/C89 插入删除行列与单元格', async () => {
    await asTable()
    await exec('home.insertColumns', { cols: 'D' })
    step('TS-C59 表样式边缘列向表样式一侧插入一列：扩充，原数据不变', (await tableRange()) === 'A1:E10' && (await value('E3')) === 21)
    await exec('home.insertColumns', { cols: 'G' })
    step('TS-C67 向另一侧插入一列：不扩充', (await tableRange()) === 'A1:E10')
    await exec('home.insertRows', { rows: '5' })
    step('TS-C69 表样式范围内插入行：扩充', (await tableRange()) === 'A1:E11')
    const look = await looks('TableStyleMedium2')
    step('TS-C69 交替渲染正确（插入行后第 4、5、6 行交替）', near(await shownFill('A4'), look.band) && near(await shownFill('A5'), WHITE) && near(await shownFill('A6'), look.band))
    await exec('home.deleteRows', { rows: '5' })
    await exec('home.deleteColumns', { cols: 'D' })
    step('TS-C70/C89 删除表样式所在行列：范围收缩，数据不变', (await tableRange()) === 'A1:D10' && (await value('D3')) === 21)
    await exec('home.insertCells', { range: 'B3', shift: 'down' })
    await exec('home.deleteCells', { range: 'B3', shift: 'up' })
    step('TS-C71 表内插入/删除单元格：表格范围不变，内容按规则移动', (await tableRange()) === 'A1:D10' && (await value('B3')) === '西')
  })

  // ———————————————— How the display is inherited (header row) ————————————————

  await section('TS-C73/C74/C75/C77/C78 标题行内容', async () => {
    await build(numbered(2), { fill: false })
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: [[null, '地区', '=1+1', 'link'], [1, 2, 3, 4], [5, 6, 7, 8]] } },
      { command: 'home.bold', params: { range: 'B1', on: true } },
      { command: 'home.fontColor', params: { range: 'B1', color: '#00AA00' } },
      { command: 'insert.link', params: { cell: 'D1', target: 'https://example.com' } },
      { command: 'selection.set', params: { range: 'B2' } },
    ])
    await apply('TableStyleMedium2')
    step('TS-C73 空白标题单元格自动命名「列1」（WPS 中文界面）', (await value('A1')) === '列1', String(await value('A1')))
    step('TS-C75 文本标题保留，不做替换', (await value('B1')) === '地区')
    step('TS-C74 带格式的标题保留其格式', (await style('B1')).font?.bold === true && hex((await style('B1')).font?.color) === '#00AA00')
    const c1 = await cellOf('C1')
    step('TS-C77 公式标题：清除公式，仅保留值（WPS/Excel 标题不能是公式；不弹提示）', String(c1.value) === '2' && !c1.formula, JSON.stringify(c1))
    const d1 = await cellOf('D1')
    step('TS-C78 链接标题：内容对象不被清除', d1.value === 'link' && JSON.stringify(d1).includes('example.com'), JSON.stringify(d1).slice(0, 160))
  })

  await section('TS-C84/C85 剪切/复制公式单元格到标题行', async () => {
    await asTable()
    await ai([{ command: 'cell.set', params: { cell: 'H1', input: '=10*3' } }])
    await exec('edit.copy', { range: 'H1' })
    await exec('edit.paste', { at: 'B1' })
    await settle(200)
    const b1 = await cellOf('B1')
    step('TS-C85 复制公式到标题行：标题为公式的值（WPS/Excel：标题不保留公式），源单元格不变', String(b1.value) === '30' && !b1.formula && (await cellOf('H1')).formula === '=10*3', JSON.stringify(b1))
    await exec('edit.cut', { range: 'H1' })
    await exec('edit.paste', { at: 'C1' })
    await settle(200)
    step('TS-C84 剪切公式到标题行：标题为值（与 B1 的「30」重名，按 Excel/WPS 变为「302」），标题行样式保持', String(await value('C1')) === '302' && near(await shownFill('C1'), (await looks('TableStyleMedium2')).header) && (await value('H1')) == null, `${JSON.stringify(await cellOf('C1')).slice(0, 120)} ${await shownFill('C1')} ${await value('H1')}`)
  })

  // ———————————————— Merging, Paste Special, hiding, filtering, grouping, sorting, removing duplicates, copies ————————————————

  await section('TS-C97 表样式范围内合并单元格', async () => {
    await asTable()
    await click('B3')
    await page.keyboard.down('Shift')
    await click('C4')
    await page.keyboard.up('Shift')
    await settle(200)
    await kit.getByTestId('sheet-merge').click()
    await settle(200)
    step('TS-C97 合并单元格置灰不可点', (await kit.getByTestId('sheet-cmd-home.mergeCells').isDisabled()) && (await kit.getByTestId('sheet-cmd-home.mergeCenter').isDisabled()))
    await closeMenus()
    const r = await ai([{ command: 'home.mergeCells', params: { range: 'B3:C4' } }]).catch((error) => String(error))
    const sheet = (await workbook()).sheets.find((s) => s.name === '工作表1')
    step('TS-C97 其他方式合并不生效并说明「表格中的单元格不能合并」', JSON.stringify(r).includes("can't be merged") && !(sheet.merges ?? []).length, JSON.stringify(r).slice(0, 160))
  })

  await section('TS-C99 复制整个表样式范围进行选择性粘贴', async () => {
    await asTable()
    // An agent's selection and clipboard last one run (KHP C3b), so its copy and paste go together.
    await ai([{ command: 'edit.copy', params: { range: 'A1:D10' } }, { command: 'selection.set', params: { range: 'H1' } }, { command: 'edit.pasteSpecial', params: { what: 'values' } }])
    await settle(200)
    step('TS-C99 选择性粘贴（值）：不带表样式，不生成新表格，源表不变', (await value('H1')) === '名称' && (await ownFill('H1')) === null && (await tables()).length === 1 && (await tableRange()) === 'A1:D10')
  })

  await section('TS-C100/C101 隐藏行列与标题行', async () => {
    await asTable()
    const look = await looks('TableStyleMedium2')
    await exec('home.hideRows', { rows: '3' })
    await settle(250)
    step('TS-C100 隐藏第 3 行：交替渲染按可见行连续计算（屏幕上第 3 行位置是第 4 行，不着色，下一行着色）', near(await shownFill('A4', 2), WHITE) && near(await shownFill('A5', 3), look.band))
    await exec('home.unhideRows', { rows: '3' })
    await settle(250)
    step('TS-C100 取消隐藏后完全恢复', near(await shownFill('A3'), WHITE) && near(await shownFill('A4'), look.band))
    await exec('home.hideRows', { rows: '1' })
    await settle(250)
    step('TS-C101 标题行可隐藏，不影响表样式渲染', near(await shownFill('A2', 0), look.band) && (await tableRange()) === 'A1:D10')
    await exec('home.unhideRows', { rows: '1' })
    await settle(250)
    step('TS-C101 取消隐藏后标题行样式完整恢复', near(await shownFill('A1'), look.header))
  })

  await section('TS-C104/C105 分组收起', async () => {
    await asTable()
    const look = await looks('TableStyleMedium2')
    // A row group widens the gutter left of the row headings by two outline slots (32 px).
    const at = (ref, screenRow) => shownFill(ref, screenRow, 34)
    await exec('data.group', { rows: '3:4' })
    await exec('data.hideDetail', { rows: '3:4' })
    await settle(300)
    step('TS-C104 收起第 3–4 行后交替渲染按可见行连续计算（第 5 行在屏幕第 3 行，不着色；第 6 行着色）', near(await at('A5', 2), WHITE) && near(await at('A6', 3), look.band), `${await at('A5', 2)} ${await at('A6', 3)}`)
    await exec('data.showDetail', { rows: '3:4' })
    await settle(300)
    step('TS-C104 展开后恢复', near(await at('A4', 3), look.band) && near(await at('A3', 2), WHITE))
    await exec('data.ungroup', { rows: '3:4' })
    await exec('data.group', { rows: '1' })
    await exec('data.hideDetail', { rows: '1' })
    await settle(300)
    step('TS-C105 标题行可收起，表样式不受影响', (await tableRange()) === 'A1:D10' && near(await at('A2', 0), look.band), `${await tableRange()} ${await at('A2', 0)} ${await at('A2', 1)} ${look.band}`)
    await exec('data.showDetail', { rows: '1' })
    await settle(300)
    step('TS-C105 展开后标题行样式完整恢复', near(await at('A1', 0), look.header), `${await at('A1', 0)} ${look.header}`)
  })

  await section('TS-C106/C107/C108 排序与去重', async () => {
    await asTable()
    const look = await looks('TableStyleMedium2')
    await click('C3')
    await exec('data.sortDescending', { column: 'C' })
    await settle(300)
    step('TS-C106/C107 排序：数据重排，标题行不参与，范围不变，交替渲染不错位', (await value('A1')) === '名称' && (await value('C2')) === 100 && (await tableRange()) === 'A1:D10' && near(await shownFill('A2'), look.band) && near(await shownFill('A3'), WHITE))
    await ai([{ command: 'range.setValues', params: { range: 'A3', values: [['n10', '东', 100, 70]] } }])
    await click('B5')
    await exec('table.removeDuplicates', {})
    await settle(300)
    step('TS-C108 去重：重复行删除，标题行保留，样式按新行数计算', (await value('A1')) === '名称' && (await tableRange()) === 'A1:D9' && near(await shownFill('A2'), look.band), await tableRange())
  })

  await section('TS-C109 创建工作表副本', async () => {
    await asTable()
    await exec('sheet.move', { sheet: '工作表1', to: 2, copy: true })
    await settle(300)
    const all = (await workbook()).sheets
    const copy = all.find((s) => s.name !== '工作表1' && s.tables?.length)
    step('TS-C109 副本带上表样式，范围与样式一致，修改副本不影响源', copy && copy.tables[0].range === 'A1:D10' && copy.tables[0].style === 'TableStyleMedium2' && all.find((s) => s.name === '工作表1').tables.length === 1, JSON.stringify(copy?.tables ?? null))
  })

  // ———————————————— Clear ————————————————

  await section('TS-C122/C123/C124/C125/C126/C127 清除', async () => {
    await asTable()
    const look = await looks('TableStyleMedium2')
    await exec('edit.clearContents', { range: 'B3:C4' })
    await settle(200)
    step('TS-C124 选中部分清除内容：只清内容，交替渲染保留', (await value('B3')) == null && near(await shownFill('B4'), look.band) && (await tables()).length === 1)
    await exec('edit.clearAll', { range: 'C5:D6' })
    await settle(200)
    step('TS-C126 选中部分全部清除：内容与单元格样式清除，表格范围保留', (await value('C5')) == null && (await tableRange()) === 'A1:D10' && near(await shownFill('C6'), look.band))
    await exec('edit.clearContents', { range: 'A1:D10' })
    await settle(200)
    step('TS-C125 全选清除内容：不清除表样式，标题行按规则重置为「列1…」', (await tables()).length === 1 && (await value('A1')) === '列1' && near(await shownFill('A2'), look.band), String(await value('A1')))
    await exec('edit.clearFormats', { range: 'A1:D10' })
    await settle(200)
    step('TS-C123 全选清除格式：清除表样式，内容保留', near(await shownFill('A1'), WHITE) && (await value('A1')) === '列1')
    await exec('edit.clearAll', { range: 'A1:D10' })
    await settle(200)
    await click('B3')
    await openGallery()
    step('TS-C127 全选全部清除：表格样式、格式与内容都清除，下拉底部恢复为「新建表格样式」', (await tables()).length === 0 && (await kit.getByTestId('sheet-table-convert').count()) === 0)
    await closeMenus()
  })

  // ———————————————— Convert to Range ————————————————

  await section('TS-C129/C130/C131 转为普通区域', async () => {
    await asTable()
    const look = await looks('TableStyleMedium2')
    await click('B3')
    await openGallery()
    await kit.getByTestId('sheet-table-convert').click()
    await settle(300)
    step('TS-C129 转换后表格规则移除，样式持久化到单元格', (await tables()).length === 0 && (await ownFill('A1')) !== null && (await ownFill('A2')) !== null && (await ownFill('A3')) === null)
    step('TS-C130 外观与转换前一致', near(await shownFill('A1'), look.header) && near(await shownFill('A2'), look.band) && near(await shownFill('A3'), WHITE))
    await saveAndReopen()
    step('TS-C130 重新打开文档后外观一致', near(await shownFill('A1'), look.header) && (await tables()).length === 0)
    await click('B3')
    await openGallery()
    step('TS-C131 再次选中：下拉底部只有「新建表格样式…」', (await kit.getByTestId('sheet-table-convert').count()) === 0)
    await kit.getByTestId('sheet-table-TableStyleLight9').click()
    await kit.getByTestId('sheet-fat-table').check()
    await kit.getByTestId('sheet-fat-ok').click()
    await settle(300)
    step('TS-C131 再次套用预设样式可正常生成，不提示「已有表」', (await tables()).length === 1 && (await kit.locator('.kui-toast').count()) === 0)
  })

  await section('TS-X3 导入：保存后重新打开，表格样式与选项保持', async () => {
    await asTable('TableStyleDark2')
    await ai([
      { command: 'table.firstColumn', params: { on: true } },
      { command: 'table.bandedColumns', params: { on: true } },
    ])
    const before = await shownFill('B1')
    await saveAndReopen()
    await click('B3')
    const t = (await details()).tableInfo
    step('TS-X3 重新打开：样式 Dark2、第一列与镶边列选项保持，渲染一致', t?.style === 'TableStyleDark2' && t.firstCol && t.bandedCols && near(await shownFill('B1'), before), JSON.stringify(t))
  })

  // ———————————————— Format Cells ————————————————

  const formatCells = () => kit.getByTestId('sheet-format-cells')
  const openFC = async (how) => {
    if (how === 'shortcut') {
      await kit.locator('.kui-text-sink').first().focus()
      await page.keyboard.press(`${mod}+1`)
    } else if (how === 'menu') {
      await page.mouse.click((await region()).x + 46 + 1 * G.cw + 32, (await region()).y + 21 + 1 * G.rh + 10, { button: 'right' })
      await kit.getByTestId('sheet-grid-menu').waitFor()
      await kit.getByTestId('sheet-grid-menu').getByTestId('sheet-cmd-home.formatCells').click()
    } else {
      await home()
      await kit.getByTestId('sheet-format-menu').click()
      await kit.getByTestId('sheet-cmd-home.formatCells').click()
    }
    await formatCells().waitFor({ timeout: 5000 })
  }

  await section('FC-X1 打开设置单元格格式', async () => {
    await dataset()
    await click('B2')
    await openFC('shortcut')
    const tabs = await kit.locator('[data-testid^="fc-tab-"]').allTextContents()
    step('FC-X1 Cmd/Ctrl+1 打开「单元格格式」：数字/对齐/字体/边框/图案/保护', same(tabs, ['数字', '对齐', '字体', '边框', '图案', '保护']), JSON.stringify(tabs))
    await page.keyboard.press('Escape')
    await until(async () => (await formatCells().count()) === 0)
    step('FC-X1 Esc 关闭', (await formatCells().count()) === 0)
    await openFC('menu')
    step('FC-X1 单元格右键「设置单元格格式…」打开', (await formatCells().count()) === 1)
    await kit.getByTestId('fc-cancel').click()
    await openFC('ribbon')
    step('FC-X1 开始 › 格式 › 设置单元格格式打开（命令请求的对话框不再被丢弃）', (await formatCells().count()) === 1)
    await kit.getByTestId('fc-cancel').click()
  })

  await section('FC-X2 数字', async () => {
    await dataset()
    await ai([{ command: 'cell.set', params: { cell: 'B2', input: '1234.5' } }])
    await click('B2')
    await openFC('shortcut')
    await kit.getByTestId('fc-cat-number').click()
    await kit.getByTestId('fc-decimals').fill('2')
    await kit.getByTestId('fc-thousands').check()
    await until(async () => (await kit.getByTestId('fc-sample').textContent()) === '1,234.50')
    step('FC-X2 数值、2 位小数、千位分隔符：示例 1,234.50', (await kit.getByTestId('fc-sample').textContent()) === '1,234.50', await kit.getByTestId('fc-sample').textContent())
    await kit.getByTestId('fc-ok').click()
    await settle(250)
    step('FC-X2 确定后格式为 #,##0.00', (await style('B2')).num_fmt === '#,##0.00', (await style('B2')).num_fmt)
    await openFC('shortcut')
    step('FC-X2 再次打开回显「数值」', (await kit.getByTestId('fc-cat-number').getAttribute('aria-selected')) === 'true' && (await kit.getByTestId('fc-thousands').isChecked()))
    await kit.getByTestId('fc-cat-percent').click()
    await kit.getByTestId('fc-decimals').fill('1')
    await kit.getByTestId('fc-ok').click()
    await settle(250)
    step('FC-X2 百分比 1 位：0.0%', (await style('B2')).num_fmt === '0.0%')
    await openFC('shortcut')
    await kit.getByTestId('fc-cat-custom').click()
    await kit.getByTestId('fc-code').fill('0.000')
    await kit.getByTestId('fc-ok').click()
    await settle(250)
    step('FC-X2 自定义格式代码', (await style('B2')).num_fmt === '0.000')
  })

  await section('FC-X3 对齐', async () => {
    await dataset()
    await click('B2')
    await page.keyboard.down('Shift')
    await click('C3')
    await page.keyboard.up('Shift')
    await openFC('shortcut')
    await kit.getByTestId('fc-tab-alignment').click()
    await kit.getByTestId('fc-h').selectOption('Center')
    await kit.getByTestId('fc-v').selectOption('Top')
    await kit.getByTestId('fc-wrap').check()
    await kit.getByTestId('fc-merge').check()
    await kit.getByTestId('fc-ok').click()
    await settle(300)
    const sheet = (await workbook()).sheets.find((s) => s.name === '工作表1')
    step('FC-X3 水平居中、靠上、自动换行、合并单元格', (await style('B2')).align?.h === 'Center' && (await style('B2')).align?.v === 'Top' && (await style('B2')).align?.wrap === true && JSON.stringify(sheet.merges ?? []).includes('B2:C3'), JSON.stringify(sheet.merges))
    await kit.getByTestId('sheet-undo').click()
    await settle(250)
    const back = (await workbook()).sheets.find((s) => s.name === '工作表1')
    step('FC-X3 一次撤销撤回整个对话框的设置', (await style('B2')).align?.h !== 'Center' && !JSON.stringify(back.merges ?? []).includes('B2:C3'))
  })

  await section('FC-X4 字体', async () => {
    await dataset()
    await click('B2')
    await openFC('shortcut')
    await kit.getByTestId('fc-tab-font').click()
    await kit.getByTestId('fc-font').selectOption('楷体')
    await kit.getByTestId('fc-font-style').selectOption('bi')
    await kit.getByTestId('fc-size').fill('14')
    await kit.getByTestId('fc-underline').selectOption('Double')
    await kit.getByTestId('fc-font-color').click()
    await kit.getByTestId('fc-font-color-hex').fill('#C00000')
    await kit.getByTestId('fc-font-color-hex-ok').click()
    await kit.getByTestId('fc-strike').check()
    step('FC-X4 预览按设置显示', /italic/.test((await kit.getByTestId('fc-preview').getAttribute('style')) ?? ''))
    await kit.getByTestId('fc-ok').click()
    await settle(250)
    const f = (await style('B2')).font
    step('FC-X4 楷体、加粗倾斜、14、双下划线、红色、删除线', f.name === '楷体' && f.bold && f.italic && f.size === 14 && f.underline === 'Double' && hex(f.color) === '#C00000' && f.strike, JSON.stringify(f))
  })

  await section('FC-X5 边框', async () => {
    await dataset()
    await click('B2')
    await page.keyboard.down('Shift')
    await click('C3')
    await page.keyboard.up('Shift')
    await openFC('shortcut')
    await kit.getByTestId('fc-tab-border').click()
    await kit.getByTestId('fc-line-style').selectOption('medium')
    await kit.getByTestId('fc-border-outside').click()
    await kit.getByTestId('fc-line-style').selectOption('thin')
    await kit.getByTestId('fc-border-inside').click()
    step('FC-X5 预览显示外边框', /border/.test((await kit.getByTestId('fc-border-preview').innerHTML()) ?? ''))
    await kit.getByTestId('fc-ok').click()
    await settle(250)
    const b = (await style('B2')).borders
    step('FC-X5 外边框中粗、内部细线', b.top?.style === 'Medium' && b.left?.style === 'Medium' && b.right?.style === 'Thin' && b.bottom?.style === 'Thin', JSON.stringify(b))
    await openFC('shortcut')
    await kit.getByTestId('fc-tab-border').click()
    await kit.getByTestId('fc-border-none').click()
    await kit.getByTestId('fc-ok').click()
    await settle(250)
    step('FC-X5 「无」清除边框', (await style('B2')).borders?.top?.style === 'None')
  })

  await section('FC-X6/FC-X7/FC-X8 图案、保护与取消', async () => {
    await dataset()
    await click('B2')
    await openFC('shortcut')
    await kit.getByTestId('fc-tab-fill').click()
    await kit.getByTestId('fc-fill-colors').locator('button').nth(5).click()
    const picked = await kit.getByTestId('fc-fill-sample').getAttribute('data-color')
    await kit.getByTestId('fc-tab-protection').click()
    await kit.getByTestId('fc-locked').uncheck()
    await kit.getByTestId('fc-ok').click()
    await settle(250)
    step('FC-X6 图案：色板颜色为单元格填充', (await ownFill('B2')) === picked, `${await ownFill('B2')} ${picked}`)
    step('FC-X7 保护：取消锁定', (await style('B2')).protection?.locked === false)
    await openFC('shortcut')
    await kit.getByTestId('fc-tab-fill').click()
    await kit.getByTestId('fc-fill-none').click()
    await kit.getByTestId('fc-cancel').click()
    await settle(200)
    step('FC-X8 取消不改变任何格式', (await ownFill('B2')) === picked)
    await openFC('shortcut')
    await kit.getByTestId('fc-tab-fill').click()
    await kit.getByTestId('fc-fill-none').click()
    await kit.getByTestId('fc-ok').click()
    await settle(200)
    step('FC-X6 「无颜色」去掉填充', (await ownFill('B2')) === null)
  })

  if (errors.length) step('no page errors', false, errors.slice(0, 3).join(' | '))
} catch (error) {
  await h.finish(error)
}
await h.finish()
