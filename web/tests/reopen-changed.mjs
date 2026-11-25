// A workbook changed outside Kition while it was closed (Kit UI protocol UI-R1, data model 7.1):
// node tests/reopen-changed.mjs [chromium|webkit]. Case sheet.platform.file-changed-outside/merge-into-replica.
//
// The host hands over the replica it stored at its last save whatever the file's stamp, so the
// editor opens the changed file with it, and the engine records what the file changed as one
// `import` change (`open_with_replica`): the outside change shows, the replica keeps its id and
// goes on from the stored version, and the sheets' and lines' object ids stay. Undo steps are the
// session's, not the replica's, so they are not looked for after a reopen.
//
// The .xlsx carries the object ids in `xl/kition/identities.json`. Two outside programs:
// - one that rewrites a shared string and keeps that part (the worksheet part is untouched, so its
//   fingerprint still matches): the ids are the file's and the replica's alike;
// - one that drops the part (as a program that does not know it does): the ids come from the
//   stored replica, which the engine writes the file onto by position. The same file opened alone
//   (no replica) shows what would happen otherwise: a new replica, with ids made from positions.
import { inflateRawSync } from 'node:zlib'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'
import { zip } from './file-builders.mjs'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
const page = await context.newPage()
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 100) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}

/** The parts of a zip (name → bytes), stored or deflated. */
function unzip(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  let end = bytes.length - 22
  while (end > 0 && view.getUint32(end, true) !== 0x06054b50) end -= 1
  const parts = {}
  let at = view.getUint32(end + 16, true)
  for (let n = view.getUint16(end + 10, true); n > 0; n -= 1) {
    const [method, size, nameLength, extra, comment, offset] = [view.getUint16(at + 10, true), view.getUint32(at + 20, true), view.getUint16(at + 28, true), view.getUint16(at + 30, true), view.getUint16(at + 32, true), view.getUint32(at + 42, true)]
    const name = new TextDecoder().decode(bytes.subarray(at + 46, at + 46 + nameLength))
    at += 46 + nameLength + extra + comment
    const start = offset + 30 + view.getUint16(offset + 26, true) + view.getUint16(offset + 28, true)
    const data = bytes.subarray(start, start + size)
    parts[name] = method === 0 ? new Uint8Array(data) : new Uint8Array(inflateRawSync(data))
  }
  return parts
}
const text = (bytes) => new TextDecoder().decode(bytes)
const encode = (value) => new TextEncoder().encode(value)

/** The ids the saved file carries: each sheet's object and its rows' and columns' objects. */
function idsOf(parts) {
  const part = parts['xl/kition/identities.json']
  if (!part) return null
  const file = JSON.parse(text(part))
  return Object.fromEntries(Object.entries(file.sheets).map(([sheetId, sheet]) => [sheetId, { object: sheet.ids.object, rows: sheet.ids.axes[0], columns: sheet.ids.axes[1] }]))
}
/** Whether every id `before` had is still where it was in `after` (lines added after it may follow). */
function kept(before, after) {
  if (!before || !after) return false
  return Object.entries(before).every(([sheetId, sheet]) => {
    const now = after[sheetId]
    return now && now.object === sheet.object && sheet.rows.every((id, i) => now.rows[i] === id) && sheet.columns.every((id, i) => now.columns[i] === id)
  })
}

try {
  await page.goto(`${origin}/dev.html?modes=classic&replica=1`)
  const opened = () => page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await opened()
  const kit = page.frameLocator('#kit')
  await kit.getByTestId('sheet-save').waitFor()
  await page.waitForTimeout(600)
  const path = await page.evaluate(() => [...window.devHost.files.keys()].find((name) => name.endsWith('.xlsx')))
  const exec = async (command, params = {}) => (await page.evaluate(async (steps) => window.devHost.connection().ai.run(steps, false), [{ command, params }])).completed[0]?.result
  const input = async (ref) => (await exec('cell.get', { cell: ref }))?.input
  const fileBytes = async () => new Uint8Array(await page.evaluate((path) => [...new Uint8Array(window.devHost.files.get(path).bytes)], path))
  const replicaInfo = () =>
    page.evaluate(async () => {
      const replica = window.devHost.connection().replica
      const b64 = (buffer) => btoa(String.fromCharCode(...new Uint8Array(buffer)))
      return { docId: await replica.info(), version: b64(await replica.version()) }
    })
  /** The size of the replica's changes since `version` (base64; '' for all of them). */
  const changesSince = (version) =>
    page.evaluate(async (version) => {
      const since = Uint8Array.from(atob(version), (c) => c.charCodeAt(0)).buffer
      const { changes } = await window.devHost.connection().replica.exportChanges(since)
      return changes.byteLength
    }, version)
  /** Saves through the host and waits until the replica taken with the saved bytes is stored. */
  const save = async () => {
    const was = await page.evaluate((path) => window.devHost.files.get(path).version, path)
    await kit.getByTestId('sheet-save').click()
    const saved = await until(async () =>
      page.evaluate(([path, was]) => {
        const file = window.devHost.files.get(path)
        return file.version > was && window.devHost.history.replica(path)?.fileStamp === String(file.version)
      }, [path, was]),
    )
    if (!saved) throw new Error('the save (and its replica) did not reach the host')
    return unzip(await fileBytes())
  }
  /** Changes the file outside the editor (`bytes`, a new version) and reopens it with the stored replica. */
  const reopenChanged = async (parts) => {
    const bytes = [...zip(parts)]
    await page.evaluate(async (bytes) => {
      document.getElementById('status').textContent = ''
      await window.devHost.reopenChanged(new Uint8Array(bytes).buffer)
    }, bytes)
    await opened()
    await kit.getByTestId('sheet-save').waitFor()
    await page.waitForTimeout(600)
  }
  /** Rewrites one shared string, as another program saving the workbook would. */
  const rewritten = (parts, from, to) => {
    const strings = text(parts['xl/sharedStrings.xml'])
    if (!strings.includes(`>${from}<`)) throw new Error(`no shared string ${from}`)
    return { ...parts, 'xl/sharedStrings.xml': encode(strings.replace(`>${from}<`, `>${to}<`)) }
  }

  // Edits and a save: a row inserted gives the rows after it ids that are not their positions'.
  step('A1 holds Region', (await input('A1')) === 'Region', await input('A1'))
  await exec('home.insertRows', { rows: '3:3' })
  await exec('cell.set', { cell: 'B3', input: 'Inserted' })
  const first = await save()
  const ids1 = idsOf(first)
  const at1 = await replicaInfo()
  step('the saved workbook carries its ids (xl/kition/identities.json)', ids1 != null && Object.values(ids1).some((sheet) => sheet.rows.length > 3), JSON.stringify(ids1)?.slice(0, 200))
  step('the host keeps the replica beside it', at1.docId != null && (await page.evaluate((path) => window.devHost.history.replica(path) != null, path)))

  // Another program changes a shared string and keeps the ids part (the worksheet part as it was).
  await reopenChanged(rewritten(first, 'Region', 'Area'))
  step('KEPT: the outside change shows', (await input('A1')) === 'Area', await input('A1'))
  step('KEPT: the edits of the last save are there', (await input('B3')) === 'Inserted', await input('B3'))
  const at2 = await replicaInfo()
  step('KEPT: the stored replica opened (same replica id)', at2.docId === at1.docId, `${at2.docId} vs ${at1.docId}`)
  step('KEPT: the file was recorded onto it (a new version)', at2.version !== at1.version)
  const sinceSaved = await changesSince(at1.version)
  const all = await changesSince('')
  step('KEPT: the stored history goes on: what came since the saved version is the import alone', sinceSaved > 0 && sinceSaved < all / 4, `${sinceSaved} of ${all} bytes`)
  step('KEPT: the host still keeps the replica', await page.evaluate((path) => window.devHost.history.replica(path) != null, path))
  await exec('cell.set', { cell: 'C2', input: '7' })
  const second = await save()
  const ids2 = idsOf(second)
  step('KEPT: the sheets\' and lines\' ids survive the outside change', kept(ids1, ids2), JSON.stringify(ids2)?.slice(0, 200))

  // Another program drops the ids part (and its relationship and content type) and changes A1.
  const dropped = rewritten(second, 'Area', 'Zone')
  delete dropped['xl/kition/identities.json']
  dropped['xl/_rels/workbook.xml.rels'] = encode(text(dropped['xl/_rels/workbook.xml.rels']).replace(/<Relationship [^>]*kition\/identities\.json[^>]*\/>/, ''))
  dropped['[Content_Types].xml'] = encode(text(dropped['[Content_Types].xml']).replace(/<Override [^>]*kition\/identities\.json[^>]*\/>/, ''))
  step('DROPPED: the outside file has no ids part', !text(dropped['xl/_rels/workbook.xml.rels']).includes('identities') && !text(dropped['[Content_Types].xml']).includes('identities'))
  await reopenChanged(dropped)
  step('DROPPED: the outside change shows', (await input('A1')) === 'Zone', await input('A1'))
  step('DROPPED: the inserted row is still row 3', (await input('B3')) === 'Inserted', await input('B3'))
  const at3 = await replicaInfo()
  step('DROPPED: the stored replica opened (same replica id)', at3.docId === at1.docId, `${at3.docId} vs ${at1.docId}`)
  step('DROPPED: the file was recorded onto it', at3.version !== at2.version)
  await exec('cell.set', { cell: 'C3', input: '8' })
  const third = await save()
  const ids3 = idsOf(third)
  step('DROPPED: the ids come back from the stored replica', kept(ids2, ids3), JSON.stringify(ids3)?.slice(0, 200))

  // The same file opened alone (no replica): another replica, whose ids are made from positions.
  const alone = [...zip(dropped)]
  await page.evaluate(async ([path, bytes]) => {
    document.getElementById('status').textContent = ''
    await window.devHost.open(path, new Uint8Array(bytes).buffer)
  }, [path, alone])
  await opened()
  await kit.getByTestId('sheet-save').waitFor()
  await page.waitForTimeout(600)
  await exec('cell.set', { cell: 'C3', input: '8' })
  const fourth = await save()
  const at4 = await replicaInfo()
  step('ALONE: another replica (another id)', at4.docId !== at1.docId, `${at4.docId}`)
  step('ALONE: without the replica the ids of the rows after the inserted one are not kept', !kept(ids2, idsOf(fourth)))
  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  step('the run finished', false, error instanceof Error ? error.message.split('\n')[0] : String(error))
} finally {
  await browser.close()
  await server.close()
}
if (failures > 0) {
  console.log(`${failures} step(s) failed`)
  process.exit(1)
}
console.log('all steps passed')
