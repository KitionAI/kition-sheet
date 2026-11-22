// The formula bar (module M3), as WPS for Mac has it, in a real browser through the
// development host: node tests/gate-formula-bar.mjs [chromium|webkit]
// View › Show › Formula Bar (`显示编辑栏`; hide and show, kept for the next time, a viewer's too), the Name Box
// and the field following the selection, typing in the field (the cell shows it), ✓ ✕ fx, long
// text, with the gridlines, frozen panes, zoom and Focus (full screen); and typing there as in the
// cell: pointing at cells (another sheet's too), Enter/Tab/Escape, Alt+Enter, expanding the bar.
// Each step names its case (BAR-C4 …; BAR-X… are the formula entry steps the case sheet leaves
// out); `GATE_ONLY=C12` runs one.
import { startGate } from './gate-harness.mjs'

const h = await startGate({ prefix: 'BAR-' })
/** The grid's pitch as the engine lays it out (the harness reads it when a workbook opens). */
const G = h.grid
const { page, kit, step, section, until, same, ai, exec, details, q, cellOf, nameBox, formulaBar, region, clickCell, goTo, choose, build, numbered, pixel, openEditor } = h

const COLS = 'ABCDEFGHIJ'
const bar = () => kit.getByTestId('sheet-formula')
const barShown = async () => (await kit.getByTestId('sheet-formula-bar').count()) === 1
const showBox = () => kit.getByTestId('sheet-show-formula-bar')
const gridBox = () => kit.getByTestId('sheet-show-gridlines')
const viewTab = async () => {
  await kit.getByTestId('ribbon-tab-view').click()
  await showBox().waitFor()
}
/** Clicks Formula Bar (`显示编辑栏`) and waits until the bar follows it. */
const toggleBar = async () => {
  const was = await showBox().isChecked()
  await showBox().click()
  return until(async () => (await showBox().isChecked()) === !was && (await barShown()) === !was)
}
const setBar = async (on) => {
  await viewTab()
  if ((await showBox().isChecked()) !== on) await toggleBar()
}
const setGridlines = async (on) => {
  await viewTab()
  if ((await gridBox().isChecked()) !== on) {
    await gridBox().click()
    await until(async () => (await gridBox().isChecked()) === on && (await details()).gridlines === on)
  }
}
const input = async (ref) => {
  const cell = await cellOf(ref)
  return cell.formula ?? cell.input ?? ''
}
const focusedTestId = () => kit.locator(':focus').evaluate((element) => element.getAttribute('data-testid')).catch(() => null)
const editText = async () => (await details()).editText ?? null
/** The page point at the centre of a cell (column, row from 0) at 100% from the top-left. */
const centre = async (col, row) => {
  const box = await region()
  return { x: box.x + 46 + col * G.cw + 32, y: box.y + 21 + row * G.rh + 10 }
}
/** The case sheets' table: 20 rows × 10 columns, each cell holding its own address, C6 selected. */
const table = async () => {
  await build(numbered(2), { fill: false })
  const values = Array.from({ length: 20 }, (_, row) => Array.from({ length: 10 }, (_, col) => `${COLS[col]}${row + 1}`))
  await ai([{ command: 'range.setValues', params: { range: `${q('工作表1')}!A1`, values } }])
  await setBar(true)
  await setGridlines(true)
  await exec('view.zoom', { percent: 100 })
  // Back to the top-left (a sheet of the same name keeps where it was scrolled).
  const box = await region()
  await page.mouse.move(box.x + 300, box.y + 200)
  await page.mouse.wheel(-20000, -20000)
  await page.waitForTimeout(300)
  await goTo('C6')
}
/** Clicks in the field and puts the caret at the end. */
const intoBar = async () => {
  await bar().click()
  // (End only scrolls in WebKit on a Mac.)
  await bar().evaluate((field) => field.setSelectionRange(field.value.length, field.value.length))
}
/** Opens the editor with Focus (full screen) available. */
const openWithFocus = async () => {
  const url = new URL(page.url())
  url.searchParams.set('modes', 'classic,focus')
  await page.goto(url.toString())
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await kit.getByTestId('sheet-strip').waitFor()
  await page.waitForTimeout(800)
}
const reopen = async () => {
  await page.reload()
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await kit.getByTestId('sheet-strip').waitFor()
  await page.waitForTimeout(800)
}

try {
  await openEditor('')
  await table()
  const initial = await kit.getByTestId('sheet-formula-bar').boundingBox()

  // ———————————————————————————— Entry points ————————————————————————————
  await section('BAR-C4 入口定位与默认勾选态', async () => {
    await table()
    await viewTab()
    const options = await kit.locator('.sheet-show-option').allTextContents()
    step('BAR-C4 「显示」分组自上而下为「显示网格线」「显示编辑栏」', options[0] === '显示网格线' && options[1] === '显示编辑栏', JSON.stringify(options))
    step('BAR-C4 「显示编辑栏」为复选框，默认已勾选', (await showBox().getAttribute('type')) === 'checkbox' && (await showBox().isChecked()))
    const order = await kit.getByTestId('sheet-formula-bar').evaluate((element) => [...element.querySelectorAll('[data-testid]')].map((node) => node.getAttribute('data-testid')))
    const want = ['sheet-name-box', 'sheet-formula-cancel', 'sheet-formula-enter', 'sheet-formula-fx', 'sheet-formula']
    step('BAR-C4 编辑栏自左向右：名称框、✕、✓、ƒx、内容编辑区；名称框显示 C6', same(order.filter((id) => want.includes(id)), want) && (await nameBox()) === 'C6', JSON.stringify(order))
  })

  await section('BAR-C5 复选开关的控件形态与勾选态视觉', async () => {
    await viewTab()
    const label = kit.locator('.sheet-show-option .kui-checkbox').nth(1)
    await page.mouse.move(5, 5)
    await page.waitForTimeout(150)
    const idle = await label.evaluate((element) => getComputedStyle(element).backgroundColor)
    await label.hover()
    await page.waitForTimeout(150)
    const hover = await label.evaluate((element) => ({ background: getComputedStyle(element).backgroundColor, cursor: getComputedStyle(element).cursor }))
    step('BAR-C5 悬停时有 hover 底色，指针为可点击样式', hover.background !== idle && hover.cursor === 'pointer', JSON.stringify({ idle, hover }))
    const off = await toggleBar()
    const offState = await showBox().isChecked()
    const on = await toggleBar()
    step('BAR-C5 取消勾选为空白框、再勾选显示对勾，即时生效', off && offState === false && on && (await showBox().isChecked()))
  })

  await section('BAR-C6 取消勾选隐藏表格顶部编辑栏', async () => {
    await table()
    const barBox = await kit.getByTestId('sheet-formula-bar').boundingBox()
    const before = await region()
    await viewTab()
    await toggleBar()
    step('BAR-C6 勾选框变为未勾选态', !(await showBox().isChecked()))
    const gone = await Promise.all(['sheet-formula-bar', 'sheet-name-box', 'sheet-formula-cancel', 'sheet-formula-enter', 'sheet-formula-fx', 'sheet-formula'].map((id) => kit.getByTestId(id).count()))
    step('BAR-C6 名称框、✕、✓、ƒx 与内容编辑区均不再显示', gone.every((count) => count === 0), JSON.stringify(gone))
    await until(async () => (await region()).y < before.y - 10)
    const after = await region()
    step('BAR-C6 编辑栏占位回收，列标行直接位于工具栏下方', Math.abs(after.y - barBox.y) <= 2 && after.height > before.height, `${barBox.y} ${before.y} → ${after.y}`)
    const shown = await details()
    step('BAR-C6 行号、列标、网格线与单元格内容正常', shown.headings && shown.gridlines && (await cellOf('C6')).value === 'C6')
  })

  await section('BAR-C7 重新勾选恢复编辑栏显示', async () => {
    await setBar(false)
    await viewTab()
    await toggleBar()
    const box = await kit.getByTestId('sheet-formula-bar').boundingBox()
    step('BAR-C7 勾选框恢复已勾选，编辑栏位置与高度与初始一致', (await showBox().isChecked()) && Math.abs(box.y - initial.y) <= 1 && Math.abs(box.height - initial.height) <= 1, JSON.stringify(box))
    const controls = await Promise.all(['sheet-name-box', 'sheet-formula-cancel', 'sheet-formula-enter', 'sheet-formula-fx', 'sheet-formula'].map((id) => kit.getByTestId(id).count()))
    step('BAR-C7 名称框、✕、✓、ƒx 与内容编辑区正常显示，选区与内容不变', controls.every((count) => count === 1) && (await nameBox()) === 'C6' && (await formulaBar()) === 'C6' && (await details()).selection === 'C6')
  })

  await section('BAR-C8 连续切换的奇偶一致性与幂等', async () => {
    await setBar(true)
    const seen = []
    for (let i = 0; i < 5; i += 1) {
      await toggleBar()
      seen.push([await showBox().isChecked(), await barShown()])
    }
    step('BAR-C8 每次切换后编辑栏显示与勾选框一致', seen.every(([checked, shown]) => checked === shown), JSON.stringify(seen))
    step('BAR-C8 连续 5 次后为隐藏态，无残影或重复', !(await showBox().isChecked()) && (await kit.locator('[data-testid="sheet-formula-bar"]').count()) === 0)
  })

  await section('BAR-C9 隐藏编辑栏后工作表上移不错位', async () => {
    await table()
    await setBar(false)
    await clickCell(4, 9)
    await until(async () => (await details()).active === 'E10')
    step('BAR-C9 点击 E10 选中框落在第 10 行 E 列', (await details()).active === 'E10', (await details()).active)
    const box = await region()
    await page.mouse.move(box.x + 300, box.y + 200)
    await page.mouse.wheel(0, 400)
    await page.waitForTimeout(400)
    await clickCell(0, 0)
    const top = Number((await details()).active.replace(/^[A-Z]+/, ''))
    await clickCell(2, 3)
    const at = (await details()).active
    step('BAR-C9 滚动后行号、列标与单元格对应正确', top > 1 && at === `C${top + 3}`, `${top} ${at}`)
  })

  await section('BAR-C10 显示隐藏编辑栏不影响网格线', async () => {
    await table()
    await page.waitForTimeout(300)
    const line = [46 + 3 * G.cw, 21 + 12 * G.rh + 6]
    const before = await pixel(...line)
    await setBar(false)
    await page.waitForTimeout(400)
    const hidden = await pixel(...line)
    step('BAR-C10 取消勾选后网格线保持显示，「显示网格线」仍勾选', same(hidden, before) && hidden[0] < 250 && (await gridBox().isChecked()) && (await details()).gridlines, `${before} ${hidden}`)
    await setBar(true)
    await page.waitForTimeout(400)
    step('BAR-C10 行号列标正常；重新勾选后网格线颜色与粗细不变', same(await pixel(...line), before) && (await details()).headings)
  })

  await section('BAR-C11 名称框与内容编辑区随选区同步', async () => {
    await table()
    await clickCell(2, 5)
    step('BAR-C11 点击 C6：名称框 C6，内容编辑区 C6', (await nameBox()) === 'C6' && (await formulaBar()) === 'C6')
    for (let i = 0; i < 6; i += 1) await page.keyboard.press('ArrowDown')
    for (let i = 0; i < 3; i += 1) await page.keyboard.press('ArrowRight')
    await until(async () => (await nameBox()) === 'F12')
    step('BAR-C11 方向键移到 F12：名称框 F12，内容编辑区与单元格一致', (await nameBox()) === 'F12' && (await formulaBar()) === 'F12', `${await nameBox()} ${await formulaBar()}`)
    await clickCell(11, 22)
    await until(async () => (await nameBox()) === 'L23')
    step('BAR-C11 选中空白单元格时内容编辑区为空', (await nameBox()) === 'L23' && (await formulaBar()) === '', await formulaBar())
  })

  // ———————————————————————————— Editing ————————————————————————————
  await section('BAR-C12 在内容编辑区编辑并确认或取消', async () => {
    await table()
    await clickCell(1, 1)
    await intoBar()
    await page.keyboard.type('追加')
    const enabled = !(await kit.getByTestId('sheet-formula-enter').isDisabled()) && !(await kit.getByTestId('sheet-formula-cancel').isDisabled())
    step('BAR-C12 编辑过程中「✓」与「✕」可点击', enabled)
    await kit.getByTestId('sheet-formula-enter').click()
    await until(async () => (await input('B2')) === 'B2追加')
    const after = await details()
    step('BAR-C12 点击「✓」后单元格更新为编辑后的文本，选中保持在 B2', (await input('B2')) === 'B2追加' && after.active === 'B2' && !after.editing, `${await input('B2')} ${after.active}`)
    await intoBar()
    await page.keyboard.type('XYZ')
    await kit.getByTestId('sheet-formula-cancel').click()
    await until(async () => !(await details()).editing)
    step('BAR-C12 点击「✕」后单元格内容回退，未修改', (await input('B2')) === 'B2追加' && (await formulaBar()) === 'B2追加' && (await details()).active === 'B2')
  })

  await section('BAR-C13 内容编辑区与单元格文本实时同步', async () => {
    await table()
    await clickCell(1, 1)
    await intoBar()
    await page.keyboard.type('同步')
    await until(async () => (await editText()) === 'B2同步')
    const typing = await details()
    step('BAR-C13 编辑过程中单元格内实时显示正在输入的内容', typing.editing && typing.editText === (await formulaBar()), `${typing.editText} / ${await formulaBar()}`)
    await kit.getByTestId('sheet-formula-enter').click()
    await until(async () => (await input('B2')) === 'B2同步')
    step('BAR-C13 确认后两处内容完全一致', (await input('B2')) === 'B2同步' && (await formulaBar()) === 'B2同步')
  })

  await section('BAR-C14 内容编辑区输入公式与插入函数入口', async () => {
    await table()
    await clickCell(11, 2)
    await bar().click()
    await page.keyboard.type('=SUM(')
    await until(async () => (await editText()) === '=SUM(')
    step('BAR-C14 输入「=SUM(」时进入公式编辑态，单元格同步显示', (await editText()) === '=SUM(' && (await formulaBar()) === '=SUM(')
    const fx = kit.getByTestId('sheet-formula-fx')
    step('BAR-C14 「ƒx」按钮可悬停（提示「插入函数」）', (await fx.getAttribute('title')) === '插入函数')
    await fx.click()
    const dialog = kit.getByTestId('sheet-insert-function')
    await dialog.waitFor()
    step('BAR-C14 点击「ƒx」唤起插入函数，内容编辑区内容不被破坏', (await dialog.isVisible()) && (await formulaBar()) === '=SUM(' && (await editText()) === '=SUM(')
    const about = await kit.getByTestId('sheet-insert-function-about').textContent()
    step('BAR-C14 插入函数列出常用函数，说明为中文', about.includes('SUM(') && about.includes('返回所有参数之和'), about)
    await kit.getByTestId('sheet-insert-function-cancel').click()
    await until(async () => (await dialog.count()) === 0 && (await focusedTestId()) === 'sheet-formula')
    step('BAR-C14 关闭函数入口后内容保持，可继续编辑', (await formulaBar()) === '=SUM(' && (await focusedTestId()) === 'sheet-formula', `${await formulaBar()} ${await focusedTestId()}`)
    await page.keyboard.type('1,2)')
    await page.keyboard.press('Enter')
    await until(async () => (await cellOf('L3')).value === 3)
    step('BAR-C14 继续输入 1,2) 回车：L3 = 3', (await cellOf('L3')).value === 3 && (await input('L3')) === '=SUM(1,2)')
    // A function chosen there goes in at the caret.
    await clickCell(11, 3)
    await bar().click()
    await page.keyboard.type('=1+')
    await fx.click()
    await dialog.waitFor()
    await kit.getByTestId('sheet-insert-function-search').fill('max')
    await kit.getByTestId('sheet-insert-function-list').locator('li[data-name="MAX"]').click()
    await kit.getByTestId('sheet-insert-function-ok').click()
    await until(async () => (await formulaBar()) === '=1+MAX(' && (await focusedTestId()) === 'sheet-formula')
    step('BAR-C14 选择 MAX 后插入光标处：=1+MAX(，可继续输入', (await formulaBar()) === '=1+MAX(' && (await focusedTestId()) === 'sheet-formula', await formulaBar())
    await page.keyboard.type('5)')
    await page.keyboard.press('Enter')
    await until(async () => (await cellOf('L4')).value === 6)
    step('BAR-C14 回车后 L4 = 6', (await cellOf('L4')).value === 6)
  })

  // ———————————————————————————— While hidden ————————————————————————————
  await section('BAR-C15 隐藏编辑栏后单元格内直接编辑仍正常', async () => {
    await table()
    await setBar(false)
    const at = await centre(1, 1)
    await page.mouse.dblclick(at.x, at.y)
    await until(async () => (await details()).editing)
    await page.keyboard.type('Z')
    // The typed key reaches the engine's edit text after `type` returns: read once it is there.
    await until(async () => (await editText()) === 'B2Z')
    step('BAR-C15 双击进入编辑态，输入正常显示', (await editText()) === 'B2Z', await editText())
    await page.keyboard.press('Enter')
    await until(async () => (await input('B2')) === 'B2Z')
    step('BAR-C15 回车写入并下移一行，编辑栏保持隐藏', (await input('B2')) === 'B2Z' && (await details()).active === 'B3' && !(await barShown()))
  })

  await section('BAR-C16 隐藏编辑栏后名称框区域不可用', async () => {
    await table()
    const spot = await kit.getByTestId('sheet-name-box').boundingBox()
    await setBar(false)
    step('BAR-C16 名称框不再显示', (await kit.getByTestId('sheet-name-box').count()) === 0)
    await page.mouse.click(spot.x + spot.width / 2, spot.y + spot.height / 2)
    await page.waitForTimeout(200)
    const focus = await kit.locator(':focus').evaluate((element) => element.tagName).catch(() => 'none')
    step('BAR-C16 原名称框位置不响应点击，不出现名称框或定位输入', focus !== 'INPUT' && (await kit.getByTestId('sheet-name-box').count()) === 0, focus)
    const from = await centre(2, 2)
    const to = await centre(4, 7)
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    await page.mouse.move(to.x, to.y, { steps: 6 })
    await page.mouse.up()
    await until(async () => (await details()).selection === 'C3:E8')
    step('BAR-C16 框选 C3:E8 正常，选区正确', (await details()).selection === 'C3:E8', (await details()).selection)
  })

  // ———————————————————————————— Mixed scenarios ————————————————————————————
  await section('BAR-C18 编辑栏与网格线均隐藏下的基础编辑', async () => {
    await table()
    await setBar(false)
    await setGridlines(false)
    await clickCell(4, 9)
    await page.keyboard.type('内容')
    // The typed text reaches the engine a moment after the keys (WebKit on a runner read it early).
    await until(async () => (await editText()) === '内容', 20)
    step('BAR-C18 单元格内直接编辑并显示输入，编辑栏保持隐藏', (await editText()) === '内容' && !(await barShown()))
    await page.keyboard.press('ArrowRight')
    await page.keyboard.press('ArrowDown')
    await until(async () => (await details()).active === 'F11')
    step('BAR-C18 方向键移动后选中框随动到 F11，E10 写入「内容」', (await details()).active === 'F11' && (await input('E10')) === '内容')
    step('BAR-C18 网格线保持隐藏', !(await details()).gridlines && !(await gridBox().isChecked()))
    await setGridlines(true)
  })

  await section('BAR-C19 编辑栏隐藏＋网格线隐藏＋冻结＋缩放', async () => {
    await table()
    await setBar(false)
    await setGridlines(false)
    await exec('view.freezePanes', { cell: 'C6' })
    await exec('view.zoom', { percent: 150 })
    await until(async () => (await details()).zoom === 150)
    const sheet = (await exec('document.inspect')).sheets[0]
    step('BAR-C19 冻结至 C6、缩放 150%，编辑栏与网格线保持隐藏', same(sheet.freeze, [5, 2]) && (await details()).zoom === 150 && !(await barShown()) && !(await details()).gridlines, JSON.stringify(sheet.freeze))
    const box = await region()
    await page.mouse.move(box.x + 400, box.y + 300)
    await page.mouse.wheel(300, 600)
    await page.waitForTimeout(400)
    await kit.locator('.kui-text-sink').evaluate((element) => element.focus())
    for (let i = 0; i < 4; i += 1) await page.keyboard.press('ArrowDown')
    await until(async () => (await details()).active === 'C10')
    step('BAR-C19 滚动后移动光标正常，编辑栏不回显', (await details()).active === 'C10' && !(await barShown()))
    await exec('view.unfreezePanes')
    await exec('view.zoom', { percent: 100 })
    await setGridlines(true)
  })

  await section('BAR-C22 与显示网格线的四种组合', async () => {
    await table()
    const seen = []
    for (const [grid, shown] of [[true, true], [false, false], [true, false], [false, true]]) {
      await setGridlines(grid)
      await setBar(shown)
      await clickCell(3, 4)
      await until(async () => (await details()).active === 'D5')
      const now = await details()
      const nameOk = shown ? (await nameBox()) === 'D5' : true
      seen.push(now.gridlines === grid && (await barShown()) === shown && (await gridBox().isChecked()) === grid && (await showBox().isChecked()) === shown && nameOk && (await cellOf('D5')).value === 'D5')
    }
    step('BAR-C22 四种组合下编辑栏与网格线各自对应勾选状态，选中与数据不变', seen.every(Boolean), JSON.stringify(seen))
    await setGridlines(true)
  })

  await section('BAR-C25 与冻结联动：隐藏编辑栏后冻结区上移不错位', async () => {
    await table()
    await exec('view.freezeTopRow')
    await setBar(false)
    const box = await region()
    await page.mouse.move(box.x + 300, box.y + 200)
    await page.mouse.wheel(0, 400)
    await page.waitForTimeout(400)
    await clickCell(0, 0)
    const frozen = (await details()).active
    await clickCell(0, 1)
    const below = Number((await details()).active.replace(/^[A-Z]+/, ''))
    step('BAR-C25 冻结行固定在顶部，可滚动区已滚过（不错行）', frozen === 'A1' && below > 2, `${frozen} ${below}`)
    await setBar(true)
    await clickCell(0, 0)
    const again = (await details()).active
    step('BAR-C25 重新勾选后冻结区内容与选中正常', again === 'A1' && (await formulaBar()) === 'A1', again)
    await exec('view.unfreezePanes')
  })

  await section('BAR-C26 与缩放联动：各档位下编辑栏显示与占位稳定', async () => {
    await table()
    const boxes = []
    for (const percent of [150, 50]) {
      await exec('view.zoom', { percent })
      await until(async () => (await details()).zoom === percent)
      await page.waitForTimeout(300)
      boxes.push(await kit.getByTestId('sheet-formula-bar').boundingBox())
    }
    step('BAR-C26 150%、50% 下编辑栏位置与高度不变', boxes.every((box) => Math.abs(box.y - initial.y) <= 1 && Math.abs(box.height - initial.height) <= 1), JSON.stringify(boxes))
    const before = await region()
    await setBar(false)
    await until(async () => (await region()).y < before.y - 10)
    await exec('view.zoom', { percent: 150 })
    await until(async () => (await details()).zoom === 150)
    step('BAR-C26 各档位下取消勾选后正常隐藏；调整缩放不改变勾选状态', !(await barShown()) && !(await showBox().isChecked()))
    await exec('view.zoom', { percent: 100 })
    await setBar(true)
  })

  await section('BAR-C29 与全屏显示联动：进出全屏后编辑栏显示保持', async () => {
    await openWithFocus()
    await setBar(false)
    await kit.locator('.kui-text-sink').evaluate((element) => element.focus())
    await page.keyboard.press('ControlOrMeta+Backslash')
    const app = kit.locator('.kui-app')
    await until(async () => (await app.getAttribute('data-mode')) === 'focus')
    step('BAR-C29 全屏（专注）下编辑栏保持隐藏', (await app.getAttribute('data-mode')) === 'focus' && !(await barShown()))
    await page.keyboard.press('Escape')
    await until(async () => (await app.getAttribute('data-mode')) === 'classic')
    await viewTab()
    step('BAR-C29 退出后仍为隐藏态，「显示编辑栏」勾选状态未变', (await app.getAttribute('data-mode')) === 'classic' && !(await barShown()) && !(await showBox().isChecked()))
    await setBar(true)
  })

  // ———————————————————————————— Edge cases ————————————————————————————
  await section('BAR-C31 状态记忆：刷新页面后隐藏态保持', async () => {
    await openEditor('')
    const a1 = (await cellOf('A1')).value
    await setBar(false)
    await reopen()
    await viewTab()
    step('BAR-C31 刷新后「显示编辑栏」仍未勾选，编辑栏保持隐藏', !(await showBox().isChecked()) && !(await barShown()))
    step('BAR-C31 表格数据与选中不受影响', same((await cellOf('A1')).value, a1) && (await details()).active === 'A1')
  })

  await section('BAR-C32 状态记忆：关闭标签页后重新打开保持', async () => {
    const url = page.url()
    const other = await page.context().newPage()
    await other.goto(url)
    await other.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
    const frame = other.frameLocator('#kit')
    await frame.getByTestId('sheet-strip').waitFor()
    await other.waitForTimeout(800)
    await frame.getByTestId('ribbon-tab-view').click()
    const checked = await frame.getByTestId('sheet-show-formula-bar').isChecked()
    step('BAR-C32 重新打开后编辑栏仍隐藏，「显示编辑栏」未勾选', !checked && (await frame.getByTestId('sheet-formula-bar').count()) === 0)
    await other.close()
  })

  await section('BAR-C33 状态记忆：显示态刷新后不回退为隐藏', async () => {
    await setBar(true)
    await reopen()
    await viewTab()
    const box = await kit.getByTestId('sheet-formula-bar').boundingBox()
    const controls = await Promise.all(['sheet-name-box', 'sheet-formula-cancel', 'sheet-formula-enter', 'sheet-formula-fx', 'sheet-formula'].map((id) => kit.getByTestId(id).count()))
    step('BAR-C33 刷新后仍为勾选态，编辑栏位置、高度与五项控件与初始一致', (await showBox().isChecked()) && Math.abs(box.y - initial.y) <= 1 && Math.abs(box.height - initial.height) <= 1 && controls.every((count) => count === 1))
    const url = page.url()
    const other = await page.context().newPage()
    await other.goto(url)
    await other.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
    const frame = other.frameLocator('#kit')
    await frame.getByTestId('sheet-formula-bar').waitFor({ timeout: 30000 }).catch(() => undefined)
    step('BAR-C33 重新打开后同样为显示态', (await frame.getByTestId('sheet-formula-bar').count()) === 1)
    await other.close()
  })

  await section('BAR-C34 只读权限下可切换编辑栏显示', async () => {
    await openEditor('&role=viewer')
    const a1 = (await cellOf('A1')).value
    await viewTab()
    const off = await toggleBar()
    const on = await toggleBar()
    step('BAR-C34 只读下「视图」可展开，「显示编辑栏」可取消与勾选，即时生效', off && on && (await barShown()) && (await showBox().isChecked()))
    const locked = (await kit.getByTestId('sheet-add').isDisabled()) && (await kit.getByTestId('sheet-formula-fx').isDisabled()) && (await bar().getAttribute('readonly')) !== null
    step('BAR-C34 编辑、插入类入口保持不可用，表格数据未被修改', locked && same((await cellOf('A1')).value, a1) && !(await exec('document.inspect')).dirty)
  })

  await section('BAR-C35 编辑栏开关不写入数据、不产生变更', async () => {
    await openEditor('')
    const before = await exec('document.inspect')
    for (let i = 0; i < 4; i += 1) {
      await viewTab()
      await toggleBar()
    }
    const after = await exec('document.inspect')
    step('BAR-C35 多次切换后文档不进入未保存状态，不产生撤销/历史记录', !after.dirty && before.dirty === after.dirty && same(after.undo, before.undo) && (await barShown()), JSON.stringify(after.undo))
  })

  await section('BAR-C36 快速连续切换编辑栏开关', async () => {
    await setBar(true)
    // A click sends the opposite of what the box shows, and the box shows the engine's preference
    // only once the previous click has come back from the engine. A click that lands before that
    // reads the old state and sends the same value again instead of toggling: on a loaded runner
    // the ten clicks ended with the bar hidden and the wait for the shown bar ran out (7 s). So
    // each click waits until the box shows the state it produced, and then the next one follows.
    for (let i = 0; i < 10; i += 1) {
      const was = await showBox().isChecked()
      await showBox().click()
      await until(async () => (await showBox().isChecked()) === !was)
    }
    await until(async () => (await showBox().isChecked()) && (await barShown()))
    const checked = await showBox().isChecked()
    step('BAR-C36 连续 10 次后与最后一次操作一致（勾选态，偶数次回到显示），状态与显示一致', checked && (await barShown()) && (await kit.locator('[data-testid="sheet-formula-bar"]').count()) === 1, String(checked))
  })

  await section('BAR-C37 长文本内容在编辑栏中的显示与编辑', async () => {
    await table()
    const long = '长文本'.repeat(70) + 'END'
    await exec('cell.set', { cell: `${q('工作表1')}!A1`, input: long })
    await clickCell(0, 0)
    await until(async () => (await formulaBar()) === long)
    const scroll = await bar().evaluate((element) => ({ width: element.scrollWidth, shown: element.clientWidth }))
    step('BAR-C37 内容编辑区完整显示全部文本（不截断），可横向滚动', (await formulaBar()) === long && (await nameBox()) === 'A1' && scroll.width > scroll.shown, JSON.stringify(scroll))
    await intoBar()
    await page.keyboard.type('尾')
    const caret = await bar().evaluate((element) => ({ at: element.selectionStart, left: element.scrollLeft }))
    step('BAR-C37 末尾追加后可正常编辑，光标与文本可见', caret.at === long.length + 1 && caret.left > 0, JSON.stringify(caret))
    await kit.getByTestId('sheet-formula-enter').click()
    await until(async () => (await input('A1')) === `${long}尾`)
    step('BAR-C37 确认后单元格内容与内容编辑区完全一致', (await input('A1')) === `${long}尾` && (await formulaBar()) === `${long}尾`)
  })

  // ———————————————————————————— Formula input in the formula bar (left over from F1) ————————————————————————————
  await section('BAR-X1 编辑栏点选模式：单击、拖动单元格插入引用', async () => {
    await table()
    await clickCell(11, 4)
    await bar().click()
    await page.keyboard.type('=SUM(')
    await until(async () => (await editText()) === '=SUM(')
    const from = await centre(2, 0)
    const to = await centre(2, 2)
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    await page.mouse.move(to.x, to.y, { steps: 6 })
    await page.mouse.up()
    await until(async () => (await formulaBar()) === '=SUM(C1:C3' && (await focusedTestId()) === 'sheet-formula')
    step('BAR-X1 拖过 C1:C3：编辑栏为 =SUM(C1:C3，光标仍在编辑栏，活动单元格仍为 L5', (await formulaBar()) === '=SUM(C1:C3' && (await focusedTestId()) === 'sheet-formula' && (await details()).active === 'L5', `${await formulaBar()} ${await focusedTestId()}`)
    await page.keyboard.type(',')
    const at = await centre(4, 1)
    await page.mouse.click(at.x, at.y)
    await until(async () => (await formulaBar()) === '=SUM(C1:C3,E2')
    step('BAR-X1 输入逗号后单击 E2 接着插入', (await formulaBar()) === '=SUM(C1:C3,E2', await formulaBar())
    await page.keyboard.type(')')
    await page.keyboard.press('Enter')
    await until(async () => (await input('L5')) === '=SUM(C1:C3,E2)')
    step('BAR-X1 回车写入 L5 并下移到 L6', (await input('L5')) === '=SUM(C1:C3,E2)' && (await details()).active === 'L6')
  })

  await section('BAR-X2 编辑栏跨表点选，名称框显示原单元格', async () => {
    await table()
    await exec('cell.set', { cell: `${q('工作表2')}!B2`, input: '21' })
    await clickCell(11, 6)
    await bar().click()
    await page.keyboard.type('=')
    await choose('工作表2')
    step('BAR-X2 点工作表2 标签后仍在编辑，名称框显示原单元格 L7', (await details()).editing && (await nameBox()) === 'L7', await nameBox())
    await clickCell(1, 1)
    await until(async () => (await formulaBar()) === '=工作表2!B2')
    step('BAR-X2 点 B2：编辑栏为 =工作表2!B2，光标回到编辑栏', (await formulaBar()) === '=工作表2!B2' && (await focusedTestId()) === 'sheet-formula' && (await nameBox()) === 'L7', `${await formulaBar()} ${await focusedTestId()}`)
    await page.keyboard.type('*2')
    await page.keyboard.press('Enter')
    await until(async () => (await details()).sheet === '工作表1')
    const l7 = await cellOf(`${q('工作表1')}!L7`)
    step('BAR-X2 回车写回工作表1!L7 并回到工作表1，值 42', l7.formula === '=工作表2!B2*2' && l7.value === 42, JSON.stringify(l7).slice(0, 120))
  })

  await section('BAR-X3 Enter / Shift+Enter / Tab / Esc', async () => {
    await table()
    await clickCell(6, 2)
    await bar().click()
    await bar().fill('下')
    await bar().press('Enter')
    await until(async () => (await details()).active === 'G4')
    step('BAR-X3 回车写入并下移', (await input('G3')) === '下' && (await details()).active === 'G4')
    await bar().click()
    await bar().fill('右')
    await bar().press('Tab')
    await until(async () => (await details()).active === 'H4')
    step('BAR-X3 Tab 写入并右移', (await input('G4')) === '右' && (await details()).active === 'H4')
    await bar().click()
    await bar().fill('上')
    await bar().press('Shift+Enter')
    await until(async () => (await details()).active === 'H3')
    step('BAR-X3 Shift+Enter 写入并上移', (await input('H4')) === '上' && (await details()).active === 'H3')
    await bar().click()
    await bar().fill('不要')
    await bar().press('Escape')
    await until(async () => !(await details()).editing)
    step('BAR-X3 Esc 取消，内容不变', (await input('H3')) === 'H3' && (await formulaBar()) === 'H3')
  })

  await section('BAR-X4 Alt+Enter 换行与展开编辑栏', async () => {
    await table()
    await clickCell(6, 6)
    await bar().click()
    await bar().fill('第一行')
    await bar().press('End')
    await bar().press('Alt+Enter')
    await page.keyboard.type('第二行')
    await until(async () => (await editText()) === '第一行\n第二行')
    step('BAR-X4 Alt+Enter 在编辑栏换行，单元格同步', (await formulaBar()) === '第一行\n第二行' && (await editText()) === '第一行\n第二行', JSON.stringify(await formulaBar()))
    await kit.getByTestId('sheet-formula-enter').click()
    await until(async () => (await input('G7')) === '第一行\n第二行')
    const g7 = await cellOf('G7')
    step('BAR-X4 确认后单元格含换行并自动换行', g7.input === '第一行\n第二行' && g7.style?.align?.wrap === true, JSON.stringify(g7.style?.align))
    const collapsed = (await bar().boundingBox()).height
    await kit.getByTestId('sheet-formula-expand').click()
    await until(async () => (await bar().boundingBox()).height > collapsed + 30)
    const open = (await bar().boundingBox()).height
    const grid = await region()
    await kit.getByTestId('sheet-formula-expand').click()
    await until(async () => (await bar().boundingBox()).height === collapsed)
    step('BAR-X4 展开编辑栏显示多行（下移网格），再点折叠恢复', open > collapsed + 30 && (await kit.getByTestId('sheet-formula-expand').getAttribute('aria-expanded')) === 'false' && (await region()).y < grid.y, `${collapsed} ${open}`)
  })

  await section('BAR-X5 单元格内 Alt+Enter 换行', async () => {
    await table()
    await clickCell(7, 7)
    await page.keyboard.type('上半')
    await page.keyboard.press('Alt+Enter')
    await page.keyboard.type('下半')
    await until(async () => (await editText()) === '上半\n下半')
    step('BAR-X5 Alt+Enter 在单元格内换行（不提交）', (await editText()) === '上半\n下半' && (await details()).active === 'H8')
    await page.keyboard.press('Enter')
    await until(async () => (await input('H8')) === '上半\n下半')
    step('BAR-X5 回车写入两行文本并下移', (await input('H8')) === '上半\n下半' && (await details()).active === 'H9')
  })

  await section('BAR-X6 F2 编辑单元格', async () => {
    await table()
    await clickCell(3, 3)
    await page.keyboard.press('F2')
    await until(async () => (await details()).editing)
    step('BAR-X6 F2 进入编辑，内容为单元格原内容', (await editText()) === 'D4', await editText())
    await page.keyboard.type('!')
    await page.keyboard.press('Enter')
    await until(async () => (await input('D4')) === 'D4!')
    step('BAR-X6 F2 后追加并回车', (await input('D4')) === 'D4!')
  })

  await section('BAR-X7 函数列表说明为中文', async () => {
    await table()
    await clickCell(11, 8)
    await bar().click()
    await page.keyboard.type('=vlo')
    const list = kit.getByTestId('sheet-formula-hints-list')
    await list.waitFor()
    // The list shows from the first letter: read it once it follows all of `=vlo` (WebKit on a
    // runner showed the `=v` list, VALUE first, a moment before).
    await until(async () => (await bar().inputValue()) === '=vlo' && (await list.locator('li').first().getAttribute('data-name'))?.startsWith('VLO'))
    const text = await kit.getByTestId('sheet-formula-hints-description').textContent()
    step('BAR-X7 =vlo 列表首项 VLOOKUP，说明为中文', (await list.locator('li').first().getAttribute('data-name')) === 'VLOOKUP' && text.includes('查找'), text)
    await bar().press('Escape')
  })
} catch (error) {
  await h.finish(error)
}
await h.finish()
