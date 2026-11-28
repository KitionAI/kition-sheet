/**
 * The editor's own keys, as a table (plan stage 0A, section 2; walkthrough A8, C7, H4, I2, Q1;
 * WPS W-B12, W-B13, W-H2, W-I5, W-I9, W-M5, W-O1, W-S3). kit-editor's frame asks the editor
 * (`EditorFrame`'s `onKeyDown`) after the engine's tool and before its own zoom keys and the
 * commands' shortcuts, so a key here takes the place of both. Each row's action runs through the
 * editor's `run` (the ordered queue with the tool's events) or opens a window of the editor.
 *
 * A row names its keys as the commands' shortcuts do: `Cmd` is Cmd on a Mac and Ctrl elsewhere
 * (either of them), `Ctrl` the Control key itself. `editing` rows also work while a cell is typed;
 * none works while an input method composes (the key is the input method's). Later stages add
 * their rows here.
 */
import type { SheetInfo } from './sheet'
import type { FindTab } from './FindReplace'

/** What a row's action can do: the engine's state, the editor's runs and dialogs. */
export type KeyContext = {
  state: SheetInfo
  run: (command: string, params?: Record<string, unknown>) => void
  /** Runs a command in the editor's ordered queue and answers what it answered. */
  answer: (command: string, params?: Record<string, unknown>) => Promise<Record<string, unknown> | null>
  /** Opens the Find and Replace window at a tab (FindReplace.tsx). */
  openFind: (tab: FindTab) => void
  clipboard: (action: 'cut' | 'copy' | 'paste') => void
  /** Moves a screenful: `rows` down (negative up), or `cols` across; `extend` keeps the anchor. */
  page: (direction: 1 | -1, across: boolean, extend: boolean) => void
  /** The status bar's + and −. */
  zoomStep: (direction: 1 | -1) => void
  openFormatCells: () => void
  openValidationList: () => void
}

type KeyEvent = { key: string; code: string; shiftKey: boolean; altKey: boolean; metaKey: boolean; ctrlKey: boolean; keyCode?: number; isComposing?: boolean; target?: EventTarget | null; nativeEvent?: { isComposing?: boolean } }

type KeyRow = {
  keys: string[]
  /**
   * Taken before kit-editor sees the key (a capture listener on the document, `pressCapturedKey`):
   * keys the engine's tool would otherwise take as typing (Shift+Space) or as another key.
   */
  capture?: boolean
  /** Also while a cell is typed. */
  editing?: boolean
  /** Only when this holds (the row's key is left to others otherwise). */
  when?: (context: KeyContext) => boolean
  act: (context: KeyContext, event: KeyEvent) => void
}

/** Inserts or deletes the selected rows, or the columns when whole columns are selected. */
const lines = (context: KeyContext, insert: boolean) => {
  const columns = context.state.lines.wholeCols
  context.run(insert ? (columns ? 'column.insert' : 'row.insert') : columns ? 'column.delete' : 'row.delete', {})
}
const notEverything = (context: KeyContext) => !(context.state.lines.wholeRows && context.state.lines.wholeCols)
const onMac = () => typeof navigator !== 'undefined' && /Mac|iPhone|iPad/i.test(navigator.platform)

export const KEYS: KeyRow[] = [
  // Shift+Space selects the rows of the selection, Ctrl+Space its columns (WPS W-A10; Excel;
  // walkthrough C8); while a cell is typed Shift+Space types a space. The editor's state may not
  // have caught up with keys typed just before (it lags the engine), so the engine decides in the
  // ordered queue: the space goes into the cell being typed, else the row is selected.
  {
    keys: ['Shift+Space'],
    capture: true,
    when: (c) => c.state.shape == null,
    act: (c) =>
      void c.answer('editor.insertText', { text: ' ', typing: true }).then((answer) => {
        if (answer?.editing === false) c.run('select.row', {})
      }),
  },
  { keys: ['Ctrl+Space'], act: (c) => c.run('select.column', {}) },
  // Cmd+Enter fills the selection with what is typed (WPS W-C10, the engine's tool); on a Mac,
  // Ctrl+Enter only enters it and moves down, as Enter does (kit-editor gives the tool Ctrl as
  // Cmd, so the key goes on as Enter). Elsewhere Ctrl is the main modifier and fills.
  { keys: ['Ctrl+Enter'], capture: true, editing: true, when: onMac, act: (_, event) => event.target?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true, cancelable: true, composed: true })) },
  // Ctrl+Page Down and Ctrl+Page Up show the next and the previous sheet (Cmd on a Mac too).
  { keys: ['Cmd+PageDown'], editing: true, act: (c) => c.run('sheet.next', {}) },
  { keys: ['Cmd+PageUp'], editing: true, act: (c) => c.run('sheet.previous', {}) },
  // Page Down and Page Up move a screenful of rows, the view with them; with Shift the selection
  // grows; with Alt (Option) a screenful of columns (WPS W-B12, W-B13).
  { keys: ['PageDown'], act: (c) => c.page(1, false, false) },
  { keys: ['PageUp'], act: (c) => c.page(-1, false, false) },
  { keys: ['Shift+PageDown'], act: (c) => c.page(1, false, true) },
  { keys: ['Shift+PageUp'], act: (c) => c.page(-1, false, true) },
  { keys: ['Alt+PageDown'], act: (c) => c.page(1, true, false) },
  { keys: ['Alt+PageUp'], act: (c) => c.page(-1, true, false) },
  { keys: ['Alt+Shift+PageDown'], act: (c) => c.page(1, true, true) },
  { keys: ['Alt+Shift+PageUp'], act: (c) => c.page(-1, true, true) },
  // Cmd+Shift+Z redoes as Cmd+Y does (WPS W-H2).
  { keys: ['Cmd+Shift+Z'], act: (c) => c.run('edit.redo', {}) },
  // Cmd+5 (Ctrl+5) strikes the selection through, as WPS and Excel for Windows have it (the
  // engine's own shortcut, Cmd+Shift+X, still works).
  { keys: ['Cmd+5'], editing: true, act: (c) => c.run('format.strikethrough', {}) },
  // Ctrl+; types today's date and Ctrl+Shift+; the time (WPS, Excel): at the caret while a cell
  // is typed, else in place of the active cell (a checkbox cell refuses it).
  {
    keys: ['Ctrl+;', 'Ctrl+Shift+;'],
    editing: true,
    act: (c, event) => {
      const now = new Date()
      const text = event.shiftKey ? `${now.getHours()}:${String(now.getMinutes()).padStart(2, '0')}` : `${now.getFullYear()}/${now.getMonth() + 1}/${now.getDate()}`
      c.run('editor.insertText', { text })
    },
  },
  // Alt+↓ (Option+↓) opens the active cell's list of entries (its data validation's arrow).
  { keys: ['Alt+ArrowDown'], when: (c) => c.state.validation?.list != null, act: (c) => c.openValidationList() },
  // Cmd+1 (Ctrl+1) opens Format Cells, as in WPS and Excel.
  { keys: ['Cmd+1'], act: (c) => c.openFormatCells() },
  // F4 while a formula is typed in the cell cycles the reference at the caret ($A$1, A$1, $A1, A1).
  { keys: ['F4'], editing: true, when: (c) => c.state.editing && (c.state.editText?.startsWith('=') ?? false), act: (c) => c.run('editor.cycleReference', {}) },
  // Cmd+0 hides the selected columns and Cmd+Shift+0 shows them again, as the menus say (WPS
  // W-S3, W-I9; Cmd+9 and Cmd+Shift+9 are the rows', the commands' own shortcuts). They are not
  // zoom keys: the status bar's percentage gives 100% (walkthrough A8, H4).
  { keys: ['Cmd+0'], editing: true, act: (c) => (c.state.editing ? undefined : c.run('column.hide', {})) },
  { keys: ['Cmd+Shift+0'], editing: true, act: (c) => (c.state.editing ? undefined : c.run('column.show', {})) },
  // With whole rows or columns selected, Cmd+Shift+= inserts and Cmd+- deletes them (WPS W-I5,
  // W-I6). With only cells selected WPS asks what to insert or delete (W-I8, plan stage 7); until
  // then they zoom 10 points as before, never deleting a row unasked. Ctrl+Shift+= and Ctrl+- act
  // on the selection's rows (or whole columns) as they did.
  { keys: ['Cmd+Shift+=', 'Cmd+-'], when: (c) => (c.state.lines.wholeRows || c.state.lines.wholeCols) && notEverything(c), act: (c, event) => lines(c, event.shiftKey) },
  { keys: ['Ctrl+Shift+='], when: notEverything, act: (c) => lines(c, true) },
  { keys: ['Ctrl+-'], when: notEverything, act: (c) => lines(c, false) },
  // Cmd+= zooms in 10 points, as the status bar's + does (web/tests/gate-sheet-ui.mjs UI-C22 and
  // UI-C62 zoom with it), and Cmd+- (cells selected) out.
  { keys: ['Cmd+=', 'Cmd+Shift+='], editing: true, act: (c) => c.zoomStep(1) },
  { keys: ['Cmd+-'], editing: true, act: (c) => c.zoomStep(-1) },
  // Find, Replace and Go To open the Find and Replace window at their tab (WPS W-O1, W-O2; Cmd+H
  // is the system's Hide).
  { keys: ['Cmd+F'], act: (c) => c.openFind('find') },
  { keys: ['Ctrl+H'], act: (c) => c.openFind('replace') },
  { keys: ['Ctrl+G'], act: (c) => c.openFind('goTo') },
  // The system clipboard: written while the key press is still the person's (WebKit refuses a
  // write once the engine has answered), read from the browser's paste.
  { keys: ['Cmd+C'], act: (c) => c.clipboard('copy') },
  { keys: ['Cmd+X'], act: (c) => c.clipboard('cut') },
  { keys: ['Cmd+V'], act: (c) => c.clipboard('paste') },
]

/** The key codes of the characters rows name (the code, not the character Shift makes of it). */
const CODES: Record<string, string> = { '=': 'Equal', '-': 'Minus', ';': 'Semicolon', '`': 'Backquote', Space: 'Space' }
/** The characters a key with Shift (or without) may report for a row's character. */
const CHARACTERS: Record<string, string[]> = { '=': ['=', '+'], '-': ['-', '_'], ';': [';', ':'], Space: [' '] }

/** Whether `event` is the key `shortcut` names (`Cmd+Shift+Z`, `Alt+PageDown`, `Ctrl+;`). */
export function keyMatches(shortcut: string, event: KeyEvent) {
  const parts = shortcut.split('+')
  const name = parts.length > 1 && parts[parts.length - 1] === '' ? '+' : (parts.pop() ?? '')
  const mods = new Set(parts.filter(Boolean))
  const command = mods.has('Cmd')
  const control = mods.has('Ctrl')
  if (command ? !(event.metaKey || event.ctrlKey) : control ? !event.ctrlKey || event.metaKey : event.metaKey || event.ctrlKey) return false
  if (mods.has('Shift') !== event.shiftKey || mods.has('Alt') !== event.altKey) return false
  if (/^[A-Za-z]$/.test(name)) return event.code === `Key${name.toUpperCase()}` || event.key.toLowerCase() === name.toLowerCase()
  if (/^[0-9]$/.test(name)) return event.code === `Digit${name}` || event.key === name
  if (CODES[name]) return event.code === CODES[name] || (CHARACTERS[name] ?? [name]).includes(event.key)
  return event.key === name
}

/** Runs the first row `event` names (of the rows taken first when `captured`); answers whether one took the key. */
export function pressKey(event: KeyEvent, context: KeyContext | null, captured = false) {
  if (!context || event.nativeEvent?.isComposing || event.isComposing || event.keyCode === 229 || event.key === 'Process') return false
  for (const row of KEYS) {
    if ((row.capture === true) !== captured) continue
    if (!row.keys.some((shortcut) => keyMatches(shortcut, event))) continue
    if (context.state.editing && !row.editing) continue
    if (row.when && !row.when(context)) continue
    row.act(context, event)
    return true
  }
  return false
}
