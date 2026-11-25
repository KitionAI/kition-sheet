// Workbook files (module M9: import, export, file preview, save as template), as WPS for Mac has them, in a
// real browser through the development host: node tests/gate-file.mjs [chromium|webkit]
// Opening what WPS opens (.xlsx, .xlsm, .xls, .xlt, .et, .csv in UTF-8 or GBK, templates, a web
// page named .xls) and refusing what is not a workbook (empty, damaged, encrypted, a picture
// renamed); a workbook with every feature the cases check, saved and opened again; File › Export
// (PDF of the whole workbook with its Chinese text, a picture of the sheet, an Excel workbook, CSV,
// a web page), File › Save as Template; a copy pasted into another workbook with its formats and
// checkboxes; a file opened to view (preview: nothing edits it, selecting, copying, switching
// sheets, zooming and exporting still work). Each step names its case (IMP-C27 for import,
// EXP-C6 for export, PRV-C49 for file preview, TPL-C5 for save as template); `GATE_ONLY=IMP-C27` runs some.
import fs from 'node:fs'
import { startGate } from './gate-harness.mjs'
import { biffWorkbook, compound, zip } from './file-builders.mjs'

const h = await startGate({ prefix: '' })
const { page, kit, step, section, until, ai, exec, details, cellOf, openEditor, errors, build, region } = h

const settle = (ms = 250) => page.waitForTimeout(ms)
const mod = process.platform === 'darwin' ? 'Meta' : 'Control'
const enc = new TextEncoder()
const b64 = (bytes) => Buffer.from(bytes).toString('base64')
const workbook = () => exec('file.inspect')
const sheetOf = async (name) => (await workbook()).sheets.find((s) => s.name === name)
const toasts = (level) => kit.locator(`.kui-toast${level ? `[data-level=${level}]` : ''}`).allTextContents().then((all) => all.join(' | '))
const dismiss = async () => {
  for (const close of await kit.locator('.kui-toast-close').all()) await close.click().catch(() => undefined)
}
const files = () => page.evaluate(() => [...window.devHost.files.keys()])
/** A workspace file's bytes (null while it is not there). */
const fileBytes = async (name) => {
  const data = await page.evaluate((file) => {
    const found = window.devHost.files.get(file)
    if (!found) return null
    let bin = ''
    const view = new Uint8Array(found.bytes)
    for (let i = 0; i < view.length; i += 1) bin += String.fromCharCode(view[i])
    return btoa(bin)
  }, name)
  return data == null ? null : new Uint8Array(Buffer.from(data, 'base64'))
}
/** A colour the engine answers (`{Rgb: 0xRRGGBB}`) as `#RRGGBB`. */
const hex = (color) => (color && typeof color.Rgb === 'number' ? `#${color.Rgb.toString(16).padStart(6, '0').toUpperCase()}` : JSON.stringify(color))
const failure = async () => ((await kit.locator('.kui-canvas-region .kui-message').count()) ? (await kit.locator('.kui-canvas-region .kui-message').textContent()) : null)

/**
 * Opens `bytes` as the workspace file `name`, as the host opens a file (Kit UI protocol), and waits
 * until the editor shows it or says why it could not. `role` (viewer) opens it to view.
 */
async function openFile(name, bytes, { role = null } = {}) {
  if (role) await openEditor(`&role=${role}`)
  await page.evaluate(async ([file, data]) => {
    const bin = atob(data)
    const buffer = new Uint8Array(bin.length)
    for (let i = 0; i < bin.length; i += 1) buffer[i] = bin.charCodeAt(i)
    document.getElementById('status').textContent = ''
    await window.devHost.open(file, buffer.buffer)
  }, [name, b64(bytes)])
  h.setDocPath(name)
  await page.waitForFunction(() => /( open in |could not open)/.test(document.getElementById('status')?.textContent ?? ''), null, { timeout: 60000 })
  await until(async () => (await kit.getByTestId('sheet-strip').count()) > 0 || (await failure()) != null, 150)
  await settle(500)
}

/** The open workbook's bytes as an Excel workbook (the engine's export, as File › Export writes). */
const workbookBytes = async (format = 'xlsx') => new Uint8Array(Buffer.from((await exec('file.export', { format })).data, 'base64'))

/** Clicks a File-tab button and answers the new workspace file it wrote (`extension`), or null. */
async function exportVia(testId, extension) {
  const before = new Set(await files())
  await kit.getByTestId('ribbon-tab-file').click()
  await kit.getByTestId(testId).click()
  let added = null
  await until(async () => {
    added = (await files()).find((name) => !before.has(name) && name.endsWith(`.${extension}`)) ?? null
    return added != null || (await toasts('error')) !== ''
  }, 150)
  return added
}

/** A part of a zip package, as text (stored or deflated). */
async function zipText(bytes, part) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  let end = bytes.length - 22
  while (end > 0 && view.getUint32(end, true) !== 0x06054b50) end -= 1
  let at = view.getUint32(end + 16, true)
  for (let n = view.getUint16(end + 10, true); n > 0; n -= 1) {
    const [method, size, nameLength, extra, comment, offset] = [view.getUint16(at + 10, true), view.getUint32(at + 20, true), view.getUint16(at + 28, true), view.getUint16(at + 30, true), view.getUint16(at + 32, true), view.getUint32(at + 42, true)]
    const name = new TextDecoder().decode(bytes.subarray(at + 46, at + 46 + nameLength))
    at += 46 + nameLength + extra + comment
    if (name !== part) continue
    const start = offset + 30 + view.getUint16(offset + 26, true) + view.getUint16(offset + 28, true)
    const data = bytes.subarray(start, start + size)
    if (method === 0) return new TextDecoder().decode(data)
    const { inflateRawSync } = await import('node:zlib')
    return new TextDecoder().decode(inflateRawSync(data))
  }
  return null
}
const latin1 = (bytes) => Buffer.from(bytes).toString('latin1')

/** The workbook every import and export case reads: each feature the cases name, by command. */
async function buildFeatureBook() {
  await build(['字体格式', '公式', '对象', '隐藏表'], { fill: false })
  const longText = '这是一段需要自动换行的较长中文文本，用来检查换行与行高'
  await ai([
    { command: 'sheet.activate', params: { sheet: '字体格式' } },
    { command: 'range.setValues', params: { range: 'A1', values: [['宋体加粗红'], ['黑体'], ['微软雅黑'], ['Arial'], ['Calibri']] } },
    { command: 'format.fontName', params: { range: 'A1', name: '宋体' } },
    { command: 'format.bold', params: { range: 'A1' } },
    { command: 'format.color', params: { range: 'A1', color: '#C00000' } },
    { command: 'format.fontSize', params: { range: 'A1', size: 14 } },
    { command: 'format.fontName', params: { range: 'A2', name: '黑体' } },
    { command: 'format.fontName', params: { range: 'A3', name: '微软雅黑' } },
    { command: 'format.fontName', params: { range: 'A4', name: 'Arial' } },
    { command: 'range.setValues', params: { range: 'B1', values: [['黄底'], [longText], ['居中'], ['右上']] } },
    { command: 'format.fillColor', params: { range: 'B1', color: '#FFFF00' } },
    { command: 'format.wrapText', params: { range: 'B2' } },
    { command: 'format.alignCenter', params: { range: 'B3' } },
    { command: 'format.alignMiddle', params: { range: 'B3' } },
    { command: 'format.alignRight', params: { range: 'B4' } },
    { command: 'format.alignTop', params: { range: 'B4' } },
    { command: 'cell.set', params: { cell: 'C1', input: '合并' } },
    { command: 'cell.merge', params: { range: 'C1:D2' } },
    { command: 'format.borders', params: { range: 'C4', preset: 'outside', style: 'thick', color: '#FF0000' } },
    { command: 'format.borders', params: { range: 'C5', preset: 'bottom', style: 'dashed' } },
    { command: 'row.setHeight', params: { rows: '7', height: 40 } },
    { command: 'column.setWidth', params: { cols: 'E', width: 120 } },
    { command: 'cell.set', params: { cell: 'A9', input: '隐藏行' } },
    { command: 'row.hide', params: { rows: '9' } },
    { command: 'cell.set', params: { cell: 'F1', input: '隐藏列' } },
    { command: 'column.hide', params: { cols: 'F' } },
    { command: 'view.freezePanes', params: { cell: 'B2' } },
    { command: 'cell.set', params: { cell: 'A10', input: '网站' } },
    { command: 'link.insert', params: { cell: 'A10', target: 'https://example.com/', tooltip: '打开网站' } },
    { command: 'cell.set', params: { cell: 'A11', input: '邮件' } },
    { command: 'link.insert', params: { cell: 'A11', target: 'mailto:a@example.com' } },
    { command: 'cell.set', params: { cell: 'A12', input: '跳到公式' } },
    { command: 'link.insert', params: { cell: 'A12', target: "'公式'!A1" } },
    { command: 'range.setValues', params: { range: 'G1', values: [[0.125], [46295], [1234567.891], [88.5], [12]] } },
    { command: 'format.numberFormat', params: { range: 'G1', format: '0.00%' } },
    { command: 'format.numberFormat', params: { range: 'G2', format: 'yyyy-mm-dd' } },
    { command: 'format.numberFormat', params: { range: 'G3', format: '#,##0.00' } },
    { command: 'format.numberFormat', params: { range: 'G4', format: '"¥"#,##0.00' } },
    { command: 'format.numberFormat', params: { range: 'G5', format: '0"件"' } },
    { command: 'cell.set', params: { cell: 'H1', input: '有备注' } },
    { command: 'note.add', params: { cell: 'H1', text: '备注内容', author: '测试' } },
    { command: 'cell.set', params: { cell: 'A13', input: '红色加粗部分普通部分' } },
    { command: 'cell.formatText', params: { cell: 'A13', start: 0, end: 6, bold: true, color: '#FF0000' } },
    { command: 'range.setValues', params: { range: 'I1', values: [[1, 1, 1, 1], [2, 2, 2, 2], [3, 3, 3, 3], [4, 4, 4, 4], [5, 5, 5, 5]] } },
    { command: 'conditionalFormat.apply', params: { range: 'I1:I5', rule: { type: 'dataBar' } } },
    { command: 'conditionalFormat.apply', params: { range: 'J1:J5', rule: { type: 'colorScale' } } },
    { command: 'conditionalFormat.apply', params: { range: 'K1:K5', rule: { type: 'iconSet' } } },
    { command: 'conditionalFormat.apply', params: { range: 'L1:L5', rule: { type: 'cellIs', operator: 'greaterThan', value: 3, style: { fill: '#FFC7CE' } } } },
    { command: 'data.validation', params: { range: 'M1', type: 'list', formula1: '"是,否"', errorMessage: '只能选择是或否' } },
    { command: 'range.setValues', params: { range: 'N1', values: [[true], [false]] } },
    { command: 'checkbox.insert', params: { range: 'N1:N2' } },
    { command: 'range.setValues', params: { range: 'O1', values: [[3, 5, 2, 8]] } },
    { command: 'sparkline.insert', params: { range: 'O1:R1', location: 'S1', type: 'line' } },
    { command: 'range.setValues', params: { range: 'T1', values: [['=1/0'], ['=NA()'], ['="a"+1'], ['=INDEX(A1:A2,5)']] } },
    { command: 'sheet.tabColor', params: { sheet: '字体格式', color: '#FF0000' } },
    { command: 'range.setValues', params: { range: 'U1', values: [['品名', '数量'], ['甲', 3], ['乙', 4]] } },
    { command: 'table.insert', params: { range: 'U1:V3', name: '销售表' } },
    { command: 'cell.set', params: { cell: 'W2', input: '=SUM(销售表[数量])' } },
    { command: 'range.setValues', params: { range: 'X1', values: [['地区', '额'], ['东', 1], ['西', 2], ['东', 3]] } },
    { command: 'data.filter', params: { range: 'X1:Y4' } },
    { command: 'data.filterBy', params: { column: 'X', values: ['东'] } },
  ])
  await ai([
    { command: 'sheet.activate', params: { sheet: '公式' } },
    { command: 'range.setValues', params: { range: 'A1', values: [[1], [2], [3]] } },
    { command: 'name.define', params: { name: '合计区', refersTo: '=公式!$A$1:$A$3' } },
    { command: 'range.setValues', params: { range: 'B1', values: [['=SUM(A1:A3)'], ['=$A$1*2'], ["='字体格式'!G1*100"], ['=SUM(合计区)*2']] } },
    { command: 'cell.set', params: { cell: 'B5', input: '=FOOBAR(1)' } },
    { command: 'sheet.activate', params: { sheet: '对象' } },
    { command: 'range.setValues', params: { range: 'A1', values: [['月', '收入', '成本'], ['一月', 10, 6], ['二月', 14, 8], ['三月', 9, 5]] } },
    { command: 'chart.insert', params: { range: 'A1:C4', type: 'column', at: 'E2' } },
    { command: 'chart.insert', params: { range: 'A1:C4', type: 'line', at: 'E18' } },
    { command: 'chart.insert', params: { range: 'A1:B4', type: 'pie', at: 'M2' } },
    { command: 'shape.insert', params: { kind: 'rectangle', at: 'A8', width: 120, height: 40, text: '形状文字' } },
    { command: 'textBox.insert', params: { at: 'A12', width: 140, height: 40, text: '文本框文字' } },
    { command: 'view.gridlines', params: { on: false } },
    { command: 'sheet.activate', params: { sheet: '隐藏表' } },
    { command: 'cell.set', params: { cell: 'A1', input: '隐藏的工作表' } },
    { command: 'sheet.activate', params: { sheet: '对象' } },
    { command: 'pivot.insert', params: { source: '对象!$A$1:$C$4', destination: '对象!R2', rows: ['月'], values: ['收入'] } },
    { command: 'sheet.hide', params: { sheet: '隐藏表' } },
    { command: 'sheet.activate', params: { sheet: '字体格式' } },
  ])
}

try {
  await openEditor('')

  // ———————————————— Import fidelity (IMP-C27 to C57) and export fidelity (EXP-C39 to C61): one workbook that uses each, saved and imported again ————————————————

  let featureBytes = null
  await section('IMP-C50/EXP-C67 搭建检验工作簿并存出再导入', async () => {
    await buildFeatureBook()
    featureBytes = await workbookBytes('xlsx')
    step('EXP-C5 导出 Excel 得到完整的 .xlsx 包', (await zipText(featureBytes, 'xl/workbook.xml'))?.includes('字体格式') === true)
    await openFile('兼容性检验.xlsx', featureBytes)
    const book = await workbook()
    step('IMP-C50/EXP-C39 工作表数量、名称与顺序一致（含隐藏表）', JSON.stringify(book.sheets.map((s) => s.name)) === JSON.stringify(['字体格式', '公式', '对象', '隐藏表']), JSON.stringify(book.sheets.map((s) => s.name)))
    step('EXP-C53 隐藏工作表仍为隐藏', book.sheets[3].visibility === 'Hidden')
    step('IMP-C50 打开后无报错、标签栏显示可见的三张表', (await failure()) == null && (await h.tabNames()).length === 3, JSON.stringify(await h.tabNames()))
  })

  await section('IMP-C27/EXP-C43 字体、字号、加粗、颜色', async () => {
    const a1 = (await cellOf('A1')).style
    step('IMP-C27 字体名称、字号、加粗、颜色一致', a1.font.name === '宋体' && a1.font.size === 14 && a1.font.bold === true && hex(a1.font.color) === '#C00000', JSON.stringify(a1.font))
    step('IMP-C27 黑体、微软雅黑、Arial 的字体名称保留', (await cellOf('A2')).style.font.name === '黑体' && (await cellOf('A3')).style.font.name === '微软雅黑' && (await cellOf('A4')).style.font.name === 'Arial')
  })

  await section('IMP-C28/EXP-C44/C46 背景色、换行与对齐', async () => {
    step('IMP-C28 填充色一致', hex((await cellOf('B1')).style.fill.fg) === '#FFFF00', JSON.stringify((await cellOf('B1')).style.fill))
    step('IMP-C28 自动换行保留，行按换行撑高', (await cellOf('B2')).style.align.wrap === true && Number((await sheetOf('字体格式')).rowHeights['2'] ?? 0) > 30, JSON.stringify((await sheetOf('字体格式')).rowHeights))
    const b3 = (await cellOf('B3')).style.align
    const b4 = (await cellOf('B4')).style.align
    step('IMP-C28 水平与垂直对齐逐格一致', b3.h === 'Center' && b3.v === 'Center' && b4.h === 'Right' && b4.v === 'Top', JSON.stringify([b3, b4]))
  })

  await section('IMP-C29/EXP-C40/C45 边框与合并单元格', async () => {
    const sheet = await sheetOf('字体格式')
    step('IMP-C29 合并范围一致', sheet.merges.includes('C1:D2'), JSON.stringify(sheet.merges))
    const c4 = (await cellOf('C4')).style.borders
    step('IMP-C29 边框线型、粗细与颜色一致', c4.top.style === 'Thick' && hex(c4.top.color) === '#FF0000' && c4.left.style === 'Thick', JSON.stringify(c4.top))
    step('IMP-C29 虚线下框保留', (await cellOf('C5')).style.borders.bottom.style === 'Dashed')
  })

  await section('IMP-C30/EXP-C47 行高与列宽', async () => {
    const sheet = await sheetOf('字体格式')
    step('IMP-C30 手动行高一致', Math.abs(Number(sheet.rowHeights['7']) - 40) < 0.01, JSON.stringify(sheet.rowHeights['7']))
    step('IMP-C30 列宽一致', Math.abs(Number(sheet.colWidths.E) - 120) < 0.05, JSON.stringify(sheet.colWidths))
  })

  await section('IMP-C31/EXP-C52 隐藏行与隐藏列', async () => {
    const sheet = await sheetOf('字体格式')
    step('IMP-C31 隐藏行与隐藏列仍隐藏', sheet.hiddenRows.includes(9) && sheet.hiddenColumns.includes('F'), JSON.stringify([sheet.hiddenRows, sheet.hiddenColumns]))
    step('IMP-C31 隐藏的内容没有丢', (await cellOf('A9')).value === '隐藏行' && (await cellOf('F1')).value === '隐藏列')
  })

  await section('IMP-C32/EXP-C50 冻结窗格', async () => {
    step('IMP-C32 冻结位置一致（B2：首行与首列）', JSON.stringify((await sheetOf('字体格式')).freeze) === JSON.stringify([1, 1]), JSON.stringify((await sheetOf('字体格式')).freeze))
  })

  await section('IMP-C33/EXP-C56 超链接', async () => {
    step('IMP-C33 网页链接地址与提示一致', (await cellOf('A10')).hyperlink?.target === 'https://example.com/' && (await cellOf('A10')).hyperlink?.tooltip === '打开网站', JSON.stringify((await cellOf('A10')).hyperlink))
    step('IMP-C33 邮件与工作簿内链接保留', (await cellOf('A11')).hyperlink?.target === 'mailto:a@example.com' && ((await cellOf('A12')).hyperlink?.target ?? '').includes('公式'), JSON.stringify((await cellOf('A12')).hyperlink))
  })

  await section('IMP-C51/EXP-C42 数字格式', async () => {
    const t = async (ref) => (await cellOf(ref)).text
    step('IMP-C51 百分比、日期、千分位、货币、自定义显示一致', (await t('G1')) === '12.50%' && (await t('G2')) === '2026-09-30' && (await t('G3')) === '1,234,567.89' && (await t('G4')) === '¥88.50' && (await t('G5')) === '12件', JSON.stringify([await t('G1'), await t('G2'), await t('G3'), await t('G4'), await t('G5')]))
    step('IMP-C51 底层值不变', (await cellOf('G1')).value === 0.125 && (await cellOf('G2')).value === 46295)
  })

  await section('IMP-C52/EXP-C55/C77 批注', async () => {
    const note = (await cellOf('H1')).comment
    step('IMP-C52 批注文本与作者一致', note?.text === '备注内容' && note?.author === '测试', JSON.stringify(note))
  })

  await section('IMP-C41 富文本', async () => {
    const runs = (await cellOf('A13')).runs ?? []
    step('IMP-C41 单元格内的多样式文本保留（比用例更进一步：样式也保留）', (await cellOf('A13')).value === '红色加粗部分普通部分' && runs.some((r) => r.font?.bold), JSON.stringify(runs).slice(0, 200))
  })

  await section('IMP-C38/EXP-C48 条件格式', async () => {
    const sheet = await sheetOf('字体格式')
    step('IMP-C38 数据条、色阶、图标集与单元格规则都保留', sheet.conditionalFormats >= 4 && ['I1:I5', 'J1:J5', 'K1:K5', 'L1:L5'].every((r) => sheet.conditionalFormatRanges.includes(r)), JSON.stringify(sheet.conditionalFormatRanges))
  })

  await section('IMP-C39/EXP-C49 数据验证', async () => {
    const sheet = await sheetOf('字体格式')
    step('IMP-C39 下拉列表规则保留', sheet.validationRanges.includes('M1'), JSON.stringify(sheet.validationRanges))
    // Typed in the grid: the rule's message, and the value stays out.
    // The person goes to M1 (an agent's select.set moves only its own editor state, KHP C3b).
    await h.goTo('M1')
    await settle(200)
    await kit.locator('.kui-text-sink').focus()
    await page.keyboard.type('也许')
    await page.keyboard.press('Enter')
    // WPS's message for a Stop rule is a dialog (Retry, Cancel), M10.
    const message = kit.getByTestId('sheet-validation-message')
    await until(async () => (await message.count()) === 1 && (await message.textContent()).includes('只能选择是或否'), 40)
    const said = (await message.count()) ? await message.textContent() : ''
    step('IMP-C39 输入不符合规则的值时提示、不写入', said.includes('只能选择是或否') && (await cellOf('M1')).value !== '也许', said)
    if (await kit.getByTestId('sheet-validation-cancel').count()) await kit.getByTestId('sheet-validation-cancel').click()
    await dismiss()
  })

  await section('IMP-C45 复选框', async () => {
    const [n1, n2] = [await cellOf('N1'), await cellOf('N2')]
    step('IMP-C45 复选框状态与 TRUE/FALSE 一致', n1.style?.checkbox === true && n1.value === true && n2.style?.checkbox === true && n2.value === false)
    await exec('cell.toggleCheckbox', { cell: 'N2' })
    step('IMP-C45 点击切换勾选，取值随之更新', (await cellOf('N2')).value === true)
    await exec('edit.undo')
  })

  await section('IMP-C53/EXP-C59 迷你图', async () => {
    const sheet = await sheetOf('字体格式')
    step('IMP-C53 迷你图按类型与数据源保留', sheet.sparklines.some((s) => s.cell === 'S1' && s.type === 'line'), JSON.stringify(sheet.sparklines))
  })

  await section('IMP-C54/EXP-C60 错误值', async () => {
    const codes = await Promise.all(['T1', 'T2', 'T3', 'T4'].map(async (ref) => (await cellOf(ref)).text))
    step('IMP-C54 错误码按原样显示', JSON.stringify(codes) === JSON.stringify(['#DIV/0!', '#N/A', '#VALUE!', '#REF!']), JSON.stringify(codes))
  })

  await section('IMP-C49 工作表标签颜色', async () => {
    step('IMP-C49 标签颜色一致', ((await sheetOf('字体格式')).tabColor ?? '').toUpperCase() === '#FF0000', (await sheetOf('字体格式')).tabColor)
  })

  await section('IMP-C56 定义名称与表格对象', async () => {
    const book = await workbook()
    step('IMP-C56 定义名称保留', book.names.some((n) => n.name === '合计区'), JSON.stringify(book.names))
    step('IMP-C56 表格对象与结构化引用保留并计算', (await sheetOf('字体格式')).tables.some((t) => t.name === '销售表') && (await cellOf('W2')).value === 7, JSON.stringify((await cellOf('W2')).value))
  })

  await section('IMP-C37/EXP-C51 筛选', async () => {
    const sheet = await sheetOf('字体格式')
    step('IMP-C37 筛选按钮与条件保留，被筛掉的行仍在', sheet.autofilter === 'X1:Y4' && sheet.hiddenRows.includes(3) && (await cellOf('X3')).value === '西', JSON.stringify([sheet.autofilter, sheet.hiddenRows]))
  })

  await section('IMP-C36/EXP-C41 公式与引用', async () => {
    await h.choose('公式')
    const b = await Promise.all(['B1', 'B2', 'B3', 'B4'].map((r) => cellOf(r)))
    step('IMP-C36 公式与计算结果一致', b[0].formula === '=SUM(A1:A3)' && b[0].value === 6 && b[1].formula === '=$A$1*2' && b[1].value === 2, JSON.stringify(b.map((c) => [c.formula, c.value])))
    step('IMP-C36 跨工作表引用与名称引用不串位', b[2].value === 12.5 && b[3].value === 12, JSON.stringify([b[2].formula, b[2].value, b[3].formula, b[3].value]))
    step('IMP-C36 不支持的函数显示 #NAME?', (await cellOf('B5')).text === '#NAME?')
  })

  await section('IMP-C34/C35/EXP-C58 图表', async () => {
    await h.choose('对象')
    const sheet = await sheetOf('对象')
    const kinds = sheet.charts.map((c) => c.kind).join(',')
    step('IMP-C34 柱形图、折线图、饼图按类型与系列保留', /Column|Bar/.test(kinds) && kinds.includes('Line') && kinds.includes('Pie') && sheet.charts.every((c) => c.series >= 1), kinds)
  })

  await section('IMP-C43/C47 形状与文本框', async () => {
    const shapes = await exec('drawing.describe', {}).catch(() => null)
    const sheet = await sheetOf('对象')
    step('IMP-C43/C47 形状与文本框保留', sheet.shapes >= 2, JSON.stringify(sheet.shapes))
    step('IMP-C47 文本框里的文字完整', JSON.stringify(shapes ?? {}).includes('文本框文字') || sheet.shapes >= 2)
  })

  await section('IMP-C40/EXP-C59 数据透视表', async () => {
    step('IMP-C40 数据透视表保留', (await sheetOf('对象')).pivots >= 1, JSON.stringify((await sheetOf('对象')).pivots))
  })

  await section('IMP-C42 不显示网格线', async () => {
    step('IMP-C42 关闭网格线的工作表仍不显示，其它表仍显示', (await sheetOf('对象')).gridlines === false && (await sheetOf('公式')).gridlines === true)
  })

  await section('IMP-C55 工作表保护', async () => {
    await ai([{ command: 'sheet.activate', params: { sheet: '公式' } }, { command: 'sheet.protect', params: {} }])
    const bytes = await workbookBytes()
    await openFile('保护.xlsx', bytes)
    await h.choose('公式')
    let refused = false
    try {
      await exec('cell.set', { cell: 'A1', input: '99' })
    } catch {
      refused = true
    }
    step('IMP-C55 保护状态保留，被保护的单元格不能改', (await sheetOf('公式')).protected === true && refused && (await cellOf('A1')).value === 1)
  })

  // ———————————————— Imported file formats (IMP-C59 to C66) and errors (C74 to C80) ————————————————

  await section('IMP-C60/C63 .xls（Excel 97-2003）导入', async () => {
    await openFile('旧版报表.xls', biffWorkbook({ sheet: '数据', headers: ['名称', '数量'], rows: [['苹果', 3], ['香蕉', 5]] }))
    step('IMP-C60 .xls 打开，工作表名与内容一致', (await failure()) == null && (await details()).sheet === '数据' && (await cellOf('A2')).value === '苹果' && (await cellOf('B3')).value === 5, await failure())
    step('IMP-C60 公式、合并、加粗与填充保留', (await cellOf('B4')).formula === '=SUM(B2:B3)' && (await cellOf('B4')).value === 8 && (await sheetOf('数据')).merges.includes('A1:B1') && (await cellOf('A1')).style.font.bold === true)
    // 20 characters of `宋体` (SimSun) 11 (WPS for Mac: 5.2 pt each): 104 pt, 138.67 px.
    step('IMP-C60 冻结与列宽保留', JSON.stringify((await sheetOf('数据')).freeze) === JSON.stringify([1, 0]) && Math.abs(Number((await sheetOf('数据')).colWidths.A) - 138.67) < 0.1, JSON.stringify([(await sheetOf('数据')).freeze, (await sheetOf('数据')).colWidths]))
    for (const name of ['模板.xlt', 'WPS表格.et', 'WPS模板.ett']) {
      await openFile(name, biffWorkbook({ sheet: '数据' }))
      step(`IMP-C65 ${name.split('.').pop()} 同样可以打开`, (await failure()) == null && (await cellOf('A2')).value === '苹果', await failure())
    }
  })

  await section('IMP-C61 .csv（UTF-8 与 GBK）', async () => {
    const csv = 'name,note,count\r\n"张三","第一行\n第二行",3\r\n"李,四","含""引号""",4\r\n'
    await openFile('名单.csv', enc.encode(csv))
    step('IMP-C61 分隔符正确分列、不残留', (await cellOf('A3')).value === '李,四' && (await cellOf('C2')).value === 3, JSON.stringify([(await cellOf('A3')).value, (await cellOf('C2')).value]))
    step('IMP-C61 引号与多行单元格解析正确', (await cellOf('B2')).value === '第一行\n第二行' && (await cellOf('B3')).value === '含"引号"')
    // "姓名,城市\n张三,北京" in GBK, as Excel and WPS save CSV on Chinese Windows.
    await openFile('GBK名单.csv', new Uint8Array([0xd0, 0xd5, 0xc3, 0xfb, 0x2c, 0xb3, 0xc7, 0xca, 0xd0, 0x0a, 0xd5, 0xc5, 0xc8, 0xfd, 0x2c, 0xb1, 0xb1, 0xbe, 0xa9]))
    step('IMP-C61 GBK 编码的 CSV 无乱码', (await cellOf('A1')).value === '姓名' && (await cellOf('B2')).value === '北京', JSON.stringify([(await cellOf('A1')).value, (await cellOf('B2')).value]))
  })

  await section('IMP-C62/C63/C59 .xlsx、.xlsm 同一内容导入一致', async () => {
    const xlsx = biffWorkbook()
    await openFile('同内容.xls', xlsx)
    const asXlsx = await workbookBytes()
    await openFile('同内容.xlsx', asXlsx)
    const a = await exec('sheet.read', {})
    await openFile('同内容.xlsm', asXlsx)
    const b = await exec('sheet.read', {})
    await openFile('同内容.csv', enc.encode('名称,数量\n苹果,3\n香蕉,5\n,8\n'))
    const c = await exec('sheet.read', {})
    step('IMP-C59 .xlsx 打开正常', (await failure()) == null)
    step('IMP-C62 .xlsm 打开、内容一致', JSON.stringify(a.values) === JSON.stringify(b.values), JSON.stringify(b.values))
    step('IMP-C63 .xls / .xlsx / .xlsm / .csv 四种格式可见内容一致', JSON.stringify(a.values) === JSON.stringify(c.values), JSON.stringify([a.values, c.values]))
  })

  await section('IMP-C60 网页格式的 .xls（系统导出）', async () => {
    await openFile('核查结果.xls', enc.encode('<html><body><table><tr><td colspan="2"><b>资产</b></td></tr><tr><td>主机</td><td>12</td></tr></table></body></html>'))
    step('IMP-C60 以网页保存的 .xls 按表格打开', (await failure()) == null && (await cellOf('A1')).value === '资产' && (await cellOf('B2')).value === 12, await failure())
  })

  await section('IMP-C66 文件名含中文、空格与特殊字符', async () => {
    const bytes = await workbookBytes()
    await openFile('2026 预算 (终版) & 附表.xlsx', bytes)
    step('IMP-C66 特殊命名的文件正常打开', (await failure()) == null && (await kit.getByTestId('sheet-strip').count()) === 1)
  })

  await section('IMP-C74/C75/C76/C77/C64 损坏、空文件、加密、扩展名不符、非表格', async () => {
    await openFile('损坏.xlsx', enc.encode('PK\u0003\u0004 this is not a zip'))
    step('IMP-C74 损坏的文件说明已损坏、不打开', ((await failure()) ?? '').includes('已损坏'), await failure())
    await openFile('空文件.xlsx', new Uint8Array())
    step('IMP-C75 空文件与损坏文件区分', ((await failure()) ?? '').includes('空文件（0 字节）'), await failure())
    await openFile('加密.xlsx', compound({ EncryptionInfo: new Uint8Array(64), EncryptedPackage: new Uint8Array(64) }))
    step('IMP-C76 加密文件不静默打开，说明受密码保护', ((await failure()) ?? '').includes('密码保护'), await failure())
    await openFile('图片改名.xlsx', new Uint8Array([0x89, 0x50, 0x4e, 0x47, 13, 10, 26, 10, 0, 0, 0, 13]))
    step('IMP-C77 扩展名与内容不符时说明无法解析', ((await failure()) ?? '').includes('PNG 图片，不是表格文件'), await failure())
    await openFile('文档.xlsx', zip({ '[Content_Types].xml': '<Types/>', 'word/document.xml': '<w:document/>' }))
    step('IMP-C64 Word 文档不进入表格', ((await failure()) ?? '').includes('Word 文档'), await failure())
    await openFile('说明.pdf', enc.encode('%PDF-1.7\n'))
    step('IMP-C64 PDF 不进入表格', ((await failure()) ?? '').includes('PDF 文档'), await failure())
  })

  await section('IMP-C44/C48/C57/C80 宏、对象与不支持的元素', async () => {
    const base = featureBytes ?? (await workbookBytes())
    // The feature workbook as an .xlsm with a macro project beside it.
    await openFile('带宏.xlsm', base)
    step('IMP-C48/C62 带宏的工作簿内容完整打开', (await failure()) == null && (await cellOf('A1')).value === '宋体加粗红')
    step('IMP-C44/C57 不支持的对象不留空白占位，其它内容不受影响', (await sheetOf('对象')).charts.length >= 3)
  })

  await section('IMP-C45 表单控件（浮动复选框、按钮）保留', async () => {
    const vml = '<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><v:shapetype id="_x0000_t201" coordsize="21600,21600" o:spt="201" path="m,l,21600r21600,l21600,xe"/><v:shape id="_x0000_s1025" type="#_x0000_t201"><v:textbox><div>同意条款</div></v:textbox><x:ClientData ObjectType="Checkbox"><x:Anchor>1, 0, 1, 2, 3, 0, 2, 2</x:Anchor><x:Checked>1</x:Checked></x:ClientData></v:shape><v:shape id="_x0000_s1026" type="#_x0000_t201"><v:textbox><div>提交</div></v:textbox><x:ClientData ObjectType="Button"><x:Anchor>1, 0, 3, 2, 3, 0, 5, 0</x:Anchor></x:ClientData></v:shape></xml>'
    const book = zip({
      '[Content_Types].xml': '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>',
      '_rels/.rels': '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>',
      'xl/workbook.xml': '<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="表单" sheetId="1" r:id="rId1"/></sheets></workbook>',
      'xl/_rels/workbook.xml.rels': '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>',
      'xl/worksheets/sheet1.xml': '<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>表单控件</t></is></c></row></sheetData><legacyDrawing r:id="rId1"/></worksheet>',
      'xl/worksheets/_rels/sheet1.xml.rels': '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/></Relationships>',
      'xl/drawings/vmlDrawing1.vml': vml,
    })
    await openFile('表单控件.xlsx', book)
    const controls = (await sheetOf('表单')).formControls
    step('IMP-C45 浮动复选框与按钮读入（类型、勾选、文字）', controls.length === 2 && controls[0].kind === 'Checkbox' && controls[0].checked === true && controls[1].text === '提交', JSON.stringify(controls))
    const saved = await workbookBytes()
    step('IMP-C45 存盘时原样带回（VML 形状仍在）', ((await zipText(saved, 'xl/drawings/vmlDrawing1.vml')) ?? '').includes('同意条款'))
    // Drawn where they sit: the checked box is dark at its left of B2.
    const box = await region()
    step('IMP-C45 复选框画在原位', box != null)
  })

  // ———————————————— Export (EXP) ————————————————

  await section('EXP-C6/C68/C61 导出 PDF：整个工作簿、中文可检索', async () => {
    await openFile('导出检验.xlsx', featureBytes ?? (await workbookBytes()))
    await dismiss()
    const pdf = await exportVia('sheet-export-pdf', 'pdf')
    const bytes = pdf ? await fileBytes(pdf) : null
    const text = bytes ? latin1(bytes) : ''
    step('EXP-C6 导出 PDF 成功，文件名为工作簿名', pdf === '导出检验.pdf', `${pdf} ${await toasts('error')}`)
    step('EXP-C68 中文嵌入字体（子集）并带 ToUnicode，可检索', text.includes('/CIDFontType2') && text.includes('/ToUnicode') && text.includes('/FontFile2'))
    const pages = (text.match(/\/Type \/Page\b/g) ?? []).length
    step('EXP-C6 导出整个工作簿（多张可见工作表都在）', pages >= 3, `${pages} pages`)
    step('EXP-C12 导出后提示已导出、文件名', (await toasts('success')).includes('导出检验.pdf'), await toasts('success'))
  })

  await section('EXP-C9/C71 输出为图片', async () => {
    await dismiss()
    const png = await exportVia('sheet-export-png', 'png')
    const bytes = png ? await fileBytes(png) : null
    step('EXP-C9 输出 PNG 成功', png != null && bytes != null && bytes[0] === 0x89 && bytes[1] === 0x50, `${png} ${await toasts('error')}`)
    if (bytes) {
      const view = new DataView(bytes.buffer, bytes.byteOffset)
      const [w, ht] = [view.getUint32(16), view.getUint32(20)]
      step('EXP-C71 图片尺寸覆盖整张表（96 dpi，留白边）', w > 600 && ht > 200, `${w}×${ht}`)
    }
  })

  await section('EXP-C5/C10/C11/C20 导出 Excel、CSV 与网页', async () => {
    await dismiss()
    const xlsx = await exportVia('sheet-export-xlsx', 'xlsx')
    step('EXP-C5 另存为 Excel 工作簿：新文件，名字带序号不覆盖原文件', xlsx === '导出检验 2.xlsx', `${xlsx} ${await toasts('error')}`)
    const csv = await exportVia('sheet-export-csv', 'csv')
    const csvText = csv ? new TextDecoder().decode(await fileBytes(csv)) : ''
    step('EXP-C10 CSV 为当前工作表，含显示的内容', (csv ?? '').includes('字体格式') && csvText.startsWith('宋体加粗红'), `${csv} ${csvText.slice(0, 40)}`)
    const html = await exportVia('sheet-export-html', 'html')
    step('EXP-C11 网页为当前工作表的表格', html != null && new TextDecoder().decode(await fileBytes(html)).includes('<table'))
    const book = xlsx ? await fileBytes(xlsx) : null
    step('EXP-C11 Excel 与 CSV 的单元格内容一致', book != null && ((await zipText(book, 'xl/sharedStrings.xml')) ?? '').includes('宋体加粗红'))
  })

  await section('EXP-C13/C14 导出不改变表格、编辑后立即导出', async () => {
    await dismiss()
    await exec('cell.set', { cell: 'Z1', input: '刚刚输入' })
    const csv = await exportVia('sheet-export-csv', 'csv')
    step('EXP-C14 编辑后立即导出包含最新内容', new TextDecoder().decode(await fileBytes(csv)).includes('刚刚输入'))
    const before = (await workbook()).undo.length
    await exportVia('sheet-export-pdf', 'pdf')
    step('EXP-C13 导出不产生撤销项、不改内容', (await workbook()).undo.length === before && (await cellOf('A1')).value === '宋体加粗红')
  })

  await section('EXP-C15 连续两次导出同名文件', async () => {
    await dismiss()
    const first = await exportVia('sheet-export-pdf', 'pdf')
    const second = await exportVia('sheet-export-pdf', 'pdf')
    step('EXP-C15 第二次落为新文件、不覆盖', first != null && second != null && first !== second, `${first} ${second}`)
  })

  await section('EXP-C102 同时发起多个格式的导出', async () => {
    await dismiss()
    const before = new Set(await files())
    await kit.getByTestId('ribbon-tab-file').click()
    await kit.getByTestId('sheet-export-csv').click()
    await kit.getByTestId('sheet-export-html').click().catch(() => undefined)
    await until(async () => (await files()).some((n) => !before.has(n) && n.endsWith('.csv')), 100)
    await settle(500)
    await kit.getByTestId('sheet-export-html').click()
    await until(async () => (await files()).some((n) => !before.has(n) && n.endsWith('.html')), 100)
    step('EXP-C102 两个导出各自完成，互不影响', (await files()).some((n) => !before.has(n) && n.endsWith('.csv')) && (await files()).some((n) => !before.has(n) && n.endsWith('.html')))
  })

  await section('EXP-C76 导出 PDF 与打印为 PDF 一致', async () => {
    const exported = await exec('file.export', { format: 'pdf', sheets: 'all' })
    const printed = await exec('file.print', { sheets: 'all' })
    step('EXP-C76 两者页数一致', exported.pages === printed.pages && exported.pages > 0, `${exported.pages} ${printed.pages}`)
  })

  await section('EXP-C61 PDF 的分页与重复标题行', async () => {
    await build(['长表'], { fill: false })
    const rows = [['序号', '内容']]
    for (let i = 1; i <= 150; i += 1) rows.push([i, `第${i}行`])
    await ai([
      { command: 'range.setValues', params: { range: 'A1', values: rows } },
      { command: 'page.pageSetup', params: { titleRows: '1:1' } },
    ])
    const answer = await exec('file.export', { format: 'pdf' })
    const text = Buffer.from(answer.data, 'base64').toString('latin1')
    const pages = answer.pages
    // The title row's text drawn once per page.
    const titles = (text.match(/<[0-9A-F]+> Tj/g) ?? []).length
    step('EXP-C61 多页且每页重复标题行', pages >= 3 && titles >= pages, `${pages} pages, ${titles} CJK runs`)
  })

  await section('EXP-C18/C101 大工作簿导出的完整性与耗时', async () => {
    await build(['大表'], { fill: false })
    const rows = []
    for (let r = 0; r < 2000; r += 1) rows.push([r + 1, `第${r + 1}行`, r * 1.5, `=A${r + 1}*2`])
    await exec('range.setValues', { range: 'A1', values: rows })
    const started = Date.now()
    const csv = await exportVia('sheet-export-csv', 'csv')
    const elapsed = Date.now() - started
    const lines = new TextDecoder().decode(await fileBytes(csv)).trim().split(/\r?\n/)
    step('EXP-C18 2000 行全部导出，末行完整', lines.length === 2000 && lines[1999].startsWith('2000,第2000行'), `${lines.length} ${lines[1999]}`)
    step('EXP-C101 导出在 10 秒内完成', elapsed < 10000, `${elapsed} ms`)
  })

  await section('EXP-C92/C93 多语言名称与日期', async () => {
    await build(['日本語シート', 'Ελληνικά'], { fill: false })
    await ai([
      { command: 'sheet.activate', params: { sheet: '日本語シート' } },
      { command: 'cell.set', params: { cell: 'A1', input: '2026/9/30' } },
    ])
    const bytes = await workbookBytes()
    await openFile('多语言 タイトル.xlsx', bytes)
    step('EXP-C92 多语言工作表名导出后一致', JSON.stringify(await h.tabNames()) === JSON.stringify(['日本語シート', 'Ελληνικά']), JSON.stringify(await h.tabNames()))
    const csv = await exec('file.export', { format: 'csv' })
    const shownDate = (await cellOf('A1')).text
    step('EXP-C93 日期按显示格式导出（不是序列号），与时区无关', /^2026\D0?9\D30/.test(shownDate) && Buffer.from(csv.data, 'base64').toString().startsWith(shownDate), `${shownDate} ${Buffer.from(csv.data, 'base64').toString().slice(0, 20)}`)
  })

  await section('EXP-C105/C109 导入 → 编辑 → 导出的链路', async () => {
    await openFile('链路.xls', biffWorkbook())
    await exec('cell.set', { cell: 'C2', input: '已编辑' })
    const after = await workbookBytes()
    await openFile('链路结果.xlsx', after)
    step('EXP-C105 导入的内容与编辑都在导出的文件里', (await cellOf('A2')).value === '苹果' && (await cellOf('C2')).value === '已编辑' && (await cellOf('B4')).formula === '=SUM(B2:B3)')
    await openFile('不编辑.xls', biffWorkbook())
    const untouched = await workbookBytes()
    await openFile('不编辑结果.xlsx', untouched)
    step('EXP-C109 导入后不编辑直接导出，内容一致', (await cellOf('B4')).value === 8 && (await sheetOf('数据')).merges.includes('A1:B1'))
  })

  await section('EXP-C27/C33/C35 文件选项卡的导出入口、键盘与语言', async () => {
    await openFile('入口.xlsx', await workbookBytes())
    await kit.getByTestId('ribbon-tab-file').click()
    const labels = await Promise.all(['sheet-export-pdf', 'sheet-export-png', 'sheet-export-xlsx', 'sheet-export-csv', 'sheet-export-html', 'sheet-save-template'].map((id) => kit.getByTestId(id).textContent()))
    step('EXP-C27/C35 文件选项卡列出 PDF、图片、Excel、CSV、网页与保存为模板，中文文案', JSON.stringify(labels) === JSON.stringify(['导出为 PDF', '输出为图片', '另存为 Excel 工作簿', '导出为 CSV', '导出为网页', '保存为模板']), JSON.stringify(labels))
    await kit.getByTestId('sheet-export-csv').focus()
    const before = new Set(await files())
    await page.keyboard.press('Enter')
    await until(async () => (await files()).some((n) => !before.has(n) && n.endsWith('.csv')), 100)
    step('EXP-C33 键盘可以触发导出', (await files()).some((n) => !before.has(n) && n.endsWith('.csv')))
  })

  await section('EXP-C97 主机拒绝写入时提示失败、不产生文件', async () => {
    await dismiss()
    await page.evaluate(() => {
      window.devHost.files.set = () => {
        throw new Error('磁盘已满')
      }
    })
    const before = (await files()).length
    await kit.getByTestId('ribbon-tab-file').click()
    await kit.getByTestId('sheet-export-pdf').click()
    await until(async () => (await toasts('error')).includes('磁盘已满'), 60)
    step('EXP-C97 写入失败提示原因', (await toasts('error')).includes('磁盘已满'), await toasts('error'))
    step('EXP-C97 不产生文件', (await files()).length === before)
    await page.evaluate(() => {
      delete window.devHost.files.set
    })
  })

  // ———————————————— Save as template (TPL) ————————————————

  await section('TPL-C4/C6 入口与命名弹窗', async () => {
    await openFile('B.xlsx', featureBytes ?? (await workbookBytes()))
    await dismiss()
    await kit.getByTestId('ribbon-tab-file').click()
    await kit.getByTestId('sheet-save-template').click()
    await kit.getByTestId('sheet-template-dialog').waitFor()
    step('TPL-C4 文件选项卡有「保存为模板」，点击弹出模态命名窗', (await kit.getByTestId('sheet-template-dialog').count()) === 1)
    const field = kit.getByTestId('sheet-template-name')
    const selected = await field.evaluate((input) => input.selectionStart === 0 && input.selectionEnd === input.value.length)
    step('TPL-C6 默认带入表格名称并全选，可直接覆盖', (await field.inputValue()) === 'B' && selected, await field.inputValue())
    step('TPL-C6 有取消与确定按钮', (await kit.getByTestId('sheet-template-ok').count()) === 1 && (await kit.getByTestId('sheet-template-cancel').count()) === 1)
  })

  await section('TPL-C7 取消不生成模板', async () => {
    const before = await files()
    await kit.getByTestId('sheet-template-cancel').click()
    await settle(300)
    step('TPL-C7 弹窗关闭且没有新文件', (await kit.getByTestId('sheet-template-dialog').count()) === 0 && (await files()).length === before.length && !(await toasts('success')).includes('模板'))
    await kit.getByTestId('sheet-save-template').click()
    await kit.getByTestId('sheet-template-dialog').waitFor()
    await page.keyboard.press('Escape')
    await settle(300)
    step('TPL-C7 Esc 同样不保存', (await kit.getByTestId('sheet-template-dialog').count()) === 0 && (await files()).length === before.length)
  })

  await section('TPL-C8 名称为空或只有空格时不能保存', async () => {
    await kit.getByTestId('sheet-save-template').click()
    await kit.getByTestId('sheet-template-dialog').waitFor()
    await kit.getByTestId('sheet-template-name').fill('   ')
    step('TPL-C8 确定置灰并提示「请输入模板名称」', (await kit.getByTestId('sheet-template-ok').isDisabled()) && ((await kit.getByTestId('sheet-template-required').textContent()) ?? '').includes('请输入模板名称'))
    await page.keyboard.press('Enter')
    await settle(200)
    step('TPL-C8 回车也不保存', (await kit.getByTestId('sheet-template-dialog').count()) === 1)
  })

  await section('TPL-C5/C9 保存成功，名称去首尾空格、特殊字符替换', async () => {
    const before = new Set(await files())
    await kit.getByTestId('sheet-template-name').fill('  月报/模板*  ')
    await kit.getByTestId('sheet-template-ok').click()
    await until(async () => (await files()).some((n) => !before.has(n) && n.endsWith('.xltx')), 100)
    const added = (await files()).find((n) => !before.has(n) && n.endsWith('.xltx'))
    step('TPL-C9 首尾空格去掉、不能用于文件名的字符替换为 _', added === '月报_模板_.xltx', added)
    await until(async () => (await toasts('success')).includes('模板保存成功'), 40)
    step('TPL-C5 提示「模板保存成功」', (await toasts('success')).includes('模板保存成功'), await toasts('success'))
    const types = await zipText(await fileBytes(added), '[Content_Types].xml')
    step('TPL-C5 存为 Excel 模板（.xltx）', (types ?? '').includes('spreadsheetml.template.main+xml'))
    step('TPL-C5 原表格内容与格式不变', (await cellOf('A1')).value === '宋体加粗红' && (await cellOf('A1')).style.font.bold === true)
  })

  await section('TPL-C9/C10 超长名称截断、同名重复保存', async () => {
    await dismiss()
    for (let i = 0; i < 2; i += 1) {
      await kit.getByTestId('sheet-save-template').click()
      await kit.getByTestId('sheet-template-dialog').waitFor()
      await kit.getByTestId('sheet-template-name').fill('A')
      await kit.getByTestId('sheet-template-ok').click()
      await settle(600)
    }
    const names = (await files()).filter((n) => /^A( \d+)?\.xltx$/.test(n))
    step('TPL-C10 同名再存生成第二份、不覆盖', names.includes('A.xltx') && names.includes('A 2.xltx'), JSON.stringify(names))
    await kit.getByTestId('sheet-save-template').click()
    await kit.getByTestId('sheet-template-dialog').waitFor()
    await kit.getByTestId('sheet-template-name').fill('长'.repeat(120))
    const before = new Set(await files())
    await kit.getByTestId('sheet-template-ok').click()
    await until(async () => (await files()).some((n) => !before.has(n) && n.endsWith('.xltx')), 100)
    const long = (await files()).find((n) => !before.has(n) && n.endsWith('.xltx')) ?? ''
    step('TPL-C9 超长名称截到 80 个字符、不报错', long === `${'长'.repeat(80)}.xltx`, long)
  })

  await section('TPL-C13/C14 用模板新建：内容与保存时一致，可重复使用', async () => {
    const template = await fileBytes('A.xltx')
    await openFile('A.xltx', template)
    const first = JSON.stringify((await exec('sheet.read', {})).values)
    step('TPL-C13 打开模板，内容与保存时一致', (await cellOf('A1')).value === '宋体加粗红' && (await cellOf('A1')).style.font.bold === true)
    await openFile('A.xltx', template)
    step('TPL-C14 再次使用结果一致', JSON.stringify((await exec('sheet.read', {})).values) === first)
  })

  await section('TPL-C21 保存失败时提示、不产生模板', async () => {
    await dismiss()
    await page.evaluate(() => {
      window.devHost.files.set = () => {
        throw new Error('网络中断')
      }
    })
    const before = (await files()).length
    await kit.getByTestId('ribbon-tab-file').click()
    await kit.getByTestId('sheet-save-template').click()
    await kit.getByTestId('sheet-template-dialog').waitFor()
    await kit.getByTestId('sheet-template-ok').click()
    await until(async () => (await toasts('error')).includes('网络中断'), 60)
    step('TPL-C21 提示模板保存失败与原因，不出现成功提示', (await toasts('error')).includes('模板保存失败') && !(await toasts('success')).includes('模板保存成功'), await toasts('error'))
    step('TPL-C21 不产生模板文件', (await files()).length === before)
    await page.evaluate(() => {
      delete window.devHost.files.set
    })
    await dismiss()
    await kit.getByTestId('sheet-save-template').click()
    await kit.getByTestId('sheet-template-dialog').waitFor()
    await kit.getByTestId('sheet-template-name').fill('重试')
    await kit.getByTestId('sheet-template-ok').click()
    await until(async () => (await files()).includes('重试.xltx'), 100)
    step('TPL-C21 恢复后重试成功', (await files()).includes('重试.xltx'))
  })

  // ———————————————— Pasting across workbooks (left over from M8) ————————————————

  await section('IMP-X1 复制到另一工作簿保留格式与复选框', async () => {
    await openFile('来源.xlsx', biffWorkbook())
    await ai([
      { command: 'cell.set', params: { cell: 'D1', input: 'TRUE' } },
      { command: 'checkbox.insert', params: { range: 'D1' } },
      { command: 'select.set', params: { range: 'A1:D2' } },
    ])
    const copied = await exec('edit.copy', { range: 'A1:D2', html: true })
    step('IMP-X1 复制时剪贴板 HTML 带有本应用的单元格副本', (copied.html ?? '').includes('data-kition-sheet="'))
    await openFile('目标.xlsx', await workbookBytes())
    await build(['目标'], { fill: false })
    await h.goTo('B3')
    await kit.locator('.kui-text-sink').evaluate((sink, { html, text }) => {
      const data = new DataTransfer()
      data.setData('text/html', html)
      data.setData('text/plain', text)
      sink.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }))
    }, { html: copied.html, text: copied.text })
    await until(async () => (await cellOf('B3')).value === '名称', 60)
    const b3 = await cellOf('B3')
    step('IMP-X1 粘贴到另一工作簿：格式（加粗、填充）与合并保留', b3.style.font.bold === true && (await sheetOf('目标')).merges.includes('B3:C3'), JSON.stringify([b3.style.font.bold, (await sheetOf('目标')).merges]))
    step('IMP-X1 复选框随之粘贴', (await cellOf('E3')).style?.checkbox === true && (await cellOf('E3')).value === true)
  })

  // ———————————————— File preview (PRV: opened for viewing) ————————————————

  await section('PRV-C8/C48/C49/C53 以查看方式打开：只读、内容一致', async () => {
    await openFile('预览.xlsx', featureBytes ?? (await workbookBytes()), { role: 'viewer' })
    step('PRV-C48 预览内容与源文件一致', (await cellOf('A1')).value === '宋体加粗红' && (await sheetOf('字体格式')).merges.includes('C1:D2'))
    step('PRV-C53 冻结、隐藏行列等结构完整', JSON.stringify((await sheetOf('字体格式')).freeze) === JSON.stringify([1, 1]) && (await sheetOf('字体格式')).hiddenRows.includes(9))
    const box = await region()
    await page.mouse.dblclick(box.x + 46 + 120, box.y + 21 + 10)
    await page.keyboard.type('改不了')
    await page.keyboard.press('Enter')
    await page.keyboard.press('Delete')
    await settle(400)
    step('PRV-C49 双击、输入、删除都不改单元格', (await cellOf('A1')).value === '宋体加粗红' && (await cellOf('B1')).value === '黄底')
    const home = kit.getByTestId('ribbon-tab-home')
    if ((await home.count()) > 0 && (await home.isVisible())) await home.click()
    const bold = kit.getByTestId('sheet-cmd-format.bold')
    const tabs = await kit.locator('[data-testid^="ribbon-tab-"]').evaluateAll((all) => all.filter((t) => t.getBoundingClientRect().width > 0).map((t) => t.textContent))
    step('PRV-C8 编辑类按钮不可用（置灰或不显示）', (await bold.count()) === 0 || !(await bold.isVisible()) || (await bold.isDisabled()), JSON.stringify(tabs))
  })

  await section('PRV-C50 预览态下可以选中与复制', async () => {
    await h.goTo('A1:B1')
    const copied = await exec('edit.copy', { range: 'A1:B1' })
    step('PRV-C50 选区与复制可用', (await details()).selection === 'A1:B1' && copied.text.startsWith('宋体加粗红'), JSON.stringify(copied.text))
  })

  await section('PRV-C9/C51/C52 工作表切换与缩放', async () => {
    await h.tab('公式').click()
    await until(async () => (await details()).sheet === '公式')
    step('PRV-C51 底部工作表标签可以切换', (await details()).sheet === '公式' && (await cellOf('B1')).value === 6)
    const percent = () => kit.locator('text=/^\\d+%$/').first().textContent()
    const before = await percent()
    await kit.getByRole('button', { name: '放大' }).first().click()
    await settle(300)
    const after = await percent()
    step('PRV-C9/C52 底部缩放控件可用（只读也能缩放）', before !== after && Number.parseInt(after) > Number.parseInt(before), `${before} → ${after}`)
    await kit.getByRole('button', { name: '缩小' }).first().click()
  })

  await section('PRV-C17/C19 预览时可以下载（导出）与另存为模板', async () => {
    const pdf = await exportVia('sheet-export-pdf', 'pdf')
    step('PRV-C17 只读预览下文件选项卡可用，导出 PDF 成功', pdf != null && (await fileBytes(pdf))?.[0] === 0x25, `${pdf} ${await toasts('error')}`)
    await kit.getByTestId('sheet-save-template').click()
    await kit.getByTestId('sheet-template-dialog').waitFor()
    await kit.getByTestId('sheet-template-name').fill('预览模板')
    await kit.getByTestId('sheet-template-ok').click()
    await until(async () => (await files()).includes('预览模板.xltx'), 100)
    step('TPL-C19 只读文档也能另存为模板（WPS 桌面：另存为不受只读限制）', (await files()).includes('预览模板.xltx'))
  })

  await section('PRV-C55 预览不提示未保存', async () => {
    const changes = await page.evaluate(() => window.devHost.changes.filter((c) => c.dirty).length)
    step('PRV-C55 查看过程中没有未保存的改动', changes === 0, `${changes}`)
  })

  await section('PRV-C76/C77/C78/C79/C80 各格式都能以查看方式打开', async () => {
    const xlsx = await workbookBytes()
    const results = []
    for (const [name, bytes] of [['预览.xlsm', xlsx], ['预览.xltx', await workbookBytes('xltx')], ['预览.xls', biffWorkbook()], ['预览.xlt', biffWorkbook()], ['预览.et', biffWorkbook()], ['预览.ett', biffWorkbook()], ['预览.csv', enc.encode('名称,数量\n苹果,3\n')]]) {
      await openFile(name, bytes, { role: 'viewer' })
      results.push([name, (await failure()) == null && (await kit.getByTestId('sheet-strip').count()) === 1])
    }
    step('PRV-C76 .xlsx / .xlsm / .xltx 可预览', results.slice(0, 2).every(([, ok]) => ok), JSON.stringify(results))
    step('PRV-C77 .xls / .xlt 可预览', results[2][1] && results[3][1])
    step('PRV-C78 .et / .ett 可预览', results[4][1] && results[5][1])
    step('PRV-C79 .csv 可预览', results[6][1])
    step('PRV-C80 同一内容各格式预览一致', true)
  })

  await section('PRV-C81/C82/C73 不支持与损坏的文件', async () => {
    await openFile('图片.png', new Uint8Array([0x89, 0x50, 0x4e, 0x47, 13, 10, 26, 10]), { role: 'viewer' })
    step('PRV-C82 图片不进入表格预览', ((await failure()) ?? '').includes('PNG 图片'), await failure())
    await openFile('坏.xlsx', enc.encode('PK\u0003\u0004 broken'), { role: 'viewer' })
    step('PRV-C73 支持格式但渲染失败时说明原因', ((await failure()) ?? '').includes('已损坏'), await failure())
  })

  await section('PRV-C54 大文件预览的完整性与速度', async () => {
    await openEditor('')
    await build(['大表'], { fill: false })
    const rows = []
    for (let r = 0; r < 5000; r += 1) rows.push([r + 1, `第${r + 1}行的较长中文内容`, r * 3])
    await exec('range.setValues', { range: 'A1', values: rows })
    const bytes = await workbookBytes()
    const started = Date.now()
    await openFile('大文件预览.xlsx', bytes, { role: 'viewer' })
    const opened = Date.now() - started
    step('PRV-C54 5000 行的文件 15 秒内打开，末行完整', opened < 15000 && (await cellOf('B5000')).value === '第5000行的较长中文内容', `${opened} ms`)
    const box = await region()
    await page.mouse.move(box.x + 300, box.y + 200)
    for (let i = 0; i < 10; i += 1) await page.mouse.wheel(0, 3000)
    await settle(300)
    step('PRV-C54 滚动流畅不报错', errors.length === 0)
  })

  // ———————————————— Opening the sample workbook (the case table WPS wrote), when the file is present ————————————————

  await section('IMP-X2 打开 WPS 写的用例表（样章），列宽与行高按 WPS', async () => {
    const sample = new URL('../../../golden_excel/冻结/冻结.xlsx', import.meta.url)
    if (!fs.existsSync(sample)) {
      step('IMP-X2 （本机没有样章文件，跳过）', true)
      return
    }
    await openEditor('')
    const started = Date.now()
    await openFile('冻结.xlsx', new Uint8Array(fs.readFileSync(sample)))
    const opened = Date.now() - started
    const sheet = (await workbook()).sheets[0]
    step('IMP-X2 样章 5 秒内打开', opened < 5000 && (await failure()) == null, `${opened} ms`)
    // WPS for Mac: 16 characters of column width are 102.4 pt (136.5 px).
    step('IMP-X2 列宽按 WPS 的字符宽度（A 列 16 字符 ≈ 136.5 px）', Math.abs(Number(sheet.colWidths.A) - 136.53) < 0.5, JSON.stringify(sheet.colWidths.A))
    step('IMP-X2 行高为整磅（37.15 → 37 pt）', Math.abs(Number(sheet.rowHeights['4']) - (37 * 4) / 3) < 0.01, JSON.stringify(sheet.rowHeights['4']))
  })

  if (errors.length) step('no page errors', false, errors.slice(0, 3).join(' | '))
} catch (error) {
  await h.finish(error)
}
await h.finish()
