// Makes public/sample/Sample.xlsx with the engine itself: a small sales table with formulas.
import fs from 'node:fs/promises'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = fileURLToPath(new URL('..', import.meta.url))
const engine = await import(pathToFileURL(`${web}public/engine/kition_sheet_kit.js`).href)
await engine.default({ module_or_path: await fs.readFile(`${web}public/engine/kition_sheet_kit_bg.wasm`) })
const doc = engine.SheetDocument.create('Sample.xlsx', 1, 1)
const run = (command, params = {}) => doc.execute(command, JSON.stringify(params))
const rows = [
  ['Region', 'Q1', 'Q2', 'Q3', 'Q4', 'Total'],
  ['North 北区', 120, 135, 150, 170],
  ['South 南区', 98, 110, 126, 140],
  ['East 东区', 76, 88, 95, 101],
  ['West 西区', 64, 72, 80, 93],
]
rows.forEach((row, index) => row.forEach((value, column) => run('cell.set', { cell: `${'ABCDEF'[column]}${index + 1}`, input: String(value) })))
for (let row = 2; row <= 5; row += 1) run('cell.set', { cell: `F${row}`, input: `=SUM(B${row}:E${row})` })
run('cell.set', { cell: 'A6', input: 'Total' })
for (const column of 'BCDEF') run('cell.set', { cell: `${column}6`, input: `=SUM(${column}2:${column}5)` })
run('select.set', { range: 'A1:F1' })
run('format.bold')
run('format.fillColor', { color: '#DDEBF7' })
run('select.set', { range: 'A6:F6' })
run('format.bold')
run('conditionalFormat.apply', { range: 'F2:F5', rule: { type: 'dataBar' } })
run('conditionalFormat.apply', { range: 'B2:E5', rule: { type: 'colorScale' } })
run('cell.set', { cell: 'H2', input: 'Totals per region for the year; the bars compare them at a glance.' })
run('column.setWidth', { cols: 'H:H', width: 180 })
run('select.set', { range: 'H2' })
run('format.wrapText')
run('row.setHeight', { rows: '2:2', height: 48 })
run('chart.insert', { range: 'A1:E5', type: 'column', at: 'H4', title: 'Sales by quarter' })
run('select.set', { range: 'A1' })
await fs.mkdir(`${web}public/sample`, { recursive: true })
await fs.writeFile(`${web}public/sample/Sample.xlsx`, doc.save())
console.log('wrote public/sample/Sample.xlsx')
