// Comments a host keeps (Kit UI protocol 2.6 section 7k), in a real browser: node tests/comments.mjs [chromium|webkit]
// A mocked host (tests/comments-host.html, as Kition One serves KitEmbed 1.7): Review › New Comment
// sends `comment.create` with the selected cells' anchor, the host's thread comes back as a marker
// at the range's top-right corner that follows inserted rows and columns, and a click on it sends
// `comments/activated`. A viewer sees and opens threads but makes none; a commenter makes them in a
// read-only workbook; a host that keeps no comments leaves New Comment to the file.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
let failures = 0
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}

/** Opens the workbook in the mocked host (`role=…`, `comments=0`), or the one another page saved (`seed`). */
async function open(query, seed = null) {
  const page = await (await browser.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
  if (seed) await page.addInitScript((saved) => { window.commentHostSeed = saved }, seed)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto(`${origin}/tests/comments-host.html?${query}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent === 'open', null, { timeout: 60000 })
  const kit = page.frameLocator('#kit')
  await kit.getByTestId('ribbon-tab-review').waitFor({ timeout: 30000 })
  const until = async (check, tries = 100) => {
    for (let i = 0; i < tries; i += 1) {
      if (await check().catch(() => false)) return true
      await page.waitForTimeout(100)
    }
    return false
  }
  const run = (command, params) => page.evaluate(async ([id, values]) => {
    const answer = await window.commentHost.connection().ai.run([{ command: id, params: values }], false)
    return answer.completed[0]?.result ?? answer
  }, [command, params])
  const host = () => page.evaluate(() => ({ created: window.commentHost.created, activated: window.commentHost.activated, threads: window.commentHost.threads }))
  const markers = kit.getByTestId('sheet-comment-marker')
  const toasts = () => kit.locator('.kui-toast[data-level=error]').allTextContents().then((all) => all.join(' | '))
  return { page, kit, errors, until, run, host, markers, toasts }
}

let seed = null
try {
  // The person who edits: New Comment goes to the host.
  {
    const { page, kit, errors, until, run, host, markers, toasts } = await open('role=editor')
    await kit.getByTestId('ribbon-tab-review').click()
    step('Review offers New Comment for the host', (await kit.getByTestId('sheet-host-comment').count()) === 1)
    step('and not the file\'s own New Comment', (await kit.getByTestId('sheet-cmd-review.newComment').count()) === 0)
    step('the file\'s other comment commands stay', (await kit.getByTestId('sheet-cmd-review.deleteComment').count()) === 1)
    step('the kit lists comment-anchors', await page.evaluate(() => window.commentHost.connection().capabilities.includes('comment-anchors')))

    await run('selection.set', { range: 'B3', active: 'B3' })
    await kit.getByTestId('sheet-host-comment').click()
    await until(async () => (await host()).created.length === 1)
    const first = (await host()).created[0]
    step('New Comment sends comment.create with the cell\'s anchor', first?.anchor?.ids?.length === 3 && /!B3$/.test(first.anchor.label), JSON.stringify(first))
    await until(async () => (await markers.count()) === 1)
    step('the host\'s thread shows as a marker', (await markers.count()) === 1)
    step('the marker names its thread', (await markers.first().getAttribute('data-thread')) === 'thread-1')
    const before = await markers.first().boundingBox()

    await markers.first().click()
    await until(async () => (await host()).activated.length === 1)
    step('clicking the marker opens the thread in the host', JSON.stringify((await host()).activated) === '["thread-1"]', JSON.stringify((await host()).activated))
    step('and keeps the selection', (await kit.getByTestId('sheet-status').textContent()).includes('B3'), await kit.getByTestId('sheet-status').textContent())

    // Rows and a column inserted before the cell: the marker and the anchor move with it.
    await run('home.insertRows', { rows: '1:2' })
    await run('home.insertColumns', { cols: 'A:A' })
    await until(async () => {
      const box = await markers.first().boundingBox()
      return box != null && box.y > (before?.y ?? 0) + 30 && box.x > (before?.x ?? 0) + 30
    })
    const after = await markers.first().boundingBox()
    step('the marker follows inserted rows and columns', after && before && after.y > before.y + 30 && after.x > before.x + 30, `${JSON.stringify(before)} -> ${JSON.stringify(after)}`)
    const placed = await run('editor.commentPlaces', { anchors: [{ ids: first.anchor.ids }] })
    step('the anchor is at C5 now', placed?.places?.[0]?.range === 'C5', JSON.stringify(placed))
    const located = await page.evaluate(async (ids) => window.commentHost.connection().locateComments([{ ids }, { ids: ['nowhere', 'r', 'c'] }]), first.anchor.ids)
    step('comments/locate finds it, and not a foreign anchor', JSON.stringify(located) === '[true,false]', JSON.stringify(located))

    // A range, and the person cancelling in the host.
    await run('selection.set', { range: 'D6:E8', active: 'D6' })
    await page.evaluate(() => window.commentHost.cancelNext())
    await kit.getByTestId('sheet-host-comment').click()
    await until(async () => (await host()).created.length === 2)
    const cancelled = (await host()).created[1]
    step('a range\'s anchor names both corners', cancelled?.anchor?.ids?.length === 5 && /!D6:E8$/.test(cancelled.anchor.label), JSON.stringify(cancelled))
    await page.waitForTimeout(300)
    step('cancelling makes no thread and shows no error', (await markers.count()) === 1 && (await toasts()) === '', await toasts())

    // A resolved thread is not marked.
    await page.evaluate(() => window.commentHost.resolve('thread-1'))
    await until(async () => (await markers.count()) === 0)
    step('a resolved thread has no marker', (await markers.count()) === 0)
    await page.evaluate(() => window.commentHost.resolve('thread-1', false))
    await until(async () => (await markers.count()) === 1)

    // The cell's row deleted: the anchor goes with it.
    await run('home.deleteRows', { rows: '5:5' })
    await until(async () => (await markers.count()) === 0)
    step('a thread whose cell was deleted has no marker', (await markers.count()) === 0)

    // A thread that stays, and the workbook saved with its replica (One's documents keep one).
    await run('selection.set', { range: 'B2', active: 'B2' })
    await kit.getByTestId('sheet-host-comment').click()
    await until(async () => (await markers.count()) === 1)
    await page.evaluate(() => window.commentHost.connection().save())
    await until(async () => (await page.evaluate(() => window.commentHost.saved())) != null)
    seed = await page.evaluate(() => window.commentHost.saved())
    step('the workbook is saved with its replica', seed != null && seed.threads.length === 2)
    step('no page errors', errors.length === 0, errors.join(' | '))
    await page.context().close()
  }

  // A viewer: sees the host's threads and opens them, makes none.
  {
    const { page, kit, errors, until, host, markers } = await open('role=viewer', seed)
    await kit.getByTestId('ribbon-tab-review').click()
    step('a viewer gets no New Comment', (await kit.getByTestId('sheet-host-comment').count()) === 0)
    await until(async () => (await markers.count()) === 1)
    step('a viewer sees the editor\'s thread, not the one whose cell was deleted', (await markers.count()) === 1 && (await markers.first().getAttribute('data-thread')) === 'thread-2')
    await markers.first().click()
    await until(async () => (await host()).activated.length === 1)
    step('and clicking it opens the thread', JSON.stringify((await host()).activated) === '["thread-2"]', JSON.stringify((await host()).activated))
    step('no page errors (viewer)', errors.length === 0, errors.join(' | '))
    await page.context().close()
  }

  // A commenter: a read-only workbook, and New Comment goes to the host.
  {
    const { page, kit, errors, until, run, host, markers, toasts } = await open('role=commenter', seed)
    await kit.getByTestId('ribbon-tab-review').click()
    step('a commenter gets New Comment', (await kit.getByTestId('sheet-host-comment').count()) === 1)
    await run('selection.set', { range: 'C4', active: 'C4' })
    await kit.getByTestId('sheet-host-comment').click()
    await until(async () => (await host()).created.length === 1)
    step('a commenter\'s comment reaches the host', /!C4$/.test((await host()).created[0]?.anchor?.label ?? ''), `${JSON.stringify((await host()).created)} ${await toasts()}`)
    await until(async () => (await markers.count()) === 2)
    step('and shows as a marker beside the editor\'s', (await markers.count()) === 2)
    step('no page errors (commenter)', errors.length === 0, errors.join(' | '))
    await page.context().close()
  }

  // A host that keeps no comments: New Comment writes into the file, as before.
  {
    const { page, kit, errors } = await open('comments=0')
    await kit.getByTestId('ribbon-tab-review').click()
    step('without a comment host, Review keeps the file\'s New Comment', (await kit.getByTestId('sheet-cmd-review.newComment').count()) === 1 && (await kit.getByTestId('sheet-host-comment').count()) === 0)
    step('no page errors (no comment host)', errors.length === 0, errors.join(' | '))
    await page.context().close()
  }
} catch (error) {
  failures += 1
  console.log(`FAIL ${error.stack ?? error}`)
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${failures} failed (${which})` : `all passed (${which})`)
process.exit(failures ? 1 : 0)
