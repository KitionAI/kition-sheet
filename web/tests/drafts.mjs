// An agent's draft and a restored version in the editor (Kit UI protocol sections 7d and 7m), on the
// sample workbook through the development host with `?checkpoints=1` (replica and checkpoints):
// node tests/drafts.mjs [chromium|webkit]
// DR-C1 the person types into a cell; the agent's draft changes another one; the comparison lists
// it and leaves the document as it is; the person types into a third cell meanwhile; the draft is
// accepted: the grid holds all three, and the draft's number recalculates the row's total.
// DR-C2 one Undo (the editor's button) takes back the draft's cell only; Redo brings it back.
// DR-C3 a labelled checkpoint of the saved workbook; a later edit; the checkpoint restored through
// the host: the cell shows the checkpoint's content; one Undo brings the later edit back.
// Each case needs the one before it.
import { startGate } from './gate-harness.mjs'

const h = await startGate({ prefix: 'DR-' })
const { page, kit, step, section, until, cellOf, nameBox, formulaBar, clickCell } = h

/** The agent's steps on its draft (`draft: true`): the live workbook stays as it is. */
const drafted = (steps) => page.evaluate(async (steps) => window.devHost.connection().ai.run(steps, false, { draft: true }), steps)
const draftValue = async (ref) => (await drafted([{ command: 'cell.get', params: { cell: ref } }])).completed[0]?.result?.value
const value = async (ref) => (await cellOf(ref)).value
const version = () => page.evaluate(async () => window.devHost.connection().ai.version())
/** The person clicks a cell (column, row from 0), types `text` and Enter, as in the grid. */
const typeInto = async (col, row, ref, text) => {
  await clickCell(col, row)
  await page.keyboard.type(text)
  await page.keyboard.press('Enter')
  if (!(await until(async () => String(await value(ref)) === text))) throw new Error(`${ref} did not take ${text}: ${JSON.stringify(await cellOf(ref))}`)
}
/** The editor's own Undo or Redo button, as a person presses it. */
const press = async (id) => {
  if (!(await until(() => kit.getByTestId(id).isEnabled(), 30))) throw new Error(`${id} is not enabled`)
  await kit.getByTestId(id).click()
}
/** What the editor shows for a cell once the person clicks it: the formula bar (the Name Box naming it). */
const shown = async (col, row, ref) => {
  await clickCell(col, row)
  await until(async () => (await nameBox()) === ref, 20)
  return `${await nameBox()} ${await formulaBar()}`
}

try {
  await h.openEditor('&checkpoints=1')
  // The sample: row 1 the headers (A1 Region, G1 empty), row 2 North 北区 with B2 120 and
  // F2 =SUM(B2:E2) 575.

  await section('DR-C1 草稿与此间编辑一起合入', async () => {
    await typeInto(6, 0, 'G1', 'Person')
    await drafted([{ command: 'cell.set', params: { cell: 'B2', input: '999' } }])
    step('DR-C1 草稿中 B2 为 999', (await draftValue('B2')) === 999, JSON.stringify(await draftValue('B2')))
    const before = await version()
    const compared = await page.evaluate(async () => window.devHost.connection().draft.compare({ maxSize: 256 }))
    const listed = compared.summary.created.length + compared.summary.changed.length + compared.summary.texts.length
    step('DR-C1 比较列出草稿的修改', listed > 0, JSON.stringify(compared.summary).slice(0, 300))
    step('DR-C1 比较不改文档：B2 仍为 120，版本不变', (await value('B2')) === 120 && (await version()) === before, JSON.stringify(await value('B2')))
    await typeInto(0, 0, 'A1', 'Region 2')
    const merged = await page.evaluate(async () => window.devHost.connection().draft.merge({ kind: 'agent', principal: 'agent:test' }))
    step('DR-C1 草稿合入', merged.merged === true, JSON.stringify(merged).slice(0, 200))
    await until(async () => (await value('B2')) === 999)
    step('DR-C1 合入后 B2 999、F2 合计 1454，此间的 G1 与 A1 都在', (await value('B2')) === 999 && (await value('F2')) === 1454 && (await value('G1')) === 'Person' && (await value('A1')) === 'Region 2', JSON.stringify([await value('B2'), await value('F2'), await value('G1'), await value('A1')]))
  })

  await section('DR-C2 一次撤销只撤回草稿', async () => {
    await press('sheet-undo')
    await until(async () => (await value('B2')) === 120)
    step('DR-C2 撤销后 B2 回到 120、F2 575，G1 与 A1 仍在', (await value('B2')) === 120 && (await value('F2')) === 575 && (await value('G1')) === 'Person' && (await value('A1')) === 'Region 2', JSON.stringify([await value('B2'), await value('F2'), await value('G1'), await value('A1')]))
    await press('sheet-redo')
    await until(async () => (await value('B2')) === 999)
    step('DR-C2 重做后 B2 又为 999', (await value('B2')) === 999 && (await value('F2')) === 1454, JSON.stringify([await value('B2'), await value('F2')]))
    // What the grid's own controls show for the cells (the formula bar, after a click).
    step('DR-C2 编辑栏显示 B2 999、F2 公式、G1 Person、A1 Region 2', (await shown(1, 1, 'B2')) === 'B2 999' && (await shown(5, 1, 'F2')) === 'F2 =SUM(B2:E2)' && (await shown(6, 0, 'G1')) === 'G1 Person' && (await shown(0, 0, 'A1')) === 'A1 Region 2', await formulaBar())
  })

  await section('DR-C3 还原检查点，撤销取回之后的编辑', async () => {
    const path = h.docPath()
    const saved = await page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, path)
    await kit.getByTestId('sheet-save').click()
    if (!(await until(async () => (await page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, path)) > saved))) throw new Error('the save did not reach the host')
    if (!(await until(() => page.evaluate((path) => window.devHost.history.replica(path) !== null, path)))) throw new Error('no replica stored with the save')
    const checkpoint = await page.evaluate(() => window.devHost.recordCheckpoint({ origin: 'save', label: 'Before the later edit' }))
    const listed = await page.evaluate((path) => window.devHost.history.checkpoints(path), path)
    step('DR-C3 带标签的检查点已记录', Boolean(checkpoint?.id) && listed.some((c) => c.id === checkpoint.id && c.label === 'Before the later edit'), JSON.stringify(listed).slice(0, 300))
    await typeInto(6, 0, 'G1', 'Later')
    await page.evaluate((id) => window.devHost.restoreCheckpoint(id), checkpoint.id)
    await until(async () => (await value('G1')) === 'Person')
    step('DR-C3 还原后 G1 为检查点的 Person，B2 999', (await value('G1')) === 'Person' && (await value('B2')) === 999 && (await shown(6, 0, 'G1')) === 'G1 Person', JSON.stringify([await value('G1'), await value('B2')]))
    await press('sheet-undo')
    await until(async () => (await value('G1')) === 'Later')
    step('DR-C3 一次撤销取回之后的 Later', (await value('G1')) === 'Later' && (await shown(6, 0, 'G1')) === 'G1 Later', JSON.stringify(await value('G1')))
  })
  step('DR 页面没有错误', h.errors.length === 0, h.errors.slice(0, 3).join(' | '))
} catch (error) {
  await h.finish(error)
}
await h.finish()
