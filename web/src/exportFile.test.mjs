// File › Export: the engine's bytes go to the host, which writes the new file; success only when
// the host answers, and the host's error otherwise.
import test from 'node:test'
import assert from 'node:assert/strict'
import { decodeBase64, exportParams, exportPath, exportThroughHost, parseAnswer } from './exportFile.ts'

const CSV = Buffer.from('Region,Sales\r\nNorth,12\r\n').toString('base64')

test('asks the engine for the whole workbook as PDF (WPS\'s Export to PDF), or one sheet\'s cells', () => {
  assert.deepEqual(exportParams('pdf', 'Sheet1'), { format: 'pdf', sheets: 'all' })
  assert.deepEqual(exportParams('png', 'Q3'), { format: 'png', sheet: 'Q3' })
  assert.deepEqual(exportParams('xlsx', 'Q3'), { format: 'xlsx' })
  assert.deepEqual(exportParams('xltx', 'Q3'), { format: 'xltx' })
  assert.deepEqual(exportParams('csv', 'Q3'), { format: 'csv', sheet: 'Q3' })
  assert.deepEqual(exportParams('html', ''), { format: 'html' })
})

test('names the new file after the workbook, in its folder', () => {
  assert.equal(exportPath('Finance/Sales.xlsx', 'Sales.xlsx', 'pdf'), 'Finance/Sales.pdf')
  assert.equal(exportPath('Sales.xlsx', 'Sales.xlsx', 'csv', 'Q3/Q4'), 'Sales - Q3_Q4.csv')
  assert.equal(exportPath('a/Plan.xlsx', '', 'html'), 'a/Plan.html')
  assert.equal(exportPath('', '', 'csv'), 'Book.csv')
  // A template is named by the person: unsafe characters become _, the ends are trimmed, 80 characters at most.
  assert.equal(exportPath('a/Plan.xlsx', 'Plan.xlsx', 'xltx', null, '  月报/模板*  '), 'a/月报_模板_.xltx')
  assert.equal(exportPath('Plan.xlsx', 'Plan.xlsx', 'xltx', null, 'x'.repeat(100)), `${'x'.repeat(80)}.xltx`)
})

test('decodes the base64 the engine answers', () => {
  assert.equal(new TextDecoder().decode(decodeBase64(CSV)), 'Region,Sales\r\nNorth,12\r\n')
})

test('refuses an answer without the file asked for', () => {
  assert.throws(() => parseAnswer('null', 'csv'), /no CSV file/)
  assert.throws(() => parseAnswer(JSON.stringify({ extension: 'pdf', data: CSV }), 'csv'), /no CSV file/)
  assert.equal(parseAnswer(JSON.stringify({ format: 'csv', extension: 'csv', mime: 'text/csv', bytes: 24, data: CSV }), 'csv').data, CSV)
})

const engine = (calls) => async (id, params) => {
  calls.push([id, params])
  return JSON.stringify({ format: params.format, extension: params.format, mime: 'x', bytes: 24, data: CSV })
}

test('hands the bytes to the host and answers the path the host wrote', async () => {
  const calls = []
  const writes = []
  const host = { init: { document: { path: 'Finance/Sales.xlsx', name: 'Sales.xlsx' } }, exportFile: async (path, bytes) => (writes.push([path, bytes]), `${path} (host)`) }
  assert.equal(await exportThroughHost(engine(calls), host, 'csv', 'Q3', ['Q3', 'Q4']), 'Finance/Sales - Q3.csv (host)')
  assert.deepEqual(calls[0], ['file.export', { format: 'csv', sheet: 'Q3' }])
  assert.equal(new TextDecoder().decode(writes[0][1]), 'Region,Sales\r\nNorth,12\r\n')
  await exportThroughHost(engine(calls), host, 'pdf', 'Q3', ['Q3', 'Q4'])
  assert.deepEqual(calls[1], ['file.export', { format: 'pdf', sheets: 'all' }])
  assert.equal(writes[1][0], 'Finance/Sales.pdf')
  await exportThroughHost(engine(calls), host, 'html', 'Sheet1', ['Sheet1'])
  assert.equal(writes[2][0], 'Finance/Sales.html', 'a one-sheet workbook needs no sheet name')
})

test('the host\'s refusal and the engine\'s error reach the caller', async () => {
  const host = { init: { document: { path: 'S.xlsx', name: 'S.xlsx' } }, exportFile: async () => { throw new Error('the disk is full') } }
  await assert.rejects(exportThroughHost(engine([]), host, 'pdf', 'Sheet1', ['Sheet1']), /the disk is full/)
  const failing = async () => { throw new Error('no sheet named `Gone`') }
  let wrote = false
  await assert.rejects(exportThroughHost(failing, { ...host, exportFile: async () => { wrote = true; return '' } }, 'csv', 'Gone', ['Gone']), /no sheet named/)
  assert.equal(wrote, false, 'nothing reaches the host when the engine fails')
})
