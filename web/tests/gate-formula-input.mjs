// Formula entry (task F1), as WPS for Mac offers it, in a real browser through the development host:
// node tests/gate-formula-input.mjs [chromium|webkit]
// Point mode (clicks, drags, arrows, another sheet's cells), F4, the function list and the argument
// hint in the cell and the formula bar, Escape, #NAME? for a typo, the formula in the bar while the
// cell shows its value, text typed through an input method, and a circular reference. Each step
// names its case (FX-C1 …); `GATE_ONLY=C3` runs one.
import { startGate } from './gate-harness.mjs'

const h = await startGate({ prefix: 'FX-' })
/** The grid's pitch as the engine lays it out (the harness reads it when a workbook opens). */
const G = h.grid
const { page, kit, step, section, until, ai, details, q, cellOf, nameBox, formulaBar, region, clickCell, choose, build, numbered, openEditor } = h

/** The page point at the centre of a cell (column, row from 0) at 100% from the top-left. */
const centre = async (col, row) => {
  const box = await region()
  return { x: box.x + 46 + col * G.cw + 32, y: box.y + 21 + row * G.rh + 10 }
}
const editText = async () => (await details()).editText ?? null
const hints = () => kit.getByTestId('sheet-cell-hints')
const input = async (ref) => {
  const cell = await cellOf(ref)
  return cell.formula ?? cell.input ?? ''
}

try {
  await openEditor('')
  // Every case clicks cells by position: a new workbook's default row heights, whichever cases run.
  await build(numbered(2), { fill: false })

  await section('FX-C1 单击、拖动选取引用（点选模式）', async () => {
    await build(numbered(2), { fill: false })
    await ai([{ command: 'range.setValues', params: { range: `${q('工作表1')}!B1`, values: [[10], [20], [30]] } }])
    await clickCell(0, 4)
    await page.keyboard.type('=SUM(')
    const from = await centre(1, 0)
    const to = await centre(1, 2)
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    await page.mouse.move(to.x, to.y, { steps: 6 })
    await page.mouse.up()
    await until(async () => (await editText()) === '=SUM(B1:B3')
    step('FX-C1 拖过 B1:B3 插入引用，活动单元格仍为 A5', (await editText()) === '=SUM(B1:B3' && (await details()).active === 'A5', JSON.stringify(await editText()))
    await page.keyboard.type(')')
    await page.keyboard.press('Enter')
    await until(async () => (await cellOf('A5')).value === 60)
    step('FX-C1 回车后 A5 为 =SUM(B1:B3)，值 60', (await input('A5')) === '=SUM(B1:B3)' && (await cellOf('A5')).value === 60)
  })

  await section('FX-C2 方向键选取引用', async () => {
    await clickCell(0, 5)
    await page.keyboard.type('=')
    await page.keyboard.press('ArrowUp')
    await page.keyboard.press('ArrowRight')
    await until(async () => (await editText()) === '=B5')
    step('FX-C2 「=」后方向键移动引用：=B5', (await editText()) === '=B5', JSON.stringify(await editText()))
    await page.keyboard.press('Shift+ArrowUp')
    await page.keyboard.press('Shift+ArrowUp')
    await until(async () => (await editText()) === '=B3:B5')
    step('FX-C2 Shift+方向键扩展为区域：=B3:B5', (await editText()) === '=B3:B5', JSON.stringify(await editText()))
    await page.keyboard.press('Escape')
  })

  await section('FX-C3 跨工作表点选引用', async () => {
    await build(numbered(2), { fill: false })
    await ai([{ command: 'cell.set', params: { cell: `${q('工作表2')}!B2`, input: '21' } }])
    await clickCell(0, 6)
    await page.keyboard.type('=')
    await choose('工作表2')
    await clickCell(1, 1)
    await until(async () => (await editText()) === '=工作表2!B2')
    step('FX-C3 输入「=」后点工作表2 标签再点 B2：=工作表2!B2', (await editText()) === '=工作表2!B2', JSON.stringify(await editText()))
    step('FX-C3 编辑栏显示正在输入的公式', (await formulaBar()) === '=工作表2!B2', await formulaBar())
    await page.keyboard.type('*2')
    await page.keyboard.press('Enter')
    await until(async () => (await details()).sheet === '工作表1')
    const a7 = await cellOf(`${q('工作表1')}!A7`)
    step('FX-C3 回车写入工作表1!A7 并回到工作表1，值 42', (await details()).sheet === '工作表1' && a7.formula === '=工作表2!B2*2' && a7.value === 42, JSON.stringify(a7).slice(0, 160))
  })

  await section('FX-C4 F4 切换绝对/相对引用', async () => {
    await clickCell(0, 7)
    await page.keyboard.type('=B1')
    const seen = []
    await until(async () => (await editText()) === '=B1')
    for (let i = 0; i < 4; i += 1) {
      const before = await editText()
      await page.keyboard.press('F4')
      await until(async () => (await editText()) !== before)
      seen.push(await editText())
    }
    step('FX-C4 F4 依次为 $B$1、B$1、$B1、B1', JSON.stringify(seen) === JSON.stringify(['=$B$1', '=B$1', '=$B1', '=B1']), JSON.stringify(seen))
    await page.keyboard.press('Escape')
  })

  await section('FX-C5 函数名自动完成与参数提示', async () => {
    await clickCell(0, 8)
    await page.keyboard.type('=vlo')
    await hints().waitFor()
    const first = await hints().locator('li').first().getAttribute('data-name')
    step('FX-C5 输入 =vlo 弹出函数列表，首项 VLOOKUP', first === 'VLOOKUP', first)
    await page.keyboard.press('Tab')
    await until(async () => (await editText()) === '=VLOOKUP(')
    step('FX-C5 Tab 选中：=VLOOKUP(', (await editText()) === '=VLOOKUP(', JSON.stringify(await editText()))
    const call = kit.getByTestId('sheet-cell-hints-call')
    await call.waitFor()
    step('FX-C5 参数提示显示 VLOOKUP 语法，当前参数加粗', (await call.textContent()).startsWith('VLOOKUP(') && (await call.locator('b').count()) === 1, await call.textContent())
    await page.keyboard.type('1,')
    await until(async () => ((await call.locator('b').textContent()) ?? 'lookup_value') !== 'lookup_value')
    const bold = await call.locator('b').textContent()
    step('FX-C5 逗号后加粗第 2 个参数', bold != null && bold.length > 0 && !bold.startsWith('lookup'), bold)
    await page.keyboard.press('Escape')
  })

  await section('FX-C6 单击列表项插入函数', async () => {
    await clickCell(0, 9)
    await page.keyboard.type('=su')
    await hints().waitFor()
    await hints().locator('li[data-name="SUM"]').click()
    // The click on the list item takes the keyboard; the editor gives it back to the cell a frame
    // later (the runner typed `1,2)` and Enter into nothing before it did).
    await until(async () => (await editText()) === '=SUM(' && (await h.sinkFocused()))
    step('FX-C6 单击 SUM：=SUM(，仍在编辑', (await editText()) === '=SUM(' && (await details()).editing === true, JSON.stringify(await editText()))
    // TEMPORARY (plain model round 5): where the keys go when FX-C6 loses its typing.
    await kit.locator('body').evaluate((body) => {
      const doc = body.ownerDocument
      doc.defaultView.__keys = []
      doc.addEventListener('keydown', (e) => doc.defaultView.__keys.push([e.key, doc.activeElement?.getAttribute('data-testid') ?? doc.activeElement?.tagName, performance.now() | 0]), true)
    })
    const typedAt = Date.now()
    await page.keyboard.type('1,2)')
    const typedDetails = await details()
    await page.keyboard.press('Enter')
    await until(async () => (await cellOf('A10')).value === 3)
    if ((await cellOf('A10')).value !== 3) {
      const keys = await kit.locator('body').evaluate((body) => body.ownerDocument.defaultView.__keys)
      await page.waitForTimeout(2000)
      console.log('TMP-FX-C6', JSON.stringify({ ms: Date.now() - typedAt, typedDetails, keys, after: await details(), focused: await h.sinkFocused(), a10: await cellOf('A10') }))
    }
    step('FX-C6 回车后值为 3', (await cellOf('A10')).value === 3, JSON.stringify(await input('A10')))
  })

  // TEMPORARY (plain model round 5): FX-C5's steps then FX-C6's, twelve more times in cells the
  // grid shows (E11:H13), logging each (no step).
  await section('TMP FX-C6 repeated', async () => {
    for (let k = 0; k < 12; k += 1) {
      const col = 4 + (k % 4)
      const row = 10 + Math.floor(k / 4)
      const ref = `${'EFGH'[k % 4]}${row + 1}`
      const log = async (what, extra = {}) => {
        const keys = await kit.locator('body').evaluate((body) => body.ownerDocument.defaultView.__keys ?? [])
        console.log('TMP-FX-C6-REPEAT', JSON.stringify({ k, ref, what, details: await details(), focused: await h.sinkFocused(), keys, ...extra }))
      }
      await kit.locator('body').evaluate((body) => {
        const doc = body.ownerDocument
        doc.defaultView.__keys = []
        if (!doc.defaultView.__listening) {
          doc.defaultView.__listening = true
          doc.addEventListener('keydown', (e) => doc.defaultView.__keys.push([e.key, doc.activeElement?.getAttribute('data-testid') ?? doc.activeElement?.tagName, performance.now() | 0]), true)
        }
      })
      // FX-C5's steps, as the gate runs them before FX-C6.
      await clickCell(col, row)
      await page.keyboard.type('=vlo')
      if (!(await hints().waitFor({ timeout: 5000 }).then(() => true, () => false))) { await log('no list after =vlo'); await page.keyboard.press('Escape'); continue }
      await page.keyboard.press('Tab')
      await until(async () => (await editText()) === '=VLOOKUP(')
      await page.keyboard.type('1,')
      await page.keyboard.press('Escape')
      // FX-C6's.
      await clickCell(col, row)
      await page.keyboard.type('=su')
      if (!(await hints().waitFor({ timeout: 5000 }).then(() => true, () => false))) { await log('no list after =su'); await page.keyboard.press('Escape'); continue }
      await hints().locator('li[data-name="SUM"]').click()
      await until(async () => (await editText()) === '=SUM(' && (await h.sinkFocused()))
      const before = { edit: await editText(), focused: await h.sinkFocused() }
      await page.keyboard.type('1,2)')
      const typed = await details()
      await page.keyboard.press('Enter')
      await until(async () => (await cellOf(ref)).value === 3)
      const value = (await cellOf(ref)).value
      if (value === 3) console.log('TMP-FX-C6-REPEAT', JSON.stringify({ k, ref, ok: true }))
      else {
        await page.waitForTimeout(2000)
        await log('lost', { before, typedEdit: typed.editText, typedEditing: typed.editing, typedActive: typed.active, later: (await cellOf(ref)).value })
      }
    }
  })

  await section('FX-C7 编辑栏 F4 与函数列表', async () => {
    await clickCell(0, 10)
    const bar = kit.getByTestId('sheet-formula')
    await bar.click()
    await bar.pressSequentially('=B1')
    await bar.press('F4')
    await until(async () => (await bar.inputValue()) === '=$B$1')
    step('FX-C7 编辑栏中 F4：=$B$1', (await bar.inputValue()) === '=$B$1', await bar.inputValue())
    await bar.pressSequentially('+cou')
    const list = kit.getByTestId('sheet-formula-hints-list')
    await list.waitFor()
    // The list shows from the first letter: read it once it follows all of `+cou` (Chromium on a
    // runner read the `+c` list a moment before).
    await until(async () => (await bar.inputValue()) === '=$B$1+cou' && ((await list.locator('li').first().getAttribute('data-name')) ?? '').startsWith('COU'))
    step('FX-C7 编辑栏下弹出 COU… 函数列表', ((await list.locator('li').first().getAttribute('data-name')) ?? '').startsWith('COU'))
    await bar.press('Tab')
    await until(async () => (await bar.inputValue()).startsWith('=$B$1+COU') && (await bar.inputValue()).endsWith('('))
    const text = await bar.inputValue()
    step('FX-C7 Tab 插入函数名与左括号', /^=\$B\$1\+COU[A-Z.]*\($/.test(text), text)
    await bar.press('Escape')
  })

  await section('FX-C8 Esc 取消输入', async () => {
    await clickCell(0, 11)
    await page.keyboard.type('=1+2')
    await page.keyboard.press('Escape')
    await page.waitForTimeout(200)
    step('FX-C8 Esc 后 A12 仍为空', (await input('A12')) === '' && (await details()).editing === false)
  })

  await section('FX-C9 函数名拼错显示 #NAME?', async () => {
    await clickCell(0, 12)
    await page.keyboard.type('=SUMM(1)')
    await page.keyboard.press('Enter')
    await until(async () => (await cellOf('A13')).value != null)
    const shown = (await cellOf('A13')).value
    step('FX-C9 =SUMM(1) 显示 #NAME?', shown?.error === '#NAME?', JSON.stringify(shown))
  })

  await section('FX-C10 单元格显示结果、编辑栏显示公式', async () => {
    await ai([
      { command: 'range.setValues', params: { range: `${q('工作表1')}!B1`, values: [[10], [20], [30]] } },
      { command: 'cell.set', params: { cell: `${q('工作表1')}!A5`, input: '=SUM(B1:B3)' } },
    ])
    await clickCell(0, 4)
    await until(async () => (await nameBox()) === 'A5')
    step('FX-C10 选中 A5：编辑栏为 =SUM(B1:B3)，单元格值 60', (await formulaBar()) === '=SUM(B1:B3)' && (await cellOf('A5')).value === 60, await formulaBar())
  })

  await section('FX-C11 输入法输入中文的公式', async () => {
    await clickCell(0, 13)
    await page.keyboard.type('=')
    await page.keyboard.insertText('"中文"&B1')
    await page.keyboard.press('Enter')
    await until(async () => (await cellOf('A14')).value === '中文10')
    step('FX-C11 ="中文"&B1 结果为 中文10', (await cellOf('A14')).value === '中文10', JSON.stringify((await cellOf('A14')).value))
  })

  await section('FX-C12 循环引用显示 0', async () => {
    await clickCell(0, 14)
    await page.keyboard.type('=A15+1')
    await page.keyboard.press('Enter')
    await until(async () => (await cellOf('A15')).value != null)
    step('FX-C12 =A15+1 显示 0（与 WPS 一致）', (await cellOf('A15')).value === 0, JSON.stringify((await cellOf('A15')).value))
  })
} catch (error) {
  await h.finish(error)
}
await h.finish()
