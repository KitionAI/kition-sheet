// The sheet tab bar against the reference sheet tab UI interaction cases (module M1), in a real browser through
// the development host: node tests/gate-sheet-ui.mjs [chromium|webkit]
// Each step names its case by the row of the case table (UI-C5 is row 5). Cases are set up by
// command (a new workbook's sheets) and, where they say so, on a workbook saved and opened again
// (an imported .xlsx). Prints PASS or FAIL per step.
import { startGate } from './gate-harness.mjs'

const h = await startGate({ prefix: 'UI-' })
/** The grid's pitch as the engine lays it out (the harness reads it when a workbook opens). */
const G = h.grid
const {
  page, kit, errors, step, section, until, same, ai, exec, details, workbook, order, activeSheet, q, cellOf,
  tab, strip, tabNames, selectedTabs, nameBox, formulaBar, region, clickCell, goTo, choose, showsTabs, build, numbered, shownA1,
  drag, after, before, end, arrowMenu, rightClick, menuRows, moveChoices, moveBy, closeMenus, listRows, inView,
  saveAndReopen, touch, savedWorkbook, pixel, signature, topRow, openEditor,
} = h

try {
  await openEditor('')

  // ———————————————————————————— 1. Switching sheets ————————————————————————————
  await section('UI-C5 单击切换至相邻工作表', async () => {
    await build(numbered(3))
    await choose('工作表2')
    step('UI-C5 单击「工作表2」：仅它呈选中，编辑区为工作表2', same(await selectedTabs(), ['工作表2']) && (await shownA1()) === '工作表2内容')
    await choose('工作表3')
    step('UI-C5 再单击「工作表3」：选中转移，编辑区为工作表3', same(await selectedTabs(), ['工作表3']) && (await shownA1()) === '工作表3内容')
  })

  await section('UI-C6 单击切换至非相邻工作表', async () => {
    await build(numbered(4))
    await choose('工作表4')
    step('UI-C6 「工作表4」呈选中，编辑区显示工作表4，工作表2/3 未选中', same(await selectedTabs(), ['工作表4']) && (await shownA1()) === '工作表4内容')
  })

  await section('UI-C7 来回切换后的内容对应', async () => {
    await build(numbered(2))
    await exec('home.bold', { range: `${q('工作表2')}!A1` })
    const seen = []
    for (const name of ['工作表1', '工作表2', '工作表1']) {
      await choose(name)
      seen.push([name, await shownA1(), (await formulaBar())])
    }
    step('UI-C7 三次切换后编辑区内容均与所点工作表一致，无串表', seen.every(([name, a1, bar]) => a1 === `${name}内容` && bar === `${name}内容`), JSON.stringify(seen))
  })

  await section('UI-C8 仅 1 张工作表时的单击', async () => {
    await build(['工作表1'])
    const errorsBefore = errors.length
    await tab('工作表1').click()
    await page.waitForTimeout(300)
    step('UI-C8 唯一标签保持选中，内容不变，无报错', same(await selectedTabs(), ['工作表1']) && (await shownA1()) === '工作表1内容' && errors.length === errorsBefore)
  })

  await section('UI-C9 快速连续单击后的落点', async () => {
    await build(numbered(3))
    for (const name of ['工作表1', '工作表2', '工作表3', '工作表2']) await tab(name).click()
    await until(async () => (await activeSheet()) === '工作表2' && same(await selectedTabs(), ['工作表2']))
    await page.waitForTimeout(300)
    step('UI-C9 最终停留在工作表2，内容与选中状态一致', (await activeSheet()) === '工作表2' && same(await selectedTabs(), ['工作表2']) && (await shownA1()) === '工作表2内容')
  })

  await section('UI-C10 从工作表列表选表切换', async () => {
    await build(numbered(3))
    const rows = await listRows()
    step('UI-C10 列表按标签顺序列出全部工作表，当前工作表带勾选', same(rows.map((row) => row.name), numbered(3)) && same(rows.map((row) => row.checked), [true, false, false]), JSON.stringify(rows))
    await kit.getByTestId('sheet-list-item-工作表3').click()
    await until(async () => (await activeSheet()) === '工作表3')
    step('UI-C10 选择后编辑区切到该表，对应标签选中', same(await selectedTabs(), ['工作表3']) && (await shownA1()) === '工作表3内容')
  })

  await section('UI-C11 选中状态互斥', async () => {
    await build(numbered(3))
    let exclusive = true
    for (const name of numbered(3)) {
      await choose(name)
      exclusive &&= same(await selectedTabs(), [name])
    }
    step('UI-C11 任意时刻有且仅有 1 个标签呈选中', exclusive)
  })

  await section('UI-C12 切换后内容、公式与格式的完整性', async () => {
    await build(numbered(2))
    await ai([
      { command: 'range.setValues', params: { range: `${q('工作表2')}!C1`, values: [[1], [2], [3]] } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!C4`, input: '=SUM(C1:C3)' } },
      { command: 'home.bold', params: { range: `${q('工作表2')}!C4` } },
    ])
    const direct = await cellOf(`${q('工作表2')}!C4`)
    await choose('工作表2')
    const shown = await cellOf('C4')
    step('UI-C12 工作表2 的数据、公式结果与格式与直接查看一致', shown.value === 6 && shown.formula === '=SUM(C1:C3)' && shown.style?.font?.bold === true && same(shown, direct), JSON.stringify(shown).slice(0, 200))
  })

  await section('UI-C13 名称框与 fx 栏随切换同步', async () => {
    await build(numbered(2))
    await exec('cell.set', { cell: `${q('工作表2')}!C5`, input: 'C5的内容' })
    await exec('cell.set', { cell: `${q('工作表1')}!B2`, input: 'B2的内容' })
    await choose('工作表2')
    await goTo('C5')
    await choose('工作表1')
    await clickCell(1, 1)
    await until(async () => (await nameBox()) === 'B2')
    await choose('工作表2')
    await until(async () => (await nameBox()) === 'C5')
    step('UI-C13 切到工作表2：名称框为 C5，fx 栏为 C5 的内容', (await nameBox()) === 'C5' && (await formulaBar()) === 'C5的内容', `${await nameBox()} ${await formulaBar()}`)
    await choose('工作表1')
    await until(async () => (await nameBox()) === 'B2')
    step('UI-C13 切回工作表1：名称框恢复 B2，fx 栏为 B2 的内容', (await nameBox()) === 'B2' && (await formulaBar()) === 'B2的内容', `${await nameBox()} ${await formulaBar()}`)
  })

  await section('UI-C14 切换后的活动单元格落点与恢复', async () => {
    await build(numbered(1))
    await clickCell(1, 9)
    await until(async () => (await nameBox()) === 'B10')
    await kit.getByTestId('sheet-add').click()
    await until(async () => (await tabNames()).length === 2)
    const added = (await tabNames())[1]
    await until(async () => (await activeSheet()) === added)
    step('UI-C14 切至新建的工作表2时活动单元格落在 A1', (await nameBox()) === 'A1', await nameBox())
    await choose('工作表1')
    await until(async () => (await nameBox()) === 'B10')
    step('UI-C14 切回工作表1时活动单元格恢复为 B10', (await nameBox()) === 'B10', await nameBox())
  })

  await section('UI-C15 切换后的滚动位置恢复', async () => {
    await build(numbered(2))
    await choose('工作表1')
    const box = await region()
    await page.mouse.move(box.x + 300, box.y + 200)
    for (let i = 0; i < 10; i += 1) await page.mouse.wheel(0, 200)
    await page.waitForTimeout(400)
    const scrolled = await topRow()
    await choose('工作表2')
    await page.waitForTimeout(300)
    const other = await topRow()
    step('UI-C15 切至工作表2：视口在其自身位置（首屏）', other === 1, `top row ${other}`)
    await choose('工作表1')
    await page.waitForTimeout(300)
    const back = await topRow()
    step('UI-C15 切回工作表1：恢复到滚动处附近，不跳回首行', scrolled > 50 && Math.abs(back - scrolled) <= 1, `scrolled to ${scrolled}, back at ${back}`)
    // Back to the top for the cases that click cells.
    await page.mouse.move(box.x + 300, box.y + 200)
    for (let i = 0; i < 12; i += 1) await page.mouse.wheel(0, -200)
    await page.waitForTimeout(300)
  })

  await section('UI-C16 状态栏统计随切换重算', async () => {
    await build(numbered(2))
    await exec('range.setValues', { range: `${q('工作表1')}!A1`, values: [[1], [2], [3]] })
    await goTo('A1:A3')
    await until(async () => (await kit.getByTestId('sheet-stats').count()) === 1)
    const figures = (await kit.getByTestId('sheet-stats').count()) ? await kit.getByTestId('sheet-stats').textContent() : ''
    step('UI-C16 工作表1 A1:A3：求和 6、平均值 2、计数 3', /求和=6/.test(figures) && /平均值=2/.test(figures) && /计数=3/.test(figures), figures)
    await choose('工作表2')
    await until(async () => (await kit.getByTestId('sheet-stats').count()) === 0)
    step('UI-C16 切到无选区的工作表2：不显示统计值', (await kit.getByTestId('sheet-stats').count()) === 0)
    await choose('工作表1')
    await until(async () => (await kit.getByTestId('sheet-stats').count()) === 1)
    step('UI-C16 切回工作表1：按其选区重新显示统计', (await kit.getByTestId('sheet-stats').count()) === 1)
  })

  await section('UI-C17 标签颜色标记的保持', async () => {
    await build(numbered(2))
    await ai([
      { command: 'sheet.tabColor', params: { sheet: '工作表1', color: '#FF0000' } },
      { command: 'sheet.tabColor', params: { sheet: '工作表2', color: '#0000FF' } },
    ])
    await until(async () => (await tab('工作表1').getAttribute('data-color')) === '#FF0000')
    let kept = true
    for (const name of ['工作表2', '工作表1', '工作表2', '工作表1']) {
      await choose(name)
      kept &&= (await tab('工作表1').getAttribute('data-color')) === '#FF0000' && (await tab('工作表2').getAttribute('data-color')) === '#0000FF'
    }
    step('UI-C17 来回切换后两个标签颜色始终为红、蓝', kept)
  })

  await section('UI-C18 行号与列标随活动单元格高亮', async () => {
    await build(numbered(2))
    await choose('工作表2')
    await goTo('C5')
    await choose('工作表1')
    await choose('工作表2')
    await until(async () => (await nameBox()) === 'C5')
    await page.waitForTimeout(300)
    const column = (index) => pixel(46 + index * G.cw + 6, 4)
    const row = (index) => pixel(4, 21 + index * G.rh + 4)
    const [c, d, e] = [await column(2), await column(3), await column(4)]
    const [r5, r6, r7] = [await row(4), await row(5), await row(6)]
    step('UI-C18 列标「C」高亮，其余列标普通', !same(c, d) && same(d, e), JSON.stringify({ c, d, e }))
    step('UI-C18 行号「5」高亮，其余行号普通', !same(r5, r6) && same(r6, r7), JSON.stringify({ r5, r6, r7 }))
  })

  await section('UI-C19 冻结行列各工作表互不影响', async () => {
    await build(numbered(2))
    await exec('view.freezeTopRow')
    await choose('工作表2')
    const freezes = async () => (await workbook()).sheets.map((sheet) => sheet.freeze)
    step('UI-C19 工作表2 无冻结', (await freezes())[1] == null, JSON.stringify(await freezes()))
    await choose('工作表1')
    const box = await region()
    await page.mouse.move(box.x + 300, box.y + 200)
    for (let i = 0; i < 4; i += 1) await page.mouse.wheel(0, 200)
    await page.waitForTimeout(400)
    await clickCell(0, 0)
    await until(async () => (await nameBox()) === 'A1')
    step('UI-C19 工作表1 冻结恢复，向下滚动时首行保持可见', JSON.stringify((await freezes())[0]) === JSON.stringify([1, 0]) && (await nameBox()) === 'A1', `${JSON.stringify(await freezes())} ${await nameBox()}`)
    await exec('view.unfreezePanes')
    await page.mouse.move(box.x + 300, box.y + 200)
    for (let i = 0; i < 6; i += 1) await page.mouse.wheel(0, -200)
    await page.waitForTimeout(300)
  })

  await section('UI-C20 筛选条件与结果的保持', async () => {
    await build(numbered(2), { fill: false })
    await ai([
      { command: 'range.setValues', params: { range: `${q('工作表1')}!A1`, values: [['数量'], [1], [2], [3], [4], [5]] } },
      { command: 'range.setValues', params: { range: `${q('工作表2')}!A1`, values: [['数量'], [1], [2]] } },
      { command: 'data.filter', params: { range: 'A1:A6' } },
      { command: 'data.filterBy', params: { column: 'A', custom: { op: '>', value: '2' } } },
    ])
    await choose('工作表2')
    const filters = async () => (await workbook()).sheets.map((sheet) => sheet.autofilter)
    await clickCell(0, 1)
    step('UI-C20 工作表2 完整显示，无筛选标记', (await filters())[1] == null && (await nameBox()) === 'A2', `${JSON.stringify(await filters())} ${await nameBox()}`)
    await choose('工作表1')
    await clickCell(0, 1)
    await page.waitForTimeout(150)
    step('UI-C20 工作表1 筛选保持（第 2、3 行仍被筛掉）', (await filters())[0] === 'A1:A6' && (await nameBox()) === 'A4', `${JSON.stringify(await filters())} ${await nameBox()}`)
  })

  await section('UI-C21 隐藏行列各工作表互不影响', async () => {
    await build(numbered(2))
    await ai([
      { command: 'home.hideColumns', params: { cols: 'C' } },
      { command: 'home.hideRows', params: { rows: '5' } },
    ])
    await choose('工作表2')
    await clickCell(2, 4)
    await page.waitForTimeout(150)
    step('UI-C21 工作表2 完整显示全部行列', (await nameBox()) === 'C5', await nameBox())
    await choose('工作表1')
    await clickCell(2, 4)
    await page.waitForTimeout(150)
    step('UI-C21 工作表1 的 C 列与第 5 行仍隐藏', (await nameBox()) === 'D6', await nameBox())
  })

  await section('UI-C22 缩放比例切换后的保持', async () => {
    await build(numbered(2))
    const box = await region()
    await page.mouse.click(box.x + 300, box.y + 200)
    await page.keyboard.press('ControlOrMeta+Equal')
    await page.keyboard.press('ControlOrMeta+Equal')
    await page.waitForTimeout(300)
    const zoomed = await kit.getByTestId('kit-zoom').textContent()
    await choose('工作表2')
    await page.waitForTimeout(300)
    step('UI-C22 切换工作表后缩放比例保持，不回落 100%', zoomed !== '100%' && (await kit.getByTestId('kit-zoom').textContent()) === zoomed, `${zoomed} -> ${await kit.getByTestId('kit-zoom').textContent()}`)
    // A fresh editor for the cases that click cells at 100% from the top-left.
    await openEditor('')
  })

  await section('UI-C23 工作表列表访问未显示在标签栏中的工作表', async () => {
    await build(numbered(30), { fill: false })
    step('UI-C23 前置：30 张时标签栏无法一次显示全部', !(await inView('工作表30')))
    const rows = await listRows()
    step('UI-C23 列表完整列出 30 张，当前工作表带勾选', rows.length === 30 && rows[0].checked && rows.filter((row) => row.checked).length === 1)
    await kit.getByTestId('sheet-list-item-工作表30').click()
    await until(async () => (await activeSheet()) === '工作表30')
    step('UI-C23 选择后切换至该表，对应标签选中', same(await selectedTabs(), ['工作表30']))
  })

  await section('UI-C24 切换后标签栏自动滚动定位', async () => {
    await build(numbered(30), { fill: false })
    await listRows()
    await kit.getByTestId('sheet-list-item-工作表25').click()
    await until(async () => (await activeSheet()) === '工作表25')
    await page.waitForTimeout(200)
    step('UI-C24 「工作表25」可见且呈选中', (await inView('工作表25')) && same(await selectedTabs(), ['工作表25']))
  })

  await section('UI-C25 Ctrl+PageDown / Ctrl+PageUp 切换相邻工作表', async () => {
    await build(numbered(3), { active: '工作表2' })
    await tab('工作表2').click()
    await page.keyboard.press('Control+PageDown')
    await until(async () => (await activeSheet()) === '工作表3')
    step('UI-C25 Ctrl+PageDown 后「工作表3」唯一选中，编辑区为工作表3', same(await selectedTabs(), ['工作表3']) && (await shownA1()) === '工作表3内容')
    await page.keyboard.press('Control+PageUp')
    await until(async () => (await activeSheet()) === '工作表2')
    step('UI-C25 Ctrl+PageUp 后「工作表2」唯一选中，编辑区为工作表2', same(await selectedTabs(), ['工作表2']) && (await shownA1()) === '工作表2内容')
  })

  await section('UI-C26 首尾工作表时组合键的边界表现', async () => {
    await build(numbered(3))
    await tab('工作表1').click()
    await page.keyboard.press('Control+PageUp')
    await page.waitForTimeout(300)
    step('UI-C26 在工作表1 按 Ctrl+PageUp：仍为工作表1，内容不变', same(await selectedTabs(), ['工作表1']) && (await shownA1()) === '工作表1内容')
    await choose('工作表3')
    await page.keyboard.press('Control+PageDown')
    await page.waitForTimeout(300)
    step('UI-C26 在工作表3 按 Ctrl+PageDown：仍为工作表3，内容不变', same(await selectedTabs(), ['工作表3']) && (await shownA1()) === '工作表3内容')
  })

  // ———————————————————————————— 2. Dragging sheets to move them ————————————————————————————
  const contentsMatch = async (names) => {
    for (const name of names) if ((await cellOf(`${q(name)}!A1`)).value !== `${name}内容`) return false
    return true
  }

  await section('UI-C28 拖拽至相邻位置', async () => {
    await build(numbered(3))
    await drag('工作表1', await after('工作表2'))
    await showsTabs(['工作表2', '工作表1', '工作表3'])
    step('UI-C28 顺序为 工作表2、工作表1、工作表3', same(await tabNames(), ['工作表2', '工作表1', '工作表3']) && same(await order(), ['工作表2', '工作表1', '工作表3']), JSON.stringify(await tabNames()))
    step('UI-C28 仍为 3 张，名称与内容不变', (await order()).length === 3 && (await contentsMatch(numbered(3))))
  })

  await section('UI-C29 拖拽至非相邻位置', async () => {
    await build(numbered(4))
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表3', '工作表4', '工作表1'])
    step('UI-C29 顺序为 工作表2、3、4、1，名称与内容对应', same(await order(), ['工作表2', '工作表3', '工作表4', '工作表1']) && (await contentsMatch(numbered(4))), JSON.stringify(await tabNames()))
  })

  await section('UI-C30 拖拽至最前', async () => {
    await build(numbered(3))
    await drag('工作表3', await before('工作表1'))
    await showsTabs(['工作表3', '工作表1', '工作表2'])
    step('UI-C30 顺序为 工作表3、工作表1、工作表2', same(await order(), ['工作表3', '工作表1', '工作表2']), JSON.stringify(await tabNames()))
    // The case's second line has 工作表3 the current sheet; its 41st and 62nd rows say a drag
    // leaves the current sheet in view. We follow those: the sheet in view (工作表1) stays.
    step('UI-C30 拖拽不切换当前工作表（与 C41/C62 一致）', same(await selectedTabs(), ['工作表1']))
  })

  await section('UI-C31 拖拽至最后', async () => {
    await build(numbered(3))
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    step('UI-C31 顺序为 工作表2、3、1；工作表1 仍为当前，内容不变', same(await order(), ['工作表2', '工作表3', '工作表1']) && same(await selectedTabs(), ['工作表1']) && (await shownA1()) === '工作表1内容')
  })

  await section('UI-C32 拖拽后当前工作表的保持', async () => {
    await build(numbered(3))
    await choose('工作表2')
    await goTo('B2')
    await drag('工作表2', await end())
    await showsTabs(['工作表1', '工作表3', '工作表2'])
    step('UI-C32 仍停留在工作表2；顺序 1、3、2；内容与名称框不变', same(await selectedTabs(), ['工作表2']) && same(await order(), ['工作表1', '工作表3', '工作表2']) && (await nameBox()) === 'B2' && (await shownA1()) === '工作表2内容')
  })

  await section('UI-C33 拖拽中标签的跟随表现', async () => {
    await build(numbered(3))
    const [two, three] = [await tab('工作表2').boundingBox(), await tab('工作表3').boundingBox()]
    const target = await after('工作表2')
    await drag('工作表1', { x: target.x - 20, y: target.y }, { release: false })
    const ghost = await kit.getByTestId('sheet-drag-ghost').boundingBox().catch(() => null)
    const still = same(await tab('工作表2').boundingBox(), two) && same(await tab('工作表3').boundingBox(), three)
    step('UI-C33 拖动中标签随指针浮起跟随，其余标签位置不变', ghost != null && Math.abs(ghost.x + 24 - (target.x - 20)) < 30 && still && (await tab('工作表1').getAttribute('data-dragging')) === 'true')
    await page.mouse.up()
  })

  await section('UI-C34 拖拽中的落点提示', async () => {
    await build(numbered(3))
    const gap = await after('工作表2')
    await drag('工作表1', gap, { release: false })
    const mark = await kit.getByTestId('sheet-drop-mark').boundingBox().catch(() => null)
    const [two, three] = [await tab('工作表2').boundingBox(), await tab('工作表3').boundingBox()]
    step('UI-C34 两标签间显示插入标记', mark != null && mark.x >= two.x + two.width - 3 && mark.x <= three.x + 3, JSON.stringify({ mark, two: two.x + two.width, three: three.x }))
    await page.mouse.up()
    await showsTabs(['工作表2', '工作表1', '工作表3'])
    step('UI-C34 松手后的落位与提示一致', same(await order(), ['工作表2', '工作表1', '工作表3']))
  })

  await section('UI-C35 拖出标签栏可放置区域', async () => {
    await build(numbered(3))
    const box = await region()
    await drag('工作表1', { x: box.x + 400, y: box.y + 200 }, { release: false })
    const cursor = await tab('工作表1').evaluate((element) => getComputedStyle(element).cursor)
    const refused = (await kit.getByTestId('sheet-tabs').getAttribute('data-drop')) === 'none'
    step('UI-C35 指针呈不可放置状态', refused && cursor === 'not-allowed', cursor)
    await page.mouse.up()
    await page.waitForTimeout(300)
    step('UI-C35 标签不落入编辑区，顺序与内容不变', same(await order(), numbered(3)) && (await contentsMatch(numbered(3))))
  })

  const scrollLeft = () => strip().evaluate((element) => element.scrollLeft)

  await section('UI-C36 拖至标签栏可视边界处的表现', async () => {
    await build(numbered(30), { fill: false })
    const box = await strip().boundingBox()
    await drag('工作表1', { x: box.x + box.width - 6, y: box.y + box.height / 2 }, { release: false, hold: 1500 })
    const scrolled = await scrollLeft()
    // Out of the scrolling band, the drop mark holds still: it is where the tab lands.
    await page.mouse.move(box.x + box.width - 80, box.y + box.height / 2, { steps: 4 })
    await page.waitForTimeout(200)
    const slot = Number(await kit.getByTestId('sheet-drop-mark').getAttribute('data-slot').catch(() => 'NaN'))
    step('UI-C36 拖至右端时标签栏横向滚动，后续标签进入可视范围', scrolled > 0 && (await inView('工作表20')), `scrollLeft ${scrolled}`)
    await page.mouse.up()
    await until(async () => (await order()).indexOf('工作表1') > 0)
    step('UI-C36 松手后落位与落点提示一致', (await order()).indexOf('工作表1') === slot - 1, `slot ${slot}, at ${(await order()).indexOf('工作表1')}`)
  })

  await section('UI-C37 移动后内容与格式的完整性', async () => {
    await build(numbered(2))
    await ai([
      { command: 'cell.set', params: { cell: `${q('工作表2')}!C1`, input: '=B2*2' } },
      { command: 'home.bold', params: { range: `${q('工作表2')}!C1` } },
      { command: 'home.mergeCenter', params: { range: `${q('工作表1')}!D1:E1` } },
      { command: 'home.columnWidth', params: { cols: 'B', width: 120 } },
    ])
    const snapshot = async () => [await cellOf(`${q('工作表1')}!A1`), await cellOf(`${q('工作表2')}!C1`), (await workbook()).sheets.map((sheet) => [sheet.name, sheet.merges])]
    const [a, b, merges] = await snapshot()
    await drag('工作表2', await before('工作表1'))
    await showsTabs(['工作表2', '工作表1'])
    const [a2, b2, merges2] = await snapshot()
    const sorted = (list) => [...list].sort((x, y) => x[0].localeCompare(y[0]))
    step('UI-C37 两表数据、公式结果、格式、合并单元格未随移动串位', same(a, a2) && same(b, b2) && b2.value === 40 && b2.style?.font?.bold === true && same(sorted(merges), sorted(merges2)), JSON.stringify(merges2))
  })

  await section('UI-C38 跨表引用公式移动后按名称定位', async () => {
    await build(numbered(2), { fill: false })
    await ai([
      { command: 'cell.set', params: { cell: `${q('工作表1')}!A1`, input: '10' } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!B1`, input: '=工作表1!A1' } },
    ])
    await drag('工作表2', await before('工作表1'))
    await showsTabs(['工作表2', '工作表1'])
    step('UI-C38 工作表2!B1 仍为 10', (await cellOf(`${q('工作表2')}!B1`)).value === 10)
  })

  await section('UI-C39 冻结与筛选移动后仍归属原工作表', async () => {
    await build(numbered(3), { fill: false })
    await ai([
      { command: 'range.setValues', params: { range: `${q('工作表1')}!A1`, values: [['h'], [1], [2]] } },
      { command: 'view.freezeTopRow', params: {} },
      { command: 'data.filter', params: { range: 'A1:A3' } },
    ])
    await drag('工作表1', await after('工作表2'))
    await showsTabs(['工作表2', '工作表1', '工作表3'])
    const sheets = Object.fromEntries((await workbook()).sheets.map((sheet) => [sheet.name, sheet]))
    step('UI-C39 工作表1 冻结与筛选仍在，工作表2 无冻结无筛选', same(sheets['工作表1'].freeze, [1, 0]) && sheets['工作表1'].autofilter === 'A1:A3' && sheets['工作表2'].freeze == null && sheets['工作表2'].autofilter == null)
  })

  await section('UI-C40 标签颜色移动后跟随', async () => {
    await build(numbered(3))
    await ai([
      { command: 'sheet.tabColor', params: { sheet: '工作表1', color: '#FF0000' } },
      { command: 'sheet.tabColor', params: { sheet: '工作表2', color: '#0000FF' } },
    ])
    await until(async () => (await tab('工作表2').getAttribute('data-color')) === '#0000FF')
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    step('UI-C40 工作表1 仍红、工作表2 仍蓝', (await tab('工作表1').getAttribute('data-color')) === '#FF0000' && (await tab('工作表2').getAttribute('data-color')) === '#0000FF')
  })

  await section('UI-C41 拖拽后当前工作表与编辑区不变', async () => {
    await build(numbered(3))
    await choose('工作表3')
    await goTo('C3')
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    step('UI-C41 仍停留在工作表3，编辑区与名称框不变', same(await selectedTabs(), ['工作表3']) && (await activeSheet()) === '工作表3' && (await nameBox()) === 'C3' && (await shownA1()) === '工作表3内容')
  })

  await section('UI-C42 拖拽后工作表数量与名称不变', async () => {
    await build(numbered(3))
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    const rows = await listRows()
    await closeMenus()
    step('UI-C42 仍为 3 张不重复；列表按拖拽后的顺序列出', (await order()).length === 3 && new Set(await order()).size === 3 && same(rows.map((row) => row.name), ['工作表2', '工作表3', '工作表1']))
  })

  await section('UI-C43 两张工作表拖拽互换', async () => {
    await build(numbered(2))
    await drag('工作表2', await before('工作表1'))
    await showsTabs(['工作表2', '工作表1'])
    step('UI-C43 顺序为 工作表2、工作表1，内容与标签对应', same(await order(), ['工作表2', '工作表1']) && (await contentsMatch(numbered(2))))
  })

  await section('UI-C44 拖回原位置后松手', async () => {
    await build(numbered(3))
    const home = await tab('工作表2').boundingBox()
    await drag('工作表2', await end(), { release: false })
    await page.mouse.move(home.x + 24, home.y + home.height / 2, { steps: 10 })
    await page.mouse.up()
    await page.waitForTimeout(400)
    step('UI-C44 顺序保持 工作表1、2、3，内容不受影响', same(await order(), numbered(3)) && (await contentsMatch(numbered(3))))
  })

  await section('UI-C45 连续多次拖拽后的顺序', async () => {
    await build(numbered(3))
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    step('UI-C45 第一次后为 工作表2、3、1', same(await order(), ['工作表2', '工作表3', '工作表1']))
    await drag('工作表3', await before('工作表2'))
    await showsTabs(['工作表3', '工作表2', '工作表1'])
    step('UI-C45 第二次后为 工作表3、2、1', same(await order(), ['工作表3', '工作表2', '工作表1']))
  })

  await section('UI-C46 移动工作表子菜单的构成', async () => {
    // The case's sheet is the leftmost one, whose submenu C49 says offers only Move Right; the
    // middle sheet shows the full submenu.
    await build(numbered(3), { active: '工作表2' })
    await arrowMenu()
    const choices = await moveChoices()
    const enabled = await kit.getByTestId('sheet-menu-move-left').isEnabled() && await kit.getByTestId('sheet-menu-move-right').isEnabled()
    const items = await kit.getByTestId('sheet-menu').locator('.kui-submenu-popover [role^="menuitem"]').count()
    step('UI-C46 子菜单为「向左移动」「向右移动」2 项，均可点击，无列表与对话框', choices.left && choices.right && enabled && items === 2, JSON.stringify({ choices, items }))
    await closeMenus()
  })

  await section('UI-C47 菜单向右移动一位', async () => {
    await build(numbered(3))
    await arrowMenu()
    await moveBy('right')
    await showsTabs(['工作表2', '工作表1', '工作表3'])
    step('UI-C47 顺序为 工作表2、1、3，3 张且名称内容对应', same(await order(), ['工作表2', '工作表1', '工作表3']) && (await contentsMatch(numbered(3))))
  })

  await section('UI-C48 菜单向左移动一位', async () => {
    await build(numbered(3), { active: '工作表2' })
    await arrowMenu()
    await moveBy('left')
    await showsTabs(['工作表2', '工作表1', '工作表3'])
    step('UI-C48 顺序为 工作表2、1、3，3 张且名称内容对应', same(await order(), ['工作表2', '工作表1', '工作表3']) && (await contentsMatch(numbered(3))))
  })

  await section('UI-C49 最左侧工作表的方向项收敛', async () => {
    await build(numbered(3))
    await arrowMenu()
    const choices = await moveChoices()
    step('UI-C49 仅呈现「向右移动」且可点击', !choices.left && choices.right && (await kit.getByTestId('sheet-menu-move-right').isEnabled()), JSON.stringify(choices))
    await closeMenus()
  })

  await section('UI-C50 最右侧工作表的方向项收敛', async () => {
    await build(numbered(3), { active: '工作表3' })
    await arrowMenu()
    const choices = await moveChoices()
    step('UI-C50 仅呈现「向左移动」且可点击', choices.left && !choices.right && (await kit.getByTestId('sheet-menu-move-left').isEnabled()), JSON.stringify(choices))
    await closeMenus()
  })

  await section('UI-C51 仅 1 张工作表时的方向项', async () => {
    await build(['工作表1'])
    await arrowMenu()
    await kit.getByTestId('sheet-menu-move').hover()
    await page.waitForTimeout(200)
    const shown = (await kit.getByTestId('sheet-menu-move-left').count()) + (await kit.getByTestId('sheet-menu-move-right').count())
    step('UI-C51 向左、向右均不呈现', shown === 0)
    await closeMenus()
    step('UI-C51 顺序与内容不变', same(await order(), ['工作表1']) && (await shownA1()) === '工作表1内容')
  })

  await section('UI-C52 菜单移动后当前工作表的保持', async () => {
    await build(numbered(3))
    await goTo('B2')
    await arrowMenu()
    await moveBy('right')
    await showsTabs(['工作表2', '工作表1', '工作表3'])
    step('UI-C52 仍停留在工作表1，编辑区与名称框不变', same(await selectedTabs(), ['工作表1']) && (await nameBox()) === 'B2' && (await shownA1()) === '工作表1内容')
  })

  await section('UI-C53 连续两次经由菜单移动的累加', async () => {
    await build(numbered(3))
    await arrowMenu()
    await moveBy('right')
    await showsTabs(['工作表2', '工作表1', '工作表3'])
    step('UI-C53 第一次后为 工作表2、1、3', same(await order(), ['工作表2', '工作表1', '工作表3']))
    await arrowMenu()
    await moveBy('right')
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    step('UI-C53 第二次后为 工作表2、3、1', same(await order(), ['工作表2', '工作表3', '工作表1']))
  })

  await section('UI-C54 右键菜单的构成', async () => {
    await build(numbered(3), { active: '工作表3' })
    await rightClick('工作表3')
    const byRightClick = await menuRows()
    await closeMenus()
    await arrowMenu()
    const byArrow = await menuRows()
    await closeMenus()
    // The cloud items (Merge Sheets, Lock Sheet, Download Sheet) are deferred (docs/gaps.md).
    const expected = ['新建工作表', '重命名', '移动工作表', '创建副本', '工作表标签', '隐藏工作表', '删除']
    step('UI-C54 菜单项自上而下为 新建工作表、重命名、移动工作表、创建副本、工作表标签、隐藏工作表、删除', same(byRightClick, expected), JSON.stringify(byRightClick))
    step('UI-C54 与展开箭头弹出的菜单完全一致', same(byRightClick, byArrow), JSON.stringify(byArrow))
  })

  await section('UI-C55 右键菜单向右移动一位', async () => {
    await build(numbered(3))
    await rightClick('工作表1')
    await moveBy('right')
    await showsTabs(['工作表2', '工作表1', '工作表3'])
    step('UI-C55 顺序为 工作表2、1、3，名称内容对应', same(await order(), ['工作表2', '工作表1', '工作表3']) && (await contentsMatch(numbered(3))))
  })

  await section('UI-C56 右键菜单向左移动一位', async () => {
    await build(numbered(3), { active: '工作表3' })
    await rightClick('工作表3')
    await moveBy('left')
    await showsTabs(['工作表1', '工作表3', '工作表2'])
    step('UI-C56 顺序为 工作表1、3、2，名称内容对应', same(await order(), ['工作表1', '工作表3', '工作表2']) && (await contentsMatch(numbered(3))))
  })

  await section('UI-C57 右键非当前标签时的操作对象', async () => {
    await build(numbered(3))
    await rightClick('工作表3')
    await until(async () => same(await selectedTabs(), ['工作表3']))
    const arrowOn = await tab('工作表3').getByTestId('sheet-menu-arrow').count()
    step('UI-C57 右键后工作表3 成为当前工作表并显示展开箭头，工作表1 不再选中', same(await selectedTabs(), ['工作表3']) && arrowOn === 1 && (await activeSheet()) === '工作表3')
    const choices = await moveChoices()
    step('UI-C57 子菜单仅「向左移动」，以工作表3 为操作对象', choices.left && !choices.right, JSON.stringify(choices))
    await closeMenus()
  })

  await section('UI-C58 右键路径与展开箭头路径结果一致', async () => {
    await build(numbered(4), { active: '工作表2' })
    await rightClick('工作表2')
    await moveBy('right')
    await showsTabs(['工作表1', '工作表3', '工作表2', '工作表4'])
    step('UI-C58 右键向右后为 工作表1、3、2、4', same(await order(), ['工作表1', '工作表3', '工作表2', '工作表4']))
    await arrowMenu()
    await moveBy('left')
    await showsTabs(numbered(4))
    step('UI-C58 展开箭头向左后为 工作表1、2、3、4', same(await order(), numbered(4)))
  })

  await section('UI-C59 批注移动后仍跟随原单元格', async () => {
    await build(numbered(2))
    await exec('review.newNote', { cell: `${q('工作表1')}!A1`, text: '批注内容' })
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表1'])
    const note1 = await cellOf(`${q('工作表1')}!A1`)
    const note2 = await cellOf(`${q('工作表2')}!A1`)
    step('UI-C59 工作表1!A1 仍带批注，工作表2!A1 无批注', JSON.stringify(note1.comment ?? note1.note ?? '').includes('批注内容') && !JSON.stringify(note2.comment ?? note2.note ?? '').includes('批注内容'), JSON.stringify([note1.comment, note2.comment]))
  })

  await section('UI-C60 浮动对象移动后随工作表一起换位', async () => {
    await build(numbered(2))
    // A 2 x 2 PNG.
    const png = 'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAFklEQVR4nGP4z8DAwMDAxMDAwMDAAAANHQEDasKb6QAAAABJRU5ErkJggg=='
    await exec('insert.picture', { base64: png, at: 'C3', width: 80, height: 60 })
    const images = async () => Object.fromEntries((await workbook()).sheets.map((sheet) => [sheet.name, sheet.images]))
    const before_ = await images()
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表1'])
    const after_ = await images()
    step('UI-C60 图片仍在工作表1，工作表2 无图片', before_['工作表1'] === 1 && after_['工作表1'] === 1 && after_['工作表2'] === 0, JSON.stringify(after_))
  })

  await section('UI-C61 命名区域移动后引用指向不变', async () => {
    await build(numbered(2), { fill: false })
    await ai([
      { command: 'range.setValues', params: { range: `${q('工作表1')}!A1`, values: [[1], [2], [3], [4], [5], [6], [7], [8], [9], [10]] } },
      { command: 'formulas.defineName', params: { name: '测试区', refersTo: '=工作表1!$A$1:$A$10' } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!B1`, input: '=SUM(测试区)' } },
    ])
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表1'])
    const name = (await workbook()).names.find((entry) => entry.name === '测试区')
    step('UI-C61 工作表2!B1 仍为 55；「测试区」仍指向工作表1!A1:A10', (await cellOf(`${q('工作表2')}!B1`)).value === 55 && name?.refersTo === '=工作表1!$A$1:$A$10', JSON.stringify(name))
  })

  await section('UI-C62 移动后当前视图状态保持', async () => {
    await build(numbered(3), { active: '工作表2' })
    const box = await region()
    await page.mouse.click(box.x + 600, box.y + 300)
    await page.keyboard.press('ControlOrMeta+Equal')
    await page.keyboard.press('ControlOrMeta+Equal')
    await goTo('B2:C4')
    await page.waitForTimeout(300)
    const unscrolled = await signature()
    await page.mouse.move(box.x + 600, box.y + 300)
    await page.mouse.wheel(300, 0)
    await page.waitForTimeout(500)
    const zoom = await kit.getByTestId('kit-zoom').textContent()
    const view = await signature()
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    await page.waitForTimeout(400)
    step('UI-C62 工作表2 仍为当前，选区仍为 B2:C4', same(await selectedTabs(), ['工作表2']) && (await details()).selection === 'B2:C4', (await details()).selection)
    step('UI-C62 缩放与横向滚动位置不变，编辑区显示与拖拽前一致', zoom !== '100%' && view !== unscrolled && (await kit.getByTestId('kit-zoom').textContent()) === zoom && (await signature()) === view, zoom)
    // A fresh editor (100%, top-left) for the cases after this one.
    await openEditor('')
  })

  await section('UI-C63 方向项随位置实时收敛', async () => {
    await build(numbered(3))
    await arrowMenu()
    await moveBy('right')
    await showsTabs(['工作表2', '工作表1', '工作表3'])
    await arrowMenu()
    await moveBy('right')
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    await arrowMenu()
    const choices = await moveChoices()
    step('UI-C63 移至最后一位后仅呈现「向左移动」', choices.left && !choices.right, JSON.stringify(choices))
    await closeMenus()
  })

  await section('UI-C64 菜单移动后标签栏的滚动跟随', async () => {
    await build(numbered(30), { fill: false })
    let followed = true
    for (let i = 0; i < 24; i += 1) {
      await arrowMenu()
      await moveBy('right')
      await until(async () => (await order()).indexOf('工作表1') === i + 1)
      await page.waitForTimeout(80)
      followed &&= await inView('工作表1')
    }
    step('UI-C64 工作表1 移出初始可视范围时标签栏滚动，使其始终可见', followed && (await scrollLeft()) > 0)
    step('UI-C64 移动后当前工作表仍为工作表1', same(await selectedTabs(), ['工作表1']))
  })

  // ———————————————————————————— 3. Errors ————————————————————————————
  await section('UI-C66 多工作表大数据量下的切换性能', async () => {
    await build(numbered(50), { fill: false })
    const values = Array.from({ length: 2000 }, (_, row) => Array.from({ length: 100 }, (_, col) => row * 100 + col))
    await exec('range.setValues', { range: `${q('工作表50')}!A1`, values })
    const times = []
    for (const name of ['工作表50', '工作表2', '工作表50', '工作表10']) {
      const start = Date.now()
      if (!(await inView(name))) {
        await listRows()
        await kit.getByTestId(`sheet-list-item-${name}`).click()
      } else {
        await tab(name).click()
      }
      await until(async () => (await activeSheet()) === name && same(await selectedTabs(), [name]), 50)
      times.push(Date.now() - start)
    }
    step('UI-C66 每次切换 1 秒内完成', times.every((ms) => ms < 1000), JSON.stringify(times))
    // UI-C72: the 200,000-cell sheet shows its content, nothing missing.
    await listRows()
    await kit.getByTestId('sheet-list-item-工作表50').click()
    await until(async () => (await activeSheet()) === '工作表50')
    const corner = await cellOf('CV2000')
    step('UI-C72 20 万单元格工作表切入后完整显示', (await workbook()).sheets[49].cells === 200000 && corner.value === 199999 && (await shownA1()) === 0)
    // UI-C84: clicking away before it has been drawn ends on the sheet clicked last.
    await listRows()
    await kit.getByTestId('sheet-list-item-工作表1').click()
    await listRows()
    await kit.getByTestId('sheet-list-item-工作表50').click()
    await listRows()
    await kit.getByTestId('sheet-list-item-工作表1').click()
    await until(async () => (await activeSheet()) === '工作表1')
    await page.waitForTimeout(300)
    step('UI-C84 加载中再切换：工作表1 唯一选中，编辑区为工作表1', same(await selectedTabs(), ['工作表1']) && (await activeSheet()) === '工作表1')
  })

  await section('UI-C67 超长名称的显示与切换', async () => {
    const long = '一二三四五六七八九十一二三四五六七八九十一二三四五六七八九十A'
    await build(['工作表1', long])
    const title = await tab(long).getAttribute('title')
    await choose(long)
    step('UI-C67 31 字符名称按规则显示（完整名可见于提示），单击后正常切换', long.length === 31 && title === long && (await shownA1()) === `${long}内容`)
    // UI-C75: dragging it to the front keeps the name whole.
    await drag(long, await before('工作表1'))
    await showsTabs([long, '工作表1'])
    step('UI-C75 超长名称拖至第一位，名称完整不变', same(await order(), [long, '工作表1']))
  })

  await section('UI-C68 纯数字与含空格名称', async () => {
    await build(['工作表1', '2026', 'Q1 Sales'])
    await choose('2026')
    const a = (await tab('2026').textContent()) === '2026' && (await shownA1()) === '2026内容'
    await choose('Q1 Sales')
    const b = (await tab('Q1 Sales').textContent()) === 'Q1 Sales' && (await shownA1()) === 'Q1 Sales内容'
    step('UI-C68 均可切换，数字与空格按原样显示', a && b)
  })

  await section('UI-C70 锁定（保护）工作表切换后的可编辑范围', async () => {
    await build(numbered(2))
    await ai([
      { command: 'sheet.activate', params: { sheet: '工作表2' } },
      { command: 'home.formatCells', params: { range: 'B1', style: { protection: { locked: false, hidden: false } } } },
      { command: 'review.protectSheet', params: {} },
      { command: 'sheet.activate', params: { sheet: '工作表1' } },
    ])
    await until(async () => (await kit.getByTestId('sheet-lock-工作表2').count()) === 1)
    step('UI-C70 工作表2 标签显示锁定标记', (await kit.getByTestId('sheet-lock-工作表2').count()) === 1 && (await kit.getByTestId('sheet-lock-工作表1').count()) === 0)
    await choose('工作表2')
    await clickCell(0, 0)
    await page.keyboard.type('改')
    await page.keyboard.press('Enter')
    await page.waitForTimeout(300)
    await clickCell(1, 0)
    await page.keyboard.type('可改')
    await page.keyboard.press('Enter')
    await until(async () => (await cellOf(`${q('工作表2')}!B1`)).value === '可改', 20)
    step('UI-C70 可切入；锁定范围不可编辑、未锁定范围可编辑', (await cellOf(`${q('工作表2')}!A1`)).value === '工作表2内容' && (await cellOf(`${q('工作表2')}!B1`)).value === '可改')
    await exec('review.unprotectSheet', {})
  })

  await section('UI-C73 仅 1 张工作表时的拖拽', async () => {
    await build(['工作表1'])
    const box = await tab('工作表1').boundingBox()
    await drag('工作表1', { x: box.x + 200, y: box.y + box.height / 2 })
    await page.waitForTimeout(300)
    step('UI-C73 标签不位移，顺序与内容不变，无报错', same(await order(), ['工作表1']) && (await shownA1()) === '工作表1内容' && same(await tab('工作表1').boundingBox(), box))
  })

  await section('UI-C74 30 张工作表拖拽时的标签栏滚动', async () => {
    await build(numbered(30), { fill: false })
    const box = await strip().boundingBox()
    await drag('工作表1', { x: box.x + box.width - 8, y: box.y + box.height / 2 }, { release: false, hold: 800 })
    const first = await scrollLeft()
    await page.waitForTimeout(800)
    const second = await scrollLeft()
    // Out of the scrolling band, the drop mark holds still: it is where the tab lands.
    await page.mouse.move(box.x + box.width - 80, box.y + box.height / 2, { steps: 4 })
    await page.waitForTimeout(200)
    const slot = Number(await kit.getByTestId('sheet-drop-mark').getAttribute('data-slot').catch(() => 'NaN'))
    step('UI-C74 拖至右端时标签栏持续横向滚动', first > 0 && second > first)
    await page.mouse.up()
    await until(async () => (await order()).indexOf('工作表1') > 0)
    step('UI-C74 松手后落位与落点一致', (await order()).indexOf('工作表1') === slot - 1, `slot ${slot}, at ${(await order()).indexOf('工作表1')}`)
  })

  await section('UI-C80 拖至「新建工作表」按钮上松手', async () => {
    await build(numbered(3))
    const add = await kit.getByTestId('sheet-add').boundingBox()
    await drag('工作表1', { x: add.x + add.width / 2, y: add.y + add.height / 2 })
    await page.waitForTimeout(400)
    step('UI-C80 不新建工作表，顺序保持 工作表1、2、3', same(await order(), numbered(3)))
  })

  await section('UI-C81 拖动中按 ESC', async () => {
    await build(numbered(3))
    await drag('工作表1', await end(), { release: false })
    await page.keyboard.press('Escape')
    await page.waitForTimeout(150)
    const ghost = await kit.getByTestId('sheet-drag-ghost').count()
    await page.mouse.up()
    await page.waitForTimeout(300)
    step('UI-C81 拖拽取消，工作表1 回到原位，顺序不变', ghost === 0 && same(await order(), numbered(3)) && same(await tabNames(), numbered(3)))
  })

  await section('UI-C82 指针移出浏览器窗口后松手', async () => {
    await build(numbered(3))
    const target = await end()
    await drag('工作表1', target, { release: false })
    await page.mouse.move(target.x, -40, { steps: 6 })
    await page.mouse.up()
    // The release outside the window: the next movement over the page has no button down.
    await page.mouse.move(400, 400)
    await page.waitForTimeout(300)
    step('UI-C82 拖拽终止，顺序保持 工作表1、2、3，无报错', same(await order(), numbered(3)) && (await kit.getByTestId('sheet-drag-ghost').count()) === 0)
  })

  await section('UI-C83 拖动中窗口失去焦点', async () => {
    await build(numbered(3))
    await drag('工作表1', await end(), { release: false })
    await page.frames().find((frame) => frame.name() === 'kit' || frame.url().includes('index.html'))?.evaluate(() => window.dispatchEvent(new Event('blur')))
    await page.waitForTimeout(200)
    const ghost = await kit.getByTestId('sheet-drag-ghost').count()
    await page.mouse.up()
    await page.waitForTimeout(300)
    step('UI-C83 拖拽结束，顺序保持，无残留浮起样式', ghost === 0 && same(await order(), numbered(3)) && (await tab('工作表1').getAttribute('data-dragging')) == null)
  })

  // ———————————————————————————— 4. Interactions ————————————————————————————
  await section('UI-C86 单元格输入未回车时切换', async () => {
    await build(numbered(2), { fill: false })
    await clickCell(0, 0)
    await page.keyboard.type('测试')
    await page.waitForTimeout(200)
    await choose('工作表2')
    step('UI-C86 切换成功，工作表2!A1 为空', (await activeSheet()) === '工作表2' && (await cellOf('A1')).value == null)
    step('UI-C86 输入内容已提交到工作表1!A1', (await cellOf(`${q('工作表1')}!A1`)).value === '测试')
    await choose('工作表1')
    step('UI-C86 切回工作表1 时 A1 显示「测试」', (await shownA1()) === '测试' && (await formulaBar()) === '测试')
  })

  await section('UI-C87 公式编辑中切换', async () => {
    await build(numbered(2))
    await clickCell(0, 2)
    await page.keyboard.type('=SUM(B1:B3')
    await page.waitForTimeout(200)
    await choose('工作表2')
    const left = (await cellOf(`${q('工作表1')}!A3`)).formula ?? (await cellOf(`${q('工作表1')}!A3`)).input ?? ''
    step('UI-C87 单元格不产生半成品内容（未写入或补全提交）', left === '' || left === '=SUM(B1:B3)', JSON.stringify(left))
    await until(async () => (await nameBox()) === 'A1')
    step('UI-C87 切换至工作表2，名称框与 fx 栏随工作表2', (await activeSheet()) === '工作表2' && (await nameBox()) === 'A1' && (await formulaBar()) === '工作表2内容', `${await nameBox()} ${await formulaBar()}`)
  })

  await section('UI-C91 撤销跨工作表的作用对象', async () => {
    await build(numbered(2))
    await clickCell(0, 0)
    await page.keyboard.type('新内容')
    await page.keyboard.press('Enter')
    await until(async () => (await cellOf(`${q('工作表1')}!A1`)).value === '新内容')
    await choose('工作表2')
    await kit.getByTestId('sheet-undo').click()
    await until(async () => (await cellOf(`${q('工作表1')}!A1`)).value === '工作表1内容')
    step('UI-C91 撤销作用于工作表1!A1 的编辑', (await cellOf(`${q('工作表1')}!A1`)).value === '工作表1内容' && (await order()).length === 2)
  })

  await section('UI-C92 跨表引用公式切换后的取值', async () => {
    await build(numbered(2), { fill: false })
    await ai([
      { command: 'cell.set', params: { cell: `${q('工作表1')}!A1`, input: '10' } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!B1`, input: '=工作表1!A1' } },
    ])
    await clickCell(0, 0)
    await page.keyboard.type('20')
    await page.keyboard.press('Enter')
    await choose('工作表2')
    step('UI-C92 工作表2!B1 显示 20', (await cellOf('B1')).value === 20)
    await choose('工作表1')
    step('UI-C92 切回工作表1 后 A1 仍为 20', (await shownA1()) === 20)
  })

  // Iterative calculation itself is checked by the engine's tests (task F1); this case's own point
  // is that switching recalculates nothing.
  await section('UI-C93 迭代计算：切换不触发额外重算', async () => {
    await build(numbered(2), { fill: false })
    await ai([
      { command: 'formulas.calculationOptions', params: { mode: 'automatic', iterative: true, maxIterations: 50 } },
      { command: 'cell.set', params: { cell: `${q('工作表1')}!A1`, input: '=A1+1' } },
    ])
    const first = (await cellOf(`${q('工作表1')}!A1`)).value
    const seen = []
    for (const name of ['工作表2', '工作表1', '工作表2', '工作表1']) {
      await choose(name)
      seen.push((await cellOf(`${q('工作表1')}!A1`)).value)
    }
    step('UI-C93 来回切换不触发额外重算，循环引用的结果前后一致', first != null && seen.every((value) => same(value, first)), JSON.stringify([first, ...seen]))
    await exec('formulas.calculationOptions', { mode: 'automatic', iterative: false })
  })

  await section('UI-C95 撤销对拖拽移动的作用', async () => {
    await build(numbered(3))
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    await kit.getByTestId('sheet-undo').click()
    await showsTabs(numbered(3))
    step('UI-C95 撤销后顺序恢复为 工作表1、2、3，内容不受影响', same(await order(), numbered(3)) && (await contentsMatch(numbered(3))))
  })

  await section('UI-C96 移动后跨表引用公式的重算', async () => {
    await build(numbered(2), { fill: false })
    await ai([
      { command: 'cell.set', params: { cell: `${q('工作表1')}!A1`, input: '10' } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!B1`, input: '=工作表1!A1' } },
    ])
    await drag('工作表2', await before('工作表1'))
    await showsTabs(['工作表2', '工作表1'])
    await exec('cell.set', { cell: `${q('工作表1')}!A1`, input: '20' })
    step('UI-C96 工作表2!B1 显示 20', (await cellOf(`${q('工作表2')}!B1`)).value === 20)
  })

  await section('UI-C101 拖拽换位后关闭并重新打开', async () => {
    await build(numbered(3))
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    await saveAndReopen()
    step('UI-C101 重新打开后顺序仍为 工作表2、3、1', same(await tabNames(), ['工作表2', '工作表3', '工作表1']))
  })

  await section('UI-C111 拖拽后刷新页面', async () => {
    await build(numbered(3))
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    await saveAndReopen()
    step('UI-C111 重新加载后顺序为 工作表2、3、1', same(await tabNames(), ['工作表2', '工作表3', '工作表1']))
  })

  await section('UI-C102 隐藏的工作表在标签栏与列表中的表现', async () => {
    await build(numbered(4), { active: '工作表4' })
    await arrowMenu()
    await kit.getByTestId('sheet-menu-hide').click()
    await showsTabs(numbered(3))
    step('UI-C102 标签栏为 工作表1、2、3，「工作表4」不再呈现', same(await tabNames(), numbered(3)))
    const rows = await listRows()
    await closeMenus()
    step('UI-C102 列表仍列出 4 张，「工作表4」灰色带隐藏标记', same(rows.map((row) => row.name), numbered(4)) && rows[3].disabled && rows[3].text.includes('已隐藏') && !rows[0].disabled, JSON.stringify(rows))
  })

  await section('UI-C103 隐藏的工作表不可拖拽', async () => {
    await build(numbered(4), { hidden: ['工作表4'] })
    step('UI-C103 被隐藏的工作表4 没有可拖拽的标签', (await tab('工作表4').count()) === 0)
    await drag('工作表1', await end())
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    const rows = await listRows()
    await closeMenus()
    step('UI-C103 松手后为 工作表2、3、1，列表中工作表4 仍灰色隐藏', same(await tabNames(), ['工作表2', '工作表3', '工作表1']) && rows.find((row) => row.name === '工作表4')?.disabled === true)
  })

  await section('UI-C104 可见标签顺序与列表行序一致', async () => {
    await build(numbered(4), { hidden: ['工作表4'] })
    await drag('工作表3', await before('工作表1'))
    await showsTabs(['工作表3', '工作表1', '工作表2'])
    const rows = await listRows()
    await closeMenus()
    step('UI-C104 标签栏为 工作表3、1、2；列表为 工作表3、1、2、4', same(await tabNames(), ['工作表3', '工作表1', '工作表2']) && same(rows.map((row) => row.name), ['工作表3', '工作表1', '工作表2', '工作表4']) && same(await order(), ['工作表3', '工作表1', '工作表2', '工作表4']))
  })

  await section('UI-C105 刷新后停留在刷新前的工作表', async () => {
    await build(numbered(3))
    await choose('工作表2')
    await touch()
    await saveAndReopen()
    step('UI-C105 重新加载后「工作表2」唯一选中，编辑区为工作表2', same(await selectedTabs(), ['工作表2']) && (await shownA1()) === '工作表2内容')
  })

  await section('UI-C106 关闭重开后停留在最后访问的工作表', async () => {
    await build(numbered(3))
    await choose('工作表3')
    await touch()
    await saveAndReopen()
    step('UI-C106 重新打开后「工作表3」唯一选中，编辑区为工作表3', same(await selectedTabs(), ['工作表3']) && (await shownA1()) === '工作表3内容')
  })

  await section('UI-C107 操作菜单打开时切换工作表', async () => {
    await build(numbered(3))
    await arrowMenu()
    await tab('工作表3').click()
    await until(async () => (await activeSheet()) === '工作表3')
    step('UI-C107 菜单关闭；「工作表3」唯一选中，编辑区为工作表3', (await kit.getByTestId('sheet-menu').count()) === 0 && same(await selectedTabs(), ['工作表3']) && (await shownA1()) === '工作表3内容')
  })

  await section('UI-C109 重命名中单击其他标签', async () => {
    await build(numbered(3))
    await tab('工作表1').dblclick()
    await kit.getByTestId('sheet-rename').fill('测试表')
    await tab('工作表3').click()
    await showsTabs(['测试表', '工作表2', '工作表3'])
    await until(async () => (await activeSheet()) === '工作表3')
    step('UI-C109 退出重命名，输入框不再呈现', (await kit.getByTestId('sheet-rename').count()) === 0)
    step('UI-C109 「工作表1」的名称为「测试表」', same(await tabNames(), ['测试表', '工作表2', '工作表3']))
    step('UI-C109 「工作表3」唯一选中，编辑区为工作表3', same(await selectedTabs(), ['工作表3']) && (await shownA1()) === '工作表3内容')
  })

  await section('UI-C110 重命名中按 ESC', async () => {
    await build(numbered(3))
    await tab('工作表1').dblclick()
    await kit.getByTestId('sheet-rename').fill('测试表')
    await kit.getByTestId('sheet-rename').press('Escape')
    await page.waitForTimeout(300)
    step('UI-C110 退出重命名，名称仍为「工作表1」', (await kit.getByTestId('sheet-rename').count()) === 0 && same(await tabNames(), numbered(3)))
  })

  await section('UI-C112 重命名中拖拽该标签', async () => {
    await build(numbered(3))
    await tab('工作表1').dblclick()
    await kit.getByTestId('sheet-rename').waitFor()
    const field = await kit.getByTestId('sheet-rename').boundingBox()
    const target = await end()
    await page.mouse.move(field.x + field.width / 2, field.y + field.height / 2)
    await page.mouse.down()
    await page.mouse.move(target.x, target.y, { steps: 16 })
    await page.mouse.up()
    await showsTabs(['工作表2', '工作表3', '工作表1'])
    step('UI-C112 退出重命名，输入框不再呈现', (await kit.getByTestId('sheet-rename').count()) === 0)
    step('UI-C112 顺序为 工作表2、3、1，「工作表1」唯一选中', same(await tabNames(), ['工作表2', '工作表3', '工作表1']) && same(await selectedTabs(), ['工作表1']))
  })

  // Renaming: references on other sheets follow the new name (quoted where a formula needs it).
  await section('UI-C38 重命名后跨表引用随名称更新', async () => {
    await build(numbered(2), { fill: false })
    await ai([
      { command: 'cell.set', params: { cell: `${q('工作表1')}!A1`, input: '10' } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!B1`, input: '=工作表1!A1*2' } },
    ])
    await tab('工作表1').dblclick()
    await kit.getByTestId('sheet-rename').fill('2026 销售')
    await kit.getByTestId('sheet-rename').press('Enter')
    await showsTabs(['2026 销售', '工作表2'])
    const b1 = await cellOf(`${q('工作表2')}!B1`)
    step('UI-C38 重命名为「2026 销售」后 工作表2!B1 为 =\'2026 销售\'!A1*2，值仍为 20', b1.formula === "='2026 销售'!A1*2" && b1.value === 20, JSON.stringify(b1.formula))
  })

  // ———————————————————————————— 5. Import and export ————————————————————————————
  await section('UI-C115 导出件的工作表数量、名称、顺序与当前表', async () => {
    await build(numbered(3), { active: '工作表3' })
    await touch()
    await kit.getByTestId('sheet-save').click()
    await page.waitForTimeout(800)
    const saved = await savedWorkbook()
    step('UI-C115 导出件 3 张，名称与顺序一致，打开停在工作表3', same(saved?.sheets.map((sheet) => sheet.name), numbered(3)) && saved?.activeTab === 2, JSON.stringify(saved))
  })

  await section('UI-C117 菜单移动后导出件的工作表顺序', async () => {
    await build(numbered(3))
    await arrowMenu()
    await moveBy('right')
    await showsTabs(['工作表2', '工作表1', '工作表3'])
    await kit.getByTestId('sheet-save').click()
    await page.waitForTimeout(800)
    const saved = await savedWorkbook()
    step('UI-C117 导出件顺序为 工作表2、1、3', same(saved?.sheets.map((sheet) => sheet.name), ['工作表2', '工作表1', '工作表3']), JSON.stringify(saved))
  })

  // An imported workbook (an .xlsx opened in the editor): the cases' second set-up.
  const quarters = ['一季度', '二季度', '三季度', '四季度']
  await section('UI-C114 导入含多工作表 .xlsx 的逐张切换', async () => {
    await build(quarters)
    await ai([
      { command: 'sheet.tabColor', params: { sheet: '一季度', color: '#FF0000' } },
      { command: 'sheet.tabColor', params: { sheet: '三季度', color: '#00B050' } },
      { command: 'home.bold', params: { range: `${q('二季度')}!A1` } },
    ])
    await saveAndReopen('季度数据.xlsx')
    let same_ = true
    for (const name of quarters) {
      await choose(name)
      same_ &&= (await shownA1()) === `${name}内容` && (await cellOf('B2')).value === (quarters.indexOf(name) + 1) * 10
    }
    step('UI-C114 各表存在，逐张切换内容一致', same(await tabNames(), quarters) && same_)
    step('UI-C114 标签颜色与源文件一致', (await tab('一季度').getAttribute('data-color')) === '#FF0000' && (await tab('三季度').getAttribute('data-color')) === '#00B050' && (await tab('二季度').getAttribute('data-color')) == null)
    step('UI-C114 格式与源文件一致', (await cellOf(`${q('二季度')}!A1`)).style?.font?.bold === true)
  })

  await section('UI-C5 导入表格：单击切换', async () => {
    await choose('一季度')
    await choose('二季度')
    step('UI-C5（导入）单击「二季度」后仅它选中，编辑区为二季度', same(await selectedTabs(), ['二季度']) && (await shownA1()) === '二季度内容')
  })

  await section('UI-C118 导入件拖拽移动工作表', async () => {
    await choose('一季度')
    await drag('一季度', await end())
    await showsTabs(['二季度', '三季度', '四季度', '一季度'])
    let same_ = true
    for (const name of quarters) same_ &&= (await cellOf(`${q(name)}!A1`)).value === `${name}内容`
    step('UI-C118 顺序为 二、三、四、一季度，数据与格式不错位', same(await order(), ['二季度', '三季度', '四季度', '一季度']) && same_ && (await cellOf(`${q('二季度')}!A1`)).style?.font?.bold === true)
  })

  await section('UI-C119 导入件经菜单移动工作表', async () => {
    await saveAndReopen('季度数据.xlsx')
    await ai([
      { command: 'sheet.move', params: { sheet: '一季度', to: 0 } },
      { command: 'sheet.activate', params: { sheet: '一季度' } },
    ])
    await showsTabs(quarters)
    await arrowMenu()
    await moveBy('right')
    await showsTabs(['二季度', '一季度', '三季度', '四季度'])
    step('UI-C119 顺序为 二、一、三、四季度，4 张且名称内容对应', same(await order(), ['二季度', '一季度', '三季度', '四季度']) && (await order()).length === 4)
  })

  await section('UI-C120 导入件源文件含隐藏表', async () => {
    await build(quarters, { hidden: ['四季度'] })
    await touch()
    await saveAndReopen('季度数据-隐藏.xlsx')
    step('UI-C120 标签栏为 一、二、三季度，「四季度」不呈现', same(await tabNames(), ['一季度', '二季度', '三季度']))
    const rows = await listRows()
    await closeMenus()
    step('UI-C120 列表仍列出 4 张，「四季度」灰色带隐藏标记', same(rows.map((row) => row.name), quarters) && rows[3].disabled && rows[3].text.includes('已隐藏'))
  })

  await section('UI-C28 导入表格：拖拽至相邻位置', async () => {
    await ai([{ command: 'sheet.activate', params: { sheet: '一季度' } }])
    await drag('一季度', await after('二季度'))
    await showsTabs(['二季度', '一季度', '三季度'])
    step('UI-C28（导入）顺序为 二季度、一季度、三季度，隐藏的四季度保持最后', same(await order(), ['二季度', '一季度', '三季度', '四季度']))
  })

  // ———————————————————————————— View only (viewer) ————————————————————————————
  await section('UI-C69 仅可查看权限', async () => {
    await build(numbered(3))
    await touch()
    await kit.getByTestId('sheet-save').click()
    await until(async () => (await page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, h.docPath())) > 1)
    const bytes = await page.evaluate((path) => [...new Uint8Array(window.devHost.files.get(path).bytes)], h.docPath())
    await openEditor('&role=viewer')
    await page.evaluate(async (bytes) => {
      document.getElementById('status').textContent = ''
      await window.devHost.open('Viewer.xlsx', new Uint8Array(bytes).buffer)
    }, bytes)
    h.setDocPath('Viewer.xlsx')
    await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
    await showsTabs(numbered(3))
    const seen = []
    for (const name of ['工作表2', '工作表3', '工作表1']) seen.push([name, await choose(name), await shownA1()])
    const switched = seen.every(([name, chosen, a1]) => chosen && a1 === `${name}内容`)
    step('UI-C69 可正常切换查看各表；编辑类入口不可用', switched && (await kit.getByTestId('sheet-add').isDisabled()), JSON.stringify(seen))

    await drag('工作表1', await end())
    await page.waitForTimeout(400)
    step('UI-C76 只读下拖拽：标签不位移，顺序不变', same(await tabNames(), numbered(3)) && same(await order(), numbered(3)))

    await arrowMenu()
    const moveDisabled = await kit.getByTestId('sheet-menu-move').isDisabled()
    await kit.getByTestId('sheet-menu-move').hover()
    await page.waitForTimeout(200)
    const directions = (await kit.getByTestId('sheet-menu-move-left').count()) + (await kit.getByTestId('sheet-menu-move-right').count())
    await closeMenus()
    step('UI-C78 只读下「移动工作表」禁用，方向项不可点，顺序不变', moveDisabled && directions === 0 && same(await order(), numbered(3)))

    await tab('工作表2').click({ button: 'right' })
    await page.waitForTimeout(300)
    step('UI-C79 只读下右键不弹菜单，标签顺序、名称与选中不变', (await kit.getByTestId('sheet-menu').count()) === 0 && same(await tabNames(), numbered(3)) && same(await selectedTabs(), ['工作表1']))
  })
} catch (error) {
  await h.finish(error)
}
await h.finish()
