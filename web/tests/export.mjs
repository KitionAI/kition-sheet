// File › Export in a real browser, through the development host: node tests/export.mjs [chromium|webkit]
// The engine writes the file's bytes and the host writes them as a new file (`host/write` mode
// `export`): the editor says it exported only when the host answers, names the file the host wrote,
// and shows the host's error when it refuses. The agent's export command answers the same bytes.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await (await browser.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 150) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const names = () => page.evaluate(() => [...window.devHost.files.keys()])
// A workspace file's bytes as Latin-1 text, or null while it is not there.
const text = (name) => page.evaluate((file) => {
  const found = window.devHost.files.get(file)
  return found ? new TextDecoder('latin1').decode(found.bytes) : null
}, name)

try {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  const kit = page.frameLocator('#kit')
  const toasts = (level) => kit.locator(`.kui-toast[data-level=${level}]`).allTextContents().then((all) => all.join(' | '))
  await page.waitForTimeout(1000)
  const run = (command, params) => page.evaluate(async ([id, values]) => (await window.devHost.connection().ai.run([{ command: id, params: values }], false)).completed[0].result, [command, params])
  // The file the next export adds to the workspace.
  const exported = async (testId, extension) => {
    const before = new Set(await names())
    await kit.getByTestId(testId).click()
    let added = null
    await until(async () => {
      added = (await names()).find((name) => !before.has(name) && name.endsWith(`.${extension}`)) ?? null
      return added != null || (await toasts('error')) !== ''
    })
    return added
  }

  await kit.getByTestId('ribbon-tab-file').click()
  step('the File tab offers PDF, CSV and web page exports', (await kit.getByTestId('sheet-export-pdf').count()) === 1 && (await kit.getByTestId('sheet-export-csv').count()) === 1 && (await kit.getByTestId('sheet-export-html').count()) === 1)
  step('no path-writing export command on the File tab', (await kit.locator('[data-testid^="sheet-cmd-file.export"]').count()) === 0)

  const pdf = await exported('sheet-export-pdf', 'pdf')
  step('Export as PDF writes Sample.pdf through the host', pdf === 'Sample.pdf' && (await text(pdf)).startsWith('%PDF-'), `${pdf} ${await toasts('error')} files ${await names()}`)
  await until(async () => (await toasts('success')).includes('Sample.pdf'))
  step('says it exported, naming the file the host wrote', (await toasts('success')).includes('Sample.pdf'), await toasts('success'))

  const a1 = (await run('cell.get', { cell: 'A1' })).input
  const csv = await exported('sheet-export-csv', 'csv')
  step('Export as CSV writes the active sheet through the host', csv?.startsWith('Sample') && (await text(csv))?.startsWith(a1), `${csv}: ${(await text(csv))?.slice(0, 40)} ${await toasts('error')}`)
  const html = await exported('sheet-export-html', 'html')
  step('Export as Web Page writes the active sheet through the host', html?.startsWith('Sample') && (await text(html))?.includes('<table'), `${html} ${await toasts('error')}`)

  // A second PDF: the host never replaces a file, and the editor names the one it chose.
  const again = await exported('sheet-export-pdf', 'pdf')
  await until(async () => (await toasts('success')).includes('Sample 2.pdf'))
  step('a second export is the new file the host names', again === 'Sample 2.pdf' && (await toasts('success')).includes('Sample 2.pdf'), `${again} ${await toasts('success')}`)

  // The host refuses the write: the editor shows its error and claims nothing.
  await page.evaluate(() => {
    window.devHost.files.set = () => {
      throw new Error('the workspace disk is full')
    }
  })
  const before = (await names()).length
  await kit.getByTestId('sheet-export-csv').click()
  await until(async () => (await toasts('error')).includes('the workspace disk is full'))
  step('the host\'s refusal shows as the export\'s error', (await toasts('error')).includes('the workspace disk is full'), await toasts('error'))
  step('and no file is written', (await names()).length === before)
  await page.evaluate(() => {
    delete window.devHost.files.set
  })

  // The agent's export (kit.json `agent.export`): the open editor runs the engine's command, which
  // answers the file for the host to write.
  const agent = await run('file.export', { format: 'pdf' })
  step('the agent\'s export command answers the PDF bytes', agent?.extension === 'pdf' && atob(agent.data.slice(0, 8)).startsWith('%PDF-'), JSON.stringify(agent).slice(0, 120))
  const changes = await page.evaluate(() => window.devHost.changes.filter((state) => state.dirty).length)
  step('exporting does not change the workbook', changes === 0, `${changes} dirty changes`)
  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  failures += 1
  console.log(`FAIL ${error.stack ?? error}`)
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${failures} failed (${which})` : `all passed (${which})`)
process.exit(failures ? 1 : 0)
