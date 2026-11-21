// Drawing objects in a real browser, through the development host: node tests/drawing.mjs [chromium|webkit]
// Inserts a shape from the shared Insert controls, moves, resizes and formats it with the pointer
// and the Shape Format tab, types its text, undoes, saves to .xlsx, reopens, edits it again, and
// checks that a viewer cannot edit it.
import assert from 'node:assert/strict'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
const kit = page.frameLocator('#kit')
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const run = (command, params = {}) => page.evaluate(async ({ command, params }) => (await window.devHost.connection().ai.run([{ command, params }], false)).completed[0].result, { command, params })
const until = async (check, message) => {
  for (let n = 0; n < 100; n += 1) { if (await check().catch(() => false)) return; await page.waitForTimeout(100) }
  assert.fail(message)
}
// Shapes and text boxes (the workbook's own chart is a drawing object too).
const shapes = async () => (await run('drawing.describe')).objects.filter((o) => o.kind !== 'chart')
const HEADER = { x: 46, y: 21 }
const open = async (query = '') => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US${query}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(500)
}
// The page point of sheet point (x, y) at 100%.
const at = async (x, y) => {
  const region = await kit.getByTestId('sheet-canvas-region').boundingBox()
  return { x: region.x + HEADER.x + x, y: region.y + HEADER.y + y }
}
const drag = async (from, to) => {
  const a = await at(from[0], from[1])
  const b = await at(to[0], to[1])
  await page.mouse.move(a.x, a.y)
  await page.mouse.down()
  await page.mouse.move((a.x + b.x) / 2, (a.y + b.y) / 2, { steps: 3 })
  await page.mouse.move(b.x, b.y, { steps: 3 })
  await page.mouse.up()
}

try {
  await open()
  // Choose an oval in the shared Insert controls, then draw it with a drag: it is selected.
  await kit.getByTestId('ribbon-tab-insert').click()
  await kit.getByTestId('sheet-drawing-shapes').click()
  await kit.getByTestId('sheet-drawing-shape-ellipse').click()
  await drag([200, 100], [320, 160])
  await until(async () => (await shapes()).length === 1, 'the oval is drawn')
  let [shape] = await shapes()
  assert.equal(shape.geometry.preset, 'ellipse')
  assert.ok(Math.abs(shape.frame.x - 200) < 2 && Math.abs(shape.frame.w - 120) < 2, `the oval is where it was drawn: ${JSON.stringify(shape.frame)}`)
  await until(async () => kit.getByTestId('ribbon-tab-shapeFormat').isVisible(), 'the Shape Format tab shows for the selected shape')
  console.log('PASS draw a shape chosen in the shared controls')

  // Drag it: the pointer moves it, one step.
  const f = shape.frame
  await drag([f.x + f.w / 2, f.y + f.h / 2], [f.x + f.w / 2 + 100, f.y + f.h / 2 + 50])
  await until(async () => Math.abs((await shapes())[0].frame.x - (f.x + 100)) < 1.5 && Math.abs((await shapes())[0].frame.y - (f.y + 50)) < 1.5, 'dragging moves the shape')
  // Its bottom-right handle resizes it.
  shape = (await shapes())[0]
  const g = shape.frame
  await drag([g.x + g.w, g.y + g.h], [g.x + g.w + 40, g.y + g.h + 20])
  await until(async () => Math.abs((await shapes())[0].frame.w - (g.w + 40)) < 1.5, 'the handle resizes the shape')
  console.log('PASS move and resize with the pointer')

  // A rounded rectangle's yellow adjust handle rounds its corners more, in one step.
  await run('drawing.format', { ids: [(await shapes())[0].id], geometry: { preset: 'roundRect' } })
  await until(async () => (await shapes())[0].geometry.preset === 'roundRect', 'a rounded rectangle')
  const r = (await shapes())[0].frame
  const side = Math.min(r.w, r.h)
  const adjust = async () => (await shapes())[0].geometry.adjust?.adj ?? 16667
  await drag([r.x + side * 0.16667, r.y], [r.x + side * 0.16667 + 15, r.y])
  await until(async () => Math.abs((await adjust()) - (16667 + (15 / side) * 100000)) < 1500, 'the adjust handle rounds the corners')
  assert.deepEqual((await shapes())[0].frame, r, 'the frame stays')
  await kit.getByTestId('sheet-undo').click()
  await until(async () => Math.abs((await adjust()) - 16667) < 2, 'undo takes the adjustment back')
  await kit.getByTestId('sheet-redo').click()
  await until(async () => (await adjust()) > 20000, 'redo puts it back')
  console.log('PASS reshape with the adjust handle, with undo')

  // Shape Format: fill and outline.
  await kit.getByTestId('ribbon-tab-shapeFormat').click()
  await kit.getByTestId('sheet-drawing-fill').click()
  await kit.getByTestId('sheet-drawing-fill-colors').locator('button').first().click()
  await until(async () => (await shapes())[0].fill.color?.rgb != null, 'a fill colour from Shape Format')
  await kit.getByTestId('sheet-drawing-fill').click()
  await kit.getByTestId('sheet-drawing-gradient-down').click()
  await until(async () => (await shapes())[0].fill.gradient?.stops.length === 2 && (await shapes())[0].fill.gradient.angle === 90, 'a gradient of the fill colour')
  await kit.getByTestId('sheet-drawing-effects').click()
  await kit.getByTestId('sheet-drawing-shadow-bottomRight').click()
  await until(async () => (await shapes())[0].effects?.shadow?.distance === 3, 'a shadow from Shape Effects')
  await kit.getByTestId('sheet-drawing-transform').click()
  await kit.getByTestId('sheet-drawing-warp-textArchUp').click()
  await until(async () => (await shapes())[0].text?.warp?.preset === 'textArchUp', 'an art text transform from Shape Format')
  await kit.getByTestId('sheet-drawing-outline').click()
  await kit.getByTestId('sheet-drawing-no-outline').click()
  await until(async () => (await shapes())[0].stroke.none === true, 'No Outline from Shape Format')
  console.log('PASS fill and outline from Shape Format')

  // Arrange: a second shape (inserted on top, then clicked: an agent's run selects in its own
  // editor state, KHP C3b), sent to the back from Shape Format, in one step. Describing without
  // ids lists every shape bottom to top once nothing is selected.
  const first = (await shapes())[0].id
  const { id: second } = await run('drawing.insert', { geometry: { preset: 'rect' }, frame: { x: 20, y: 20, w: 60, h: 40 } })
  const placed = (await run('drawing.describe', { ids: [second] })).objects[0].frame
  const onSecond = await at(placed.x + placed.w / 2, placed.y + placed.h / 2)
  await page.mouse.click(onSecond.x, onSecond.y)
  const stack = async () => {
    const away = await at(700, 420)
    await page.mouse.click(away.x, away.y)
    return (await shapes()).map((o) => o.id)
  }
  await until(async () => kit.getByTestId('sheet-drawing-send').isEnabled(), 'Shape Format shows for the new shape')
  await kit.getByTestId('sheet-drawing-send').click()
  await kit.getByTestId('sheet-drawing-send-back').click()
  await until(async () => (await stack()).join() === [second, first].join(), 'Send to Back puts it under the first')
  await kit.getByTestId('sheet-undo').click()
  await until(async () => (await stack()).join() === [first, second].join(), 'undo puts it back on top')
  await run('drawing.delete', { ids: [second] })
  await until(async () => (await stack()).join() === first, 'one shape again')
  console.log('PASS arrange from Shape Format, with undo')

  // Group: Shift adds the first shape to a second's selection; Group makes one object of them,
  // Ungroup splits it, each one step.
  const objs = async () => {
    const away = await at(700, 420)
    await page.mouse.click(away.x, away.y)
    return (await run('drawing.describe')).objects.filter((o) => o.kind !== 'chart')
  }
  const { id: other } = await run('drawing.insert', { geometry: { preset: 'rect' }, frame: { x: 20, y: 20, w: 60, h: 40 } })
  const middleOf = async (id) => {
    const f = (await run('drawing.describe', { ids: [id] })).objects[0].frame
    return at(f.x + f.w / 2, f.y + f.h / 2)
  }
  const c1 = await middleOf(other)
  await page.mouse.click(c1.x, c1.y)
  const c2 = await middleOf(first)
  await page.keyboard.down('Shift')
  await page.mouse.click(c2.x, c2.y)
  await page.keyboard.up('Shift')
  await until(async () => kit.getByTestId('ribbon-tab-shapeFormat').isVisible(), 'Shape Format shows for the selection')
  await kit.getByTestId('ribbon-tab-shapeFormat').click()
  await until(async () => kit.getByTestId('sheet-drawing-group').isEnabled({ timeout: 500 }), 'Group is offered for two shapes')
  await kit.getByTestId('sheet-drawing-group').click()
  await until(async () => {
    const o = await objs()
    return o.length === 1 && o[0].kind === 'group' && [...o[0].members].sort().join() === [first, other].sort().join()
  }, 'Group makes one object of the two')
  const groupId = (await objs())[0].id
  const cg = await middleOf(other)
  await page.mouse.click(cg.x, cg.y)
  await until(async () => kit.getByTestId('ribbon-tab-shapeFormat').isVisible(), 'Shape Format shows for the group')
  await kit.getByTestId('ribbon-tab-shapeFormat').click()
  await until(async () => kit.getByTestId('sheet-drawing-ungroup').isEnabled({ timeout: 500 }), 'Ungroup is offered for the group')
  await kit.getByTestId('sheet-drawing-ungroup').click()
  await until(async () => (await objs()).length === 2, 'Ungroup splits it')
  await kit.getByTestId('sheet-undo').click()
  await until(async () => (await objs()).map((o) => o.id).join() === groupId, 'undo groups them again')
  await kit.getByTestId('sheet-redo').click()
  await until(async () => (await objs()).length === 2, 'redo splits them')
  await run('drawing.delete', { ids: [other] })
  await until(async () => (await objs()).map((o) => o.id).join() === first, 'one shape again')
  console.log('PASS group and ungroup from Shape Format, with undo')

  // Connect: two shapes selected, Connect joins them with a connector glued to both.
  const { id: target } = await run('drawing.insert', { geometry: { preset: 'rect' }, frame: { x: 20, y: 160, w: 60, h: 40 } })
  const t1 = await middleOf(target)
  await page.mouse.click(t1.x, t1.y)
  const t2 = await middleOf(first)
  await page.keyboard.down('Shift')
  await page.mouse.click(t2.x, t2.y)
  await page.keyboard.up('Shift')
  await until(async () => kit.getByTestId('ribbon-tab-shapeFormat').isVisible(), 'Shape Format shows for the two')
  await kit.getByTestId('ribbon-tab-shapeFormat').click()
  await until(async () => kit.getByTestId('sheet-drawing-connect').isEnabled({ timeout: 500 }), 'Connect is offered for two shapes')
  await kit.getByTestId('sheet-drawing-connect').click()
  await until(async () => (await objs()).some((o) => o.connection?.start?.id === target && o.connection?.end?.id === first), 'a connector glued to both')
  const connector = (await objs()).find((o) => o.connection).id
  await run('drawing.delete', { ids: [connector, target] })
  await until(async () => (await objs()).map((o) => o.id).join() === first, 'one shape again')
  console.log('PASS connect two shapes from Shape Format')

  // Typing on the selected shape sets its text (an input method's text arrives whole).
  shape = (await shapes())[0]
  const activeBefore = await run('cell.get', { cell: 'A1' })
  const centre = await at(shape.frame.x + shape.frame.w / 2, shape.frame.y + shape.frame.h / 2)
  await page.mouse.click(centre.x, centre.y)
  // Typed only once the engine has the shape selected and the text input has the keyboard back.
  const inspected = () => page.evaluate(async () => (await window.devHost.connection().ai.inspect()).details)
  await until(async () => (await inspected()).shape === shape.id, 'the shape is selected')
  await until(async () => kit.getByTestId('kit-text-sink').evaluate((element) => element.ownerDocument.activeElement === element), 'the text input takes focus')
  await page.keyboard.insertText('Plan 计划')
  await page.keyboard.press('Escape')
  await until(async () => (await shapes())[0].text.paragraphs[0].runs[0].text === 'Plan 计划', 'typing sets the shape text')
  assert.equal((await run('cell.get', { cell: 'A1' })).input, activeBefore.input, 'the active cell got nothing')
  // The typing is the person's step, so the person's Undo and Redo take it back and put it back
  // (an agent's edit.undo takes back only the agent's steps: KitModel 1.1 M12a).
  await kit.getByTestId('sheet-undo').click()
  await until(async () => ((await shapes())[0].text.paragraphs[0]?.runs[0]?.text ?? '') === '', 'undo takes the text back')
  await kit.getByTestId('sheet-redo').click()
  await until(async () => (await shapes())[0].text.paragraphs[0].runs[0].text === 'Plan 计划', 'redo puts it back')
  console.log('PASS type the shape text, undo and redo')

  // Save as .xlsx, reopen, and edit again.
  const before = (await shapes())[0]
  const { base64 } = await run('file.saveBytes', { format: 'xlsx' })
  await page.evaluate(async (data) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
    await window.devHost.open('Shapes.xlsx', bytes.buffer)
  }, base64)
  await until(async () => (await shapes()).length === 1 && (await shapes())[0].text.paragraphs[0].runs[0].text === 'Plan 计划', 'the shape reopens')
  const reopened = (await shapes())[0]
  assert.ok(Math.abs(reopened.frame.x - before.frame.x) < 0.5 && Math.abs(reopened.frame.w - before.frame.w) < 0.5, 'the frame survives')
  assert.equal(reopened.geometry.preset, 'roundRect')
  assert.ok(Math.abs(reopened.geometry.adjust.adj - before.geometry.adjust.adj) <= 1, 'the adjustment survives')
  assert.equal(reopened.stroke.none, true)
  await drag([reopened.frame.x + reopened.frame.w / 2, reopened.frame.y + reopened.frame.h / 2], [reopened.frame.x + reopened.frame.w / 2 + 30, reopened.frame.y + reopened.frame.h / 2])
  await until(async () => Math.abs((await shapes())[0].frame.x - (reopened.frame.x + 30)) < 1.5, 'the reopened shape moves again')
  console.log('PASS save as .xlsx, reopen and edit again')

  // Delete removes it.
  await page.keyboard.press('Delete')
  await until(async () => (await shapes()).length === 0, 'Delete removes the selected shape')
  console.log('PASS delete with the keyboard')

  // A viewer cannot change a shape.
  await open('&role=viewer')
  await page.evaluate(async (data) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
    await window.devHost.open('Shapes.xlsx', bytes.buffer)
  }, base64)
  await until(async () => (await shapes()).length === 1, 'the viewer sees the shape')
  const id = (await shapes())[0].id
  await assert.rejects(() => run('drawing.transform', { ids: [id], dx: 10 }), /read-only|not allowed|only/i)
  assert.deepEqual(errors, [])
  console.log('PASS a viewer cannot edit the shape')
} catch (error) {
  // Outside the repository: its asset check counts every untracked file.
  const shot = join(tmpdir(), `kition-sheet-drawing-${which}-failure.png`)
  await page.screenshot({ path: shot })
  console.log(`screenshot: ${shot}`)
  throw error
} finally {
  await browser.close()
  await server.close()
}
