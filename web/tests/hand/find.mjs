// Find, Replace and Go To in the editor's own window (plan stage 4A, section 2 row 20; walkthrough
// Q1, Q2, C10; WPS W-O1–W-O4): Cmd+F, Ctrl+H and Ctrl+G open a small window beside the grid at
// their tab; Enter finds the next match by rows and the cell found becomes the active cell, in
// view; Shift+Enter the one before; a match not there says so in the window; Escape closes it and
// the grid has the keyboard, the active cell where it was found; the grid stays usable while it is
// open; Replace All is one undo step; Go To selects a reference and closes, an invalid one says so.
// A person's keys and clicks; the engine's state read back.
// node tests/hand/find.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 250) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
const win = (e) => e.kit.getByTestId('sheet-find')
const shown = (e) => win(e).isVisible().catch(() => false)
const waitShown = async (e) => {
  await e.until(() => shown(e), 40)
  assert(await shown(e), 'the Find and Replace window is open')
}
const activeIs = async (e, ref) => {
  await e.until(async () => (await e.details()).active === ref, 40)
  assert.equal((await e.details()).active, ref, `the active cell is ${ref}`)
}
/** Words of their own in T1:T21 (the sample workbook does not hold them): Zq1 in row 2, Zq10 in row 11. */
const words = async (e) => {
  await e.agent([{ command: 'range.setValues', params: { range: 'T1:T21', values: [['Name'], ...Array.from({ length: 20 }, (_, index) => [`Zq${index + 1}`])] } }])
}
/** Closes the window if a segment left it open. */
const closeWindow = async (e) => {
  if (await shown(e)) {
    await win(e).getByTestId('sheet-find-close').click()
    await e.until(async () => !(await shown(e)), 30)
  }
}
const inView = async (e, ref) => {
  const at = await e.cellPoint(ref)
  const box = await e.surface()
  return at.x >= box.x && at.y >= box.y && at.x <= box.x + box.width && at.y <= box.y + box.height
}

const list = [
  ['q1-find-keys', async (e) => {
    await words(e)
    await e.click('T1')
    await press(e, 'ControlOrMeta+f', 400)
    await waitShown(e)
    assert.equal(await e.active(), 'input.kui-field', 'the window has the keyboard, in Find what')
    await e.page.keyboard.type('Zq1')
    await press(e, 'Enter', 400)
    await activeIs(e, 'T2')
    await press(e, 'Enter', 400)
    await activeIs(e, 'T11')
    assert(await inView(e, 'T11'), 'the cell found is in view')
    assert(/\d/.test(await win(e).getByTestId('sheet-find-status').innerText()), 'the window says how many it found')
    await press(e, 'Escape', 400)
    await e.until(async () => !(await shown(e)), 30)
    assert.equal(await shown(e), false, 'Escape closes the window')
    await e.until(async () => (await e.active()) === 'textarea.kui-text-sink', 30)
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the grid has the keyboard')
    assert.equal((await e.details()).active, 'T11', 'the active cell stays where it was found')
    await e.page.keyboard.type('x')
    await press(e, 'Enter', 400)
    await e.until(async () => (await e.value('T11')) === 'x')
    assert.equal(await e.value('T11'), 'x', 'typing goes into the cell found')
    await e.agent([{ command: 'cell.set', params: { cell: 'T11', input: 'Zq10' } }])
  }],
  ['q1-find-again', async (e) => {
    await words(e)
    await e.click('T1')
    await press(e, 'ControlOrMeta+f', 400)
    await waitShown(e)
    await e.page.keyboard.type('Zq1')
    // Cmd+F again in the window: the window's own (not the browser's Find), Find what chosen.
    await press(e, 'ControlOrMeta+f', 400)
    assert.equal(await e.kit.locator('[data-testid=sheet-find]').count(), 1, 'one window')
    await e.page.keyboard.type('Zq2')
    assert.equal(await win(e).getByTestId('sheet-find-what').inputValue(), 'Zq2', 'Find what was chosen, so typing replaced it')
    await press(e, 'Enter', 400)
    await activeIs(e, 'T3')
    // Shift+Enter finds the one before (round to the last: Zq20 in T21).
    await press(e, 'Shift+Enter', 400)
    await activeIs(e, 'T21')
    await win(e).getByTestId('sheet-find-next').click()
    await activeIs(e, 'T3')
    await win(e).getByTestId('sheet-find-previous').click()
    await activeIs(e, 'T21')
    await closeWindow(e)
  }],
  ['q1-not-found', async (e) => {
    await words(e)
    await e.click('T1')
    await press(e, 'ControlOrMeta+f', 400)
    await waitShown(e)
    await press(e, 'ControlOrMeta+a', 100)
    await e.page.keyboard.type('nothere')
    await press(e, 'Enter', 500)
    const status = await win(e).getByTestId('sheet-find-status').innerText()
    assert(status.includes('nothere'), `the window says it found nothing (${status})`)
    assert.equal(await e.kit.locator('.kui-toast').count(), 0, 'no notice pops up for it')
    assert.equal((await e.details()).active, 'T1', 'the active cell did not move')
    await closeWindow(e)
  }],
  ['q1-grid-stays-usable', async (e) => {
    await e.click('T1')
    await press(e, 'ControlOrMeta+f', 400)
    await waitShown(e)
    await e.click('U5')
    await e.page.keyboard.type('7')
    await press(e, 'Enter', 400)
    await e.until(async () => (await e.value('U5')) === 7)
    assert.equal(await e.value('U5'), 7, 'a click and typing go to the grid with the window open')
    assert(await shown(e), 'the window stays open')
    await closeWindow(e)
    await e.agent([{ command: 'edit.clearContents', params: { range: 'U5' } }])
  }],
  ['q2-replace-dialog', async (e) => {
    await words(e)
    await e.click('T1')
    await press(e, 'Control+h', 400)
    await waitShown(e)
    assert(await win(e).getByTestId('sheet-find-with').isVisible(), 'Ctrl+H opens at Replace')
    await press(e, 'ControlOrMeta+a', 100)
    await e.page.keyboard.type('Zq')
    await press(e, 'Tab', 100)
    await e.page.keyboard.type('Rq')
    // Replace: T1 (the active cell) holds no match, so it finds T2; again replaces T2 and finds T3.
    await win(e).getByTestId('sheet-replace-one').click()
    await activeIs(e, 'T2')
    await win(e).getByTestId('sheet-replace-one').click()
    await e.until(async () => (await e.value('T2')) === 'Rq1')
    assert.equal(await e.value('T2'), 'Rq1', 'Replace changes the match in the active cell')
    await activeIs(e, 'T3')
    await win(e).getByTestId('sheet-replace-all').click()
    await e.until(async () => (await e.value('T21')) === 'Rq20')
    assert.equal(await e.value('T21'), 'Rq20', 'Replace All changes every match')
    assert.equal(await e.value('T3'), 'Rq2')
    const status = await win(e).getByTestId('sheet-find-status').innerText()
    assert(status.includes('19'), `the window says how many it replaced (${status})`)
    await press(e, 'Escape', 400)
    await e.until(async () => (await e.active()) === 'textarea.kui-text-sink', 30)
    await press(e, 'ControlOrMeta+z', 500)
    await e.until(async () => (await e.value('T21')) === 'Zq20')
    assert.equal(await e.value('T21'), 'Zq20', 'one undo takes Replace All back')
    assert.equal(await e.value('T3'), 'Zq2')
    assert.equal(await e.value('T2'), 'Rq1', 'the Replace before it was a step of its own')
  }],
  ['c10-go-to', async (e) => {
    await e.click('A1')
    await press(e, 'Control+g', 400)
    await waitShown(e)
    assert(await win(e).getByTestId('sheet-goto-reference').isVisible(), 'Ctrl+G opens at Go To')
    await press(e, 'ControlOrMeta+a', 100)
    await e.page.keyboard.type('D5')
    await press(e, 'Enter', 400)
    await activeIs(e, 'D5')
    await e.until(async () => !(await shown(e)), 30)
    assert.equal(await shown(e), false, 'Go To closes the window')
    await e.until(async () => (await e.active()) === 'textarea.kui-text-sink', 30)
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the grid has the keyboard')
    await press(e, 'Control+g', 400)
    await waitShown(e)
    await e.page.keyboard.type('T2:T4')
    await press(e, 'Enter', 400)
    await e.until(async () => (await e.details()).selection === 'T2:T4', 30)
    assert.equal((await e.details()).selection, 'T2:T4', 'Go To selects a range')
    await press(e, 'Control+g', 400)
    await waitShown(e)
    await e.page.keyboard.type('!!')
    await press(e, 'Enter', 400)
    assert(await shown(e), 'an invalid reference keeps the window open')
    assert((await win(e).getByTestId('sheet-goto-reference').inputValue()) === '!!')
    assert((await win(e).getByTestId('sheet-find-status').innerText()).length > 0, 'and says it is invalid')
    await closeWindow(e)
  }],
  ['menu-opens-window', async (e) => {
    // Home › Find & Select › Replace opens the same window, not the command's parameters.
    await e.click('A1')
    await e.kit.getByTestId('ribbon-tab-home').click()
    await e.wait(250)
    const trigger = e.kit.getByRole('button', { name: /Find & Select/ }).first()
    if (!(await trigger.isVisible().catch(() => false))) await e.kit.getByTestId('kui-line-more-Editing').click()
    await e.kit.getByRole('button', { name: /Find & Select/ }).first().click()
    await e.kit.getByRole('menuitem').filter({ hasText: /^Replace/ }).first().click()
    await waitShown(e)
    assert(await win(e).getByTestId('sheet-find-with').isVisible(), 'the menu opens the window at Replace')
    assert.equal(await e.kit.getByTestId('command-dialog-run').count(), 0, 'no parameters dialog')
    await closeWindow(e)
  }],
  ['zh-labels', async (e) => {
    // A page of its own in Chinese.
    const z = await open({ which: e.which, query: 'locale=zh-CN' })
    try {
      await z.click('A1')
      await press(z, 'ControlOrMeta+f', 400)
      await waitShown(z)
      const english = /\b(Find|Replace|Go To|Match|Within|Search|Look in|Close|what|matchCase|wholeCell|Sheet|Workbook)\b/
      for (const tab of ['find', 'replace', 'goTo']) {
        await win(z).getByTestId(`sheet-find-tab-${tab}`).click()
        await z.wait(150)
        const text = await win(z).innerText()
        assert(!english.test(text), `the ${tab} tab has no English words (${text.replace(/\s+/g, ' ')})`)
      }
    } finally {
      await z.close()
    }
  }],
]

await segments('find', list, (which) => open({ which }))
