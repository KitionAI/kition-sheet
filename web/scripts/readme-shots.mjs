// Takes the README's pictures of the Sheet editor (assets/readme/*.png at the repository root) on
// kition-sdk's shared runner (tools/readme-shots/README.md there): the development host in Classic
// mode, light theme, English, 1440 × 900 at device scale 2, on kition-showcase samples. Not a test;
// re-run it by hand when the editor changes:
//
//   cd web && pnpm install && pnpm run engine      # the engine and fonts in public/
//   node scripts/readme-shots.mjs [name ...]       # every shot, or only the named ones
//
// It needs kition-sdk and kition-showcase checkouts next to this repository (or KITION_SDK and
// KITION_SHOWCASE naming them). Run it with no HTTP proxy set. Every step is a person's: the Name
// Box, a click on the grid or the ribbon, typing, a shortcut. The pictures are PNG at device scale
// 2; shrink them afterwards with `pngquant --quality 70-90` (the README's are at most 400 KB each).
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const sdk = path.resolve(process.env.KITION_SDK ?? path.join(web, '..', '..', 'kition-sdk'))
const showcase = path.resolve(process.env.KITION_SHOWCASE ?? path.join(web, '..', '..', 'kition-showcase'))
const { openFile, takeShots } = await import(pathToFileURL(path.join(sdk, 'tools/readme-shots/shots.mjs')).href)
const MOD = process.platform === 'darwin' ? 'Meta' : 'Control'

/** The showcase sample each shot opens. */
const SAMPLE = {
  hero: 'cafe-chain-quarterly-rollup',
  formulas: 'cafe-chain-quarterly-rollup',
  'number-formats': 'number-format-gallery',
  filter: 'filtered-sales-orders',
  'table-styles': 'warehouse-inventory-table',
}

/** A point in the kit's frame (CSS pixels from its top left corner), in the page's coordinates. */
async function at(page, x, y) {
  const frame = await page.locator('#kit').boundingBox()
  return { x: frame.x + x, y: frame.y + y }
}

/** Selects a cell as a person does with the Name Box: its reference, then Enter. */
async function goTo(page, kit, ref) {
  await kit.getByTestId('sheet-name-box').fill(ref)
  await kit.getByTestId('sheet-name-box').press('Enter')
  await page.waitForTimeout(400)
}

await takeShots({
  web,
  sample: async (page, shot) => {
    const name = SAMPLE[shot.name]
    await openFile(page, { name: `${name}.xlsx`, bytes: fs.readFileSync(path.join(showcase, 'sheet', name, `${name}.xlsx`)) })
  },
  ready: '[data-testid="sheet-canvas-region"]',
  query: { modes: 'classic' },
  shots: [
    // A summary over three store sheets (coloured tabs); B11 sums one cell across them,
    // =SUM(Riverside:Hilltop!E5), as the formula bar shows.
    { name: 'hero', steps: async (page, { kit }) => goTo(page, kit, 'B11') },
    // A formula typed in B16: the range it names outlined in its colour on the grid, and the
    // function's argument hint under the cell.
    {
      name: 'formulas',
      steps: async (page) => {
        const cell = await at(page, 228, 524)
        await page.mouse.click(cell.x, cell.y)
        await page.waitForTimeout(400)
        await page.keyboard.type('=SUM(B11:B15', { delay: 40 })
      },
    },
    // Format Cells (Cmd/Ctrl+1) on a long date: its format code and a sample, over the gallery of
    // the same numbers in a dozen formats.
    {
      name: 'number-formats',
      steps: async (page, { kit }) => {
        await goTo(page, kit, 'I6')
        await kit.locator('.kui-text-sink').first().focus()
        await page.keyboard.press(`${MOD}+1`)
        await kit.getByTestId('sheet-format-cells').waitFor()
      },
    },
    // The Region column's filter button: sorting, the values with their counts, a search.
    {
      name: 'filter',
      steps: async (page, { kit }) => {
        const button = await at(page, 276, 294)
        await page.mouse.click(button.x, button.y)
        await kit.getByTestId('sheet-filter-menu').waitFor()
      },
    },
    // A cell in a formatted table, and Home › Format as Table: preset styles by theme colour.
    {
      name: 'table-styles',
      steps: async (page, { kit }) => {
        await goTo(page, kit, 'B8')
        await kit.getByTestId('sheet-format-table').click()
        await kit.getByTestId('sheet-table-gallery').waitFor()
      },
    },
  ],
})
