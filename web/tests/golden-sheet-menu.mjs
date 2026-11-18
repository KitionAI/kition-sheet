// The sheet menu (right-click a tab, or the active tab's arrow) against the golden sheet tab menu
// cases (module M2), in a real browser through the development host:
// node tests/golden-sheet-menu.mjs [chromium|webkit]   (GOLDEN_ONLY=C12,C46 runs only those cases)
// Each step names its case by the row of the case table (MENU-C12 is row 12). Where the case table
// and WPS for Mac differ, the steps follow WPS (docs/parity/ui-parity.md, the M2 decisions): a copy is
// named `工作表3(2)`, `工作表3(3)` (the next number free), a new sheet `Sheet4` after the highest
// SheetN, and a comma is a valid character in a sheet name. Prints PASS or FAIL per step.
import { startGolden } from './golden-harness.mjs'

const h = await startGolden({ prefix: 'MENU-' })
const {
  page, kit, errors, step, section, until, same, ai, exec, workbook, order, activeSheet, q, cellOf,
  tab, strip, tabNames, selectedTabs, nameBox, formulaBar, region, clickCell, goTo, choose, showsTabs, build, numbered, shownA1,
  drag, end, arrowMenu, rightClick, menuRows, moveChoices, moveBy, closeMenus, inView, saveAndReopen, touch, savedWorkbook, zipPart,
  unescapeXml, pixel, openEditor,
} = h

// ———————————————————————————— helpers ————————————————————————————

const sheets = (count) => Array.from({ length: count }, (_, index) => `Sheet${index + 1}`)
const field = () => kit.getByTestId('sheet-rename')
const renaming = async () => (await field().count()) === 1
const menuShown = async () => (await kit.getByTestId('sheet-menu').count()) === 1
const dialogShown = async () => (await kit.getByTestId('sheet-delete-dialog').count()) === 1
const anyDialog = async () => (await kit.locator('[role="dialog"]').count()) > 0
/** Points at a submenu's row and waits for it to open. */
const sub = async (testId) => {
  await kit.getByTestId(testId).hover()
  await page.waitForTimeout(200)
}
/** The rows of the open submenu, top to bottom. */
const subRows = () => kit.getByTestId('sheet-menu').locator('.kui-submenu-popover [role^="menuitem"]').evaluateAll((rows) => rows.map((row) => row.textContent.trim()))
const hasSubmenu = (testId) => kit.getByTestId(testId).getAttribute('aria-haspopup').then((value) => value === 'menu')
const enabledRow = (testId) => kit.getByTestId(testId).isEnabled()
/** Opens the sheet menu on `name` (right-click, or the active tab's arrow after choosing it). */
const menuOn = async (name, { arrow = false } = {}) => {
  if (arrow) {
    await choose(name)
    await arrowMenu()
  } else {
    await rightClick(name)
  }
}

/** Create a Copy: the copy's name opens to edit. */
const startCopy = async (name, options) => {
  await menuOn(name, options)
  await kit.getByTestId('sheet-menu-copy').click()
  await field().waitFor()
}
/** Enter in the rename field: it closes. */
const enter = async () => {
  await field().press('Enter')
  await until(async () => !(await renaming()))
}
/** Copies `name` keeping the name it gets (Enter), and answers that name. */
const copyOf = async (name, options) => {
  await startCopy(name, options)
  const copy = await field().inputValue()
  await enter()
  await until(async () => (await tabNames()).includes(copy) || (await order()).includes(copy))
  return copy
}

const startRename = async (name, { arrow = false, double = false } = {}) => {
  if (double) await tab(name).dblclick()
  else {
    await menuOn(name, { arrow })
    await kit.getByTestId('sheet-menu-rename').click()
  }
  await field().waitFor()
  await page.waitForTimeout(80)
}
const renameTo = async (name, text) => {
  await startRename(name)
  await field().fill(text)
  await enter()
  await until(async () => (await order()).includes(text.trim()) && (await tabNames()).includes(text.trim()))
}
/** The rename field's selection: [start, end, length of its text]. */
const fieldSelection = () => field().evaluate((input) => [input.selectionStart, input.selectionEnd, input.value.length])
/** Whether a notice saying `text` shows. */
const notice = (text) => until(async () => (await kit.locator('.kui-toast').allTextContents()).some((shown) => shown.includes(text)), 30)
const notices = () => kit.locator('.kui-toast').allTextContents()
const clearNotices = async () => {
  for (const close of await kit.locator('.kui-toast-close').all()) await close.click().catch(() => undefined)
}

const insertAt = async (name, where, options) => {
  await menuOn(name, options)
  await sub('sheet-menu-insert')
  await kit.getByTestId(`sheet-menu-insert-${where}`).click()
}

/** The colour swatches of the open Tab Color panel (50: ten columns of five). */
const swatches = () => kit.getByTestId('sheet-menu-colors').locator('button')
const openColors = async (name, options) => {
  await menuOn(name, options)
  await sub('sheet-menu-color')
}
/** Picks a tab colour for `name`: a swatch's index, or `none` (No Fill). Answers the colour picked. */
const colorTab = async (name, index, options) => {
  await openColors(name, options)
  if (index === 'none') {
    await kit.getByTestId('sheet-menu-no-color').click()
    return null
  }
  const color = await swatches().nth(index).getAttribute('title')
  await swatches().nth(index).click()
  await until(async () => (await colorOf(name)) === color)
  return color
}
const colorOf = (name) => tab(name).getAttribute('data-color')
/** The colour of a tab's foot band (`::after`) and its size. */
const band = (name) =>
  tab(name).evaluate((element) => {
    const style = getComputedStyle(element, '::after')
    return { color: style.backgroundColor, height: parseFloat(style.height), width: element.getBoundingClientRect().width, content: style.content }
  })
const rgb = (hex) => `rgb(${parseInt(hex.slice(1, 3), 16)}, ${parseInt(hex.slice(3, 5), 16)}, ${parseInt(hex.slice(5, 7), 16)})`

const hideSheet = async (name, options) => {
  await menuOn(name, options)
  await kit.getByTestId('sheet-menu-hide').click()
}
/** Delete in the sheet menu: its confirmation opens. */
const askDelete = async (name, options) => {
  await menuOn(name, options)
  await kit.getByTestId('sheet-menu-delete').click()
  await kit.getByTestId('sheet-delete-dialog').waitFor()
}
const deleteSheet = async (name, options) => {
  await askDelete(name, options)
  await kit.getByTestId('sheet-delete-confirm').click()
  await until(async () => !(await dialogShown()) && !(await order()).includes(name))
}
/** Ctrl+Z (Cmd+Z on a Mac) where the keyboard is. */
const undoKey = async () => {
  await page.keyboard.press('ControlOrMeta+z')
  await page.waitForTimeout(300)
}
const undoButton = async () => {
  await kit.getByTestId('sheet-undo').click()
  await page.waitForTimeout(300)
}

/** The sheet list's rows: name, checked, hidden (greyed), the colour bar's colour, and the Unhide button's tooltip. */
const openList = async () => {
  await kit.getByTestId('sheet-list-button').click()
  await kit.getByTestId('sheet-list').waitFor()
}
const listShown = () =>
  kit
    .getByTestId('sheet-list')
    .locator('.sheet-list-row')
    .evaluateAll((rows) =>
      rows.map((row) => {
        const item = row.querySelector('[role^="menuitem"]')
        const unhide = row.querySelector('.sheet-list-unhide')
        return {
          name: item.getAttribute('data-testid').slice('sheet-list-item-'.length),
          checked: item.getAttribute('aria-checked') === 'true',
          hidden: item.disabled,
          color: row.querySelector('.sheet-list-color')?.getAttribute('data-color') ?? null,
          unhide: unhide ? unhide.getAttribute('title') : null,
        }
      }),
    )
const listInfo = async () => {
  await openList()
  return listShown()
}

/** A new workbook's sheets, saved and opened again (an imported .xlsx). */
const imported = async (names, options = {}) => {
  await build(names, options)
  await touch()
  await saveAndReopen()
  await showsTabs(names.filter((name) => !(options.hidden ?? []).includes(name)))
}

/** The workbook as its JSON (rows, columns, and the rest the model keeps), for what no query reads. */
const model = async () => {
  const { base64 } = await exec('file.saveBytes', { format: 'json' })
  return page.evaluate((text) => JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(text), (c) => c.charCodeAt(0)))), base64)
}
const sheetModel = async (name) => (await model()).sheets.find((sheet) => sheet.name === name)

/** The saved .xlsx's worksheet part for sheet `name` (through the workbook's relationships). */
async function savedSheet(name) {
  const book = await zipPart('xl/workbook.xml')
  const rels = await zipPart('xl/_rels/workbook.xml.rels')
  const entry = [...book.matchAll(/<sheet\b[^>]*>/g)].map((match) => match[0]).find((tag) => unescapeXml(/\bname="([^"]*)"/.exec(tag)?.[1] ?? '') === name)
  const id = entry && /\br:id="([^"]*)"/.exec(entry)?.[1]
  const target = id && new RegExp(`<Relationship\\b[^>]*\\bId="${id}"[^>]*>`).exec(rels)?.[0]
  const path = target && /\bTarget="([^"]*)"/.exec(target)?.[1]
  return path ? zipPart(path.startsWith('/') ? path.slice(1) : `xl/${path}`) : null
}
/** A saved sheet's tab colour (`#RRGGBB`), or null. */
const savedTabColor = async (name) => {
  const xml = await savedSheet(name)
  const rgb = xml && /<tabColor\b[^>]*\brgb="([0-9A-Fa-f]{6,8})"/.exec(xml)?.[1]
  return rgb ? `#${rgb.slice(-6).toUpperCase()}` : null
}

/** Saves, and opens what was saved as a viewer (only reading), with `names` showing. */
async function openAsViewer(names) {
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
  await showsTabs(names)
}

/** Blurs the editor's window (another browser tab or window taking the focus). */
const blurWindow = () => page.frames().find((frame) => frame.name() === 'kit' || frame.url().includes('index.html'))?.evaluate(() => window.dispatchEvent(new Event('blur')))

/** Scrolls the tab strip to its start or its end. */
const scrollTabs = (to) => strip().evaluate((element, to) => {
  element.scrollLeft = to === 'start' ? 0 : element.scrollWidth
}, to)
const tabsScroll = () => strip().evaluate((element) => element.scrollLeft)

/** The 7 rows of the sheet menu (the cloud rows Merge Sheets, Lock Sheet and Download Sheet are deferred). */
const MENU = ['新建工作表', '重命名', '移动工作表', '创建副本', '工作表标签', '隐藏工作表', '删除']
const MENU_IDS = ['sheet-menu-insert', 'sheet-menu-rename', 'sheet-menu-move', 'sheet-menu-copy', 'sheet-menu-color', 'sheet-menu-hide', 'sheet-menu-delete']
/** A 2 x 2 PNG. */
const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAFklEQVR4nGP4z8DAwMDAxMDAwMDAAAANHQEDasKb6QAAAABJRU5ErkJggg=='
const RED = 41
const YELLOW = 43
const GREEN = 45
const BLUE = 47
const PURPLE = 49

try {
  await openEditor('')

  // ———————————————————————————— 1. The menu items and Create a Copy ————————————————————————————
  await section('MENU-C5/C7/C46/C65/C89/C92/C112/C159/C177/C190 菜单构成与各项形态', async () => {
    await build(numbered(3))
    await rightClick('工作表2')
    const rows = await menuRows()
    const enabled = []
    for (const id of MENU_IDS) enabled.push(await enabledRow(id))
    const subs = []
    for (const id of MENU_IDS) subs.push(await hasSubmenu(id))
    step('MENU-C5 右键「工作表2」弹出菜单，自上而下为 新建工作表、重命名、移动工作表、创建副本、工作表标签、隐藏工作表、删除（合并/锁定/下载为云功能，后置）', same(rows, MENU), JSON.stringify(rows))
    step('MENU-C5 各项均可点击', enabled.every(Boolean), JSON.stringify(enabled))
    step('MENU-C159 「新建工作表」第 1 项，带子菜单箭头', rows[0] === '新建工作表' && subs[0])
    step('MENU-C65 「重命名」第 2 项，无子菜单', rows[1] === '重命名' && !subs[1])
    step('MENU-C177 「移动工作表」第 3 项，带子菜单箭头', rows[2] === '移动工作表' && subs[2])
    step('MENU-C7 「创建副本」第 4 项（移动工作表与工作表标签之间），无子菜单箭头', rows[3] === '创建副本' && !subs[3])
    step('MENU-C46 「工作表标签」在创建副本下、隐藏工作表上', rows[4] === '工作表标签' && subs[4])
    step('MENU-C190 「隐藏工作表」在工作表标签之后，无子菜单', rows[5] === '隐藏工作表' && !subs[5])
    step('MENU-C89 「删除」为最后一项，无子菜单', rows[6] === '删除' && !subs[6])
    await kit.getByTestId('sheet-menu-delete').hover()
    await page.waitForTimeout(250)
    step('MENU-C92 指向「删除」不展开子菜单，呈可点击', (await kit.getByTestId('sheet-menu').locator('.kui-submenu-popover').count()) === 0 && (await enabledRow('sheet-menu-delete')))
    step('MENU-C112 「合并工作表」为云功能，后置（菜单中不呈现）', !rows.includes('合并工作表'))
    await closeMenus()
  })

  await section('MENU-C6 右键非当前标签后副本的复制对象', async () => {
    await build(numbered(3))
    await rightClick('工作表3')
    await until(async () => same(await selectedTabs(), ['工作表3']))
    const selected = await selectedTabs()
    await kit.getByTestId('sheet-menu-copy').click()
    await field().waitFor()
    const copy = await field().inputValue()
    await enter()
    step('MENU-C6 右键后「工作表3」先成为当前工作表', same(selected, ['工作表3']), JSON.stringify(selected))
    step('MENU-C6 副本以工作表3 为复制对象，命名「工作表3(2)」', copy === '工作表3(2)' && (await cellOf(`${q('工作表3(2)')}!A1`)).value === '工作表3内容', copy)
  })

  await section('MENU-C8 创建副本-点击后即执行', async () => {
    await build(numbered(3))
    await rightClick('工作表1')
    await kit.getByTestId('sheet-menu-copy').click()
    await until(async () => (await order()).length === 4)
    step('MENU-C8 菜单关闭，工作表由 3 张增至 4 张，不弹对话框', !(await menuShown()) && (await order()).length === 4 && !(await anyDialog()))
    await enter()
  })

  await section('MENU-C9 点击菜单外区域后的关闭', async () => {
    await build(numbered(3))
    await rightClick('工作表2')
    const box = await region()
    await page.mouse.click(box.x + 400, box.y + 300)
    await until(async () => !(await menuShown()))
    step('MENU-C9 菜单关闭；仍为 3 张，内容不变', !(await menuShown()) && same(await order(), numbered(3)) && (await cellOf(`${q('工作表2')}!A1`)).value === '工作表2内容')
  })

  await section('MENU-C10/C11/C12 副本的命名（按 WPS：名称后加 (2)、(3)…）', async () => {
    await build(numbered(3), { active: '工作表3' })
    const first = await copyOf('工作表3')
    step('MENU-C10 首次副本名为「工作表3(2)」，源表名不变', first === '工作表3(2)' && (await order()).includes('工作表3'), first)
    const second = await copyOf('工作表3')
    step('MENU-C11 第二次副本名为「工作表3(3)」，已有副本名不变', second === '工作表3(3)' && (await order()).includes('工作表3(2)'), second)
    const third = await copyOf('工作表3')
    const names = await order()
    step('MENU-C12 第三次为「工作表3(4)」，无重名', third === '工作表3(4)' && new Set(names).size === names.length, third)
  })

  await section('MENU-C13 源表名含中文、英文、数字与符号', async () => {
    await build(['2026-Q1 汇总 A（终版）'])
    const copy = await copyOf('2026-Q1 汇总 A（终版）')
    step('MENU-C13 副本名为「2026-Q1 汇总 A（终版）(2)」，字符原样保留', copy === '2026-Q1 汇总 A（终版）(2)', copy)
  })

  await section('MENU-C14/C301 源表改名后再创建副本', async () => {
    await build(numbered(3), { active: '工作表3' })
    await renameTo('工作表3', '三月数据')
    const copy = await copyOf('三月数据')
    step('MENU-C14 副本名取当前名称「三月数据(2)」', copy === '三月数据(2)', copy)
    step('MENU-C301 副本插入在「三月数据」右侧', same(await order(), ['工作表1', '工作表2', '三月数据', '三月数据(2)']))
  })

  await section('MENU-C15 副本再创建副本', async () => {
    await build(['工作表1', '工作表2', '工作表3', '工作表3(2)'], { active: '工作表3(2)' })
    const copy = await copyOf('工作表3(2)')
    step('MENU-C15 副本的副本按原表名续号为「工作表3(3)」', copy === '工作表3(3)', copy)
  })

  await section('MENU-C16/C17/C18 已有副本被改名、删除或同名占用时', async () => {
    await build(['工作表1', '工作表2', '工作表3', '备份A'], { active: '工作表3' })
    const copy = await copyOf('工作表3')
    step('MENU-C16 副本被改名为「备份A」后再建：取空出的「工作表3(2)」（WPS 取最小空号）', copy === '工作表3(2)', copy)
    await build(['工作表1', '工作表2', '工作表3', '工作表3(2)'], { active: '工作表3' })
    const before = (await cellOf(`${q('工作表3(2)')}!A1`)).value
    const avoided = await copyOf('工作表3')
    const names = await order()
    step('MENU-C17 已有同名「工作表3(2)」：新副本避让为「工作表3(3)」，原表名称内容不变，无重名', avoided === '工作表3(3)' && (await cellOf(`${q('工作表3(2)')}!A1`)).value === before && new Set(names).size === names.length, avoided)
    await build(['工作表1', '工作表2', '工作表3', '备份A', '备份B', '工作表3(4)'], { active: '工作表3' })
    const next = await copyOf('工作表3')
    const all = await order()
    step('MENU-C18 已有「工作表3(4)」等：新副本取最小空号「工作表3(2)」，已有表名称内容不变，无重名（按 WPS）', next === '工作表3(2)' && all.includes('工作表3(4)') && all.includes('备份A') && all.includes('备份B') && new Set(all).size === all.length, next)
  })

  await section('MENU-C19/C20/C21 副本插入在源工作表右侧', async () => {
    await build(numbered(3), { active: '工作表2' })
    await copyOf('工作表2')
    step('MENU-C19 依次为 工作表1、工作表2、工作表2(2)、工作表3，副本唯一选中', same(await tabNames(), ['工作表1', '工作表2', '工作表2(2)', '工作表3']) && same(await selectedTabs(), ['工作表2(2)']))
    await build(numbered(3))
    await copyOf('工作表1')
    step('MENU-C20 源表为第一张：依次为 工作表1、工作表1(2)、工作表2、工作表3', same(await tabNames(), ['工作表1', '工作表1(2)', '工作表2', '工作表3']))
    await build(numbered(3))
    await copyOf('工作表3')
    step('MENU-C21 源表为最后一张：副本落在最右端', same(await tabNames(), ['工作表1', '工作表2', '工作表3', '工作表3(2)']))
  })

  await section('MENU-C22 存在隐藏工作表时的插入位置', async () => {
    await build(numbered(4), { hidden: ['工作表3'], active: '工作表2' })
    await copyOf('工作表2')
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C22 标签栏为 工作表1、工作表2、工作表2(2)、工作表4', same(await tabNames(), ['工作表1', '工作表2', '工作表2(2)', '工作表4']))
    step('MENU-C22 工作表3 仍隐藏，并在列表中占一行', rows.some((row) => row.name === '工作表3' && row.hidden) && rows.length === 5, JSON.stringify(rows.map((row) => row.name)))
  })

  await section('MENU-C23 工作表列表中副本行的位置与状态', async () => {
    await build(numbered(3), { active: '工作表2' })
    await copyOf('工作表2')
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C23 列表为 工作表1、工作表2、工作表2(2)、工作表3，副本行带勾选', same(rows.map((row) => row.name), ['工作表1', '工作表2', '工作表2(2)', '工作表3']) && same(rows.map((row) => row.checked), [false, false, true, false]), JSON.stringify(rows))
  })

  await section('MENU-C24 标签栏已排满时副本的可见性', async () => {
    await build(numbered(30), { fill: false, active: '工作表2' })
    await scrollTabs('end')
    await page.waitForTimeout(150)
    await copyOf('工作表28')
    step('MENU-C24 副本插在工作表28 右侧、选中并完整可见', (await order())[28] === '工作表28(2)' && same(await selectedTabs(), ['工作表28(2)']) && (await inView('工作表28(2)')))
    await scrollTabs('start')
    await page.waitForTimeout(150)
    const firstIn = await inView('工作表1')
    await scrollTabs('end')
    await page.waitForTimeout(150)
    step('MENU-C24 横向滚动可访问到首尾标签，标签不重叠', firstIn && (await inView('工作表30')) && (await tabNames()).length === 31)
  })

  await section('MENU-C25/C39 单元格值与文本的复制，副本与源表相互独立', async () => {
    await build(numbered(3), { fill: false })
    await exec('range.setValues', { range: `${q('工作表2')}!A1`, values: [['中文内容'], ['English'], [12345], [3.5], ['#@!&*（）']] })
    await copyOf('工作表2')
    const read = async (name) => (await exec('sheet.read', { sheet: name, range: 'A1:A5' })).values
    const source = await read('工作表2')
    const copy = await read('工作表2(2)')
    step('MENU-C25 副本 A1:A5 与源表逐格一致，源表不变', same(source, copy) && JSON.stringify(source).includes('中文内容') && JSON.stringify(source).includes('#@!&*（）'), JSON.stringify(copy))
    await exec('cell.set', { cell: `${q('工作表2(2)')}!A1`, input: '副本改动' })
    step('MENU-C39 改副本后源表 A1 不变', (await cellOf(`${q('工作表2')}!A1`)).value === '中文内容')
    await exec('cell.set', { cell: `${q('工作表2')}!A2`, input: '源表改动' })
    step('MENU-C39 改源表后副本对应单元格不变', (await cellOf(`${q('工作表2(2)')}!A2`)).value === 'English')
  })

  await section('MENU-C26/C27/C28 数字格式、单元格格式、合并与行高列宽的复制', async () => {
    await build(numbered(3), { fill: false })
    await ai([
      { command: 'cell.set', params: { cell: `${q('工作表2')}!B1`, input: '0.25' } },
      { command: 'home.numberFormat', params: { range: `${q('工作表2')}!B1`, format: '0.00%' } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!B2`, input: '3.14159' } },
      { command: 'home.numberFormat', params: { range: `${q('工作表2')}!B2`, format: '0.00' } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!B3`, input: '2026/9/29' } },
      { command: 'home.numberFormat', params: { range: `${q('工作表2')}!B3`, format: 'yyyy/m/d' } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!D1`, input: '格式' } },
      { command: 'home.formatCells', params: { range: `${q('工作表2')}!D1`, style: { font: { bold: true, color: '#FF0000' }, fill: '#FFFF00', align: { h: 'Center' }, borders: { top: { style: 'Thin', color: 'Auto' }, bottom: { style: 'Thin', color: 'Auto' }, left: { style: 'Thin', color: 'Auto' }, right: { style: 'Thin', color: 'Auto' } } } } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!F1`, input: '合并' } },
      { command: 'home.mergeCells', params: { range: `${q('工作表2')}!F1:H1` } },
      { command: 'sheet.activate', params: { sheet: '工作表2' } },
      { command: 'home.rowHeight', params: { rows: '1:1', height: 40 } },
      { command: 'home.columnWidth', params: { cols: 'F:F', chars: 25 } },
      { command: 'sheet.activate', params: { sheet: '工作表1' } },
    ])
    await copyOf('工作表2')
    const pairs = []
    for (const cell of ['B1', 'B2', 'B3', 'D1']) pairs.push([await cellOf(`${q('工作表2')}!${cell}`), await cellOf(`${q('工作表2(2)')}!${cell}`)])
    const texts = pairs.slice(0, 3).map(([, copy]) => copy.text)
    step('MENU-C26 副本 B1:B3 显示为 25.00%、3.14、2026/9/29，格式与源表一致', same(texts, ['25.00%', '3.14', '2026/9/29']) && pairs.slice(0, 3).every(([a, b]) => same(a.style, b.style)), JSON.stringify(texts))
    const [d1, d1copy] = pairs[3]
    step('MENU-C27 副本 D1 的加粗、字色、底色、居中、边框与源表一致', same(d1.style, d1copy.style) && d1copy.style?.font?.bold === true && JSON.stringify(d1copy.style?.borders ?? {}).includes('Thin'), JSON.stringify(d1copy.style).slice(0, 200))
    const [source, copy] = [await sheetModel('工作表2'), await sheetModel('工作表2(2)')]
    step('MENU-C28 副本合并区域、第 1 行行高与 F 列列宽与源表一致', same(source.merges, copy.merges) && same(source.rows, copy.rows) && same(source.cols, copy.cols) && JSON.stringify(copy.merges).length > 2, JSON.stringify([copy.merges, copy.rows, copy.cols]).slice(0, 200))
  })

  await section('MENU-C29/C30 公式与跨表引用的复制', async () => {
    await build(numbered(3), { fill: false })
    await ai([
      { command: 'cell.set', params: { cell: `${q('工作表1')}!A1`, input: '10' } },
      { command: 'range.setValues', params: { range: `${q('工作表2')}!A1`, values: [[3, 5]] } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!C1`, input: '=A1+B1' } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!D1`, input: '=工作表1!A1' } },
    ])
    await copyOf('工作表2')
    const c1 = await cellOf(`${q('工作表2(2)')}!C1`)
    step('MENU-C29 副本 C1 公式为 =A1+B1，结果 8', c1.formula === '=A1+B1' && c1.value === 8, JSON.stringify([c1.formula, c1.value]))
    await exec('cell.set', { cell: `${q('工作表2(2)')}!A1`, input: '10' })
    step('MENU-C29 副本 A1 改为 10 后 C1 重算为 15', (await cellOf(`${q('工作表2(2)')}!C1`)).value === 15)
    const d1 = await cellOf(`${q('工作表2(2)')}!D1`)
    step('MENU-C30 副本跨表公式仍为 =工作表1!A1，取值 10', d1.formula === '=工作表1!A1' && d1.value === 10, d1.formula)
    await exec('cell.set', { cell: `${q('工作表1')}!A1`, input: '20' })
    step('MENU-C30 工作表1!A1 改为 20 后副本同步为 20', (await cellOf(`${q('工作表2(2)')}!D1`)).value === 20)
  })

  await section('MENU-C31 条件格式的复制', async () => {
    await build(numbered(3), { fill: false })
    await ai([
      { command: 'range.setValues', params: { range: `${q('工作表2')}!A1`, values: [[10], [20], [30]] } },
      { command: 'sheet.activate', params: { sheet: '工作表2' } },
      { command: 'home.conditionalFormat', params: { range: 'A1:A10', rule: { type: 'cellIs', operator: 'greaterThan', value: '50', style: { fill: '#FF0000' } } } },
      { command: 'sheet.activate', params: { sheet: '工作表1' } },
    ])
    await copyOf('工作表2')
    const sheetsInfo = Object.fromEntries((await workbook()).sheets.map((sheet) => [sheet.name, sheet]))
    step('MENU-C31 副本 A1:A10 带同样的条件格式规则', sheetsInfo['工作表2(2)'].conditionalFormats === 1)
    await exec('cell.set', { cell: `${q('工作表2(2)')}!A1`, input: '60' })
    await page.waitForTimeout(400)
    const [r, g, b] = await pixel(46 + 40, 21 + 12)
    step('MENU-C31 副本 A1 改为 60 后填充红色，规则生效', r > 200 && g < 90 && b < 90, JSON.stringify([r, g, b]))
  })

  await section('MENU-C32 数据验证的复制', async () => {
    await build(numbered(3), { fill: false })
    await ai([
      { command: 'sheet.activate', params: { sheet: '工作表2' } },
      { command: 'data.validation', params: { range: 'D1:D10', type: 'list', formula1: '是,否' } },
      { command: 'sheet.activate', params: { sheet: '工作表1' } },
    ])
    await copyOf('工作表2')
    const info = Object.fromEntries((await workbook()).sheets.map((sheet) => [sheet.name, sheet]))
    const ok = await exec('data.validate', { cell: 'D1', input: '是' })
    const refused = await exec('data.validate', { cell: 'D1', input: '待定' })
    step('MENU-C32 副本 D1:D10 带序列验证「是/否」：序列内通过，「待定」被规则拒绝', info['工作表2(2)'].validations === 1 && ok?.ok === true && refused?.ok === false, JSON.stringify([ok, refused]))
  })

  await section('MENU-C33/C34 批注、图表与图片的复制', async () => {
    await build(numbered(3), { fill: false })
    await ai([
      { command: 'range.setValues', params: { range: `${q('工作表2')}!A1`, values: [['项', '值'], ['a', 3], ['b', 5]] } },
      { command: 'review.newNote', params: { cell: `${q('工作表2')}!E1`, text: '待复核' } },
      { command: 'sheet.activate', params: { sheet: '工作表2' } },
      { command: 'insert.chart', params: { range: 'A1:B3', type: 'column', at: 'G2' } },
      { command: 'insert.picture', params: { base64: PNG, at: 'C8', width: 80, height: 60 } },
      { command: 'sheet.activate', params: { sheet: '工作表1' } },
    ])
    await copyOf('工作表2')
    const e1 = await cellOf(`${q('工作表2(2)')}!E1`)
    step('MENU-C33 副本 E1 带批注「待复核」', JSON.stringify(e1.comment ?? '').includes('待复核'), JSON.stringify(e1.comment))
    const info = Object.fromEntries((await workbook()).sheets.map((sheet) => [sheet.name, sheet]))
    const [source, copy] = [info['工作表2'], info['工作表2(2)']]
    step('MENU-C34 副本有同样位置的柱状图与图片', copy.charts.length === 1 && copy.images === 1 && copy.charts[0].at === source.charts[0].at && copy.charts[0].kind === source.charts[0].kind, JSON.stringify(copy.charts))
    const series = (await sheetModel('工作表2(2)')).charts[0].series[0].values
    step('MENU-C34 副本图表的数据源指向副本自身的数据区域', series.includes('工作表2(2)'), series)
  })

  await section('MENU-C35/C36/C37 隐藏行列、冻结与筛选的复制', async () => {
    await build(numbered(3))
    await ai([
      { command: 'sheet.activate', params: { sheet: '工作表2' } },
      { command: 'home.hideRows', params: { rows: '3' } },
      { command: 'home.hideColumns', params: { cols: 'C' } },
      { command: 'sheet.activate', params: { sheet: '工作表1' } },
    ])
    await copyOf('工作表2')
    await clickCell(2, 2)
    await page.waitForTimeout(150)
    step('MENU-C35 副本第 3 行与 C 列同样隐藏', (await nameBox()) === 'D4', await nameBox())
    await build(numbered(3), { fill: false })
    await ai([
      { command: 'range.setValues', params: { range: `${q('工作表2')}!A1`, values: [['数量'], [1], [2], [3], [4], [5]] } },
      { command: 'sheet.activate', params: { sheet: '工作表2' } },
      { command: 'view.freezePanes', params: { cell: 'B2' } },
      { command: 'data.filter', params: { range: 'A1:A6' } },
      { command: 'data.filterBy', params: { column: 'A', custom: { op: '>', value: '2' } } },
      { command: 'sheet.activate', params: { sheet: '工作表1' } },
    ])
    await copyOf('工作表2')
    const info = Object.fromEntries((await workbook()).sheets.map((sheet) => [sheet.name, sheet]))
    step('MENU-C36 副本首行首列同样冻结', same(info['工作表2(2)'].freeze, info['工作表2'].freeze) && info['工作表2(2)'].freeze != null, JSON.stringify(info['工作表2(2)'].freeze))
    step('MENU-C37 副本首行带同样的筛选', info['工作表2(2)'].autofilter === 'A1:A6')
    await clickCell(0, 1)
    await page.waitForTimeout(150)
    step('MENU-C37 副本筛选结果一致（第 2、3 行被筛掉）', (await nameBox()) === 'A4', await nameBox())
  })

  await section('MENU-C38/C288 标签颜色随副本复制', async () => {
    await build(numbered(3))
    await exec('sheet.tabColor', { sheet: '工作表2', color: '#FF0000' })
    await until(async () => (await colorOf('工作表2')) === '#FF0000')
    await copyOf('工作表2')
    step('MENU-C38 副本标签同为红色，工作表1、工作表3 仍无颜色', (await colorOf('工作表2(2)')) === '#FF0000' && (await colorOf('工作表1')) == null && (await colorOf('工作表3')) == null)
    step('MENU-C288 副本自动成为当前工作表，颜色与源表一致', same(await selectedTabs(), ['工作表2(2)']))
  })

  await section('MENU-C40/C41/C42 副本成为当前表、展开箭头与重命名输入态', async () => {
    await build(numbered(3), { active: '工作表2' })
    await startCopy('工作表1')
    step('MENU-C42 副本名称处呈输入框，预填「工作表1(2)」，编辑区已是副本内容', (await field().inputValue()) === '工作表1(2)' && (await activeSheet()) === '工作表1(2)' && (await shownA1()) === '工作表1内容')
    await enter()
    step('MENU-C40 当前工作表为副本，唯一选中，「工作表2」不再选中', same(await selectedTabs(), ['工作表1(2)']) && (await shownA1()) === '工作表1内容')
    const arrows = await kit.getByTestId('sheet-menu-arrow').count()
    step('MENU-C41 展开箭头仅在副本标签上', arrows === 1 && (await tab('工作表1(2)').getByTestId('sheet-menu-arrow').count()) === 1)
  })

  await section('MENU-C43/C44 副本的活动单元格、名称框与 fx 栏', async () => {
    await build(numbered(3), { fill: false, active: '工作表2' })
    await exec('cell.set', { cell: `${q('工作表2')}!D4`, input: 'D4 的内容' })
    await goTo('D4')
    await copyOf('工作表2')
    await until(async () => (await nameBox()) === 'A1')
    step('MENU-C43 副本活动单元格为 A1，名称框 A1，fx 为空', (await nameBox()) === 'A1' && (await formulaBar()) === '')
    await exec('cell.set', { cell: 'A1', input: '副本 A1' })
    await until(async () => (await formulaBar()) === '副本 A1')
    step('MENU-C44 名称框为副本活动单元格地址，fx 与其内容一致', (await nameBox()) === 'A1' && (await formulaBar()) === '副本 A1')
  })

  // ———————————————————————————— 2. Sheet tab colour ————————————————————————————
  await section('MENU-C46/C48/C49 颜色面板的构成', async () => {
    await build(numbered(3))
    await openColors('工作表1')
    const layout = await kit.getByTestId('sheet-menu').evaluate((menu) => {
      const none = menu.querySelector('[data-testid="sheet-menu-no-color"]')
      const grid = menu.querySelector('[data-testid="sheet-menu-colors"]')
      const boxes = [...grid.querySelectorAll('button')].map((button) => button.getBoundingClientRect())
      return {
        noneFirst: none != null && grid != null && (none.compareDocumentPosition(grid) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0,
        noneAbove: none.getBoundingClientRect().bottom <= Math.min(...boxes.map((box) => box.top)) + 1,
        count: boxes.length,
        columns: new Set(boxes.map((box) => Math.round(box.left))).size,
        rows: new Set(boxes.map((box) => Math.round(box.top))).size,
        text: grid.textContent.trim(),
        panel: grid.parentElement.textContent,
      }
    })
    step('MENU-C46 指向「工作表标签」右侧展开颜色面板：顶部「无填充」，下方 10 列 × 5 行色块', layout.noneFirst && layout.noneAbove && layout.count === 50 && layout.columns === 10 && layout.rows === 5, JSON.stringify(layout))
    step('MENU-C48 色块网格每格一个颜色，不呈现文字形式的颜色名称', layout.text === '')
    await swatches().nth(12).hover()
    await page.waitForTimeout(200)
    step('MENU-C49 面板内无「自定义颜色」「其他颜色」入口，指向色块不弹取色器', !/自定义|其他颜色/.test(layout.panel) && !(await anyDialog()) && (await kit.locator('input[type="color"]').count()) === 0)
    await closeMenus()
  })

  await section('MENU-C47 展开箭头入口指向颜色面板', async () => {
    await build(numbered(3))
    await rightClick('工作表1')
    const byRight = await menuRows()
    await closeMenus()
    await openColors('工作表1', { arrow: true })
    const byArrow = await menuRows()
    step('MENU-C47 展开箭头的菜单与右键菜单一致，同样展开颜色面板', same(byRight, byArrow) && (await swatches().count()) === 50)
    await closeMenus()
  })

  await section('MENU-C50/C235 颜色面板的关闭不改色', async () => {
    await build(numbered(3))
    await openColors('工作表1')
    const box = await region()
    await page.mouse.click(box.x + 400, box.y + 300)
    await until(async () => !(await menuShown()))
    step('MENU-C50 面板外单击：面板与菜单关闭，颜色不变', !(await menuShown()) && (await colorOf('工作表1')) == null)
    await openColors('工作表1')
    await blurWindow()
    await until(async () => !(await menuShown()))
    step('MENU-C235 窗口失去焦点：面板关闭，颜色不变', !(await menuShown()) && (await colorOf('工作表1')) == null)
  })

  await section('MENU-C51/C52/C55/C56 改色即生效，标签底部色条', async () => {
    await build(numbered(3))
    await goTo('B2')
    const look = async () => tab('工作表1').evaluate((element) => [getComputedStyle(element).backgroundColor, getComputedStyle(element).color])
    const before = await look()
    const color = await colorTab('工作表1', RED)
    const bar = await band('工作表1')
    step('MENU-C52 点击色块立即生效，无确认框，面板关闭', color === '#FF0000' && !(await menuShown()) && !(await anyDialog()))
    step('MENU-C51 标签底部沿宽度呈该色横条，底色与文字色不变', bar.color === rgb('#FF0000') && bar.height > 0 && bar.height <= 6 && same(await look(), before), JSON.stringify([bar, await look(), before]))
    step('MENU-C55 当前工作表仍为工作表1，名称框不变，箭头仍在工作表1', same(await selectedTabs(), ['工作表1']) && (await nameBox()) === 'B2' && (await tab('工作表1').getByTestId('sheet-menu-arrow').count()) === 1)
    step('MENU-C56 工作表1 内容与工作表数量、名称不变', (await cellOf(`${q('工作表1')}!A1`)).value === '工作表1内容' && same(await order(), numbered(3)))
  })

  await section('MENU-C53/C54/C227/C225 换色、同色再点、连续换色', async () => {
    await build(numbered(3))
    await colorTab('工作表1', RED)
    await colorTab('工作表1', BLUE)
    step('MENU-C53 第二次选色后只呈第二种颜色', (await colorOf('工作表1')) === '#0070C0' && (await band('工作表1')).color === rgb('#0070C0'))
    await colorTab('工作表1', BLUE)
    step('MENU-C54 再点同一色块：颜色不被取消', (await colorOf('工作表1')) === '#0070C0')
    const seen = []
    for (const index of [GREEN, YELLOW, PURPLE]) seen.push(await colorTab('工作表1', index))
    step('MENU-C227 依次选 3 种颜色：每次立即变色，最终为第 3 种', same(seen, ['#00B050', '#FFFF00', '#7030A0']) && (await colorOf('工作表1')) === '#7030A0')
    for (const index of [RED, GREEN, BLUE]) {
      await openColors('工作表1')
      await swatches().nth(index).click()
    }
    await until(async () => (await colorOf('工作表1')) === '#0070C0')
    step('MENU-C225 快速连点 3 种颜色：最终为最后一种，数量与内容不变', (await colorOf('工作表1')) === '#0070C0' && same(await order(), numbered(3)))
  })

  await section('MENU-C57/C58/C59/C62 无填充', async () => {
    await build(numbered(3))
    await colorTab('工作表1', RED)
    await colorTab('工作表1', 'none')
    await until(async () => (await colorOf('工作表1')) == null)
    step('MENU-C57 「无填充」后色条消失', (await colorOf('工作表1')) == null && (await band('工作表1')).content === 'none')
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C62 列表中工作表1 行的色条消失', rows[0].color == null, JSON.stringify(rows[0]))
    await colorTab('工作表2', GREEN)
    await colorTab('工作表1', 'none')
    await page.waitForTimeout(300)
    step('MENU-C58 无色时点「无填充」：仍无色条，其余标签颜色不受影响', (await colorOf('工作表1')) == null && (await colorOf('工作表2')) === '#00B050')
    await colorTab('工作表1', PURPLE)
    step('MENU-C59 清除后重新设色：呈新颜色，不残留旧色', (await colorOf('工作表1')) === '#7030A0')
  })

  await section('MENU-C60/C61/C63/C287 工作表列表中的颜色标记', async () => {
    await build(numbered(3))
    await colorTab('工作表2', RED)
    let rows = await listInfo()
    const bar = await kit.getByTestId('sheet-list-color-工作表2').evaluate((element) => {
      const box = element.getBoundingClientRect()
      return { width: box.width, height: box.height, color: getComputedStyle(element).backgroundColor }
    })
    await closeMenus()
    step('MENU-C60 列表中工作表2 行首呈该色竖条，其余行无色', rows[1].color === '#FF0000' && rows[0].color == null && rows[2].color == null && bar.color === rgb('#FF0000'), JSON.stringify(rows))
    const tabBar = await band('工作表2')
    step('MENU-C61 标签栏为底部横条、列表为行首竖条，颜色一致', bar.height > bar.width && tabBar.width > tabBar.height && tabBar.color === bar.color, JSON.stringify([bar, tabBar]))
    await colorTab('工作表1', GREEN)
    rows = await listInfo()
    await closeMenus()
    step('MENU-C63 工作表1、工作表2 各呈自己的颜色，工作表3 无', rows[0].color === '#00B050' && rows[1].color === '#FF0000' && rows[2].color == null && (await colorOf('工作表1')) === rows[0].color)
    await colorTab('工作表1', BLUE)
    rows = await listInfo()
    await closeMenus()
    step('MENU-C287 换色后列表色条变为第二次的颜色，与标签一致', rows[0].color === '#0070C0' && (await colorOf('工作表1')) === '#0070C0')
  })

  await section('MENU-C224 色块网格首尾色块均可选中', async () => {
    await build(numbered(3))
    const first = await colorTab('工作表1', 0)
    step('MENU-C224 第一行第一个色块生效', first != null && (await colorOf('工作表1')) === first, first)
    const last = await colorTab('工作表1', 49)
    step('MENU-C224 最后一行最后一个色块生效', last != null && last !== first && (await colorOf('工作表1')) === last, last)
  })

  await section('MENU-C226 只有 1 张工作表时改色', async () => {
    await build(['工作表1'])
    await colorTab('工作表1', RED)
    step('MENU-C226 「工作表标签」可用，色条为所选颜色', (await colorOf('工作表1')) === '#FF0000')
  })

  await section('MENU-C228/C248 标签栏可滚动时对靠前标签改色与重命名', async () => {
    await build(numbered(30), { fill: false, active: '工作表30' })
    await scrollTabs('end')
    await page.waitForTimeout(150)
    const hiddenFirst = !(await inView('工作表1'))
    await scrollTabs('start')
    await page.waitForTimeout(150)
    step('MENU-C228 滚动后工作表1 完整可见', hiddenFirst && (await inView('工作表1')))
    const scroll = await tabsScroll()
    await colorTab('工作表1', RED)
    step('MENU-C228 改色生效；标签栏滚动位置不变', (await colorOf('工作表1')) === '#FF0000' && Math.abs((await tabsScroll()) - scroll) <= 1, `${scroll} ${await tabsScroll()}`)
    await startRename('工作表1')
    const inside = await tab('工作表1').getByTestId('sheet-rename').count()
    step('MENU-C248 重命名输入框呈现在工作表1 标签上，滚动位置不变', inside === 1 && Math.abs((await tabsScroll()) - scroll) <= 1)
    await field().press('Escape')
  })

  await section('MENU-C229 改色作用于右键的那张（先成为当前表）', async () => {
    await build(numbered(3))
    await rightClick('工作表3')
    await until(async () => same(await selectedTabs(), ['工作表3']) && (await shownA1()) === '工作表3内容')
    const current = await selectedTabs()
    const a1 = await shownA1()
    await sub('sheet-menu-color')
    await swatches().nth(RED).click()
    await until(async () => (await colorOf('工作表3')) === '#FF0000')
    step('MENU-C229 右键后工作表3 先成为当前工作表，颜色落在工作表3', same(current, ['工作表3']) && a1 === '工作表3内容' && (await colorOf('工作表1')) == null)
  })

  await section('MENU-C230/C247/C319 隐藏的工作表在列表中只能取消隐藏', async () => {
    await build(numbered(3), { hidden: ['工作表3'] })
    const rows = await listInfo()
    const row = rows.find((entry) => entry.name === '工作表3')
    await kit.getByTestId('sheet-list-item-工作表3').click({ button: 'right', force: true })
    await page.waitForTimeout(250)
    const menu = await menuShown()
    await kit.getByTestId('sheet-list-item-工作表3').dblclick({ force: true })
    await page.waitForTimeout(250)
    step('MENU-C230 工作表3 只在列表中占一行，没有改色入口', (await tab('工作表3').count()) === 0 && row.hidden && !menu)
    step('MENU-C247 该行只有「取消隐藏」按钮，不能进入重命名', row.unhide === '取消隐藏' && !(await renaming()))
    step('MENU-C319 在该行右键不弹工作表操作菜单，无删除入口', !menu && same(await order(), numbered(3)))
    await closeMenus()
  })

  await section('MENU-C232 被保护工作表上的改色', async () => {
    await build(numbered(3))
    await exec('review.protectSheet', {})
    await until(async () => (await kit.getByTestId('sheet-lock-工作表1').count()) === 1)
    await colorTab('工作表1', RED)
    step('MENU-C232 工作表保护不影响改色：标签底部呈所选颜色', (await colorOf('工作表1')) === '#FF0000')
    await exec('review.unprotectSheet', {})
  })

  await section('MENU-C289/C290/C298/C291 颜色随拖拽、重命名与隐藏保留', async () => {
    await build(numbered(3))
    await colorTab('工作表1', RED)
    await drag('工作表1', await end())
    const tabsShown1 = await showsTabs(['工作表2', '工作表3', '工作表1'])
    step('MENU-C289 拖到最右后色条随标签移动，颜色不变', tabsShown1 && (await colorOf('工作表1')) === '#FF0000')
    await startRename('工作表1')
    step('MENU-C73 重命名输入态下标签底部色条保持', (await colorOf('工作表1')) === '#FF0000' && (await band('工作表1')).color === rgb('#FF0000'))
    await field().fill('汇总')
    await enter()
    const tabsShown2 = await showsTabs(['工作表2', '工作表3', '汇总'])
    step('MENU-C290/C298 重命名为「汇总」后色条保持', tabsShown2 && (await colorOf('汇总')) === '#FF0000')
    await colorTab('工作表3', GREEN)
    await hideSheet('工作表3')
    await showsTabs(['工作表2', '汇总'])
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C291 隐藏后工作表3 标签消失，列表行仍呈该色竖条', rows.find((row) => row.name === '工作表3')?.color === '#00B050', JSON.stringify(rows))
  })

  await section('MENU-C292 撤销改色', async () => {
    await build(numbered(3))
    await colorTab('工作表1', RED)
    await undoButton()
    await until(async () => (await colorOf('工作表1')) == null)
    step('MENU-C292 撤销后颜色回到改色前（无色条）', (await colorOf('工作表1')) == null)
  })

  await section('MENU-C293/C334 颜色的保存与导出', async () => {
    await build(numbered(3))
    await colorTab('工作表1', RED)
    await saveAndReopen()
    step('MENU-C293 重新打开后停留在工作表1，色条仍为红色', same(await selectedTabs(), ['工作表1']) && (await colorOf('工作表1')) === '#FF0000')
    step('MENU-C334 导出件中工作表1 的标签颜色为红色', (await savedTabColor('工作表1')) === '#FF0000', String(await savedTabColor('工作表1')))
  })

  await section('MENU-C335 导入件自带标签颜色的改色', async () => {
    await build(numbered(3))
    await exec('sheet.tabColor', { sheet: '工作表1', color: '#00B050' })
    await touch()
    await saveAndReopen()
    step('MENU-C335 导入件自带的颜色完整呈现', (await colorOf('工作表1')) === '#00B050')
    await colorTab('工作表1', RED)
    step('MENU-C335 选新颜色后原颜色被替换', (await colorOf('工作表1')) === '#FF0000')
  })

  // ———————————————————————————— 3. Rename ————————————————————————————
  await section('MENU-C66 展开箭头入口指向重命名', async () => {
    await build(numbered(3))
    await rightClick('工作表1')
    const byRight = await menuRows()
    await closeMenus()
    await arrowMenu()
    const byArrow = await menuRows()
    await kit.getByTestId('sheet-menu-rename').click()
    await field().waitFor()
    step('MENU-C66 箭头菜单与右键菜单一致，「重命名」为第 2 项；点击后预填「工作表1」', same(byRight, byArrow) && byArrow[1] === '重命名' && (await field().inputValue()) === '工作表1')
    await field().press('Escape')
  })

  await section('MENU-C67/C72 进入重命名时整段名称被选中', async () => {
    await build(numbered(3))
    await startRename('工作表2', { double: true })
    const [start, finish, length] = await fieldSelection()
    step('MENU-C67 双击「工作表2」进入输入态，预填并全选', (await field().inputValue()) === '工作表2' && start === 0 && finish === length)
    await field().press('Escape')
    await startRename('工作表1')
    const selection = await fieldSelection()
    step('MENU-C72 菜单重命名：「工作表1」整段高亮', selection[0] === 0 && selection[1] === selection[2] && selection[2] === '工作表1'.length, JSON.stringify(selection))
    await field().press('Escape')
  })

  await section('MENU-C68/C69 右键非当前表后重命名的对象与当前表', async () => {
    await build(numbered(3))
    await rightClick('工作表3')
    await until(async () => same(await selectedTabs(), ['工作表3']))
    const current = await selectedTabs()
    await kit.getByTestId('sheet-menu-rename').click()
    await field().waitFor()
    step('MENU-C69 输入态下展开箭头在工作表3 上，编辑区为工作表3', (await tab('工作表3').getByTestId('sheet-menu-arrow').count()) === 1 && (await activeSheet()) === '工作表3' && (await shownA1()) === '工作表3内容')
    await field().fill('三月数据')
    await enter()
    const tabsShown3 = await showsTabs(['工作表1', '工作表2', '三月数据'])
    step('MENU-C68 右键后工作表3 先成为当前表，改名的是工作表3', tabsShown3 && same(current, ['工作表3']) && same(await tabNames(), ['工作表1', '工作表2', '三月数据']))
  })

  await section('MENU-C70 列表中的名称不提供重命名', async () => {
    await build(numbered(3), { active: '工作表2' })
    await openList()
    await kit.getByTestId('sheet-list-item-工作表1').dblclick()
    await until(async () => (await activeSheet()) === '工作表1')
    step('MENU-C70 双击列表中的名称只切换到该表，不进入重命名', (await activeSheet()) === '工作表1' && !(await renaming()))
    await openList()
    await kit.getByTestId('sheet-list-item-工作表1').click({ button: 'right' })
    await page.waitForTimeout(250)
    step('MENU-C70 在列表名称上右键不弹工作表操作菜单，名称不变', !(await menuShown()) && same(await tabNames(), numbered(3)))
    await closeMenus()
  })

  await section('MENU-C71/C74 输入框在原标签位置，编辑区不变', async () => {
    await build(numbered(3))
    await startRename('工作表1')
    const width = (await field().boundingBox()).width
    step('MENU-C71 名称在原标签处以输入框呈现，不弹命名对话框', (await tab('工作表1').getByTestId('sheet-rename').count()) === 1 && !(await anyDialog()))
    await field().fill('一个更长一些的工作表名称')
    await page.waitForTimeout(100)
    step('MENU-C71 标签宽度随输入框自适应', (await field().boundingBox()).width > width + 20, `${width} -> ${(await field().boundingBox()).width}`)
    step('MENU-C74 编辑区仍为工作表1，A1 不变，当前表与箭头仍在工作表1', (await shownA1()) === '工作表1内容' && same(await selectedTabs(), ['工作表1']) && (await tab('工作表1').getByTestId('sheet-menu-arrow').count()) === 1)
    await field().press('Escape')
  })

  await section('MENU-C75/C76 直接输入整体替换，取消全选后插入', async () => {
    await build(numbered(3))
    await startRename('工作表1')
    await page.keyboard.type('九月汇总')
    step('MENU-C75 键入即替换全部选中文本', (await field().inputValue()) === '九月汇总')
    await enter()
    const tabsShown4 = await showsTabs(['九月汇总', '工作表2', '工作表3'])
    step('MENU-C75 回车后名称为「九月汇总」', tabsShown4 && same(await tabNames(), ['九月汇总', '工作表2', '工作表3']))
    await startRename('工作表2')
    await field().press('ArrowRight')
    await page.keyboard.type('表')
    step('MENU-C76 右方向键取消全选后键入插入在光标处', (await field().inputValue()) === '工作表2表')
    await field().press('Escape')
  })

  await section('MENU-C77/C78/C79 回车确认与 ESC 放弃', async () => {
    await build(numbered(3))
    await startRename('工作表1')
    await enter()
    let rows = await listInfo()
    await closeMenus()
    step('MENU-C77 不改直接回车：输入框收起，名称仍为工作表1，列表该行带勾选', same(await tabNames(), numbered(3)) && rows[0].name === '工作表1' && rows[0].checked)
    await renameTo('工作表1', '三月数据')
    rows = await listInfo()
    await closeMenus()
    step('MENU-C78 回车确认后名称为「三月数据」，列表同步', same(await tabNames(), ['三月数据', '工作表2', '工作表3']) && rows[0].name === '三月数据')
    await startRename('三月数据')
    await field().fill('临时名称')
    await field().press('Escape')
    await page.waitForTimeout(250)
    rows = await listInfo()
    await closeMenus()
    step('MENU-C79 ESC 放弃本次输入，名称与列表仍为「三月数据」', !(await renaming()) && same(await tabNames(), ['三月数据', '工作表2', '工作表3']) && rows[0].name === '三月数据')
  })

  await section('MENU-C80/C81 失焦与点击其他标签保留输入', async () => {
    await build(numbered(3))
    await startRename('工作表1')
    await field().fill('三月数据')
    await clickCell(0, 0)
    const tabsShown5 = await showsTabs(['三月数据', '工作表2', '工作表3'])
    step('MENU-C80 点击 A1：输入框收起，名称为「三月数据」，当前表仍是它', tabsShown5 && !(await renaming()) && same(await selectedTabs(), ['三月数据']))
    await build(numbered(3))
    await startRename('工作表1')
    await field().fill('三月数据')
    await tab('工作表2').click()
    await showsTabs(['三月数据', '工作表2', '工作表3'])
    await until(async () => (await activeSheet()) === '工作表2')
    step('MENU-C81 单击工作表2：名称保留为「三月数据」，当前表切到工作表2', !(await renaming()) && same(await selectedTabs(), ['工作表2']))
  })

  await section('MENU-C82 再次进入时预填新名称并全选', async () => {
    await build(numbered(3))
    await renameTo('工作表1', '三月数据')
    await startRename('三月数据')
    const [start, finish, length] = await fieldSelection()
    step('MENU-C82 第二次进入预填「三月数据」，整段选中', (await field().inputValue()) === '三月数据' && start === 0 && finish === length)
    await field().press('Escape')
  })

  await section('MENU-C83/C84/C85 名称长度与合法字符', async () => {
    await build(numbered(3))
    await clearNotices()
    await renameTo('工作表1', '1234567890123456789012345678901')
    step('MENU-C83 31 个字符的名称被接受，无报错', (await tabNames())[0] === '1234567890123456789012345678901' && !(await notices()).length)
    await startRename('工作表2')
    await page.keyboard.type('12345678901234567890123456789012')
    const typed = await field().inputValue()
    step('MENU-C84 第 32 个字符不进入输入框，最多 31 个字符', typed === '1234567890123456789012345678901' && typed.length === 31, typed)
    await field().press('Escape')
    await renameTo('工作表3', '九月Data_2026')
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C85 「九月Data_2026」被接受，列表同步，无报错', (await tabNames())[2] === '九月Data_2026' && rows[2].name === '九月Data_2026')
  })

  await section('MENU-C86/C245 名称过长时标签的显示', async () => {
    await build(numbered(3))
    const before = await tab('工作表3').boundingBox()
    await renameTo('工作表2', '这是一个非常非常长的工作表名称用来测试截断显示效果的')
    const long = (await tabNames())[1]
    const shown = await tab(long).evaluate((element) => {
      const name = element.querySelector('.sheet-tab-name')
      return { width: element.getBoundingClientRect().width, clipped: name.scrollWidth > name.clientWidth, ellipsis: getComputedStyle(name).textOverflow }
    })
    const strip_ = await strip().boundingBox()
    step('MENU-C86/C245 长名称以省略号截断，标签宽度有限、不撑破标签栏', shown.clipped && shown.ellipsis === 'ellipsis' && shown.width <= 260 && shown.width < strip_.width, JSON.stringify(shown))
    const after = await tab('工作表3').boundingBox()
    step('MENU-C86/C245 其余标签照常显示', (await inView('工作表1')) && after.width === before.width)
  })

  await section('MENU-C87 公式引用本工作表自身时的更新', async () => {
    await build(numbered(3), { active: '工作表2' })
    await exec('cell.set', { cell: `${q('工作表2')}!D1`, input: '=工作表2!B2*2' })
    const value = (await cellOf(`${q('工作表2')}!D1`)).value
    await renameTo('工作表2', '三月数据')
    const d1 = await cellOf(`${q('三月数据')}!D1`)
    step('MENU-C87 D1 更新为 =三月数据!B2*2，结果不变', d1.formula === '=三月数据!B2*2' && d1.value === value && value === 40, JSON.stringify([d1.formula, d1.value]))
  })

  await section('MENU-C236/C237/C239/C240/C241 名称不合法时的提示', async () => {
    const cases = [
      ['MENU-C236 「销售/汇总」', '销售/汇总', '工作表名不能包含无效字符：\\ / : ? * [ ]'],
      ['MENU-C237 「汇总[表]」', '汇总[表]', '工作表名不能包含无效字符'],
      ['MENU-C239 与已有「工作表2」同名', '工作表2', '工作表名称已经存在!'],
      ['MENU-C240 删除全部文字', '', '工作表名称不能为空!'],
      ['MENU-C241 纯空格', '   ', '工作表名称不能为空!'],
    ]
    await build(numbered(3))
    for (const [label, text, message] of cases) {
      await clearNotices()
      await startRename('工作表1')
      await field().fill(text)
      await field().press('Enter')
      const said = await notice(message)
      step(`${label}：名称不被保存，浮层提示「${message}」，输入框保持输入状态`, said && (await renaming()) && same(await order(), numbered(3)) && (await cellOf(`${q('工作表2')}!A1`)).value === '工作表2内容', JSON.stringify(await notices()))
      await field().press('Escape')
      await page.waitForTimeout(150)
    }
  })

  await section('MENU-C242 只有 1 张工作表时的重命名', async () => {
    await build(['工作表1'])
    await rightClick('工作表1')
    const enabled = await enabledRow('sheet-menu-rename')
    await kit.getByTestId('sheet-menu-rename').click()
    await field().waitFor()
    await field().fill('汇总')
    await enter()
    step('MENU-C242 「重命名」可用，名称变为「汇总」', enabled && (await showsTabs(['汇总'])), JSON.stringify(await tabNames()))
  })

  await section('MENU-C243 改为与原名称相同', async () => {
    await build(numbered(3))
    await clearNotices()
    await startRename('工作表1')
    await field().fill('')
    await page.keyboard.type('工作表1')
    await enter()
    await page.waitForTimeout(200)
    step('MENU-C243 被接受，名称仍为工作表1，无重名提示，位置与数量不变', same(await tabNames(), numbered(3)) && !(await notices()).some((text) => text.includes('已经存在')))
  })

  await section('MENU-C244 连续快速双击', async () => {
    await build(numbered(3))
    await tab('工作表1').dblclick()
    await tab('工作表1').dblclick().catch(() => undefined)
    await tab('工作表1').dblclick().catch(() => undefined)
    await page.waitForTimeout(200)
    step('MENU-C244 只出现一个输入框，名称与数量不变', (await kit.getByTestId('sheet-rename').count()) === 1 && same(await order(), numbered(3)))
    await field().press('Escape')
  })

  await section('MENU-C246 重命名后切走再切回', async () => {
    await build(numbered(3))
    await renameTo('工作表1', '三月数据')
    await choose('工作表2')
    await choose('三月数据')
    step('MENU-C246 编辑区为「三月数据」的内容，箭头在它上面', (await shownA1()) === '工作表1内容' && (await tab('三月数据').getByTestId('sheet-menu-arrow').count()) === 1)
  })

  await section('MENU-C296/C297/C304/C305/C339 跨表引用随重命名更新（单引号）', async () => {
    await build(numbered(3), { fill: false, active: '工作表2' })
    await ai([
      { command: 'cell.set', params: { cell: `${q('工作表1')}!A1`, input: '5' } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!B2`, input: '=工作表1!A1*2' } },
      { command: 'cell.set', params: { cell: `${q('工作表3')}!C3`, input: '=工作表1!A1+100' } },
    ])
    await renameTo('工作表1', '222')
    const b2 = await cellOf(`${q('工作表2')}!B2`)
    const c3 = await cellOf(`${q('工作表3')}!C3`)
    step("MENU-C296 改名「222」：B2 为 ='222'!A1*2，不断链", b2.formula === "='222'!A1*2" && b2.value === 10, b2.formula)
    step("MENU-C304 B2 与 C3 都更新为引用 '222'，结果不变", c3.formula === "='222'!A1+100" && c3.value === 105, c3.formula)
    await renameTo('222', '三月数据')
    step('MENU-C297/C339 改回中文：=三月数据!A1*2，不带单引号', (await cellOf(`${q('工作表2')}!B2`)).formula === '=三月数据!A1*2')
    await renameTo('三月数据', 'A表')
    await renameTo('A表', '222')
    const final = await cellOf(`${q('工作表2')}!B2`)
    step("MENU-C305 连续重命名后为 ='222'!A1*2，无残留与引号叠加", final.formula === "='222'!A1*2" && final.value === 10, final.formula)
  })

  await section('MENU-C299/C300 重命名后位置与列表同步', async () => {
    await build(numbered(3))
    await colorTab('工作表2', RED)
    await choose('工作表1')
    await renameTo('工作表2', '三月数据')
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C299 顺序为 工作表1、三月数据、工作表3', same(await tabNames(), ['工作表1', '三月数据', '工作表3']))
    step('MENU-C300 列表该行为「三月数据」，位置、勾选与颜色与重命名前一致', rows[1].name === '三月数据' && rows[1].color === '#FF0000' && rows[1].checked === true, JSON.stringify(rows))
  })

  await section('MENU-C302/C336 重命名的保存与导出', async () => {
    await build(numbered(3))
    await renameTo('工作表1', '三月数据')
    await saveAndReopen()
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C302 重新打开后停留在「三月数据」，标签与列表名称一致', same(await selectedTabs(), ['三月数据']) && rows[0].name === '三月数据')
    const saved = await savedWorkbook()
    step('MENU-C336 导出件中该表名为「三月数据」', saved.sheets[0].name === '三月数据', JSON.stringify(saved.sheets))
  })

  await section('MENU-C306/C307 撤销重命名', async () => {
    await build(numbered(3))
    await renameTo('工作表1', '三月数据')
    await undoKey()
    await showsTabs(numbered(3))
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C306 Ctrl+Z 后名称回到工作表1，列表同步', same(await tabNames(), numbered(3)) && rows[0].name === '工作表1')
    await colorTab('工作表1', RED)
    await renameTo('工作表1', '三月数据')
    await undoButton()
    const tabsShown6 = await showsTabs(numbered(3))
    step('MENU-C307 工具栏撤销只回退名称，A1 与颜色不变', tabsShown6 && (await cellOf(`${q('工作表1')}!A1`)).value === '工作表1内容' && (await colorOf('工作表1')) === '#FF0000')
  })

  await section('MENU-C337 导入件重命名后导出', async () => {
    await imported(sheets(3))
    await renameTo('Sheet1', '三月数据')
    await touch()
    await saveAndReopen()
    const saved = await savedWorkbook()
    step('MENU-C337 导出件中为「三月数据」，未回退为 Sheet1', same(saved.sheets.map((sheet) => sheet.name), ['三月数据', 'Sheet2', 'Sheet3']), JSON.stringify(saved.sheets))
  })

  await section('MENU-C338 重命名后导出件的名称与引用', async () => {
    await build(numbered(3), { fill: false, active: '工作表2' })
    await ai([
      { command: 'cell.set', params: { cell: `${q('工作表1')}!A1`, input: '5' } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!B2`, input: '=工作表1!A1*2' } },
    ])
    await renameTo('工作表1', '三月数据')
    await saveAndReopen()
    const xml = await savedSheet('工作表2')
    const b2 = await cellOf(`${q('工作表2')}!B2`)
    step('MENU-C338 导出件中表名为「三月数据」，B2 公式引用同步指向它', (await savedWorkbook()).sheets[0].name === '三月数据' && /<f\b[^>]*>三月数据!A1\*2<\/f>/.test(xml ?? '') && b2.formula === '=三月数据!A1*2' && b2.value === 10, b2.formula)
  })

  // ———————————————————————————— 4. Delete ————————————————————————————
  await section('MENU-C90/C93/C94/C95 删除的二次确认框', async () => {
    await build(numbered(3))
    await rightClick('工作表1')
    const byRight = await menuRows()
    await closeMenus()
    await askDelete('工作表1', { arrow: true })
    const dialog = kit.getByTestId('sheet-delete-dialog')
    const title = (await dialog.locator('.kui-modal-title').textContent())?.trim()
    const message = (await kit.getByTestId('sheet-delete-message').textContent())?.trim()
    step('MENU-C90 箭头菜单与右键一致，「删除」为最后一项；点击后确认框标题「删除工作表」，对象为工作表1', same(byRight[byRight.length - 1], '删除') && title === '删除工作表' && message.includes('工作表1'))
    const confirm = kit.getByTestId('sheet-delete-confirm')
    const cancel = kit.getByTestId('sheet-delete-cancel')
    step('MENU-C93 确认框有「确认删除」「取消」两个按钮与右上角「×」', (await confirm.textContent()) === '确认删除' && (await cancel.textContent()) === '取消' && (await dialog.locator('.kui-modal-close').count()) === 1)
    step('MENU-C94 提示文案为「确认要删除“工作表1”吗？」', message === '确认要删除“工作表1”吗？', message)
    const look = async (button) =>
      button.evaluate((element) => {
        const style = getComputedStyle(element)
        const box = element.getBoundingClientRect()
        const luminance = (color) => {
          const [r, g, b] = (color.match(/\d+(\.\d+)?/g) ?? ['255', '255', '255']).map(Number)
          return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255
        }
        return { variant: element.getAttribute('data-variant'), x: box.x, y: box.y, background: luminance(style.backgroundColor), text: luminance(style.color), border: style.borderTopWidth }
      })
    const [ok, no] = [await look(confirm), await look(cancel)]
    step('MENU-C95 「确认删除」深色实底白字在左，「取消」白底细边框在右', ok.variant === 'primary' && no.variant === 'secondary' && ok.x < no.x && Math.abs(ok.y - no.y) < 4 && ok.background < 0.6 && ok.text > 0.8 && no.background > 0.8, JSON.stringify([ok, no]))
    await cancel.click()
  })

  await section('MENU-C91/C100 右键非当前表时删除的对象', async () => {
    await build(numbered(3))
    await rightClick('工作表3')
    await until(async () => same(await selectedTabs(), ['工作表3']))
    step('MENU-C91 右键后工作表3 先成为当前表，箭头在工作表3', same(await selectedTabs(), ['工作表3']) && (await tab('工作表3').getByTestId('sheet-menu-arrow').count()) === 1)
    await kit.getByTestId('sheet-menu-delete').click()
    await kit.getByTestId('sheet-delete-dialog').waitFor()
    step('MENU-C91/C100 删除的对象为工作表3：「确认要删除“工作表3”吗？」，当前表已是工作表3', (await kit.getByTestId('sheet-delete-message').textContent()) === '确认要删除“工作表3”吗？' && (await activeSheet()) === '工作表3')
    await kit.getByTestId('sheet-delete-cancel').click()
  })

  await section('MENU-C96/C97/C98/C99 取消、关闭、ESC 与遮罩', async () => {
    await build(numbered(3))
    await askDelete('工作表1')
    await kit.getByTestId('sheet-delete-cancel').click()
    await until(async () => !(await dialogShown()))
    step('MENU-C96 点「取消」：确认框收起，无提示，仍为 3 张，顺序内容不变', !(await dialogShown()) && same(await order(), numbered(3)) && (await shownA1()) === '工作表1内容')
    await askDelete('工作表1')
    await kit.getByTestId('sheet-delete-dialog').locator('.kui-modal-close').click()
    await until(async () => !(await dialogShown()))
    step('MENU-C97 点右上角「×」：确认框收起，未删除', !(await dialogShown()) && same(await order(), numbered(3)))
    await askDelete('工作表1')
    await page.keyboard.press('Escape')
    await until(async () => !(await dialogShown()))
    step('MENU-C98 按 ESC：确认框收起，未删除', !(await dialogShown()) && same(await order(), numbered(3)))
    await goTo('C5')
    await askDelete('工作表1')
    const box = await region()
    await page.mouse.click(box.x + 46 + 32, box.y + 21 + 10)
    await page.waitForTimeout(300)
    step('MENU-C99 点击 A1 不生效（A1 未选中），确认框仍在，仍为 3 张', (await dialogShown()) && (await nameBox()) === 'C5' && same(await order(), numbered(3)))
    await kit.getByTestId('sheet-delete-cancel').click()
  })

  await section('MENU-C101/C102/C108/C308 删除当前工作表', async () => {
    await build(numbered(3))
    await clearNotices()
    await deleteSheet('工作表1')
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C101 确认后工作表1 消失，标签栏剩 工作表2、工作表3，列表无工作表1', same(await tabNames(), ['工作表2', '工作表3']) && same(rows.map((row) => row.name), ['工作表2', '工作表3']))
    step('MENU-C102 删除直接生效，无浮层提示', !(await notices()).length)
    step('MENU-C108 当前表落到右邻的工作表2，箭头在工作表2', same(await selectedTabs(), ['工作表2']) && (await tab('工作表2').getByTestId('sheet-menu-arrow').count()) === 1)
    await build(numbered(3))
    await deleteSheet('工作表2')
    const after = await listInfo()
    await closeMenus()
    step('MENU-C308 标签栏剩 工作表1、工作表3，列表同步', same(await tabNames(), ['工作表1', '工作表3']) && same(after.map((row) => row.name), ['工作表1', '工作表3']))
  })

  await section('MENU-C103/C107/C109/C110 删除后的当前表落点', async () => {
    await build(numbered(3))
    await deleteSheet('工作表3')
    step('MENU-C103 只删被右键的工作表3；当前表落到工作表2，编辑区为工作表2', same(await tabNames(), ['工作表1', '工作表2']) && same(await selectedTabs(), ['工作表2']) && (await shownA1()) === '工作表2内容')
    await build(numbered(3), { active: '工作表2' })
    await deleteSheet('工作表2')
    step('MENU-C107 删中间的当前表：落到右邻工作表3，箭头与编辑区随之', same(await tabNames(), ['工作表1', '工作表3']) && same(await selectedTabs(), ['工作表3']) && (await shownA1()) === '工作表3内容' && (await tab('工作表3').getByTestId('sheet-menu-arrow').count()) === 1)
    step('MENU-C110 名称框与 fx 随工作表3 呈现', (await formulaBar()) === '工作表3内容' && (await nameBox()) === 'A1')
    await build(numbered(3), { active: '工作表3' })
    await deleteSheet('工作表3')
    step('MENU-C109 删最后一张：落到左邻工作表2', same(await tabNames(), ['工作表1', '工作表2']) && same(await selectedTabs(), ['工作表2']))
  })

  await section('MENU-C104/C105/C254/C258 连续删除', async () => {
    await build(numbered(2))
    await deleteSheet('工作表1')
    step('MENU-C104 只剩 2 张时删工作表1：剩工作表2，为当前表并带箭头', same(await tabNames(), ['工作表2']) && same(await selectedTabs(), ['工作表2']) && (await kit.getByTestId('sheet-menu-arrow').count()) === 1)
    await rightClick('工作表2')
    step('MENU-C258 删至 1 张后「删除」不可用', !(await enabledRow('sheet-menu-delete')))
    await closeMenus()
    await build(numbered(3))
    await deleteSheet('工作表1')
    step('MENU-C105 第一次删除后剩 工作表2、工作表3，当前表为工作表2', same(await tabNames(), ['工作表2', '工作表3']) && same(await selectedTabs(), ['工作表2']))
    await deleteSheet('工作表2')
    step('MENU-C105 第二次删除后仅剩工作表3', same(await tabNames(), ['工作表3']) && (await order()).length === 1)
    await build(numbered(3), { active: '工作表2' })
    await deleteSheet('工作表2')
    step('MENU-C254 第一次删除后当前表为工作表3', same(await selectedTabs(), ['工作表3']))
    await deleteSheet('工作表3')
    step('MENU-C254 第二次删除后仅剩工作表1，当前表与箭头在工作表1', same(await tabNames(), ['工作表1']) && same(await selectedTabs(), ['工作表1']) && (await kit.getByTestId('sheet-menu-arrow').count()) === 1)
  })

  await section('MENU-C106/C317/C318 删除与副本互不影响', async () => {
    await build(['工作表1', '工作表2', '工作表3', '工作表3(2)'], { active: '工作表3' })
    await deleteSheet('工作表2')
    step('MENU-C106 删工作表2 后其余顺序不变：工作表1、工作表3、工作表3(2)', same(await tabNames(), ['工作表1', '工作表3', '工作表3(2)']))
    await build(['工作表1', '工作表2', '工作表3', '工作表3(2)'], { active: '工作表3' })
    await deleteSheet('工作表3(2)')
    step('MENU-C317 删副本后工作表3 仍在，位置名称与内容不变', same(await tabNames(), numbered(3)) && (await cellOf(`${q('工作表3')}!A1`)).value === '工作表3内容')
    await build(['工作表1', '工作表2', '工作表3', '工作表3(2)'], { active: '工作表3' })
    await deleteSheet('工作表3')
    step('MENU-C318 删源表后副本仍在，位置名称与内容不变', same(await tabNames(), ['工作表1', '工作表2', '工作表3(2)']) && (await cellOf(`${q('工作表3(2)')}!A1`)).value === '工作表3(2)内容')
  })

  await section('MENU-C255/C256/C257 只剩 1 张表时删除不可用', async () => {
    await build(['工作表1'])
    await rightClick('工作表1')
    const states = {}
    for (const id of ['sheet-menu-delete', 'sheet-menu-rename', 'sheet-menu-copy', 'sheet-menu-color']) states[id] = await enabledRow(id)
    await kit.getByTestId('sheet-menu-delete').click({ force: true })
    await page.waitForTimeout(250)
    step('MENU-C255 右键菜单「删除」不可用，点击不弹确认框，仍为 1 张', !states['sheet-menu-delete'] && !(await dialogShown()) && same(await order(), ['工作表1']))
    step('MENU-C257 「重命名」「创建副本」「工作表标签」仍可点击', states['sheet-menu-rename'] && states['sheet-menu-copy'] && states['sheet-menu-color'], JSON.stringify(states))
    await closeMenus()
    await arrowMenu()
    const arrowState = await enabledRow('sheet-menu-delete')
    await kit.getByTestId('sheet-menu-delete').click({ force: true })
    await page.waitForTimeout(250)
    step('MENU-C256 展开箭头路径同样不可删', !arrowState && !(await dialogShown()) && same(await tabNames(), ['工作表1']))
    await closeMenus()
  })

  await section('MENU-C259/C260 删除可用的情形', async () => {
    await build(numbered(2))
    await askDelete('工作表1')
    step('MENU-C259 仅 2 张时「删除」可点，弹出「删除工作表」确认框', (await dialogShown()) && (await kit.getByTestId('sheet-delete-dialog').locator('.kui-modal-title').textContent()) === '删除工作表')
    await kit.getByTestId('sheet-delete-cancel').click()
    await build(numbered(3), { hidden: ['工作表2', '工作表3'] })
    await rightClick('工作表1')
    const enabled = await enabledRow('sheet-menu-delete')
    await kit.getByTestId('sheet-menu-delete').click()
    await kit.getByTestId('sheet-delete-dialog').waitFor()
    step('MENU-C260 实际有 3 张（只 1 张可见）：「删除」可点，确认框对象为工作表1', enabled && (await kit.getByTestId('sheet-delete-message').textContent()).includes('工作表1'))
    await kit.getByTestId('sheet-delete-cancel').click()
  })

  await section('MENU-C309/C310/C311/C312/C313/C314 撤销删除', async () => {
    await build(numbered(3))
    await exec('cell.set', { cell: `${q('工作表2')}!A1`, input: '汇总' })
    await deleteSheet('工作表1')
    await undoKey()
    await showsTabs(numbered(3))
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C309 Ctrl+Z 后工作表1 回到原位，列表与内容恢复', same(await tabNames(), numbered(3)) && rows.length === 3 && (await cellOf(`${q('工作表1')}!A1`)).value === '工作表1内容')
    await deleteSheet('工作表2')
    await undoKey()
    await showsTabs(numbered(3))
    await choose('工作表2')
    step('MENU-C310 工作表2 回到工作表1 与工作表3 之间，A1「汇总」一并恢复', same(await tabNames(), numbered(3)) && (await shownA1()) === '汇总')
    await deleteSheet('工作表1')
    await undoButton()
    const tabsShown7 = await showsTabs(numbered(3))
    step('MENU-C311 工具栏撤销：工作表1 重新出现，3 张', tabsShown7 && same(await order(), numbered(3)))
    await deleteSheet('工作表1')
    await tab('工作表3').click()
    await until(async () => (await activeSheet()) === '工作表3')
    await undoKey()
    const tabsShown8 = await showsTabs(numbered(3))
    step('MENU-C312 切到工作表3 后 Ctrl+Z：恢复的是工作表1，内容无其他变化', tabsShown8 && same(await tabNames(), numbered(3)) && (await cellOf(`${q('工作表3')}!A1`)).value === '工作表3内容')
    await deleteSheet('工作表1')
    await deleteSheet('工作表2')
    await undoKey()
    const tabsShown9 = await showsTabs(['工作表2', '工作表3'])
    step('MENU-C313 第一次 Ctrl+Z 恢复工作表2', tabsShown9 && same(await tabNames(), ['工作表2', '工作表3']))
    await undoKey()
    const tabsShown10 = await showsTabs(numbered(3))
    step('MENU-C313 第二次 Ctrl+Z 恢复工作表1', tabsShown10 && same(await tabNames(), numbered(3)))
    await deleteSheet('工作表1')
    await undoKey()
    await showsTabs(numbered(3))
    await deleteSheet('工作表1')
    step('MENU-C314 撤销后再次删除：标签消失，剩工作表2、工作表3', same(await tabNames(), ['工作表2', '工作表3']))
  })

  await section('MENU-C315 删除带颜色的工作表后其余标签的颜色', async () => {
    await build(numbered(3))
    await colorTab('工作表1', GREEN)
    await colorTab('工作表2', RED)
    await deleteSheet('工作表2')
    step('MENU-C315 剩工作表1、工作表3，工作表1 色条不变', same(await tabNames(), ['工作表1', '工作表3']) && (await colorOf('工作表1')) === '#00B050')
  })

  await section('MENU-C316 被删工作表被公式引用', async () => {
    await build(numbered(3), { fill: false, active: '工作表2' })
    await ai([
      { command: 'cell.set', params: { cell: `${q('工作表3')}!A1`, input: '4' } },
      { command: 'cell.set', params: { cell: `${q('工作表2')}!B2`, input: '=工作表3!A1*2' } },
    ])
    await deleteSheet('工作表3')
    const b2 = await cellOf(`${q('工作表2')}!B2`)
    step('MENU-C316 B2 公式变为引用错误形态（#REF!），不再指向工作表3', b2.formula.includes('#REF!') && !b2.formula.includes('工作表3') && JSON.stringify(b2.value).includes('#REF!'), JSON.stringify([b2.formula, b2.value]))
  })

  await section('MENU-C320 删除被保护的工作表', async () => {
    await build(numbered(3))
    await exec('review.protectSheet', {})
    await deleteSheet('工作表1')
    step('MENU-C320 工作表1 连同保护一并删除，当前表为工作表2', same(await tabNames(), ['工作表2', '工作表3']) && same(await selectedTabs(), ['工作表2']))
  })

  await section('MENU-C322/C323/C342/C345 删除的保存与导出', async () => {
    await build(numbered(3))
    await deleteSheet('工作表1')
    await saveAndReopen()
    step('MENU-C322 重新打开后剩工作表2、工作表3，当前表为工作表2', same(await tabNames(), ['工作表2', '工作表3']) && same(await selectedTabs(), ['工作表2']))
    step('MENU-C342 导出件含工作表2、工作表3，顺序与界面一致', same((await savedWorkbook()).sheets.map((sheet) => sheet.name), ['工作表2', '工作表3']))
    await touch()
    await saveAndReopen('Again.xlsx')
    step('MENU-C345 导出再导入后为工作表2、工作表3', same(await tabNames(), ['工作表2', '工作表3']))
    await build(numbered(3))
    await deleteSheet('工作表2')
    await saveAndReopen()
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C323 重新打开后为工作表1、工作表3，列表一致', same(await tabNames(), ['工作表1', '工作表3']) && same(rows.map((row) => row.name), ['工作表1', '工作表3']))
  })

  await section('MENU-C341 删除后导出件的名称、顺序与颜色', async () => {
    await build(numbered(3))
    await colorTab('工作表2', RED)
    await colorTab('工作表3', BLUE)
    await deleteSheet('工作表2')
    await saveAndReopen()
    const saved = await savedWorkbook()
    step('MENU-C341 导出件不含工作表2，其余名称、顺序与颜色一致', same(saved.sheets.map((sheet) => sheet.name), ['工作表1', '工作表3']) && (await savedTabColor('工作表3')) === '#0070C0' && (await savedTabColor('工作表1')) == null)
  })

  await section('MENU-C340/C343/C344 导入件的菜单与删除', async () => {
    await imported(sheets(3), { active: 'Sheet2' })
    await rightClick('Sheet2')
    const rows = await menuRows()
    const subs = []
    for (const id of MENU_IDS) subs.push(await hasSubmenu(id))
    const deletable = await enabledRow('sheet-menu-delete')
    await closeMenus()
    step('MENU-C340 导入件菜单与自建件逐项一致，「删除」为最后一项、可点', same(rows, MENU) && same(subs, [true, false, true, false, true, false, false]) && deletable)
    await deleteSheet('Sheet1')
    await saveAndReopen()
    step('MENU-C343 导出件含 Sheet2、Sheet3，保留原名', same((await savedWorkbook()).sheets.map((sheet) => sheet.name), ['Sheet2', 'Sheet3']))
    await deleteSheet('Sheet2')
    await rightClick('Sheet3')
    const last = await enabledRow('sheet-menu-delete')
    await closeMenus()
    await saveAndReopen()
    step('MENU-C344 两次删除后仅剩 Sheet3，「删除」不可用；导出件只含 Sheet3', same(await tabNames(), ['Sheet3']) && !last && same((await savedWorkbook()).sheets.map((sheet) => sheet.name), ['Sheet3']))
  })

  // ———————————————————————————— 5. New sheet (before or after the current sheet) ————————————————————————————
  // New sheets are named as WPS names them (Sheet4 after Sheet1–Sheet3): these cases' workbooks
  // use those names where the case table writes 工作表N.
  await section('MENU-C160/C161/C162 新建工作表子菜单', async () => {
    await build(sheets(3))
    await rightClick('Sheet1')
    await sub('sheet-menu-insert')
    const rows = await subRows()
    const enabled = (await enabledRow('sheet-menu-insert-before')) && (await enabledRow('sheet-menu-insert-after'))
    step('MENU-C160 子菜单为「在当前工作表前」「在当前工作表后」，均可点', same(rows, ['在当前工作表前', '在当前工作表后']) && enabled, JSON.stringify(rows))
    step('MENU-C161 展开子菜单不直接创建：仍为 3 张', same(await order(), sheets(3)))
    const menuBox = await kit.getByTestId('sheet-menu').boundingBox()
    await page.mouse.move(menuBox.x + menuBox.width + 300, menuBox.y - 60, { steps: 6 })
    await page.waitForTimeout(300)
    step('MENU-C162 鼠标移出后子菜单收起（菜单本身按 WPS 保持，点外部才关闭）', (await kit.getByTestId('sheet-menu-insert-before').count()) === 0)
    await closeMenus()
  })

  await section('MENU-C163/C164/C171/C172/C173 在当前工作表前、后新建', async () => {
    await build(sheets(3), { active: 'Sheet2' })
    await insertAt('Sheet2', 'before')
    const tabsShown11 = await showsTabs(['Sheet1', 'Sheet4', 'Sheet2', 'Sheet3'])
    step('MENU-C163 「在当前工作表前」：Sheet1、Sheet4、Sheet2、Sheet3，新表唯一选中', tabsShown11 && same(await selectedTabs(), ['Sheet4']))
    await build(sheets(3), { active: 'Sheet2' })
    await insertAt('Sheet2', 'after')
    const tabsShown12 = await showsTabs(['Sheet1', 'Sheet2', 'Sheet4', 'Sheet3'])
    step('MENU-C164 「在当前工作表后」：Sheet1、Sheet2、Sheet4、Sheet3，新表唯一选中', tabsShown12 && same(await selectedTabs(), ['Sheet4']))
    step('MENU-C171 编辑区为新表，箭头挂在新表上', (await activeSheet()) === 'Sheet4' && (await tab('Sheet4').getByTestId('sheet-menu-arrow').count()) === 1 && (await tab('Sheet2').getByTestId('sheet-menu-arrow').count()) === 0)
    step('MENU-C173 新表为普通标签形态，不进入重命名', !(await renaming()))
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C172 列表中新行在 Sheet2 下一行并带勾选', rows[2].name === 'Sheet4' && rows[2].checked)
  })

  await section('MENU-C165/C166/C167/C168 新建的位置与内容', async () => {
    await build(sheets(3))
    await insertAt('Sheet1', 'before')
    const tabsShown13 = await showsTabs(['Sheet4', 'Sheet1', 'Sheet2', 'Sheet3'])
    step('MENU-C165 在首张前：Sheet4、Sheet1、Sheet2、Sheet3', tabsShown13 && (await order()).length === 4)
    await build(sheets(3), { active: 'Sheet3' })
    await insertAt('Sheet3', 'after')
    const tabsShown14 = await showsTabs(['Sheet1', 'Sheet2', 'Sheet3', 'Sheet4'])
    step('MENU-C166 在末张后：Sheet1、Sheet2、Sheet3、Sheet4', tabsShown14 && (await order()).length === 4)
    await build(sheets(3), { active: 'Sheet2' })
    await exec('home.bold', { range: `Sheet2!A1` })
    await insertAt('Sheet2', 'after')
    await showsTabs(['Sheet1', 'Sheet2', 'Sheet4', 'Sheet3'])
    const info = (await workbook()).sheets.find((sheet) => sheet.name === 'Sheet4')
    step('MENU-C167 新表无内容，不继承 Sheet2 的内容、样式与公式', info.cells === 0 && info.usedRange == null && (await cellOf('Sheet4!A1')).style?.font?.bold !== true)
    await build(sheets(3))
    await insertAt('Sheet3', 'after')
    const tabsShown15 = await showsTabs(['Sheet1', 'Sheet2', 'Sheet3', 'Sheet4'])
    step('MENU-C168 右键 Sheet3：新表在 Sheet3 右邻，不在 Sheet1 旁', tabsShown15)
  })

  await section('MENU-C169/C170/C264 新表的命名', async () => {
    await build(sheets(3))
    await insertAt('Sheet1', 'after')
    const tabsShown16 = await showsTabs(['Sheet1', 'Sheet4', 'Sheet2', 'Sheet3'])
    step('MENU-C169 新表命名为 Sheet4（接续最大序号），已有名称不变', tabsShown16)
    await build(sheets(4))
    await deleteSheet('Sheet2')
    await choose('Sheet1')
    await insertAt('Sheet1', 'after')
    const tabsShown17 = await showsTabs(['Sheet1', 'Sheet5', 'Sheet3', 'Sheet4'])
    step('MENU-C170 删除 Sheet2 后新建为 Sheet5，Sheet2 不被重新使用', tabsShown17 && !(await order()).includes('Sheet2'))
    await build(sheets(3), { active: 'Sheet2' })
    await insertAt('Sheet2', 'after')
    await showsTabs(['Sheet1', 'Sheet2', 'Sheet4', 'Sheet3'])
    await insertAt('Sheet4', 'after')
    const tabsShown18 = await showsTabs(['Sheet1', 'Sheet2', 'Sheet4', 'Sheet5', 'Sheet3'])
    step('MENU-C264 连续两次：第二张为 Sheet5，顺序 Sheet1、Sheet2、Sheet4、Sheet5、Sheet3', tabsShown18)
  })

  await section('MENU-C174 标签栏「新建工作表 ＋」按钮', async () => {
    await build(sheets(3), { active: 'Sheet2' })
    await kit.getByTestId('sheet-add').click()
    const tabsShown19 = await showsTabs(['Sheet1', 'Sheet2', 'Sheet3', 'Sheet4'])
    step('MENU-C174 新表在最后一位并唯一选中', tabsShown19 && same(await selectedTabs(), ['Sheet4']))
  })

  await section('MENU-C175 仅 1 张工作表时新建', async () => {
    await build(['Sheet1'])
    await rightClick('Sheet1')
    const enabled = await enabledRow('sheet-menu-insert')
    await sub('sheet-menu-insert')
    const rows = await subRows()
    await kit.getByTestId('sheet-menu-insert-after').click()
    const tabsShown20 = await showsTabs(['Sheet1', 'Sheet2'])
    step('MENU-C175 「新建工作表」可用，含「在当前工作表后」，新表在右邻，1 → 2 张', tabsShown20 && enabled && rows.includes('在当前工作表后'))
  })

  await section('MENU-C325/C326 撤销与保存新建', async () => {
    await build(sheets(3), { active: 'Sheet2' })
    await insertAt('Sheet2', 'after')
    await showsTabs(['Sheet1', 'Sheet2', 'Sheet4', 'Sheet3'])
    await undoKey()
    const tabsShown21 = await showsTabs(sheets(3))
    step('MENU-C325 Ctrl+Z 后恢复为 Sheet1、Sheet2、Sheet3，4 → 3 张', tabsShown21 && (await order()).length === 3)
    await insertAt('Sheet2', 'after')
    await showsTabs(['Sheet1', 'Sheet2', 'Sheet4', 'Sheet3'])
    await saveAndReopen()
    step('MENU-C326 重新打开后为 Sheet1、Sheet2、Sheet4、Sheet3', same(await tabNames(), ['Sheet1', 'Sheet2', 'Sheet4', 'Sheet3']))
  })

  // ———————————————————————————— 6. Move sheet (left or right) ————————————————————————————
  await section('MENU-C178/C179/C180 移动工作表子菜单', async () => {
    await build(numbered(3), { active: '工作表2' })
    await rightClick('工作表2')
    await sub('sheet-menu-move')
    const rows = await subRows()
    step('MENU-C178 子菜单为「向左移动」「向右移动」，无列表与其他控件', same(rows, ['向左移动', '向右移动']) && (await kit.getByTestId('sheet-menu').locator('.kui-submenu-popover input, .kui-submenu-popover select').count()) === 0, JSON.stringify(rows))
    step('MENU-C179 展开子菜单不改变顺序', same(await order(), numbered(3)))
    await kit.getByTestId('sheet-menu-move-right').click()
    const tabsShown22 = await showsTabs(['工作表1', '工作表3', '工作表2'])
    step('MENU-C180 点方向项后菜单与子菜单关闭，无确认框', tabsShown22 && !(await menuShown()) && !(await anyDialog()))
  })

  await section('MENU-C181/C182/C183 方向项按真实次序收敛', async () => {
    await build(numbered(3))
    await rightClick('工作表1')
    const first = await moveChoices()
    await closeMenus()
    step('MENU-C181 最左侧只呈「向右移动」', !first.left && first.right, JSON.stringify(first))
    await rightClick('工作表3')
    const last = await moveChoices()
    await closeMenus()
    step('MENU-C182 最右侧只呈「向左移动」', last.left && !last.right, JSON.stringify(last))
    await build(numbered(3), { hidden: ['工作表3'], active: '工作表2' })
    await rightClick('工作表2')
    const hiddenAfter = await moveChoices()
    await closeMenus()
    step('MENU-C183 工作表3 隐藏：标签栏只有工作表1、工作表2；工作表2 仍呈向左与向右（隐藏表占位）', same(await tabNames(), ['工作表1', '工作表2']) && hiddenAfter.left && hiddenAfter.right, JSON.stringify(hiddenAfter))
  })

  await section('MENU-C184/C185/C186 向左、向右移动一位', async () => {
    await build(numbered(3), { active: '工作表2' })
    await rightClick('工作表2')
    await moveBy('left')
    const tabsShown23 = await showsTabs(['工作表2', '工作表1', '工作表3'])
    step('MENU-C184 向左：工作表2、工作表1、工作表3', tabsShown23)
    await build(numbered(3), { active: '工作表2' })
    await rightClick('工作表2')
    await moveBy('right')
    const tabsShown24 = await showsTabs(['工作表1', '工作表3', '工作表2'])
    step('MENU-C185 向右：工作表1、工作表3、工作表2', tabsShown24)
    await build(numbered(3))
    await rightClick('工作表3')
    await moveBy('left')
    const tabsShown25 = await showsTabs(['工作表1', '工作表3', '工作表2'])
    step('MENU-C186 右键工作表3 向左：工作表3 成为当前表；工作表1 位置不变', tabsShown25 && same(await selectedTabs(), ['工作表3']))
  })

  await section('MENU-C187 菜单移动后标签栏的滚动跟随', async () => {
    await build(numbered(30), { fill: false })
    await scrollTabs('end')
    await page.waitForTimeout(150)
    const away = !(await inView('工作表1'))
    await scrollTabs('start')
    await page.waitForTimeout(150)
    await arrowMenu()
    await moveBy('right')
    await until(async () => (await order())[1] === '工作表1')
    await page.waitForTimeout(150)
    step('MENU-C187 工作表1 与右邻互换，且在标签栏可视范围内', away && (await order())[0] === '工作表2' && (await inView('工作表1')))
  })

  await section('MENU-C188 移动跨过隐藏的工作表', async () => {
    await build(numbered(3), { hidden: ['工作表3'], active: '工作表2' })
    await rightClick('工作表2')
    await moveBy('right')
    await until(async () => same(await order(), ['工作表1', '工作表3', '工作表2']))
    step('MENU-C188 工作表2 在真实次序中换到隐藏的工作表3 之后', same(await order(), ['工作表1', '工作表3', '工作表2']))
    await rightClick('工作表2')
    const choices = await moveChoices()
    await closeMenus()
    step('MENU-C188 再次展开只呈「向左移动」', choices.left && !choices.right, JSON.stringify(choices))
    await openList()
    await kit.getByTestId('sheet-list-unhide-工作表3').click()
    const restored = await showsTabs(['工作表1', '工作表3', '工作表2'])
    await closeMenus()
    step('MENU-C188 取消隐藏后标签栏为 工作表1、工作表3、工作表2', restored)
  })

  await section('MENU-C267 连续移动后方向项实时刷新', async () => {
    await build(numbered(3), { active: '工作表2' })
    await rightClick('工作表2')
    await moveBy('right')
    await showsTabs(['工作表1', '工作表3', '工作表2'])
    await rightClick('工作表2')
    const choices = await moveChoices()
    await closeMenus()
    step('MENU-C267 再次展开只呈「向左移动」', choices.left && !choices.right)
  })

  // ———————————————————————————— 7. Hide and unhide ————————————————————————————
  await section('MENU-C191/C192/C193/C194 隐藏当前工作表', async () => {
    await build(numbered(3), { active: '工作表2' })
    await clearNotices()
    await hideSheet('工作表2')
    const tabsShown26 = await showsTabs(['工作表1', '工作表3'])
    step('MENU-C191 菜单关闭，无确认框，隐藏立即生效', tabsShown26 && !(await menuShown()) && !(await anyDialog()))
    step('MENU-C192 标签栏为 工作表1、工作表3，无浮层提示', !(await notices()).length)
    step('MENU-C194 当前表落到右邻的工作表3', same(await selectedTabs(), ['工作表3']))
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C193 列表 3 行，第 2 行工作表2 呈灰色并带斜杠眼睛', rows.length === 3 && rows[1].name === '工作表2' && rows[1].hidden && rows[1].unhide === '取消隐藏', JSON.stringify(rows))
  })

  await section('MENU-C195/C196/C197 隐藏的对象与其后的编辑', async () => {
    await build(numbered(3))
    await hideSheet('工作表3')
    const tabsShown27 = await showsTabs(['工作表1', '工作表2'])
    step('MENU-C195 右键工作表3 隐藏：隐藏的是工作表3，当前表回落到工作表2', tabsShown27 && same(await selectedTabs(), ['工作表2']))
    await build(numbered(3))
    await hideSheet('工作表1')
    await showsTabs(['工作表2', '工作表3'])
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C196 标签栏为 工作表2、工作表3；列表首行为灰色的工作表1', rows[0].name === '工作表1' && rows[0].hidden && rows[1].name === '工作表2' && rows[2].name === '工作表3')
    await build(numbered(3), { active: '工作表2' })
    await hideSheet('工作表2')
    await showsTabs(['工作表1', '工作表3'])
    await clickCell(0, 0)
    await page.keyboard.type('测试')
    await page.keyboard.press('Enter')
    await until(async () => (await cellOf(`${q('工作表3')}!A1`)).value === '测试')
    step('MENU-C197 工作表3 A1 为「测试」，隐藏状态不变', (await cellOf(`${q('工作表3')}!A1`)).value === '测试' && same(await tabNames(), ['工作表1', '工作表3']))
  })

  await section('MENU-C269/C270 只剩 1 张可见表时不可隐藏', async () => {
    await build(numbered(2), { hidden: ['工作表2'] })
    await rightClick('工作表1')
    const enabled = await enabledRow('sheet-menu-hide')
    const others = []
    for (const id of ['sheet-menu-rename', 'sheet-menu-copy', 'sheet-menu-delete']) others.push(await enabledRow(id))
    const hint = await kit.getByTestId('sheet-menu-hide-hint').getAttribute('title')
    await kit.getByTestId('sheet-menu-hide').click({ force: true })
    await page.waitForTimeout(250)
    step('MENU-C270 「隐藏工作表」置灰，其余可点；悬浮提示「至少显示两个工作表时才能隐藏」', !enabled && others.every(Boolean) && hint === '至少显示两个工作表时才能隐藏', JSON.stringify([enabled, others, hint]))
    await closeMenus()
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C269 点击不执行隐藏：标签栏仍为工作表1，列表中工作表2 仍灰', same(await tabNames(), ['工作表1']) && rows[1].hidden)
  })

  await section('MENU-C327/C328 撤销与保存隐藏', async () => {
    await build(numbered(3), { active: '工作表2' })
    await hideSheet('工作表2')
    await showsTabs(['工作表1', '工作表3'])
    await undoKey()
    const tabsShown28 = await showsTabs(numbered(3))
    step('MENU-C327 Ctrl+Z 后 工作表1、工作表2、工作表3，工作表2 唯一选中', tabsShown28 && same(await selectedTabs(), ['工作表2']))
    await hideSheet('工作表2')
    await showsTabs(['工作表1', '工作表3'])
    await saveAndReopen()
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C328 重新打开后为 工作表1、工作表3，列表中工作表2 仍灰', same(await tabNames(), ['工作表1', '工作表3']) && rows[1].hidden)
  })

  await section('MENU-C199/C200/C201 取消隐藏的入口', async () => {
    await build(numbered(3), { hidden: ['工作表2'] })
    const rows = await listInfo()
    const eye = kit.getByTestId('sheet-list-unhide-工作表2')
    await eye.hover()
    const shownIn = await eye.isVisible()
    await kit.getByTestId('sheet-list-item-工作表1').hover()
    const shownOut = await eye.isVisible()
    const others = rows.filter((row) => row.unhide != null).length
    step('MENU-C199 工作表2 行呈灰色，常驻斜杠眼睛按钮（移入移出都在），无其他操作按钮', rows[1].hidden && rows[1].unhide != null && shownIn && shownOut && others === 1)
    step('MENU-C200 按钮悬浮提示「取消隐藏」，可点', (await eye.getAttribute('title')) === '取消隐藏' && (await eye.isEnabled()))
    await closeMenus()
    await build(numbered(3))
    const plain = await listInfo()
    await closeMenus()
    step('MENU-C201 无隐藏表时 3 行均无眼睛按钮', plain.length === 3 && plain.every((row) => row.unhide == null))
  })

  await section('MENU-C202 工作表操作菜单中不提供「取消隐藏」', async () => {
    await build(numbered(3), { hidden: ['工作表2'] })
    await rightClick('工作表1')
    const rows = await menuRows()
    await closeMenus()
    step('MENU-C202 菜单为 7 项（云功能 3 项后置），无「取消隐藏工作表」', same(rows, MENU) && !rows.some((row) => row.includes('取消隐藏')), JSON.stringify(rows))
  })

  await section('MENU-C203/C204/C205/C274 从列表取消隐藏', async () => {
    await build(numbered(3), { hidden: ['工作表2'] })
    await openList()
    await kit.getByTestId('sheet-list-unhide-工作表2').click()
    await showsTabs(numbered(3))
    const rows = await listShown()
    step('MENU-C203 工作表2 重新呈现：工作表1、工作表2、工作表3；列表中该行灰色与眼睛消失', !rows[1].hidden && rows[1].unhide == null)
    step('MENU-C274 列表中工作表1 带勾选，工作表2、工作表3 无', same(rows.map((row) => row.checked), [true, false, false]))
    await closeMenus()
    step('MENU-C204 工作表2 位于工作表1 与工作表3 之间，名称顺序与隐藏前一致', same(await order(), numbered(3)))
    step('MENU-C205 工作表1 仍唯一选中、编辑区仍为工作表1，工作表2 未选中', same(await selectedTabs(), ['工作表1']) && (await shownA1()) === '工作表1内容')
  })

  await section('MENU-C206/C207 取消隐藏后内容与颜色保持，可再次隐藏', async () => {
    await build(numbered(3), { fill: false })
    await exec('cell.set', { cell: `${q('工作表2')}!A1`, input: '汇总' })
    await exec('sheet.tabColor', { sheet: '工作表2', color: '#FF0000' })
    await exec('sheet.hide', { sheet: '工作表2' })
    await showsTabs(['工作表1', '工作表3'])
    await openList()
    await kit.getByTestId('sheet-list-unhide-工作表2').click()
    await showsTabs(numbered(3))
    await closeMenus()
    step('MENU-C206 工作表2 A1 为「汇总」，色条仍为隐藏前的颜色', (await cellOf(`${q('工作表2')}!A1`)).value === '汇总' && (await colorOf('工作表2')) === '#FF0000')
    await hideSheet('工作表2')
    await showsTabs(['工作表1', '工作表3'])
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C207 再次隐藏：标签栏为工作表1、工作表3，列表中该行重新呈灰', rows[1].hidden)
  })

  await section('MENU-C273 多个隐藏表逐个取消隐藏', async () => {
    await build(numbered(4), { hidden: ['工作表2', '工作表4'] })
    await openList()
    await kit.getByTestId('sheet-list-unhide-工作表2').click()
    await showsTabs(numbered(3))
    const rows = await listShown()
    await closeMenus()
    step('MENU-C273 标签栏为 工作表1、工作表2、工作表3；工作表4 仍灰', rows[3].name === '工作表4' && rows[3].hidden)
  })

  await section('MENU-C330 取消隐藏后保存', async () => {
    await build(numbered(3), { hidden: ['工作表2'] })
    await openList()
    await kit.getByTestId('sheet-list-unhide-工作表2').click()
    await showsTabs(numbered(3))
    await closeMenus()
    await saveAndReopen()
    step('MENU-C330 重新打开后为 工作表1、工作表2、工作表3', same(await tabNames(), numbered(3)))
  })

  await section('MENU-C203 功能区「格式 › 取消隐藏工作表…」对话框', async () => {
    await build(numbered(3), { hidden: ['工作表2', '工作表3'] })
    await kit.getByTestId('ribbon-tab-home').click().catch(() => undefined)
    await kit.getByTestId('sheet-format-menu').click()
    await kit.getByTestId('sheet-format-unhide').click()
    await kit.getByTestId('sheet-unhide-dialog').waitFor()
    const options = await kit.getByTestId('sheet-unhide-dialog').locator('[role="option"]').allTextContents()
    await kit.getByTestId('sheet-unhide-option-工作表3').click()
    await kit.getByTestId('sheet-unhide-ok').click()
    const tabsShown29 = await showsTabs(['工作表1', '工作表3'])
    step('MENU-C203 「取消隐藏工作表…」列出隐藏的表，选工作表3 确定后它重新显示（sheet.unhide 无参数时的对话框）', tabsShown29 && same(options, ['工作表2', '工作表3']) && same(await selectedTabs(), ['工作表3']), JSON.stringify(options))
  })

  // ———————————————————————————— 8. Errors and interactions ————————————————————————————
  await section('MENU-C209 仅 1 张工作表时创建副本', async () => {
    await build(['工作表1'])
    await rightClick('工作表1')
    const enabled = await enabledRow('sheet-menu-copy')
    await closeMenus()
    const copy = await copyOf('工作表1')
    step('MENU-C209 「创建副本」可用，新增「工作表1(2)」，1 → 2 张', enabled && copy === '工作表1(2)' && (await order()).length === 2)
  })

  await section('MENU-C210 连续两次创建副本的位置', async () => {
    await build(numbered(3))
    await copyOf('工作表3')
    const first = await tabNames()
    await copyOf('工作表3')
    step('MENU-C210 第一次后为 工作表1、2、3、工作表3(2)', same(first, ['工作表1', '工作表2', '工作表3', '工作表3(2)']))
    step('MENU-C210 第二次新副本插在源表右侧、已有副本左侧', same(await tabNames(), ['工作表1', '工作表2', '工作表3', '工作表3(3)', '工作表3(2)']))
  })

  await section('MENU-C211/C220 被保护的工作表创建副本', async () => {
    await build(numbered(3))
    await ai([
      { command: 'sheet.activate', params: { sheet: '工作表2' } },
      { command: 'review.protectSheet', params: {} },
      { command: 'sheet.activate', params: { sheet: '工作表1' } },
    ])
    await copyOf('工作表2')
    const info = Object.fromEntries((await workbook()).sheets.map((sheet) => [sheet.name, sheet]))
    step('MENU-C220 副本插入在工作表2 右侧', same(await tabNames(), ['工作表1', '工作表2', '工作表2(2)', '工作表3']))
    step('MENU-C211 副本沿用源表的保护（锁定标记），不丢失', info['工作表2(2)'].protected && (await kit.getByTestId('sheet-lock-工作表2(2)').count()) === 1)
  })

  await section('MENU-C212 名称接近长度上限的表创建副本', async () => {
    await build(['1234567890123456789012345678901'])
    const copy = await copyOf('1234567890123456789012345678901')
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C212 副本名不超 31 个字符（截短后加 (2)），列表中名称与副本一致', copy === '1234567890123456789012345678(2)' && [...copy].length === 31 && rows[1].name === copy, copy)
  })

  await section('MENU-C214 大数据量工作表创建副本', async () => {
    await build(numbered(2), { fill: false, active: '工作表2' })
    const rows = 4000
    const values = Array.from({ length: rows }, (_, row) => [row + 1, `文本${row}`, (row * 7) % 13])
    await exec('range.setValues', { range: `${q('工作表2')}!A1`, values })
    await exec('cell.set', { cell: `${q('工作表2')}!D1`, input: '=A1*2+C1' })
    await exec('edit.fillDown', { range: `${q('工作表2')}!D1:D${rows}` })
    const started = Date.now()
    const copy = await copyOf('工作表2')
    const took = Date.now() - started
    const [source, target] = [(await workbook()).sheets.find((sheet) => sheet.name === '工作表2'), (await workbook()).sheets.find((sheet) => sheet.name === copy)]
    const sample = await cellOf(`${q(copy)}!D${rows}`)
    step('MENU-C214 副本在可接受时间内完成（无卡死），单元格数与公式一致，抽查取值正确', took < 15000 && source.cells === target.cells && target.cells >= rows * 4 && sample.formula === `=A${rows}*2+C${rows}` && sample.value === rows * 2 + ((rows - 1) * 7) % 13, `${took}ms ${target.cells}`)
  })

  await section('MENU-C215 连续快速多次创建副本', async () => {
    await build(numbered(3))
    const names = []
    for (let i = 0; i < 3; i += 1) names.push(await copyOf('工作表1'))
    const all = await order()
    const same3 = []
    for (const name of names) same3.push((await cellOf(`${q(name)}!A1`)).value === '工作表1内容')
    step('MENU-C215 新增 3 张副本，序号连续不重复（(2)、(3)、(4)），3 → 6 张，内容与源表一致', same([...names].sort(), ['工作表1(2)', '工作表1(3)', '工作表1(4)']) && all.length === 6 && new Set(all).size === 6 && same3.every(Boolean), JSON.stringify(all))
  })

  await section('MENU-C216 空白表创建副本', async () => {
    await build(numbered(2), { fill: false })
    const copy = await copyOf('工作表1')
    const info = (await workbook()).sheets.find((sheet) => sheet.name === copy)
    step('MENU-C216 新增空白副本，数量加 1，无报错', (await order()).length === 3 && info.cells === 0 && !(await notices()).some((text) => text.includes('失败')))
  })

  await section('MENU-C217 重命名未确认时再次创建副本', async () => {
    await build(numbered(3))
    await startCopy('工作表1')
    await rightClick('工作表1')
    await kit.getByTestId('sheet-menu-copy').click()
    await until(async () => (await order()).length === 5)
    if (await renaming()) await enter()
    const names = await order()
    step('MENU-C217 第一次副本按预填名生效，第二次为「工作表1(3)」，2 张副本不重名', names.includes('工作表1(2)') && names.includes('工作表1(3)') && new Set(names).size === 5, JSON.stringify(names))
  })

  await section('MENU-C218 源表名含下划线', async () => {
    await build(['工作表_1'])
    const first = await copyOf('工作表_1')
    const second = await copyOf('工作表_1')
    step('MENU-C218 两张副本为「工作表_1(2)」「工作表_1(3)」，与源名中的下划线不混淆', first === '工作表_1(2)' && second === '工作表_1(3)', `${first} ${second}`)
  })

  await section('MENU-C223 菜单弹出后窗口失去焦点', async () => {
    await build(numbered(3), { active: '工作表2' })
    await rightClick('工作表2')
    await blurWindow()
    await until(async () => !(await menuShown()))
    step('MENU-C223 切走时菜单关闭，切回无残留，数量与内容不变', !(await menuShown()) && same(await order(), numbered(3)) && (await shownA1()) === '工作表2内容')
  })

  await section('MENU-C277 撤销创建副本', async () => {
    await build(numbered(3), { active: '工作表2' })
    await copyOf('工作表2')
    await undoButton()
    const restored = await showsTabs(numbered(3))
    step('MENU-C277 撤销后副本移除，3 张，顺序为 工作表1、2、3，源表内容不变', restored && (await cellOf(`${q('工作表2')}!A1`)).value === '工作表2内容')
  })

  await section('MENU-C278 其它工作表引用源表的公式取值不变', async () => {
    await build(numbered(3), { fill: false, active: '工作表2' })
    await ai([
      { command: 'cell.set', params: { cell: `${q('工作表2')}!A1`, input: '10' } },
      { command: 'cell.set', params: { cell: `${q('工作表3')}!B1`, input: '=工作表2!A1' } },
    ])
    await copyOf('工作表2')
    await exec('cell.set', { cell: `${q('工作表2(2)')}!A1`, input: '99' })
    const b1 = await cellOf(`${q('工作表3')}!B1`)
    step('MENU-C278 工作表3!B1 仍为 10，仍指向工作表2', b1.value === 10 && b1.formula === '=工作表2!A1')
  })

  await section('MENU-C279 副本被拖拽移动', async () => {
    await build(numbered(3), { active: '工作表2' })
    await copyOf('工作表2')
    await drag('工作表2(2)', await end())
    const moved = await showsTabs(['工作表1', '工作表2', '工作表3', '工作表2(2)'])
    step('MENU-C279 依次为 工作表1、2、3、工作表2(2)，副本内容不变', moved && (await cellOf(`${q('工作表2(2)')}!A1`)).value === '工作表2内容')
  })

  await section('MENU-C280 源工作表删除后副本存活', async () => {
    await build(numbered(3), { active: '工作表2' })
    await exec('cell.set', { cell: `${q('工作表2')}!C1`, input: '=工作表2!B2+1' })
    await exec('cell.set', { cell: `${q('工作表2')}!C2`, input: '=工作表3!B2+1' })
    await copyOf('工作表2')
    await deleteSheet('工作表2')
    await deleteSheet('工作表3')
    const [c1, c2] = [await cellOf(`${q('工作表2(2)')}!C1`), await cellOf(`${q('工作表2(2)')}!C2`)]
    step('MENU-C280 副本名称与内容完整；指向自身的公式已是副本自己的（按 WPS/Excel），仍取值 21', (await order()).includes('工作表2(2)') && (await cellOf(`${q('工作表2(2)')}!A1`)).value === '工作表2内容' && c1.formula === "='工作表2(2)'!B2+1" && c1.value === 21, JSON.stringify(c1))
    step('MENU-C280 指向已删除工作表的公式转为错误值，公式本身不被清除', c2.formula.includes('#REF!') && JSON.stringify(c2.value).includes('#REF!'), JSON.stringify(c2))
  })

  await section('MENU-C281 副本被隐藏', async () => {
    await build(numbered(3), { active: '工作表2' })
    await copyOf('工作表2')
    await hideSheet('工作表2(2)')
    const hid = await showsTabs(numbered(3))
    const rows = await listInfo()
    await closeMenus()
    const row = rows.find((entry) => entry.name === '工作表2(2)')
    step('MENU-C281 副本标签消失，列表中该行灰色带斜杠眼睛', hid && row?.hidden && row?.unhide === '取消隐藏')
  })

  await section('MENU-C284/C332 创建副本的保存与导出', async () => {
    await build(numbered(3), { active: '工作表2' })
    await copyOf('工作表2')
    await saveAndReopen()
    step('MENU-C284 重新打开后为 工作表1、2、工作表2(2)、3，副本内容一致', same(await tabNames(), ['工作表1', '工作表2', '工作表2(2)', '工作表3']) && (await cellOf(`${q('工作表2(2)')}!A1`)).value === '工作表2内容')
    step('MENU-C332 导出件中工作表的名称与顺序与界面一致', same((await savedWorkbook()).sheets.map((sheet) => sheet.name), ['工作表1', '工作表2', '工作表2(2)', '工作表3']))
  })

  await section('MENU-C333 导入件上创建副本', async () => {
    await imported(sheets(2))
    await exec('home.bold', { range: 'Sheet1!A1' })
    const copy = await copyOf('Sheet1')
    const [a, b] = [await cellOf('Sheet1!A1'), await cellOf(`${q(copy)}!A1`)]
    step('MENU-C333 命名规则同自建件（Sheet1(2)），内容格式一致，2 → 3 张', copy === 'Sheet1(2)' && same(a.style, b.style) && a.value === b.value && (await order()).length === 3)
  })

  await section('MENU-C5 导入表格：右键菜单', async () => {
    await imported(numbered(3))
    await rightClick('工作表2')
    const rows = await menuRows()
    await closeMenus()
    step('MENU-C5 导入件右键「工作表2」：菜单与新建表格一致', same(rows, MENU))
  })

  await section('MENU-C19 导入表格：副本插入在源表右侧', async () => {
    await imported(numbered(3), { active: '工作表2' })
    await copyOf('工作表2')
    step('MENU-C19 导入件：工作表1、工作表2、工作表2(2)、工作表3', same(await tabNames(), ['工作表1', '工作表2', '工作表2(2)', '工作表3']))
  })

  await section('MENU-C163 导入表格：在当前工作表前新建', async () => {
    await imported(sheets(3), { active: 'Sheet2' })
    await insertAt('Sheet2', 'before')
    step('MENU-C163 导入件：Sheet1、Sheet4、Sheet2、Sheet3', await showsTabs(['Sheet1', 'Sheet4', 'Sheet2', 'Sheet3']))
  })

  await section('MENU-C184 导入表格：向左移动', async () => {
    await imported(numbered(3), { active: '工作表2' })
    await rightClick('工作表2')
    await moveBy('left')
    step('MENU-C184 导入件：工作表2、工作表1、工作表3', await showsTabs(['工作表2', '工作表1', '工作表3']))
  })

  await section('MENU-C192 导入表格：隐藏工作表', async () => {
    await imported(numbered(3), { active: '工作表2' })
    await hideSheet('工作表2')
    step('MENU-C192 导入件：标签栏为 工作表1、工作表3', await showsTabs(['工作表1', '工作表3']))
  })

  await section('MENU-C203 导入表格：从列表取消隐藏', async () => {
    await imported(numbered(3), { hidden: ['工作表2'] })
    await openList()
    await kit.getByTestId('sheet-list-unhide-工作表2').click()
    const back = await showsTabs(numbered(3))
    await closeMenus()
    step('MENU-C203 导入件：工作表2 重新呈现在原位', back)
  })

  await section('MENU-C101 导入表格：删除当前工作表', async () => {
    await imported(numbered(3))
    await deleteSheet('工作表1')
    step('MENU-C101 导入件：剩 工作表2、工作表3', await showsTabs(['工作表2', '工作表3']))
  })

  await section('MENU-C78 导入表格：重命名', async () => {
    await imported(numbered(3))
    await renameTo('工作表1', '三月数据')
    step('MENU-C78 导入件：名称变为「三月数据」', await showsTabs(['三月数据', '工作表2', '工作表3']))
  })

  await section('MENU-C51 导入表格：改色', async () => {
    await imported(numbered(3))
    await colorTab('工作表1', RED)
    step('MENU-C51 导入件：标签底部为红色色条', (await band('工作表1')).color === rgb('#FF0000'))
  })

  // ———————————————————————————— 9. View only ————————————————————————————
  await section('MENU-C219/C231/C249/C250/C261/C262/C265/C266/C271/C275/C252/C253 仅可查看时', async () => {
    await build(numbered(3), { hidden: [] })
    await exec('sheet.tabColor', { sheet: '工作表1', color: '#FF0000' })
    await openAsViewer(numbered(3))
    const before = { tabs: await tabNames(), selected: await selectedTabs(), color: await colorOf('工作表1') }
    for (const name of ['工作表2', '工作表1']) {
      await tab(name).click({ button: 'right' })
      await page.waitForTimeout(250)
    }
    const menu = await menuShown()
    const after = { tabs: await tabNames(), selected: await selectedTabs(), color: await colorOf('工作表1') }
    step('MENU-C219/C266/C271 右键「工作表2」不弹菜单，顺序、名称、选中与隐藏状态不变', !menu && same(before, after), JSON.stringify(after))
    step('MENU-C231/C249/C253/C261/C265 右键「工作表1」不弹菜单，数量、名称、顺序与颜色不变', !menu && same(after.tabs, numbered(3)) && after.color === '#FF0000')
    await tab('工作表1').dblclick()
    await page.waitForTimeout(250)
    step('MENU-C250/C252 双击不进入重命名，名称与当前表不变', !(await renaming()) && same(await selectedTabs(), ['工作表1']))
    step('MENU-C262 单击工作表2 可正常切换', await choose('工作表2'))
    await openEditor('')
    await build(numbered(3), { hidden: ['工作表2'] })
    await openAsViewer(['工作表1', '工作表3'])
    const rows = await listInfo()
    await closeMenus()
    step('MENU-C275 仅可查看：列表中工作表2 灰色，常驻眼睛，悬浮提示「取消隐藏」', rows[1].hidden && rows[1].unhide === '取消隐藏')
    await openEditor('')
    await build(numbered(3))
    await rightClick('工作表1')
    const editable = await menuRows()
    await closeMenus()
    step('MENU-C252 进入编辑后右键弹出含「重命名」的完整菜单，与自建件一致', same(editable, MENU))
  })
} catch (error) {
  await h.finish(error)
}
await h.finish()
