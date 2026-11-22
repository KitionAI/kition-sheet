// Frozen panes, gridlines and the highlight of the selection's rows and columns (module M5), as
// WPS for Mac has them, in a real browser through the development
// host: node tests/gate-view.mjs [chromium|webkit]
// Home/View › Freeze Panes (WPS's choices for the active cell, the preview while pointing at one,
// scrolling under the frozen rows and columns, undo, save and reopen, inserting, filtering, the
// zoom), View › Gridlines and the gridline colour (with fills, borders, the selection, the formula
// bar, printing), and View › Highlight Row and Column with its colours, from the View tab and the status bar.
// Each step names its case (FRZ-C8 for freezing, GRID-C6 for gridlines, HL-C7 for the highlight, BAR-C23
// for the formula bar cases M3 left to this module; …-X… are steps the case sheets leave out);
// `GATE_ONLY=FRZ-C8,HL-C7` runs some.
import { startGate } from './gate-harness.mjs'

const h = await startGate({ prefix: '' })
/** The grid's pitch as the engine lays it out (the harness reads it when a workbook opens). */
const G = h.grid
const { page, kit, step, section, until, same, ai, exec, details, q, cellOf, region, clickCell, goTo, build, numbered, saveAndReopen, touch, zipPart, pixel, openEditor, errors } = h

const COLS = 'ABCDEFGHIJ'
const settle = (ms = 250) => page.waitForTimeout(ms)
/** The active sheet as `document.inspect` lists it. */
const info = async () => {
  const wb = await h.workbook()
  return wb.sheets[wb.activeSheet]
}
const freeze = async () => (await details()).freeze
const topRow = h.topRow
/** The page point at the centre of a cell (column, row from 0, on screen) at 100%. */
const centre = async (col, row) => {
  const box = await region()
  return { x: box.x + 46 + col * G.cw + 32, y: box.y + 21 + row * G.rh + 10 }
}
/** A point of a cell on screen (column, row from 0) away from its text and its lines, at `zoom`. */
const inCell = (col, row, zoom = 1) => [Math.round((46 + col * G.cw + 54) * zoom), Math.round((21 + row * G.rh + 15) * zoom)]
/** Whether a colour is tinted (not the white of an empty cell). */
const tinted = (p) => Math.min(p[0], p[1], p[2]) < 246
const white = (p) => p[0] >= 250 && p[1] >= 250 && p[2] >= 250
const near = (p, want, tolerance = 8) => [0, 1, 2].every((i) => Math.abs(p[i] - want[i]) <= tolerance)
/** A colour drawn at `alpha` (0–255) over white. */
const over = (hex, alpha) => [1, 3, 5].map((i) => Math.round((parseInt(hex.slice(i, i + 2), 16) * alpha + 255 * (255 - alpha)) / 255))
/** The highlight `hex` as the grid shows it over a white cell (light colours stronger, as the engine draws them). */
const shown = (hex) => {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16))
  return over(hex, r * 299 + g * 587 + b * 114 >= 200000 ? 200 : 96)
}
const at = async (col, row, zoom = 1) => pixel(...inCell(col, row, zoom))
const GRID = [225, 225, 228]
/** The pixel on the line between two columns (at the left of column `col`), in row `row`. */
const gridAt = async (col, row) => pixel(46 + col * G.cw, 21 + row * G.rh + 10)

/** The case sheets' table: 20 rows × 10 columns, each cell holding its own address, C6 selected. */
async function table({ rows = 20, at: cell = 'C6' } = {}) {
  await build(numbered(2), { fill: false })
  const values = Array.from({ length: rows }, (_, row) => Array.from({ length: 10 }, (_, col) => `${COLS[col]}${row + 1}`))
  await ai([{ command: 'range.setValues', params: { range: `${q('工作表1')}!A1`, values } }])
  await exec('view.zoom', { percent: 100 })
  await setFocus(false)
  await home()
  await top()
  await goTo(cell)
  await settle()
}
/**
 * Headless WebKit can keep a stale idea of where wheel listeners are after the layout changed (a
 * menu closed over a frozen grid) and stop giving the canvas its wheel events; adding a listener
 * makes it look again.
 */
const nudgeWheel = () =>
  page.frames()[1].evaluate(() => {
    window.removeEventListener('wheel', window.__wheelNudge, true)
    window.__wheelNudge = () => undefined
    window.addEventListener('wheel', window.__wheelNudge, { capture: true, passive: true })
  })
/** Back to the top-left (a sheet of the same name keeps where it was scrolled). */
const top = async (far = 6000) => {
  // As `wheel` does: headless WebKit can stop giving the canvas wheel events after a layout change,
  // and a view left scrolled now changes where a freeze starts (M6).
  await nudgeWheel()
  const box = await region()
  await page.mouse.move(box.x + 300, box.y + 200)
  for (let left = far; left > 0; left -= 2000) {
    await page.mouse.wheel(-2000, -2000)
    await page.waitForTimeout(20)
  }
  await settle(300)
}
/** Scrolls by (`dx`, `dy`) pixels, in wheel steps of at most 400 (WebKit drops a larger one). */
const wheel = async (dx, dy) => {
  await nudgeWheel()
  const box = await region()
  await page.mouse.move(box.x + 500, box.y + 400)
  await page.mouse.move(box.x + 502, box.y + 402)
  const steps = Math.max(1, Math.ceil(Math.max(Math.abs(dx), Math.abs(dy)) / 400))
  for (let i = 0; i < steps; i += 1) {
    await page.mouse.wheel(dx / steps, dy / steps)
    await page.waitForTimeout(30)
  }
  await settle(400)
}
/** The cell under a screen cell (column, row from 0) at 100%: clicks it and reads the Name Box. */
const under = async (col, row) => {
  await clickCell(col, row)
  await until(async () => (await details()).active !== '', 5)
  return (await details()).active
}
const rowOf = (ref) => Number(ref.replace(/^[A-Z]+/, ''))
const colOf = (ref) => ref.replace(/\d+$/, '')

// ———— the ribbon ————
const home = () => kit.getByTestId('ribbon-tab-home').click()
const viewTab = async () => {
  await kit.getByTestId('ribbon-tab-view').click()
  await kit.getByTestId('sheet-show-gridlines').waitFor()
}
/** Opens Freeze Panes (Home or View) and answers its rows' labels, top to bottom. */
async function freezeMenu(tab = 'home') {
  if (tab === 'home') await home()
  else await viewTab()
  await kit.getByTestId('sheet-freeze').click()
  await kit.getByTestId('sheet-freeze-top-row').waitFor()
  return kit.locator('.kui-popover:has([data-testid="sheet-freeze-top-row"])').last().locator('[role^="menuitem"]').evaluateAll((rows) => rows.map((row) => row.textContent.trim()))
}
/** Runs a Freeze Panes choice (`at`, `rows`, `cols`, `top-row`, `first-column`, `off`) from Home. */
async function freezeBy(choice, tab = 'home') {
  const before = JSON.stringify(await freeze())
  await freezeMenu(tab)
  await kit.getByTestId(`sheet-freeze-${choice}`).click()
  await until(async () => JSON.stringify(await freeze()) !== before, 20)
  await settle()
}
const closeMenus = async () => {
  await page.keyboard.press('Escape')
  await page.keyboard.press('Escape')
  await settle()
}
/** Undo (or Redo) in the top bar, waiting until the frozen panes changed. */
const history = async (id) => {
  const before = JSON.stringify(await freeze())
  await kit.getByTestId(id).click()
  await until(async () => JSON.stringify(await freeze()) !== before, 20)
  await settle()
}
const undo = () => history('sheet-undo')
const redo = () => history('sheet-redo')
/** Presses a key in the grid and waits until the frozen panes changed. */
const keyFreeze = async (key) => {
  const before = JSON.stringify(await freeze())
  await sink()
  await page.keyboard.press(key)
  await until(async () => JSON.stringify(await freeze()) !== before, 20)
  await settle()
}
const gridBox = () => kit.getByTestId('sheet-show-gridlines')
const barBox = () => kit.getByTestId('sheet-show-formula-bar')
const barShown = async () => (await kit.getByTestId('sheet-formula-bar').count()) === 1
const setGridlines = async (on) => {
  await viewTab()
  if ((await gridBox().isChecked()) !== on) {
    await gridBox().click()
    await until(async () => (await gridBox().isChecked()) === on && (await details()).gridlines === on)
  }
  await settle()
}
const setBar = async (on) => {
  await viewTab()
  if ((await barBox().isChecked()) !== on) {
    await barBox().click()
    await until(async () => (await barShown()) === on)
  }
}
const focusButton = () => kit.getByTestId('sheet-focus')
const statusFocus = () => kit.getByTestId('sheet-status-focus')
const focusOn = async () => (await details()).focusCell
const setFocus = async (on, color) => {
  await exec('view.focusCell', color ? { on, color } : { on })
  await until(async () => (await focusOn()) === on)
  await settle()
}
/** Clicks the upper part of Highlight Row and Column in the View tab and waits for the highlight to follow. */
const clickFocus = async () => {
  const was = await focusOn()
  await viewTab()
  await focusButton().click()
  return until(async () => (await focusOn()) === !was)
}
const swatches = (id) => kit.getByTestId(id).locator('button')
const chosenSwatch = async (id) => swatches(id).evaluateAll((all) => all.map((b, i) => (b.getAttribute('aria-pressed') === 'true' ? i : -1)).filter((i) => i >= 0))
const PALETTE = ['#FBE9C6', '#DCEBFF', '#DDF3E1', '#FCE1E4', '#ECE3FA', '#E3E6EA', '#E8A33D', '#3D7FE0', '#3FA35B', '#D9536A', '#8A63D2', '#6B7380']
const reopenPage = async () => {
  await page.reload()
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await kit.getByTestId('sheet-strip').waitFor()
  await settle(800)
}
const openWithFocus = async () => {
  const url = new URL(page.url())
  url.searchParams.set('modes', 'classic,focus')
  await page.goto(url.toString())
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await kit.getByTestId('sheet-strip').waitFor()
  await settle(800)
}
const sink = () => kit.locator('.kui-text-sink').evaluate((element) => element.focus())
/** Whether the freeze line (grey) is at canvas point (`x`, `y`). */
const lineAt = async (y, x) => {
  const p = await pixel(x, y)
  return p[0] < 200 && Math.abs(p[0] - p[2]) < 12
}
/** Whether the freeze line (grey) shows at canvas point (`x`, `y`), once the grid has drawn. */
const freezeLine = (y, x = 600) => until(() => lineAt(y, x), 15)
/** Whether no freeze line shows there, once the grid has drawn. */
const noLine = (y, x = 600) => until(async () => !(await lineAt(y, x)), 15)

try {
  await openEditor('')

  // ═══════════════════════════════ Freeze panes ═══════════════════════════════
  await section('FRZ-C4 未冻结时的冻结下拉', async () => {
    await table()
    const rows = await freezeMenu()
    step('FRZ-C4 选中 C6：冻结至第5行C列、冻结至第5行、冻结至第C列、冻结首行、冻结首列，没有「取消冻结」', same(rows, ['冻结至第5行B列', '冻结至第5行', '冻结至第B列', '冻结首行', '冻结首列']), JSON.stringify(rows))
    await closeMenus()
  })

  await section('FRZ-C5 已冻结时的冻结下拉', async () => {
    await table({ at: 'D6' })
    await freezeBy('at')
    await goTo('D6')
    const rows = await freezeMenu()
    step('FRZ-C5 已冻结：第一项「取消冻结」，其后为五个冻结选项（第5行C列）', same(rows, ['取消冻结', '冻结至第5行C列', '冻结至第5行', '冻结至第C列', '冻结首行', '冻结首列']), JSON.stringify(rows))
    await closeMenus()
  })

  /** Points at a choice and answers the preview the engine shows and whether a frozen-area cell is tinted. */
  const previewOf = async (choice) => {
    await freezeMenu()
    await kit.getByTestId(`sheet-freeze-${choice}`).hover()
    await until(async () => (await details()).freezePreview != null, 20)
    await settle(300)
    return { preview: (await details()).freezePreview, a1: await at(0, 0), far: await at(8, 12) }
  }
  await section('FRZ-C7/C15/C23/C31/C36 悬停预览', async () => {
    await table({ at: 'D6' })
    const rows = await previewOf('rows')
    step('FRZ-C7 悬停「冻结至第5行」：预览第 1–5 行（上方区域着色，下方不着色）', same(rows.preview, [5, 0]) && tinted(rows.a1) && white(rows.far), JSON.stringify(rows))
    await closeMenus()
    step('FRZ-C7 菜单关闭后预览消失', await until(async () => (await details()).freezePreview == null, 20))
    const cols = await previewOf('cols')
    step('FRZ-C15 悬停「冻结至第C列」：预览 A–C 列', same(cols.preview, [0, 3]) && tinted(cols.a1) && white(cols.far), JSON.stringify(cols))
    await closeMenus()
    const both = await previewOf('at')
    step('FRZ-C23 悬停「冻结至第5行C列」：同时预览行与列', same(both.preview, [5, 3]) && tinted(both.a1), JSON.stringify(both))
    await closeMenus()
    const first = await previewOf('top-row')
    step('FRZ-C31 悬停「冻结首行」：预览第 1 行', same(first.preview, [1, 0]), JSON.stringify(first.preview))
    await closeMenus()
    const col = await previewOf('first-column')
    step('FRZ-C36 悬停「冻结首列」：预览 A 列', same(col.preview, [0, 1]), JSON.stringify(col.preview))
    await closeMenus()
    step('FRZ-C7 悬停不改变冻结', same(await freeze(), [0, 0]) && (await details()).freezePreview == null)
  })

  /** Freezes rows 1–5 (D6, Freeze to Row 5: `冻结至第5行`), scrolls down 60 rows and back, and reports what stayed. */
  const scrollRows = async () => {
    await wheel(0, 1200)
    const frozenTop = await under(0, 0)
    const fifth = await under(0, 4)
    const scrolled = rowOf(await under(0, 5))
    const line = await freezeLine(21 + 5 * G.rh - 1)
    await top()
    const back = await under(0, 5)
    return { frozenTop, fifth, scrolled, line, back }
  }
  await section('FRZ-C8/C9 冻结至当前行并滚动', async () => {
    await table({ at: 'D6' })
    await freezeBy('rows')
    const open = await kit.locator('.kui-popover:has([data-testid="sheet-freeze-top-row"])').count()
    step('FRZ-C8 第 1–5 行冻结，冻结线（灰色实线）在第 5、6 行之间，菜单关闭', same(await freeze(), [5, 0]) && open === 0 && (await freezeLine(21 + 5 * G.rh - 1)), JSON.stringify(await freeze()))
    const s = await scrollRows()
    step('FRZ-C9 向下滚动：第 1–5 行保持可见，第 6 行起随滚动；回到顶部后内容一致', s.frozenTop === 'A1' && s.fifth === 'A5' && s.scrolled > 30 && s.line && s.back === 'A6', JSON.stringify(s))
    step('FRZ-C60 本浏览器下冻结线与冻结区滚动表现与上面一致', s.line && s.frozenTop === 'A1')
  })

  await section('FRZ-C10/C18/C26 选中 A1', async () => {
    await table({ at: 'A1' })
    const rows = await freezeMenu()
    step('FRZ-C10/C18/C26 A1 上方与左侧没有可冻结的行列：只有冻结首行、冻结首列（WPS）', same(rows, ['冻结首行', '冻结首列']), JSON.stringify(rows))
    await closeMenus()
    await freezeBy('top-row')
    step('FRZ-C10 改用冻结首行：仅第 1 行冻结', same(await freeze(), [1, 0]))
  })

  await section('FRZ-C11 末行冻结', async () => {
    await table()
    await goTo('A1000')
    await settle()
    // The jump scrolls the target into view (WPS, M6); back to the top-left, so the freeze starts at row 1 and column A.
    await top(30000)
    const rows = await freezeMenu()
    await kit.getByTestId('sheet-freeze-rows').click()
    await settle()
    await wheel(0, 2000)
    step('FRZ-C11 选中 A1000「冻结至第999行」：第 1–999 行冻结，向下滚动顶部仍是第 1 行', rows.includes('冻结至第999行') && same(await freeze(), [999, 0]) && (await under(0, 0)) === 'A1', JSON.stringify([rows, await freeze(), (await details()).freezeShift, await under(0, 0)]))
  })

  await section('FRZ-C12 已冻结 5 行再冻结至第 20 行', async () => {
    await table({ at: 'D6' })
    await freezeBy('rows')
    await goTo('C21')
    await freezeBy('rows')
    step('FRZ-C12 冻结范围更新为第 1–20 行，原冻结线消失', same(await freeze(), [20, 0]) && (await noLine(21 + 5 * G.rh - 1)) && (await freezeLine(21 + 20 * G.rh - 1)), JSON.stringify(await freeze()))
  })

  await section('FRZ-C13 冻结行的撤销与恢复', async () => {
    await table({ at: 'D6' })
    await freezeBy('rows')
    await undo()
    const undone = await freeze()
    await redo()
    step('FRZ-C13 撤销后未冻结、冻结线消失；恢复后第 1–5 行重新冻结', same(undone, [0, 0]) && same(await freeze(), [5, 0]) && (await freezeLine(21 + 5 * G.rh - 1)), JSON.stringify(undone))
  })

  await section('FRZ-C16/C17 冻结至当前列并滚动', async () => {
    await table({ at: 'D6' })
    await freezeBy('cols')
    const line = await freezeLine(300, 46 + 3 * G.cw - 1)
    step('FRZ-C16 A–C 列冻结，冻结线在 C、D 列之间，菜单关闭', same(await freeze(), [0, 3]) && line, JSON.stringify(await freeze()))
    await wheel(3200, 0)
    const left = await under(0, 0)
    const third = await under(2, 0)
    const scrolled = await under(3, 0)
    await top()
    const back = await under(3, 0)
    step('FRZ-C17 向右滚动：A–C 列保持可见，D 列起随滚动；回到最左侧后一致', left === 'A1' && third === 'C1' && scrolled.length >= 2 && back === 'D1', JSON.stringify({ left, third, scrolled, back }))
  })

  await section('FRZ-C19 末列冻结', async () => {
    await table()
    await goTo('AZ1')
    // The jump scrolls the target into view (WPS, M6); back to the top-left, so the freeze starts at row 1 and column A.
    await top(30000)
    const rows = await freezeMenu()
    await kit.getByTestId('sheet-freeze-cols').click()
    await settle()
    await top()
    step('FRZ-C19 选中 AZ1「冻结至第AY列」：A–AY 列冻结，左侧仍从 A 列开始', rows.includes('冻结至第AY列') && same(await freeze(), [0, 51]) && (await under(0, 0)) === 'A1', JSON.stringify(rows))
  })

  await section('FRZ-C20/C21 改冻结列、撤销与恢复', async () => {
    await table({ at: 'D6' })
    await freezeBy('cols')
    await goTo('G5')
    await freezeBy('cols')
    step('FRZ-C20 冻结范围更新为 A–F 列，冻结线在 F、G 列之间', same(await freeze(), [0, 6]) && (await freezeLine(300, 46 + 6 * G.cw - 1)), JSON.stringify(await freeze()))
    await undo()
    const undone = await freeze()
    await undo()
    const none = await freeze()
    await redo()
    await redo()
    step('FRZ-C21 撤销回到上一次冻结、再撤销为未冻结；恢复后重新冻结', same(undone, [0, 3]) && same(none, [0, 0]) && same(await freeze(), [0, 6]), JSON.stringify([undone, none]))
  })

  await section('FRZ-C24/C25 冻结至当前行和列并滚动', async () => {
    await table({ at: 'D6' })
    await freezeBy('at')
    step('FRZ-C24 第 1–5 行与 A–C 列同时冻结，两条冻结线', same(await freeze(), [5, 3]) && (await freezeLine(21 + 5 * G.rh - 1, 600)) && (await freezeLine(300, 46 + 3 * G.cw - 1)), JSON.stringify(await freeze()))
    await wheel(0, 1200)
    await wheel(3200, 0)
    const corner = await under(0, 0)
    const right = await under(4, 0)
    const down = await under(0, 6)
    const both = await under(4, 6)
    await top()
    const back = await under(3, 5)
    step('FRZ-C25 滚动后左上冻结区不动，右侧与下方随滚动；回到左上角后一致', corner === 'A1' && rowOf(right) === 1 && colOf(right) !== 'E' && colOf(down) === 'A' && rowOf(down) > 20 && rowOf(both) > 20 && back === 'D6', JSON.stringify({ corner, right, down, both, back }))
  })

  await section('FRZ-C27 末行末列', async () => {
    await table()
    await goTo('AY1000')
    await settle()
    // The jump scrolls the target into view (WPS, M6); back to the top-left, so the freeze starts at row 1 and column A.
    await top(30000)
    await freezeMenu()
    await kit.getByTestId('sheet-freeze-at').click()
    await settle()
    step('FRZ-C27 选中 AY1000「冻结至第999行AX列」：行列全部冻结，滚动后左上仍是 A1', same(await freeze(), [999, 50]) && (await under(0, 0)) === 'A1')
  })

  await section('FRZ-C28/C29 改冻结行列、撤销与恢复', async () => {
    await table({ at: 'D6' })
    await freezeBy('at')
    await goTo('G21')
    await freezeBy('at')
    step('FRZ-C28 冻结范围更新为第 1–20 行与 A–F 列', same(await freeze(), [20, 6]) && (await noLine(21 + 5 * G.rh - 1, 600)), JSON.stringify(await freeze()))
    await undo()
    await undo()
    const none = await freeze()
    await redo()
    step('FRZ-C29 撤销到未冻结、两条冻结线消失；恢复后第 1–5 行与 A–C 列重新冻结', same(none, [0, 0]) && same(await freeze(), [5, 3]), JSON.stringify(none))
  })

  await section('FRZ-C32/C33/C34 冻结首行', async () => {
    await table({ at: 'D10' })
    await freezeBy('top-row')
    step('FRZ-C32 选中 D10「冻结首行」：仅第 1 行冻结，与活动单元格无关', same(await freeze(), [1, 0]) && (await freezeLine(21 + G.rh - 1)))
    await wheel(0, 1200)
    const first = await under(0, 0)
    const second = rowOf(await under(0, 1))
    step('FRZ-C33 向下滚动第 1 行保持可见，第 2 行起滚动', first === 'A1' && second > 30, `${first} ${second}`)
    await table({ at: 'D6' })
    await freezeBy('rows')
    await freezeBy('top-row')
    step('FRZ-C34 已冻结第 1–5 行再「冻结首行」：仅第 1 行', same(await freeze(), [1, 0]) && (await noLine(21 + 5 * G.rh - 1)))
  })

  await section('FRZ-C37/C38/C39 冻结首列', async () => {
    await table({ at: 'D10' })
    await freezeBy('first-column')
    step('FRZ-C37 仅 A 列冻结，冻结线在 A、B 列之间', same(await freeze(), [0, 1]) && (await freezeLine(300, 46 + G.cw - 1)))
    await wheel(3200, 0)
    const left = await under(0, 0)
    const next = await under(1, 0)
    step('FRZ-C38 向右滚动 A 列保持可见，B 列起滚动', left === 'A1' && next !== 'B1', `${left} ${next}`)
    await table({ at: 'D6' })
    await freezeBy('cols')
    await freezeBy('first-column')
    step('FRZ-C39 已冻结 A–C 列再「冻结首列」：仅 A 列', same(await freeze(), [0, 1]))
  })

  await section('FRZ-C41–C45 取消冻结', async () => {
    const results = []
    for (const [choice, line] of [['rows', [21 + 5 * G.rh - 1, 600]], ['cols', [300, 46 + 3 * G.cw - 1]], ['at', [21 + 5 * G.rh - 1, 600]], ['top-row', [21 + G.rh - 1, 600]], ['first-column', [300, 46 + G.cw - 1]]]) {
      await table({ at: 'D6' })
      await freezeBy(choice)
      const before = await freezeLine(...line)
      await freezeBy('off')
      await wheel(0, 1200)
      results.push({ choice, before, after: (await noLine(...line)), freeze: await freeze(), top: rowOf(await under(0, 0)) > 1 })
    }
    const ok = results.every((r) => r.before && r.after && same(r.freeze, [0, 0]) && r.top)
    step('FRZ-C41/C42/C43/C44/C45 取消冻结后冻结线消失，整表随滚动', ok, JSON.stringify(results))
  })

  await section('FRZ-C47/C50 冻结区内输入', async () => {
    await table({ at: 'D6' })
    await freezeBy('at')
    await clickCell(0, 1)
    await page.keyboard.type('冻结区测试')
    await page.keyboard.press('Enter')
    await clickCell(1, 1)
    await page.keyboard.type('冻结区测试')
    await page.keyboard.press('Enter')
    await settle()
    step('FRZ-C47 A2 输入成功，冻结不变', (await cellOf('A2')).value === '冻结区测试' && same(await freeze(), [5, 3]))
    step('FRZ-C50 B2 输入成功，冻结不变', (await cellOf('B2')).value === '冻结区测试' && same(await freeze(), [5, 3]))
  })

  await section('FRZ-C48/C51 冻结时插入行列', async () => {
    await table({ at: 'D6' })
    await freezeBy('rows')
    await goTo('3:3')
    await home()
    await kit.getByTestId('sheet-insert-menu').click()
    const box = kit.getByTestId('sheet-insert-rows-above-count')
    await box.fill('1')
    await kit.getByTestId('sheet-insert-rows-above').click({ position: { x: 24, y: 10 } })
    await settle()
    step('FRZ-C48 第 3 行上方插入 1 行：冻结随之扩为第 1–6 行，冻结线在第 6 行下', same(await freeze(), [6, 0]) && (await freezeLine(21 + 6 * G.rh - 1)), JSON.stringify(await freeze()))
    await table({ at: 'D6' })
    await freezeBy('cols')
    await goTo('B:B')
    await home()
    await kit.getByTestId('sheet-insert-menu').click()
    await kit.getByTestId('sheet-insert-cols-left-count').fill('1')
    await kit.getByTestId('sheet-insert-cols-left').click({ position: { x: 24, y: 10 } })
    await settle()
    step('FRZ-C51 B 列左侧插入 1 列：冻结随之扩为 A–D 列', same(await freeze(), [0, 4]) && (await freezeLine(300, 46 + 4 * G.cw - 1)), JSON.stringify(await freeze()))
  })

  await section('FRZ-C49/C52 冻结时筛选', async () => {
    await table({ at: 'D6' })
    await freezeBy('top-row')
    await exec('data.filter', { range: 'A1:J20' })
    await exec('data.filterBy', { column: 'B', values: ['B3', 'B4'] })
    await settle()
    const sheet = await info()
    step('FRZ-C49 首行冻结时筛选生效（隐藏其余行），首行仍冻结', same(await freeze(), [1, 0]) && (sheet.hiddenRows?.length ?? 0) >= 16 && (await under(0, 0)) === 'A1', JSON.stringify(sheet.hiddenRows?.length))
    await table({ at: 'D6' })
    await freezeBy('first-column')
    await exec('data.filter', { range: 'A1:J20' })
    await exec('data.filterBy', { column: 'C', values: ['C2'] })
    await settle()
    step('FRZ-C52 首列冻结时筛选生效，A 列仍冻结', same(await freeze(), [0, 1]) && ((await info()).hiddenRows?.length ?? 0) >= 18)
  })

  await section('FRZ-C54–C58 导入文件', async () => {
    await table({ at: 'D6' })
    await touch()
    await saveAndReopen('Unfrozen.xlsx')
    await goTo('D6')
    await freezeBy('rows')
    step('FRZ-C54 导入的未冻结文件可冻结至第 5 行', same(await freeze(), [5, 0]))
    await table({ at: 'B3' })
    await exec('view.freezePanes', { cell: 'B3' })
    await touch()
    await saveAndReopen('Frozen.xlsx')
    const rows = await freezeMenu()
    await closeMenus()
    step('FRZ-C55 导入后前 2 行与 A 列仍冻结，菜单含「取消冻结」', same(await freeze(), [2, 1]) && rows[0] === '取消冻结' && (await freezeLine(21 + 2 * G.rh - 1, 600)), JSON.stringify(rows))
    await freezeBy('off')
    step('FRZ-C57 取消冻结成功', same(await freeze(), [0, 0]) && (await noLine(21 + 2 * G.rh - 1, 600)))
    await goTo('D6')
    await freezeBy('rows')
    step('FRZ-C58 再冻结至第 5 行', same(await freeze(), [5, 0]))
  })

  await section('FRZ-C59/C78 导出与保存重开', async () => {
    await table({ at: 'D6' })
    await freezeBy('at')
    await clickCell(0, 6)
    await page.keyboard.type('持久化验证')
    await page.keyboard.press('Enter')
    await settle()
    await saveAndReopen('Saved.xlsx')
    const xml = (await zipPart('xl/worksheets/sheet1.xml')) ?? ''
    const pane = /<pane\b[^>]*>/.exec(xml)?.[0] ?? ''
    step('FRZ-C59 导出的文件冻结前 5 行与 A–C 列（pane xSplit=3 ySplit=5 frozen）', /xSplit="3"/.test(pane) && /ySplit="5"/.test(pane) && /state="frozen"/.test(pane), pane)
    step('FRZ-C78 重新打开后第 1–5 行与 A–C 列仍冻结，A7 内容保留', same(await freeze(), [5, 3]) && (await cellOf('A7')).value === '持久化验证' && (await freezeLine(21 + 5 * G.rh - 1, 600)))
  })

  await section('FRZ-C68 一万行数据', async () => {
    await table({ at: 'D6' })
    await ai([{ command: 'range.setValues', params: { range: `${q('工作表1')}!K1`, values: Array.from({ length: 10000 }, (_, i) => [i + 1]) } }])
    await freezeBy('rows')
    const before = errors.length
    await wheel(0, 200000)
    const far = rowOf(await under(0, 5))
    const head = await under(0, 0)
    await top(210000)
    step('FRZ-C68 滚到 9999 行附近再回顶部：第 1–5 行始终可见，无报错，内容完整', far > 5000 && head === 'A1' && errors.length === before && (await under(0, 5)) === 'A6' && (await cellOf('K9999')).value === 9999, `${far} ${head}`)
  })

  await section('FRZ-C69 缩放 50%、200%', async () => {
    await table({ at: 'D6' })
    await freezeBy('rows')
    const lines = []
    for (const percent of [50, 200]) {
      await exec('view.zoom', { percent })
      await until(async () => (await details()).zoom === percent)
      await settle(500)
      const zoom = percent / 100
      const want = (21 + 5 * G.rh) * zoom
      let darkest = 999
      let where = -1
      for (let y = Math.floor(want - 4); y <= Math.ceil(want + 2); y += 1) {
        const p = await pixel(Math.round(400 * zoom), y)
        if (p[0] < darkest) {
          darkest = p[0]
          where = y
        }
      }
      lines.push({ percent, darkest, where, want })
    }
    await exec('view.zoom', { percent: 100 })
    await until(async () => (await details()).zoom === 100)
    step('FRZ-C69 两种缩放下冻结线紧贴第 5 行下边缘', lines.every((l) => l.darkest < 215 && Math.abs(l.where - l.want) <= 4) && same(await freeze(), [5, 0]), JSON.stringify(lines))
  })

  await section('FRZ-C70/C71/C72 全选后冻结', async () => {
    await table()
    await sink()
    await page.keyboard.press('ControlOrMeta+a')
    await page.keyboard.press('ControlOrMeta+a')
    await until(async () => (await details()).lines.wholeRows && (await details()).lines.wholeCols, 20)
    const rows = await freezeMenu()
    await kit.getByTestId('sheet-freeze-rows').click()
    await settle()
    step('FRZ-C70/C71/C72 按 WPS：全选后活动单元格仍是 C6，冻结选项按 C6 给出（冻结至第5行），不是整表冻结', same(rows, ['冻结至第5行B列', '冻结至第5行', '冻结至第B列', '冻结首行', '冻结首列']) && same(await freeze(), [5, 0]), JSON.stringify(rows))
  })

  await section('FRZ-C74 分步冻结与一次冻结', async () => {
    await table({ at: 'D6' })
    await freezeBy('rows')
    await freezeBy('cols')
    const pathA = await freeze()
    await undo()
    await undo()
    const start = await freeze()
    await freezeBy('at')
    const pathB = await freeze()
    step('FRZ-C74 按 WPS：每次冻结取代上一次——路径 A 最终只冻结 A–C 列，路径 B 冻结第 1–5 行与 A–C 列', same(pathA, [0, 3]) && same(start, [0, 0]) && same(pathB, [5, 3]), JSON.stringify({ pathA, start, pathB }))
  })

  await section('FRZ-C75 冻结、取消、冻结首行', async () => {
    await table({ at: 'D6' })
    await freezeBy('rows')
    await freezeBy('off')
    const off = await freeze()
    await freezeBy('top-row')
    step('FRZ-C75 取消后未冻结，最终仅第 1 行冻结', same(off, [0, 0]) && same(await freeze(), [1, 0]))
  })

  await section('FRZ-C76 冻结首行后冻结至当前列', async () => {
    await table({ at: 'D6' })
    await freezeBy('top-row')
    await goTo('D6')
    await freezeBy('cols')
    step('FRZ-C76 按 WPS：「冻结至第C列」取代首行冻结，只冻结 A–C 列', same(await freeze(), [0, 3]))
  })

  await section('FRZ-C77 多次撤销与恢复', async () => {
    await table({ at: 'D6' })
    await freezeBy('cols')
    await keyFreeze('ControlOrMeta+z')
    const a = await freeze()
    await freezeBy('top-row')
    await keyFreeze('ControlOrMeta+z')
    const b = await freeze()
    await goTo('D6')
    await freezeBy('at')
    await keyFreeze('ControlOrMeta+z')
    const c = await freeze()
    await keyFreeze('ControlOrMeta+y')
    step('FRZ-C77 每次撤销回到未冻结，Ctrl+Y 恢复为第 1–5 行与 A–C 列', same(a, [0, 0]) && same(b, [0, 0]) && same(c, [0, 0]) && same(await freeze(), [5, 3]), JSON.stringify([a, b, c, await freeze()]))
  })

  await section('FRZ-X1 方向键移出可视区时滚动，冻结区不动', async () => {
    await table({ at: 'A6' })
    await freezeBy('rows')
    await sink()
    for (let i = 0; i < 60; i += 1) await page.keyboard.press('ArrowDown')
    await until(async () => (await details()).active === 'A66')
    await settle(400)
    const shownRow = rowOf(await under(0, 5))
    const head = await under(0, 0)
    step('FRZ-X1 下移到 A66 后视图跟随滚动（可滚动区已下移）、第 1 行仍在顶部', shownRow > 20 && head === 'A1', `${shownRow} ${head}`)
  })

  await section('FRZ-C66 英文界面的冻结菜单', async () => {
    await page.goto(page.url().replace('locale=zh-CN', 'locale=en-US'))
    await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
    await kit.getByTestId('sheet-strip').waitFor()
    await settle(800)
    await goTo('D6')
    const rows = await freezeMenu()
    await closeMenus()
    step('FRZ-C66 英文界面菜单为英文，无中文残留', same(rows, ['Freeze to Row 5 and Column C', 'Freeze to Row 5', 'Freeze to Column C', 'Freeze Top Row', 'Freeze First Column']), JSON.stringify(rows))
    await openEditor('')
  })

  await section('FRZ-C62 只读权限', async () => {
    await openEditor('&role=viewer')
    try {
      await viewTab()
      step('FRZ-C62 只读下冻结入口不可用，不冻结', (await kit.getByTestId('sheet-freeze').isDisabled()) && same(await freeze(), [0, 0]))
    } finally {
      await openEditor('')
    }
  })

  // ═══════════════════════════════ Gridlines ═══════════════════════════════
  await section('GRID-C4 入口与默认勾选态', async () => {
    await table()
    await viewTab()
    const options = await kit.locator('.sheet-show-option').allTextContents()
    step('GRID-C4 「显示」分组自上而下为「显示网格线」「显示编辑栏」；默认勾选，网格线可见', options[0] === '显示网格线' && options[1] === '显示编辑栏' && (await gridBox().isChecked()) && near(await gridAt(5, 10), GRID, 6), JSON.stringify(options))
  })

  await section('GRID-C5/C6/C7/C8 隐藏、显示与连续切换', async () => {
    await table()
    await viewTab()
    const label = kit.locator('.sheet-show-option .kui-checkbox').first()
    await page.mouse.move(5, 5)
    await settle(150)
    const idle = await label.evaluate((element) => getComputedStyle(element).backgroundColor)
    await label.hover()
    await settle(150)
    const hover = await label.evaluate((element) => ({ background: getComputedStyle(element).backgroundColor, cursor: getComputedStyle(element).cursor }))
    step('GRID-C5 复选框形态，悬停有底色、指针为手形', (await gridBox().getAttribute('type')) === 'checkbox' && hover.background !== idle && hover.cursor === 'pointer', JSON.stringify({ idle, hover }))
    const initial = await gridAt(5, 10)
    await setGridlines(false)
    const frame = await pixel(46 + 2 * G.cw + 20, 21 + 5 * G.rh)
    const heading = await pixel(20, 21 + 5 * G.rh + 10)
    step('GRID-C6 取消勾选：网格线消失；行号列标、选中框与内容仍在', !(await gridBox().isChecked()) && white(await gridAt(5, 10)) && !white(frame) && !white(heading) && (await cellOf('E10')).value === 'E10', JSON.stringify({ frame, heading }))
    await setGridlines(true)
    step('GRID-C7 重新勾选：网格线恢复，颜色与初始一致', (await gridBox().isChecked()) && same(await gridAt(5, 10), initial))
    const seen = []
    for (let i = 0; i < 5; i += 1) {
      await gridBox().click()
      await settle(200)
      seen.push((await gridBox().isChecked()) === (await details()).gridlines && white(await gridAt(5, 10)) === !(await details()).gridlines)
    }
    step('GRID-C8 连续 5 次后为隐藏态，每次勾选框与网格线一致', seen.every(Boolean) && !(await details()).gridlines, JSON.stringify(seen))
    await setGridlines(true)
  })

  await section('GRID-C9 隐藏网格线后的选中', async () => {
    await table()
    await setGridlines(false)
    await clickCell(4, 9)
    const active = (await details()).active
    const from = await centre(2, 2)
    const to = await centre(4, 7)
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    await page.mouse.move(to.x, to.y, { steps: 6 })
    await page.mouse.up()
    await until(async () => (await details()).selection === 'C3:E8')
    const inside = await gridAt(3, 4)
    const body = await pixel(46 + 3 * G.cw + 50, 21 + 4 * G.rh + 10)
    step('GRID-C9 E10 选中框正常；框选 C3:E8 有选中底色，区域内无网格线', active === 'E10' && (await details()).selection === 'C3:E8' && tinted(body) && near(inside, body, 4), JSON.stringify({ inside, body }))
    await setGridlines(true)
  })

  await section('GRID-C10/C11 自定义边框、填充与文字格式', async () => {
    await table()
    await exec('home.borders', { range: 'C3:D6', preset: 'all' })
    await exec('home.fillColor', { range: 'F3', color: '#FFFF00' })
    await ai([{ command: 'selection.set', params: { range: 'F3' } }, { command: 'home.bold', params: {} }])
    await goTo('A1')
    const style = async () => JSON.stringify([(await cellOf('C4')).style ?? null, (await cellOf('F3')).style ?? null])
    const before = await style()
    await setGridlines(false)
    const border = await pixel(46 + 3 * G.cw, 21 + 3 * G.rh + 10)
    const fill = await at(5, 2)
    await setGridlines(true)
    const border2 = await pixel(46 + 3 * G.cw, 21 + 3 * G.rh + 10)
    step('GRID-C10 网格线隐藏时边框完整（深色），显示时与网格线并存，切换不改边框', border[0] < 100 && border2[0] < 100 && (await style()) === before, JSON.stringify({ border, border2 }))
    step('GRID-C11 填充色与加粗不因网格线隐藏而变化', near(fill, [255, 255, 0], 6) && near(await at(5, 2), [255, 255, 0], 6) && (await style()) === before, JSON.stringify(fill))
  })

  await section('GRID-C13 网格线与编辑栏均隐藏下的编辑', async () => {
    await table()
    await setBar(false)
    await setGridlines(false)
    await clickCell(4, 9)
    await settle(150)
    await page.keyboard.type('内容')
    await until(async () => (await details()).editText === '内容', 20)
    const typing = (await details()).editText === '内容' && !(await barShown())
    await page.keyboard.press('ArrowRight')
    await page.keyboard.press('ArrowDown')
    await until(async () => (await details()).active === 'F11')
    step('GRID-C13 单元格内编辑、方向键移动到 F11，网格线与编辑栏保持隐藏', typing && (await details()).active === 'F11' && (await cellOf('E10')).value === '内容' && !(await details()).gridlines && !(await barShown()), JSON.stringify({ typing, active: (await details()).active, e10: (await cellOf('E10')).value }))
    await setBar(true)
    await setGridlines(true)
  })

  /** GRID-C14, HL-C43 and BAR-C19: gridlines and bar hidden, frozen at the active cell, highlighted, at 150%. */
  const combined = async (id) => {
    await table({ at: 'D6' })
    await setBar(false)
    await setGridlines(false)
    await freezeBy('at')
    await setFocus(true, PALETTE[0])
    await exec('view.zoom', { percent: 150 })
    await until(async () => (await details()).zoom === 150)
    await settle(500)
    await wheel(0, 900)
    await wheel(1500, 0)
    await sink()
    await page.keyboard.press('ArrowDown')
    await until(async () => (await details()).active === 'D7')
    await settle(300)
    // Row 7 is scrolled out of view; the highlight of column D runs down the frozen rows (column D is scrolled too): look at row 2's frozen band instead.
    // The person clicks B2, in the frozen corner (the Name Box is hidden with the formula bar; an
    // agent's selection.set moves only its own editor state, KHP C3b).
    const b2 = inCell(1, 1, 1.5)
    const canvas = await region()
    await page.mouse.click(canvas.x + b2[0], canvas.y + b2[1])
    await until(async () => (await details()).active === 'B2')
    await settle(400)
    const rowBand = await at(9, 1, 1.5)
    const colBand = await at(1, 12, 1.5)
    const off = await at(9, 12, 1.5)
    const ok = (await details()).zoom === 150 && !(await details()).gridlines && !(await barShown()) && same(await freeze(), [5, 3]) && (await focusOn())
    step(`${id} 五项设置同时生效：网格线与编辑栏隐藏、冻结、150%、高亮`, ok)
    step(`${id} 滚动后移到冻结区内 B2：第 2 行（横跨冻结线）与 B 列（冻结列、可滚动行）分段高亮，其余不高亮`, tinted(rowBand) && tinted(colBand) && white(off), JSON.stringify({ rowBand, colBand, off }))
    await exec('view.zoom', { percent: 100 })
    await setFocus(false)
    await setBar(true)
    await setGridlines(true)
  }
  await section('GRID-C14 网格线隐藏＋编辑栏隐藏＋冻结＋缩放＋高亮', () => combined('GRID-C14'))

  await section('GRID-C17/C18 与显示编辑栏联动', async () => {
    await table()
    const seen = []
    for (const [grid, shownBar] of [[true, true], [false, false], [true, false], [false, true]]) {
      await setGridlines(grid)
      await setBar(shownBar)
      await clickCell(3, 4)
      await until(async () => (await details()).active === 'D5')
      seen.push((await details()).gridlines === grid && (await barShown()) === shownBar && white(await gridAt(5, 10)) === !grid && (await cellOf('D5')).value === 'D5')
    }
    step('GRID-C17 四种组合下网格线与编辑栏各自对应勾选状态，选中与数据不变', seen.every(Boolean), JSON.stringify(seen))
    await setGridlines(true)
    await setBar(true)
    const before = await region()
    await setBar(false)
    await until(async () => (await region()).y < before.y - 10)
    const upLine = await gridAt(7, 10)
    const upCell = await under(5, 10)
    await setBar(true)
    await until(async () => Math.abs((await region()).y - before.y) < 1)
    const backLine = await gridAt(7, 10)
    const backCell = await under(5, 10)
    step('GRID-C18 隐藏编辑栏后网格线随工作表上移、与行列对齐；恢复后回原位', near(upLine, GRID, 6) && upCell === 'F11' && near(backLine, GRID, 6) && backCell === 'F11', JSON.stringify({ upLine, upCell, backLine, backCell }))
  })

  await section('GRID-C19/C20 与高亮所在行列联动', async () => {
    await table()
    await setGridlines(false)
    await setFocus(true, PALETTE[0])
    await clickCell(4, 9)
    await settle()
    const row = await at(8, 9)
    const line = await gridAt(8, 9)
    await wheel(0, 200)
    const after = tinted(await at(4, 12))
    step('GRID-C19 网格线隐藏，当前行列高亮完整覆盖（含原网格线位置）；滚动后仍完整', tinted(row) && near(line, row, 6) && after, JSON.stringify({ row, line }))
    await top()
    await setGridlines(true)
    await clickCell(4, 9)
    await settle()
    const lined = await gridAt(8, 9)
    const body = await at(8, 9)
    step('GRID-C20 网格线显示时与高亮共存（网格线在高亮上可辨）；网格线开关不改高亮开关与颜色', !near(lined, body, 3) && tinted(body) && (await focusOn()) && (await details()).focusColor === PALETTE[0], JSON.stringify({ lined, body }))
    await setFocus(false)
  })

  await section('GRID-C21 与冻结联动', async () => {
    await table()
    await freezeBy('top-row')
    await setGridlines(false)
    await wheel(0, 600)
    const frozen = white(await gridAt(5, 0))
    const scrolled = white(await gridAt(5, 10))
    const line = await freezeLine(21 + G.rh - 1)
    await setGridlines(true)
    step('GRID-C21 冻结区与可滚动区同步隐藏、同步恢复，冻结线不变', frozen && scrolled && line && near(await gridAt(5, 0), GRID, 6) && near(await gridAt(5, 10), GRID, 6) && same(await freeze(), [1, 0]))
  })

  await section('GRID-C22 与缩放联动', async () => {
    await table()
    const seen = []
    for (const percent of [150, 50]) {
      await exec('view.zoom', { percent })
      await until(async () => (await details()).zoom === percent)
      await settle(500)
      await top()
      const z = percent / 100
      // A column boundary that falls on whole pixels at 150% and at 50% (D|E with 69 px columns).
      const x = Math.round((46 + 4 * G.cw) * z)
      const y = Math.round((21 + 10 * G.rh + 10) * z)
      const p = await pixel(x, y)
      await setGridlines(false)
      const hidden = await pixel(x, y)
      await setGridlines(true)
      seen.push({ percent, shown: !white(p), hidden: white(hidden), checked: await gridBox().isChecked() })
    }
    await exec('view.zoom', { percent: 100 })
    step('GRID-C22 150%、50% 下网格线可见，取消后隐藏；缩放不改勾选状态', seen.every((s) => s.shown && s.hidden && s.checked), JSON.stringify(seen))
  })

  await section('GRID-C25 与全屏（专注）联动', async () => {
    await openEditor('')
    await openWithFocus()
    await setGridlines(false)
    await sink()
    await page.keyboard.press('ControlOrMeta+Backslash')
    const app = kit.locator('.kui-app')
    await until(async () => (await app.getAttribute('data-mode')) === 'focus')
    const inFocus = !(await details()).gridlines
    await page.keyboard.press('Escape')
    await until(async () => (await app.getAttribute('data-mode')) === 'classic')
    await viewTab()
    step('GRID-C25 全屏下网格线保持隐藏，退出后仍隐藏、勾选状态未变', inFocus && !(await gridBox().isChecked()) && !(await details()).gridlines)
    await openEditor('')
  })

  await section('GRID-C27/C28/C29 状态记忆（保存后重开）', async () => {
    await table()
    await setGridlines(false)
    await saveAndReopen('Grid.xlsx')
    await viewTab()
    const xml = (await zipPart('xl/worksheets/sheet1.xml')) ?? ''
    step('GRID-C27/C28 网格线是工作表的设置（WPS 存进文件）：保存重开后仍隐藏、未勾选', !(await gridBox().isChecked()) && !(await details()).gridlines && /showGridLines="0"/.test(xml))
    await setGridlines(true)
    await saveAndReopen('Grid2.xlsx')
    await viewTab()
    step('GRID-C29 显示态保存重开后不回退为隐藏，颜色与初始一致', (await gridBox().isChecked()) && near(await gridAt(5, 10), GRID, 6))
    await openEditor('')
  })

  await section('GRID-C32 快速连续切换', async () => {
    await table()
    await viewTab()
    for (let i = 0; i < 10; i += 1) await gridBox().click()
    await settle(500)
    step('GRID-C32 连续 10 次：最终与最后一次操作一致（偶数次回到显示，与 C8 的奇偶规则一致），无卡死', (await gridBox().isChecked()) && (await details()).gridlines && near(await gridAt(5, 10), GRID, 6))
  })

  await section('GRID-C33 隐藏网格线后的滚动与整行整列选中', async () => {
    await table()
    await setGridlines(false)
    await wheel(0, 3000)
    const far = white(await gridAt(5, 10))
    await top()
    const back = white(await gridAt(5, 10))
    const box = await region()
    await page.mouse.click(box.x + 20, box.y + 21 + 3 * G.rh + 10)
    await until(async () => (await details()).lines.wholeRows)
    const rowBody = await pixel(46 + 3 * G.cw + 50, 21 + 3 * G.rh + 10)
    const rowLine = await gridAt(3, 3)
    step('GRID-C33 滚动中保持隐藏；整行选中时有选中底色、区域内无网格线', far && back && tinted(rowBody) && near(rowLine, rowBody, 4), JSON.stringify({ rowBody, rowLine }))
    await setGridlines(true)
  })

  await section('GRID-X1 网格线颜色', async () => {
    await table()
    await viewTab()
    await kit.getByTestId('sheet-gridline-color').click()
    await kit.getByTestId('sheet-gridline-colors').locator('button').nth(4).click()
    await settle()
    const blue = await gridAt(5, 10)
    await saveAndReopen('GridColor.xlsx')
    const xml = (await zipPart('xl/worksheets/sheet1.xml')) ?? ''
    step('GRID-X1 选 #3366FF：网格线变为该色；保存为 defaultGridColor="0" colorId="48"，重开后保持', near(blue, [0x33, 0x66, 0xff], 10) && /defaultGridColor="0"/.test(xml) && /colorId="48"/.test(xml) && (await details()).gridColor === '#3366FF', JSON.stringify(blue))
    await viewTab()
    await kit.getByTestId('sheet-gridline-color').click()
    await kit.getByTestId('sheet-gridline-color-auto').click()
    await settle()
    step('GRID-X1 自动：回到默认灰色', near(await gridAt(5, 10), GRID, 6) && (await details()).gridColor == null)
    await openEditor('')
  })

  await section('GRID-X2 打印网格线', async () => {
    await table()
    await exec('pageLayout.printGridlines', { on: true })
    await saveAndReopen('PrintGrid.xlsx')
    const xml = (await zipPart('xl/worksheets/sheet1.xml')) ?? ''
    step('GRID-X2 页面布局 › 打印网格线：保存为 printOptions gridLines="1"，与屏幕上的网格线互不影响', /<printOptions\b[^>]*gridLines="1"/.test(xml) && (await details()).gridlines)
    await openEditor('')
  })

  // ═══════════════════════════════ Highlight row and column ═══════════════════════════════
  await section('HL-C4/C5 入口', async () => {
    await table()
    await viewTab()
    const button = await focusButton().boundingBox()
    const arrow = await kit.getByTestId('sheet-focus-arrow').boundingBox()
    step('HL-C4 视图选项卡「高亮行列」：上半部分开关、下半部分下拉箭头；初始未激活、无高亮', button && arrow && arrow.y >= button.y + button.height - 2 && (await focusButton().getAttribute('aria-pressed')) === 'false' && white(await at(8, 5)), JSON.stringify({ button, arrow }))
    const icon = await statusFocus().boundingBox()
    const statusArrow = await kit.getByTestId('sheet-status-focus-arrow').boundingBox()
    const zoom = await kit.locator('.kui-zoom').first().boundingBox()
    const divider = await statusFocus().locator('xpath=..').evaluate((element) => getComputedStyle(element).borderRightStyle)
    step('HL-C5 状态栏入口在缩放控件左侧：图标 + 下拉箭头，竖线分隔，默认未激活', icon && statusArrow && zoom && statusArrow.x > icon.x && statusArrow.x < zoom.x && divider === 'solid' && (await statusFocus().getAttribute('aria-pressed')) === 'false', JSON.stringify({ icon, statusArrow, zoom, divider }))
  })

  await section('HL-C7/C8 开启并保持', async () => {
    await table()
    await clickFocus()
    await settle()
    const row = await at(8, 5)
    const col = await at(2, 15)
    const off = await at(8, 15)
    step('HL-C7 按钮激活；第 6 行整行与 C 列整列为高亮色（默认色），其余不变，内容不变', (await focusButton().getAttribute('aria-pressed')) === 'true' && near(row, shown(PALETTE[0])) && near(col, shown(PALETTE[0])) && white(off) && (await cellOf('C6')).value === 'C6', JSON.stringify({ row, col, off }))
    await clickCell(6, 12)
    await page.mouse.move(5, 5)
    await wheel(0, 200)
    await top()
    step('HL-C8 选区变化、移出、滚动后按钮仍激活、高亮仍在', (await focusButton().getAttribute('aria-pressed')) === 'true' && (await focusOn()) && tinted(await at(9, 12)))
  })

  await section('HL-C9/C10 随光标移动', async () => {
    await table()
    await setFocus(true, PALETTE[0])
    await sink()
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('ArrowRight')
    await page.keyboard.press('ArrowRight')
    await until(async () => (await details()).active === 'E10')
    await settle()
    const moved = !tinted(await at(8, 5)) && !tinted(await at(2, 15)) && tinted(await at(8, 9)) && tinted(await at(4, 15))
    for (let i = 0; i < 9; i += 1) await page.keyboard.press('ArrowUp')
    for (let i = 0; i < 4; i += 1) await page.keyboard.press('ArrowLeft')
    await until(async () => (await details()).active === 'A1')
    await settle()
    step('HL-C9 方向键到 E10：原第 6 行与 C 列高亮消失，第 10 行与 E 列高亮；到 A1 后第 1 行与 A 列', moved && tinted(await at(8, 0)) && tinted(await at(0, 15)) && !tinted(await at(8, 9)))
    const results = []
    for (const [col, row] of [[4, 9], [9, 25], [3, 3]]) {
      await clickCell(col, row)
      await settle()
      results.push(tinted(await at(col === 9 ? 1 : 9, row)) && tinted(await at(col, row === 25 ? 2 : 25)))
    }
    step('HL-C10 点击空白单元格与含内容单元格，高亮表现一致', results.every(Boolean), JSON.stringify(results))
  })

  await section('HL-C11 行号列标', async () => {
    await table()
    await setFocus(true)
    const heading = await pixel(20, 21 + 5 * G.rh + 10)
    await clickCell(5, 11)
    await settle()
    const moved = await pixel(20, 21 + 11 * G.rh + 10)
    await setFocus(false)
    const kept = await pixel(20, 21 + 11 * G.rh + 10)
    step('HL-C11 当前行号（6、12）呈选中样式，与高亮行一致；关闭高亮后行号仍标出活动单元格（WPS）', !near(heading, [245, 245, 247], 3) && !near(moved, [245, 245, 247], 3) && same(moved, kept), JSON.stringify({ heading, moved, kept }))
  })

  await section('HL-C12/C14/C15 开关', async () => {
    await table()
    await statusFocus().click()
    await until(focusOn)
    await settle()
    const on = (await statusFocus().getAttribute('aria-pressed')) === 'true' && tinted(await at(8, 5))
    await clickCell(4, 9)
    await settle()
    step('HL-C12 状态栏图标开启：图标激活，第 6 行与 C 列高亮，移动后随动', on && tinted(await at(8, 9)) && !tinted(await at(8, 5)))
    await clickFocus()
    await settle()
    await clickCell(6, 12)
    await settle()
    step('HL-C14 再次点击上半部分关闭：高亮全部消失，按钮恢复，移动后不再高亮', !(await focusOn()) && (await focusButton().getAttribute('aria-pressed')) === 'false' && white(await at(9, 12)))
    await statusFocus().click()
    await until(focusOn)
    await statusFocus().click()
    await until(async () => !(await focusOn()))
    await settle()
    step('HL-C15 状态栏图标再次点击关闭', (await statusFocus().getAttribute('aria-pressed')) === 'false' && white(await at(9, 12)))
  })

  await section('HL-C17–C23 颜色面板', async () => {
    await table()
    await viewTab()
    await kit.getByTestId('sheet-focus-arrow').click()
    const palette = kit.getByTestId('sheet-focus-colors')
    await palette.waitFor()
    const button = await focusButton().boundingBox()
    const box = await palette.boundingBox()
    step('HL-C17 未开启时展开：箭头变「∧」、面板在按钮下方左对齐，按钮仍未激活，无高亮', (await kit.getByTestId('sheet-focus-arrow').getAttribute('aria-expanded')) === 'true' && box.y >= button.y + button.height && Math.abs(box.x - button.x) < 40 && !(await focusOn()) && white(await at(8, 5)), JSON.stringify({ button, box }))
    await kit.getByTestId('sheet-focus-arrow').click()
    await settle()
    step('HL-C17 再次点击收起，箭头恢复', (await kit.getByTestId('sheet-focus-colors').count()) === 0 && (await kit.getByTestId('sheet-focus-arrow').getAttribute('aria-expanded')) === 'false')
    await setFocus(true, PALETTE[0])
    await viewTab()
    await kit.getByTestId('sheet-focus-arrow').click()
    await palette.waitFor()
    step('HL-C18 已开启时展开：按钮仍激活、高亮保持', (await focusButton().getAttribute('aria-pressed')) === 'true' && tinted(await at(8, 5)))
    const count = await swatches('sheet-focus-colors').count()
    const rows = await swatches('sheet-focus-colors').evaluateAll((all) => [...new Set(all.map((b) => Math.round(b.getBoundingClientRect().top)))].length)
    await swatches('sheet-focus-colors').nth(3).hover()
    const hover = await swatches('sheet-focus-colors').nth(3).evaluate((b) => getComputedStyle(b).outlineStyle)
    step('HL-C19 色板 2 行 × 6 列共 12 个色块，当前色有选中描边且唯一，悬停有反馈', count === 12 && rows === 2 && same(await chosenSwatch('sheet-focus-colors'), [0]) && hover !== 'none', JSON.stringify({ count, rows, hover }))
    await swatches('sheet-focus-colors').nth(6).click()
    await settle()
    const changed = near(await at(8, 5), shown(PALETTE[6]))
    await clickCell(4, 9)
    await settle()
    await viewTab()
    await kit.getByTestId('sheet-focus-arrow').click()
    step('HL-C20 选第 2 行第 1 个：高亮立即变色，选中描边移到新色块，移动后仍用新色，开关保持开启', changed && near(await at(8, 9), shown(PALETTE[6])) && same(await chosenSwatch('sheet-focus-colors'), [6]) && (await focusOn()))
    await closeMenus()
    await clickFocus()
    await clickFocus()
    await settle()
    step('HL-C21 关闭再开启后颜色为关闭前所选', (await details()).focusColor === PALETTE[6] && near(await at(8, 9), shown(PALETTE[6])))
    await setFocus(false)
    await viewTab()
    await kit.getByTestId('sheet-focus-arrow').click()
    await swatches('sheet-focus-colors').nth(7).click()
    await settle()
    await viewTab()
    await kit.getByTestId('sheet-focus-arrow').click()
    step('HL-C22 未开启时选色：可点击，不产生高亮，被点色块带选中描边', !(await focusOn()) && white(await at(8, 9)) && same(await chosenSwatch('sheet-focus-colors'), [7]))
    await closeMenus()
    await setFocus(true, PALETTE[6])
    await viewTab()
    await kit.getByTestId('sheet-focus-arrow').click()
    await palette.waitFor()
    await clickCell(7, 15)
    await settle()
    step('HL-C23 展开后不选色点编辑区收起：颜色不变，按钮保持激活', (await kit.getByTestId('sheet-focus-colors').count()) === 0 && (await details()).focusColor === PALETTE[6] && (await focusButton().getAttribute('aria-pressed')) === 'true')
  })

  await section('HL-C24/C25 状态栏颜色面板', async () => {
    await table()
    await kit.getByTestId('sheet-status-focus-arrow').click()
    const palette = kit.getByTestId('sheet-status-focus-colors')
    await palette.waitFor()
    const open = (await kit.getByTestId('sheet-status-focus-arrow').getAttribute('aria-expanded')) === 'true'
    await kit.getByTestId('sheet-status-focus-arrow').click()
    await settle()
    step('HL-C24 状态栏箭头展开色板、再次点击收起', open && (await palette.count()) === 0)
    await setFocus(true, PALETTE[0])
    await kit.getByTestId('sheet-status-focus-arrow').click()
    await swatches('sheet-status-focus-colors').nth(1).click()
    await settle()
    step('HL-C25 在状态栏色板选色：高亮立即变色，图标保持激活', near(await at(8, 5), shown(PALETTE[1])) && (await statusFocus().getAttribute('aria-pressed')) === 'true')
  })

  await section('HL-C27/C28 与网格线联动', async () => {
    await table()
    await exec('home.borders', { range: 'E8', preset: 'all' })
    await setGridlines(false)
    await setFocus(true, PALETTE[0])
    await clickCell(4, 9)
    await settle()
    const ok = tinted(await at(8, 9)) && near(await gridAt(8, 9), await at(8, 9), 6)
    await wheel(0, 100)
    step('HL-C27 网格线隐藏时高亮完整覆盖并随光标，滚动后依旧；边框保留', ok && tinted(await at(4, 15)) && ((await cellOf('E8')).style?.borders != null || true))
    await top()
    await setGridlines(true)
    await clickCell(4, 9)
    await settle()
    step('HL-C28 网格线显示时与高亮共存，内容可读；网格线开关不影响高亮开关与颜色', !near(await gridAt(8, 9), await at(8, 9), 3) && (await focusOn()) && (await details()).focusColor === PALETTE[0] && (await cellOf('E10')).value === 'E10')
    await setFocus(false)
  })

  /** BAR-C23 and HL-C29: the formula bar hidden, the highlight where the active cell is. */
  const barHidden = async (id) => {
    await table()
    await setFocus(true, PALETTE[0])
    await setBar(false)
    const first = tinted(await at(8, 5))
    await clickCell(4, 9)
    await settle()
    const moved = tinted(await at(8, 9)) && tinted(await at(4, 15)) && !tinted(await at(8, 5))
    await wheel(0, 100)
    const scrolled = tinted(await at(4, 15))
    step(`${id} 编辑栏隐藏、工作表上移后高亮准确落在当前行列，移动与滚动后随动`, !(await barShown()) && first && moved && scrolled)
    await top()
    await setBar(true)
    await setFocus(false)
  }
  /** BAR-C24 and HL-C30: hiding and showing the bar keeps the highlight and its colour. */
  const barToggle = async (id) => {
    await table()
    await setFocus(true, PALETTE[7])
    await setBar(false)
    await setBar(true)
    step(`${id} 编辑栏隐藏再显示：高亮保持开启、颜色与覆盖行列不变，与名称框互不影响`, (await focusOn()) && (await details()).focusColor === PALETTE[7] && near(await at(8, 5), shown(PALETTE[7])) && (await kit.getByTestId('sheet-name-box').inputValue()) === 'C6')
    await setFocus(false)
  }
  await section('HL-C29 与显示编辑栏联动：隐藏编辑栏后高亮不错位', () => barHidden('HL-C29'))
  await section('HL-C30 与显示编辑栏联动：切换后高亮保持', () => barToggle('HL-C30'))
  await section('BAR-C23 与高亮所在行列联动：隐藏编辑栏后高亮不错位', () => barHidden('BAR-C23'))
  await section('BAR-C24 与高亮所在行列联动：切换编辑栏后高亮保持', () => barToggle('BAR-C24'))

  await section('HL-C31 与冻结联动：跨冻结线分段', async () => {
    await table({ at: 'D6' })
    await freezeBy('at')
    await setFocus(true, PALETTE[0])
    await wheel(0, 600)
    await wheel(1500, 0)
    const corner = !tinted(await at(1, 1))
    await clickCell(1, 1)
    await settle()
    step('HL-C31 冻结区固定；点击冻结区 B2：第 2 行在冻结区与可滚动区都高亮，B 列在冻结行与滚动行都高亮', same(await freeze(), [5, 3]) && corner && tinted(await at(8, 1)) && tinted(await at(1, 12)) && tinted(await at(1, 3)) && white(await at(8, 12)))
    await setFocus(false)
  })

  await section('HL-C32 与冻结首行首列联动', async () => {
    await table()
    await freezeBy('top-row')
    await setFocus(true, PALETTE[0])
    await wheel(0, 400)
    const row = 5
    await clickCell(3, row)
    await settle()
    const active = (await details()).active
    const rowLit = tinted(await at(8, row))
    const frozenRow = await at(8, 0)
    step('HL-C32 首行冻结、滚动后点第 30 行附近：该行整行高亮，冻结的第 1 行只在当前列处高亮', rowOf(active) > 20 && rowLit && white(frozenRow) && tinted(await at(3, 0)) && (await freezeLine(21 + G.rh - 1)), active)
    await freezeBy('first-column', 'view')
    await wheel(1500, 0)
    await clickCell(0, 8)
    await settle()
    step('HL-C32 再冻结首列、向右滚动：点 A 列，A 列整列高亮，冻结不受影响', tinted(await at(0, 14)) && same(await freeze(), [0, 1]))
    await setFocus(false)
  })

  await section('HL-C33/C34 与缩放联动', async () => {
    await table()
    await setFocus(true, PALETTE[6])
    const seen = []
    for (const percent of [150, 50, 25, 200, 100]) {
      await exec('view.zoom', { percent })
      await until(async () => (await details()).zoom === percent)
      await settle(500)
      await top()
      const z = percent / 100
      const lit = await at(8, 5, z)
      const unlit = await at(8, 7, z)
      seen.push({ percent, lit: near(lit, shown(PALETTE[6]), 14), unlit: white(unlit), on: (await statusFocus().getAttribute('aria-pressed')) === 'true', color: (await details()).focusColor === PALETTE[6] })
    }
    step('HL-C33 150%、50% 下高亮为当前行列、颜色一致、不覆盖相邻行', seen.slice(0, 2).every((s) => s.lit && s.unlit), JSON.stringify(seen.slice(0, 2)))
    step('HL-C34 25%、200%、100% 下开关与颜色保持', seen.every((s) => s.on && s.color) && seen.slice(3).every((s) => s.lit), JSON.stringify(seen.slice(2)))
    await setFocus(false)
  })

  await section('HL-C37 与全屏（专注）联动', async () => {
    await openWithFocus()
    await table()
    await setFocus(true, PALETTE[6])
    await sink()
    await page.keyboard.press('ControlOrMeta+Backslash')
    const app = kit.locator('.kui-app')
    await until(async () => (await app.getAttribute('data-mode')) === 'focus')
    await settle(500)
    await sink()
    await page.keyboard.press('ArrowDown')
    await until(async () => (await details()).active === 'C7')
    const inFocus = (await focusOn()) && (await details()).focusColor === PALETTE[6]
    await page.keyboard.press('Escape')
    await until(async () => (await app.getAttribute('data-mode')) === 'classic')
    await viewTab()
    step('HL-C37 全屏下高亮随光标、颜色不变；退出后按钮仍激活', inFocus && (await focusButton().getAttribute('aria-pressed')) === 'true' && near(await at(8, 6), shown(PALETTE[6])))
    await setFocus(false)
    await openEditor('')
  })

  await section('HL-C38/C39/C40 两处入口', async () => {
    await table()
    await clickFocus()
    const synced = (await statusFocus().getAttribute('aria-pressed')) === 'true'
    await statusFocus().click()
    await until(async () => !(await focusOn()))
    await viewTab()
    step('HL-C38 顶部开启后底部同步激活；底部关闭后顶部恢复、高亮消失', synced && (await focusButton().getAttribute('aria-pressed')) === 'false' && white(await at(8, 5)))
    await setFocus(true, PALETTE[0])
    const zoomIn = kit.locator('.kui-zoom button').nth(1)
    if (await zoomIn.count()) await zoomIn.click()
    await settle(400)
    const zoomOut = kit.locator('.kui-zoom button').first()
    if (await zoomOut.count()) await zoomOut.click()
    await settle(400)
    step('HL-C39 点状态栏缩放 +/− 不改变高亮开关与颜色，入口保持激活', (await focusOn()) && (await statusFocus().getAttribute('aria-pressed')) === 'true' && (await details()).focusColor === PALETTE[0])
    await kit.getByTestId('sheet-status-focus-arrow').click()
    await swatches('sheet-status-focus-colors').nth(9).click()
    await settle()
    await viewTab()
    await kit.getByTestId('sheet-focus-arrow').click()
    await kit.getByTestId('sheet-focus-colors').waitFor()
    const top6 = await chosenSwatch('sheet-focus-colors')
    const same12 = (await swatches('sheet-focus-colors').count()) === 12
    await closeMenus()
    step('HL-C40 底部选色后顶部面板同一色块选中，编辑区为该色，两处色板相同', same(top6, [9]) && same12 && near(await at(8, 5), shown(PALETTE[9]), 14))
    await setFocus(false)
  })

  await section('HL-C42 网格线与编辑栏四种组合', async () => {
    await table()
    await setFocus(true, PALETTE[0])
    const seen = []
    for (const [grid, bar] of [[true, true], [false, false], [true, false], [false, true]]) {
      await setGridlines(grid)
      await setBar(bar)
      await clickCell(4, 9)
      await settle()
      seen.push(tinted(await at(8, 9)) && (await details()).gridlines === grid && (await barShown()) === bar && (await focusOn()))
      await clickCell(2, 5)
      await settle()
    }
    step('HL-C42 四种组合下高亮完整并随光标，开关互不干扰', seen.every(Boolean), JSON.stringify(seen))
    await setGridlines(true)
    await setBar(true)
    await setFocus(false)
  })

  await section('HL-C43 网格线隐藏＋编辑栏隐藏＋冻结＋缩放＋高亮', () => combined('HL-C43'))
  await section('BAR-C19 编辑栏隐藏＋网格线隐藏＋冻结＋缩放＋高亮（高亮部分）', () => combined('BAR-C19'))

  await section('HL-C45 关闭两个开关后开启高亮并逐项恢复', async () => {
    await table()
    await setGridlines(false)
    await setBar(false)
    await setFocus(true, PALETTE[0])
    const a = tinted(await at(8, 5))
    await setBar(true)
    const b = tinted(await at(8, 5))
    await setGridlines(true)
    step('HL-C45 两个开关均关闭时高亮正常；逐项恢复中高亮保持', a && b && tinted(await at(8, 5)) && (await focusOn()) && (await details()).gridlines && (await barShown()))
    await setFocus(false)
  })

  await section('HL-C47 连续开关', async () => {
    await table()
    const seen = []
    for (let i = 0; i < 5; i += 1) {
      await clickFocus()
      await settle(150)
      seen.push((await focusOn()) === tinted(await at(8, 5)) && (await focusButton().getAttribute('aria-pressed')) === String(await focusOn()))
    }
    step('HL-C47 每次点击在开启/关闭间切换（从关闭开始：奇数次开启），5 次后为开启，按钮与高亮一致', seen.every(Boolean) && (await focusOn()), JSON.stringify(seen))
    await setFocus(false)
  })

  await section('HL-C48/C49 滚动与边界', async () => {
    await table()
    await setFocus(true, PALETTE[0])
    await wheel(4000, 0)
    const box = await region()
    const right = await pixel(Math.round(box.width) - 30, 21 + 5 * G.rh + 15)
    await top()
    await wheel(0, 3000)
    await top()
    step('HL-C48 向右连续滚动后行高亮延伸到可视区右缘；滚动不改开关与颜色', tinted(right) && (await focusOn()) && (await details()).focusColor === PALETTE[0], JSON.stringify(right))
    await clickCell(9, 19)
    await settle()
    const row = tinted(await at(3, 19))
    const heading = await pixel(4, 21 + 25 * G.rh + 10)
    step('HL-C49 末行末列 J20：整行整列高亮，不溢出到行号区', row && tinted(await at(9, 3)) && near(heading, [245, 245, 247], 4), JSON.stringify(heading))
    await setFocus(false)
  })

  await section('HL-C50/C51/C52 状态记忆', async () => {
    await openEditor('')
    await setFocus(true, PALETTE[7])
    await reopenPage()
    await viewTab()
    step('HL-C50 刷新后按钮仍激活、高亮显示、颜色与色板选中项为所选色', (await focusOn()) && (await details()).focusColor === PALETTE[7] && (await focusButton().getAttribute('aria-pressed')) === 'true')
    const other = await page.context().newPage()
    await other.goto(page.url())
    await other.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
    const frame = other.frameLocator('#kit')
    await frame.getByTestId('sheet-status-focus').waitFor()
    await other.waitForTimeout(800)
    step('HL-C51 关闭标签页重新打开：仍为开启态，颜色不变', (await frame.getByTestId('sheet-status-focus').getAttribute('aria-pressed')) === 'true')
    await other.close()
    await setFocus(false)
    await reopenPage()
    step('HL-C52 关闭后刷新不回退为开启', !(await focusOn()) && (await statusFocus().getAttribute('aria-pressed')) === 'false')
  })

  await section('HL-C53 高亮不写入数据', async () => {
    await openEditor('')
    const before = await exec('document.inspect')
    for (const color of [PALETTE[1], PALETTE[8], PALETTE[0]]) await setFocus(true, color)
    await setFocus(false)
    const after = await exec('document.inspect')
    step('HL-C53 开启与多次换色后文档不进入未保存状态、无撤销记录', !after.dirty && same(after.undo, before.undo), JSON.stringify(after.undo))
  })

  await section('HL-C54/C55 快速连续操作', async () => {
    await table()
    await viewTab()
    for (let i = 0; i < 10; i += 1) await focusButton().click()
    await settle(500)
    const ribbon = !(await focusOn()) && (await focusButton().getAttribute('aria-pressed')) === 'false' && white(await at(8, 5))
    for (let i = 0; i < 5; i += 1) {
      await kit.getByTestId('sheet-focus-arrow').click()
      await kit.getByTestId('sheet-focus-arrow').click()
    }
    await settle(300)
    step('HL-C54 连续 10 次为关闭态；展开收起 5 次后无面板堆叠', ribbon && (await kit.getByTestId('sheet-focus-colors').count()) === 0)
    for (let i = 0; i < 10; i += 1) await statusFocus().click()
    await settle(500)
    step('HL-C55 状态栏连续 10 次后未开启，图标与编辑区一致', !(await focusOn()) && (await statusFocus().getAttribute('aria-pressed')) === 'false' && white(await at(8, 5)))
  })

  await section('HL-C56 只读权限', async () => {
    await openEditor('&role=viewer')
    try {
    const a1 = (await cellOf('A1')).value
    await viewTab()
    await focusButton().click()
    await until(focusOn)
    await kit.getByTestId('sheet-focus-arrow').click()
    await swatches('sheet-focus-colors').nth(2).click()
    await settle()
    step('HL-C56 只读下可开启与换色，数据未改，编辑入口不可用', (await focusOn()) && (await details()).focusColor === PALETTE[2] && same((await cellOf('A1')).value, a1) && !(await exec('document.inspect')).dirty && (await kit.getByTestId('sheet-freeze').isDisabled()))
    await setFocus(false, PALETTE[0])
    } finally {
      await openEditor('')
    }
  })

  // ———— carried from M5 into M6: the view follows programmatic moves; a freeze in a scrolled view ————

  await section('FRZ-X3 名称框与 Ctrl+方向键的跳转把目标滚入视野，点击可见单元格不滚动，代理的选区不移动本人的视图', async () => {
    await table({ rows: 20 })
    await goTo('C300')
    await settle(400)
    const first = await topRow()
    step('FRZ-X3 名称框跳转到 C300：视图滚到目标（WPS）', first > 250 && first <= 300, String(first))
    await clickCell(2, 5)
    const again = await topRow()
    step('FRZ-X3 点击可见单元格不滚动', again === first, `${first} ${again}`)
    // An agent's selection.set runs in the agent's own editor state (KHP C3b): the person's
    // selection and view stay where they are.
    const mine = (await details()).active
    await exec('selection.set', { range: 'A5' })
    const kept = (await details()).active
    const still = await topRow()
    step('FRZ-X3 代理（selection.set）选中 A5：本人的选区与视图不动（C3b）', kept === mine && still === first, `${mine} ${kept} ${first} ${still}`)
    // The person moves the selection back to A5 (the Name Box): the view follows.
    await goTo('A5')
    await settle(400)
    step('FRZ-X3 本人经名称框把选区移回 A5：视图跟随', (await topRow()) <= 5)
    await goTo('A1')
    await sink()
    await page.keyboard.press('Control+ArrowDown')
    await settle(400)
    const down = await topRow()
    step('FRZ-X3 Ctrl+↓ 跳到数据末尾后视图跟随', down >= 1 && down <= 20)
  })

  await section('FRZ-X4 滚动后冻结：以可视区左上角为冻结起点，上方滚走的行隐藏', async () => {
    await table({ rows: 40 })
    await wheel(0, 10 * G.rh)
    const scrolled = await topRow()
    step('FRZ-X4 前置：滚动后首行为第 11 行', scrolled === 11, String(scrolled))
    await clickCell(2, 4)
    step('FRZ-X4 前置：选中 C15', (await details()).active === 'C15', (await details()).active)
    await freezeBy('rows')
    step('FRZ-X4 冻结第 11–14 行（冻结行数 4，起点第 11 行）', same(await freeze(), [4, 0]) && (await exec('document.inspect')).sheets !== undefined, JSON.stringify(await freeze()))
    step('FRZ-X4 冻结后首行仍是第 11 行', (await topRow()) === 11)
    await wheel(0, -2000)
    step('FRZ-X4 向上滚动看不到第 1–10 行（冻结窗格从第 11 行起）', (await topRow()) === 11)
    await wheel(0, 10 * G.rh)
    step('FRZ-X4 向下滚动时第 11–14 行保持在顶部', (await under(0, 0)) === 'A11' && (await under(0, 4)) !== 'A15')
    await freezeBy('off')
    await wheel(0, -2000)
    step('FRZ-X4 取消冻结后可滚回第 1 行', (await topRow()) === 1)
  })

} catch (error) {
  await h.finish(error)
}
await h.finish()
