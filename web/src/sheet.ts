import type { KitEngineInfo } from '@kition/sdk/editor'
import type { FormulaHint } from './FormulaHints'

type Color = 'Auto' | { Rgb: number } | { Theme: unknown } | null

/** The active cell's formatting, as the engine's `inspect().style` gives it. */
export type CellStyle = {
  font?: { name?: string; size?: number; bold?: boolean; italic?: boolean; strike?: boolean; underline?: string; color?: Color }
  fill?: { pattern?: string; fg?: Color }
  align?: { h?: string; v?: string; wrap?: boolean }
  num_fmt?: string
}

/** A sheet as its tab shows it (`inspect().tabs`, in tab order). */
export type SheetTab = {
  name: string
  /** A hidden sheet has no tab; the sheet list shows it greyed. */
  hidden: boolean
  /** The tab colour (`#RRGGBB`). */
  color: string | null
  /** A protected sheet's tab shows a lock. */
  protected: boolean
}

/** The status bar's figures for a selection of more than one cell (`inspect().stats`). */
export type SelectionStats = { count: number; numeric: number; sum: number; average: number | null }

/** The current area of the selection (`inspect().lines`). */
export type SelectionLines = { rows: number; cols: number; wholeRows: boolean; wholeCols: boolean }

/** A table's options, as `inspect().tableInfo` gives them. */
export type TableInfo = {
  name: string
  range: string
  style: string
  header: boolean
  totals: boolean
  bandedRows: boolean
  bandedCols: boolean
  firstCol: boolean
  lastCol: boolean
  filter: boolean
}

/** A data validation rule in `data.validation`'s terms (`inspect().validation.rule`). */
export type ValidationRule = {
  range: string
  type: 'any' | 'whole' | 'decimal' | 'list' | 'date' | 'time' | 'textLength' | 'custom'
  operator: string
  formula1: string
  formula2: string | null
  ignoreBlank: boolean
  dropdown: boolean
  showInput: boolean
  inputTitle: string
  inputMessage: string
  showError: boolean
  errorStyle: 'stop' | 'warning' | 'information'
  errorTitle: string
  errorMessage: string
}

/**
 * The active cell's data validation (`inspect().validation`): its rectangle (document pixels),
 * its list's entries when it offers them in a dropdown, its input message, and its rule.
 */
export type CellValidation = {
  cell: string
  rect: [number, number, number, number]
  list: string[] | null
  prompt: { title: string; message: string } | null
  rule: ValidationRule
}

/** A dialog a command asked for (`inspect().dialog`): its count, name, values and command. */
export type DialogAsk = { seq: number; name: string; params: Record<string, unknown>; command: string }

/** What the editor reads from the engine's `inspect()`. */
export type SheetInfo = {
  sheets: string[]
  tabs: SheetTab[]
  sheet: string
  /** The active cell (`B3`) and the selection (`B3:D7`). */
  active: string
  selection: string
  stats: SelectionStats | null
  /** The current area's rows and columns, and whether it is whole rows or columns (the insert and delete menus). */
  lines: SelectionLines
  /** What the active cell holds: its formula (`=SUM(A1:A3)`) or text. */
  content: string
  editing: boolean
  editText: string | null
  /** What an input method composes in the cell being typed, and where it goes in `editText` (UTF-16). */
  preedit: { text: string; at: number } | null
  /** The caret of the cell being typed (document points: x0, y0, x1, y1), for the input method's field. */
  caretRect: [number, number, number, number] | null
  /** View › Show: the formula bar (the person's setting), the sheet's gridlines and headings. */
  formulaBar: boolean
  gridlines: boolean
  /** The sheet's gridline colour (`#RRGGBB`), or null for the automatic grey. */
  gridColor: string | null
  headings: boolean
  /** View › 高亮行列 (the person's): whether the selection's rows and columns are highlighted, and in which colour. */
  focusCell: boolean
  focusColor: string
  /** The frozen rows and columns, and the freeze the Freeze Panes menu previews. */
  freeze: [number, number]
  freezePreview: [number, number] | null
  /** How far frozen panes that start at a scrolled view's top-left shift the grid (document points, x and y). */
  freezeShift: [number, number]
  /** The format painter: up for one paint, sticky (until put down), or down. */
  formatPainter: 'once' | 'sticky' | null
  /** The Draw tab's pen or eraser, when one is up (a drag draws instead of selecting). */
  drawTool: string | null
  /** The person's copy the grid marks with marching ants (Escape ends it, Enter pastes it once). */
  copied: { sheet: string | null; range: string; cut: boolean } | null
  /** The active cell's rectangle in document pixels (x0, y0, x1, y1). */
  activeRect: [number, number, number, number] | null
  /** The sheet's column width and row height where a column or row has no size of its own (document pixels). */
  defaultSize: [number, number]
  /** The sheet's zoom in percent (View › Zoom), and how many times it was set (the view follows). */
  zoom: number
  zoomAsked: number
  formulaText: string
  formulaReferences: Array<{ text: string; start: number; end: number; color: [number, number, number, number] }>
  /** The function list and argument hint for the cell being typed. */
  formulaHint: FormulaHint | null
  /** The cell the formula being typed goes into, while another sheet is shown to point at its cells. */
  editCell: { sheet: string; cell: string } | null
  style: CellStyle
  /** The table the active cell is in, and its options (the Table Design tab). */
  table: string | null
  tableInfo: TableInfo | null
  /** The last dialog a command asked the editor to open (a menu item run without its values), by count. */
  dialog: DialogAsk | null
  /** The selected drawing object's id, and whether its text is being typed. */
  shape: string | null
  /** Every selected drawing object's id (the selected one, then those added with Shift). */
  shapes: string[]
  shapeEditing: boolean
  /** The selected chart's id (`drawing.chart`). */
  chart: string | null
  /** Whether Escape has something to cancel: typing, a gesture, a shape chosen to draw, a selected object. */
  escape: boolean
  /** Whether the active cell shows a checkbox (it is not typed in; Insert Link and the formula bar stay shut). */
  checkbox: boolean
  /** The last notice the engine has for the person (shown once, by count). */
  notice: { seq: number; text: string } | null
  /** The active cell's data validation, if it has a rule. */
  validation: CellValidation | null
}

export function describe(info: KitEngineInfo): SheetInfo {
  const inspected = info.inspected as Partial<SheetInfo> & { table?: string | null; shape?: string | null; chart?: string | null }
  return {
    sheets: inspected.sheets ?? [],
    tabs: Array.isArray(inspected.tabs) ? inspected.tabs : [],
    sheet: inspected.sheet ?? '',
    active: inspected.active ?? 'A1',
    selection: inspected.selection ?? 'A1',
    stats: inspected.stats ?? null,
    lines: inspected.lines ?? { rows: 1, cols: 1, wholeRows: false, wholeCols: false },
    content: inspected.content ?? '',
    editing: inspected.editing === true,
    editText: inspected.editText ?? null,
    preedit: inspected.preedit && typeof inspected.preedit.text === 'string' ? { text: inspected.preedit.text, at: Number(inspected.preedit.at) || 0 } : null,
    caretRect: Array.isArray(inspected.caretRect) && inspected.caretRect.length === 4 ? inspected.caretRect : null,
    formulaBar: inspected.formulaBar !== false,
    gridlines: inspected.gridlines !== false,
    gridColor: typeof inspected.gridColor === 'string' ? inspected.gridColor : null,
    headings: inspected.headings !== false,
    focusCell: inspected.focusCell === true,
    focusColor: typeof inspected.focusColor === 'string' ? inspected.focusColor : FOCUS_COLORS[0],
    freeze: Array.isArray(inspected.freeze) ? inspected.freeze : [0, 0],
    freezePreview: Array.isArray(inspected.freezePreview) ? inspected.freezePreview : null,
    freezeShift: Array.isArray(inspected.freezeShift) ? inspected.freezeShift : [0, 0],
    formatPainter: inspected.formatPainter === 'once' || inspected.formatPainter === 'sticky' ? inspected.formatPainter : null,
    drawTool: typeof inspected.drawTool === 'string' ? inspected.drawTool : null,
    copied: inspected.copied && typeof inspected.copied === 'object' && typeof inspected.copied.range === 'string' ? { sheet: typeof inspected.copied.sheet === 'string' ? inspected.copied.sheet : null, range: inspected.copied.range, cut: inspected.copied.cut === true } : null,
    activeRect: Array.isArray(inspected.activeRect) ? inspected.activeRect : null,
    defaultSize: Array.isArray(inspected.defaultSize) && inspected.defaultSize.length === 2 && inspected.defaultSize.every((size) => typeof size === 'number' && size > 0) ? inspected.defaultSize : [69, 19],
    zoom: typeof inspected.zoom === 'number' ? inspected.zoom : 100,
    zoomAsked: typeof inspected.zoomAsked === 'number' ? inspected.zoomAsked : 0,
    formulaText: inspected.formulaText ?? '',
    formulaReferences: inspected.formulaReferences ?? [],
    formulaHint: inspected.formulaHint ?? null,
    editCell: inspected.editCell ?? null,
    style: inspected.style ?? {},
    table: inspected.table ?? null,
    tableInfo: inspected.tableInfo ?? null,
    dialog: inspected.dialog && typeof inspected.dialog === 'object' ? inspected.dialog : null,
    shape: inspected.shape ?? null,
    shapes: Array.isArray(inspected.shapes) ? inspected.shapes : [],
    shapeEditing: inspected.shapeEditing === true,
    chart: inspected.chart ?? null,
    escape: inspected.escape === true,
    checkbox: inspected.checkbox === true,
    notice: inspected.notice && typeof inspected.notice === 'object' && typeof inspected.notice.text === 'string' ? inspected.notice : null,
    validation: inspected.validation && typeof inspected.validation === 'object' && Array.isArray(inspected.validation.rect) ? inspected.validation : null,
  }
}

/** File commands the host's own Save, Open, and Close already cover. */
export const HOST_FILE_COMMANDS = ['file.new', 'file.open', 'file.save', 'file.saveAs', 'file.close', 'file.print', 'file.printPreview', 'file.saveBytes']

/**
 * The engine's export commands, which the File tab's own Export group runs instead: the engine's
 * bytes go to the host, which writes the file (`ribbon/ExportGroup.tsx`). `file.exportPdf`,
 * `file.exportCsv` and `file.exportHtml` write to a path, which only the headless kit can.
 */
export const EXPORT_COMMANDS = ['file.export', 'file.exportPdf', 'file.exportCsv', 'file.exportHtml']

/** The engine commands each generated tab shows in Single line's row (Office's Simplified Ribbon), by menu. */
export const SHEET_PRIMARY: Record<string, readonly string[]> = {
  File: ['file.properties'],
  Insert: ['pivot.insert', 'table.insert', 'sparkline.insert', 'link.insert', 'symbol.insert'],
  Draw: ['ink.lasso', 'ink.toShape'],
  'Page Layout': ['theme.set', 'page.margins', 'page.orientation', 'page.size', 'page.printArea', 'page.breaks', 'page.printTitles', 'page.scaleToFit', 'page.printGridlines'],
  Formulas: ['formula.insertFunction', 'formula.autoSum', 'formula.financial', 'formula.logical', 'formula.text', 'formula.dateTime', 'formula.lookupReference', 'formula.mathTrig', 'name.manage', 'formula.calculateNow'],
  Data: ['data.getData', 'data.refreshAll', 'data.sortAscending', 'data.sortDescending', 'data.sort', 'data.filter', 'data.textToColumns', 'data.removeDuplicates', 'data.validation', 'data.group'],
  Automate: ['script.record', 'script.new', 'script.list'],
  Review: ['spelling.check', 'workbook.statistics', 'accessibility.check', 'comment.add', 'comment.delete', 'note.add', 'sheet.protect', 'workbook.protect'],
  View: ['view.normal', 'view.pageBreakPreview', 'view.pageLayout', 'view.freezePanes', 'view.gridlines', 'view.headings', 'view.split'],
  Table: ['table.rename', 'table.resize', 'table.removeDuplicates', 'table.convertToRange', 'table.headerRow', 'table.totalRow', 'table.bandedRows', 'table.style'],
  'Chart Design': ['chart.quickLayout', 'chart.changeColors', 'chart.selectData', 'chart.switchRowColumn', 'chart.move'],
}

/**
 * View › 高亮行列's colours, two rows of six: light ones, then deeper ones (Kition Sheet's own).
 * The first is the default (the engine's `DEFAULT_FOCUS_COLOR`).
 */
export const FOCUS_COLORS = [
  '#FBE9C6', '#DCEBFF', '#DDF3E1', '#FCE1E4', '#ECE3FA', '#E3E6EA',
  '#E8A33D', '#3D7FE0', '#3FA35B', '#D9536A', '#8A63D2', '#6B7380',
]

/** Gridline colours (each one of the legacy palette .xlsx keeps exactly). */
export const GRIDLINE_COLORS = ['#C0C0C0', '#808080', '#000000', '#99CCFF', '#3366FF', '#CCFFCC', '#339966', '#FFCC99', '#FF6600', '#FF99CC', '#993366', '#CC99FF']

/** The active cell's row (from 1) and column letters (`C5` → 5, `C`). */
export function cellParts(ref: string): { row: number; col: string; colIndex: number } {
  const match = /^([A-Z]+)(\d+)/.exec(ref) ?? ['', 'A', '1']
  const col = match[1]
  let colIndex = 0
  for (const letter of col) colIndex = colIndex * 26 + letter.charCodeAt(0) - 64
  return { row: Number(match[2]), col, colIndex }
}

/** Column letters for a column number from 1 (`2` → `B`). */
export function columnLetters(index: number): string {
  let out = ''
  for (let n = index; n > 0; n = Math.floor((n - 1) / 26)) out = String.fromCharCode(65 + ((n - 1) % 26)) + out
  return out
}

/**
 * A font the Font list offers: its name as cells keep it, other names it is found by (its English
 * or Chinese name, the pinyin of a Chinese name and its initials), and the CSS stack its row is
 * shown in (the face itself where the browser has it or the editor loads its stand-in).
 */
export type FontChoice = { name: string; also: string[]; css: string }

/** The Font list: the Chinese fonts WPS offers first, then the common Western ones. */
export const FONTS: FontChoice[] = [
  { name: '宋体', also: ['SimSun', 'songti', 'st'], css: '"SimSun", "Songti SC", "STSong", serif' },
  { name: '黑体', also: ['SimHei', 'heiti', 'ht'], css: '"SimHei", "Heiti SC", "STHeiti", sans-serif' },
  { name: '微软雅黑', also: ['Microsoft YaHei', 'weiruanyahei', 'wryh'], css: '"Microsoft YaHei", "PingFang SC", sans-serif' },
  { name: '仿宋', also: ['FangSong', 'fangsong', 'fs'], css: '"FangSong", "STFangsong", serif' },
  { name: '楷体', also: ['KaiTi', 'kaiti', 'kt'], css: '"KaiTi", "Kaiti SC", "STKaiti", "Ma Shan Zheng", serif' },
  { name: '等线', also: ['DengXian', 'dengxian', 'dx'], css: '"DengXian", "PingFang SC", sans-serif' },
  { name: '新宋体', also: ['NSimSun', 'xinsongti', 'xst'], css: '"NSimSun", "Songti SC", serif' },
  { name: 'Calibri', also: [], css: '"Calibri", "Carlito", sans-serif' },
  { name: 'Arial', also: [], css: '"Arial", "Arimo", sans-serif' },
  { name: 'Times New Roman', also: [], css: '"Times New Roman", "Tinos", serif' },
  { name: 'Courier New', also: [], css: '"Courier New", "Cousine", monospace' },
  { name: 'Cambria', also: [], css: '"Cambria", "Caladea", serif' },
  { name: 'Verdana', also: [], css: 'Verdana, sans-serif' },
  { name: 'Georgia', also: [], css: 'Georgia, serif' },
  { name: 'Helvetica', also: [], css: 'Helvetica, "Arimo", sans-serif' },
  { name: 'Tahoma', also: [], css: 'Tahoma, sans-serif' },
  { name: 'Consolas', also: [], css: 'Consolas, monospace' },
  { name: 'Aptos', also: [], css: 'Aptos, sans-serif' },
]

/** Whether a font is found by `query` (its names, the pinyin or the initials; any case). */
export function fontMatches(font: FontChoice, query: string): boolean {
  const q = query.trim().toLowerCase()
  if (!q) return true
  return [font.name, ...font.also].some((name) => name.toLowerCase().includes(q))
}

/** The Font Size list, as WPS lists it; Increase and Decrease Font Size step through it. */
export const SIZES = [6, 8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72]
export const NUMBER_FORMATS = ['General', 'Number', 'Currency', 'Accounting', 'Short Date', 'Long Date', 'Time', 'Percentage', 'Fraction', 'Scientific', 'Text']

/**
 * Gives the keyboard back to the grid (the editor's text sink) once the focused field or dialog is
 * gone: after a rename ended by a key, or a sheet dialog, Ctrl+Z and typing reach the sheet again.
 */
export const backToGrid = () => {
  window.requestAnimationFrame(() => document.querySelector<HTMLElement>('.kui-text-sink')?.focus({ preventScroll: true }))
}
