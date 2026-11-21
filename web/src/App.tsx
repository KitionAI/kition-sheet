import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from 'react'
import {
  CommandTab,
  ChartDesignGroups,
  ChartInsertMenu,
  DrawingFormatGroup,
  RibbonGroup,
  DrawingInsertGroup,
  InkGroup,
  KitDocumentCanvas,
  KitEditorProvider,
  EditorFrame,
  FrameChrome,
  createKitStrings,
  readKitStore,
  RibbonButton,
  toast,
  writeKitStore,
  useEngineTools,
  useKitEditorState,
  useKitSession,
  type FrameTab,
  type HostConnection,
  type KitCanvasHandle,
  type Phrases,
  type RasterView,
} from '@kition/sdk/editor'
import { ChartColumnIcon as ChartColumn, ShieldCheckIcon, SquareFunctionIcon as FunctionSquare } from '@kition/tessera/icons/react'
import { engineManifest } from './engine'
import { CATALOGS, localizeError } from './strings'
import { EXPORT_COMMANDS, HOST_FILE_COMMANDS, SHEET_PRIMARY, describe, type SelectionStats } from './sheet'
import { FormulaBar } from './FormulaBar'
import { SheetTabs } from './SheetTabs'
import { SheetDialogs, askConfirm, openUnhideSheets, tell } from './SheetDialogs'
import { FilterMenu, openFilterMenu, type FilterAsk } from './FilterMenu'
import { ValidationAlert, ValidationDialog, ValidationOverlay, askValidation, openValidationDialog, openValidationList, type ValidationAsk } from './Validation'
import { FormatCellsDialog, openFormatCells } from './FormatCells'
import { TableDesignTab, TableStyleDialogs } from './TableStyles'
import { HomeTab } from './ribbon/HomeTab'
import { ViewTab } from './ribbon/ViewTab'
import { InsertFunctionDialog, openInsertFunction } from './InsertFunction'
import { backToGrid } from './sheet'
import { ExportGroup } from './ribbon/ExportGroup'
import { CommentMarkerTargets, HostCommentGroup, createViewStore, useHostComments } from './HostComments'
import { CellFormulaHints } from './FormulaHints'
import { GridMenu, showGridMenu } from './LineMenus'
import { FocusCellButton } from './ViewControls'
import { SymbolDialog, SymbolGroup } from './Symbols'
import { useColorScheme } from './colorScheme'

const TABS: Array<[string, string]> = [
  ['file', 'File'],
  ['home', 'Home'],
  ['insert', 'Insert'],
  ['draw', 'Draw'],
  ['pageLayout', 'Page Layout'],
  ['formulas', 'Formulas'],
  ['data', 'Data'],
  ['review', 'Review'],
  ['view', 'View'],
  ['automate', 'Automate'],
]

export function App({ host: given, phrase }: { host: HostConnection; phrase: Phrases }) {
  const t = useMemo(() => createKitStrings(CATALOGS, given.init.locale), [given])
  // A workbook that does not open says why in the editor's language too: the host hears the
  // engine's English reason translated (kit-editor hands the host the engine's error as it is).
  const host = useMemo<HostConnection>(() => ({ ...given, failed: (error: unknown) => given.failed(new Error(localizeError(phrase, t, error))) }), [given, phrase, t])
  const manifest = useMemo(engineManifest, [])
  const opened = useKitSession(host, manifest, describe)
  // The editor shows why a workbook did not open in its own language too.
  const session = useMemo(() => (opened.failure ? { ...opened, failure: localizeError(phrase, t, opened.failure) } : opened), [opened, phrase, t])
  // A command refused says why in the editor's language (the engine's messages are English).
  const editor = useKitEditorState(session, host, t, 'sheet', phrase, undefined, {
    report: (title, error) => toast('error', title, localizeError(phrase, t, error)),
  })
  const { doc, change, state } = session
  // Escape cancels first (KitPointer 4.4): the select tool takes Escape only while the engine has
  // something for it to cancel (`escape`: typing in a cell or a shape, a gesture, a shape chosen to
  // draw, a selected object). Any other Escape is left untaken, so in Focus it leaves Focus.
  // `escape` is the engine's answer as last read: an Escape pressed right after a click or typing
  // that the engine has not answered yet (a shape just selected, text typed on it) would find it
  // stale and leave the shape's text unentered. Until the state read after such an event arrives,
  // Escape goes to the engine, which ignores it when there is nothing to cancel.
  const unanswered = useRef<number | null>(null)
  const revision = doc?.info.revision ?? 0
  const latestRevision = useRef(revision)
  latestRevision.current = revision
  if (unanswered.current !== null && revision > unanswered.current) unanswered.current = null
  useEffect(() => {
    const sent = (event: Event) => {
      const target = event.target
      const sink = target instanceof HTMLTextAreaElement && target.classList.contains('kui-text-sink')
      const canvas = target instanceof HTMLCanvasElement && target.closest('.kui-canvas') != null
      if (sink || canvas) unanswered.current = latestRevision.current
    }
    document.addEventListener('input', sent, true)
    document.addEventListener('pointerdown', sent, true)
    return () => {
      document.removeEventListener('input', sent, true)
      document.removeEventListener('pointerdown', sent, true)
    }
  }, [])
  const tools = useEngineTools(session.tool, doc != null, change, { initial: 'select', hostTools: {}, escapable: () => state?.escape === true || unanswered.current !== null })
  const canvas = useRef<KitCanvasHandle>(null)
  // A sheet opens at its top-left cell at 1:1, as spreadsheets do.
  const viewMemory = useRef<RasterView | null>({ x: 0, y: 0, zoom: 1 })
  const [zoom, setZoom] = useState(1)
  const [tab, setTab] = useState('home')
  const reach = useRef({ doc, right: 0, bottom: 0 })
  const scrolled = useRef({ x: 0, y: 0, timer: 0 })
  // Comment threads the host keeps (Kit UI protocol section 7k): marked here, created there.
  const comments = useHostComments(host, doc, change, state?.sheet, t('commentFailed'))
  const views = useMemo(createViewStore, [])
  // The overlays follow what the engine draws them from: the cell being typed, the highlight of the
  // selection's rows and columns, the frozen panes and the freeze being previewed.
  useEffect(() => { tools.refresh() }, [state?.formulaText, doc?.info.width, doc?.info.height, state?.focusCell, state?.focusColor, state?.freeze.join(), state?.freezePreview?.join(), state?.freezeShift.join(), state?.formatPainter, tools.refresh])

  // The system clipboard. A copy hands the clipboard the engine's answer while the key press is
  // still the person's (WebKit refuses a write once the engine has answered). A paste reads the
  // clipboard's HTML beside its text: another spreadsheet's table keeps its merged cells, the
  // line breaks in a cell and its formatting there, which its tab-separated text loses.
  // The copy goes as text and as an HTML table (formats, merged cells), which carries the
  // engine's own copy of the cells: another workbook pastes it as within one (checkboxes too).
  const copy = async (cut: boolean) => {
    const answer = editor
      .run(cut ? 'edit.cut' : 'edit.copy', { html: true })
      .then((raw) => (raw ? (JSON.parse(raw) as { text?: string; html?: string }) : {}))
    const text = answer.then((copied) => copied.text ?? '')
    try {
      // Written before the engine answers (WebKit refuses a write once the key press is past).
      await navigator.clipboard.write([
        new ClipboardItem({
          'text/plain': text.then((plain) => new Blob([plain], { type: 'text/plain' })),
          'text/html': answer.then((copied) => new Blob([copied.html ?? ''], { type: 'text/html' })),
        }),
      ])
    } catch {
      await navigator.clipboard.writeText(await text)
    }
  }
  const readClipboard = async (): Promise<{ text: string; html: string }> => {
    try {
      const items = await navigator.clipboard.read()
      const read = async (type: string) => {
        const item = items.find((entry) => entry.types.includes(type))
        return item ? (await item.getType(type)).text() : ''
      }
      return { text: await read('text/plain'), html: await read('text/html') }
    } catch {
      return { text: await navigator.clipboard.readText(), html: '' }
    }
  }
  const paste = async (from?: { text: string; html: string }) => {
    const { text, html } = from ?? (await readClipboard())
    await editor.run('edit.paste', { ...(text ? { text } : {}), ...(html ? { html } : {}) })
  }
  const clipboard = async (action: 'cut' | 'copy' | 'paste') => {
    try {
      await (action === 'paste' ? paste() : copy(action === 'cut'))
    } catch (error) {
      toast('error', t('commandFailed'), error instanceof Error ? error.message : String(error))
    }
  }
  const latest = useRef({ editing: false, paste })
  latest.current = { editing: state?.editing === true, paste }

  // Cmd/Ctrl+V where typing goes (the text sink) is left to the browser, whose paste event
  // carries the clipboard's HTML and text without asking for permission; the editor's shortcuts
  // do not see the key (they would prevent that event). Typing in a cell, the sink takes it.
  useEffect(() => {
    const inSink = (target: EventTarget | null) => target instanceof HTMLTextAreaElement && target.classList.contains('kui-text-sink')
    const onKey = (event: globalThis.KeyboardEvent) => {
      const command = event.metaKey || event.ctrlKey
      if (command && !event.shiftKey && !event.altKey && event.key.toLowerCase() === 'v' && inSink(event.target) && !latest.current.editing) event.stopPropagation()
    }
    const onPaste = (event: ClipboardEvent) => {
      if (!inSink(event.target) || latest.current.editing || !event.clipboardData) return
      event.preventDefault()
      const from = { text: event.clipboardData.getData('text/plain'), html: event.clipboardData.getData('text/html') }
      latest.current.paste(from).catch((error: unknown) => toast('error', t('commandFailed'), error instanceof Error ? error.message : String(error)))
    }
    window.addEventListener('keydown', onKey, true)
    document.addEventListener('paste', onPaste, true)
    return () => {
      window.removeEventListener('keydown', onKey, true)
      document.removeEventListener('paste', onPaste, true)
    }
  }, [t])

  // Each sheet keeps where it was scrolled: switching sheets shows the other sheet where it was
  // left (its top-left the first time), and switching back returns to the same place. The zoom
  // stays as it is. A renamed sheet keeps its place under its new name.
  const sheetViews = useRef(new Map<string, { x: number; y: number }>())
  const shown = useRef<{ sheet: string; sheets: string[] } | null>(null)
  useEffect(() => {
    if (!state) return
    const before = shown.current
    shown.current = { sheet: state.sheet, sheets: state.sheets }
    if (!before || before.sheet === state.sheet) return
    const renamed = before.sheets.length === state.sheets.length && !state.sheets.includes(before.sheet) && before.sheets.indexOf(before.sheet) === state.sheets.indexOf(state.sheet)
    if (renamed) {
      const place = sheetViews.current.get(before.sheet)
      sheetViews.current.delete(before.sheet)
      if (place) sheetViews.current.set(state.sheet, place)
      return
    }
    const place = sheetViews.current.get(state.sheet) ?? { x: 0, y: 0 }
    canvas.current?.panTo(place.x, place.y)
  }, [state?.sheet, state?.sheets])

  // Frozen panes that start at a scrolled view's top-left shift the grid by the rows and columns
  // above and left of it: the view moves with them, so the same cells stay in place.
  const shift = useRef<{ sheet: string; x: number; y: number } | null>(null)
  useEffect(() => {
    if (!state) return
    const [x, y] = state.freezeShift
    const before = shift.current
    shift.current = { sheet: state.sheet, x, y }
    if (!before || before.sheet !== state.sheet || (before.x === x && before.y === y)) return
    const view = views.get()
    if (view) canvas.current?.panTo(Math.max(0, view.x - (x - before.x)), Math.max(0, view.y - (y - before.y)))
  }, [state?.sheet, state?.freezeShift.join()])

  // View › Zoom (100%, Zoom to Selection, a percentage) zooms the view.
  const zoomAsked = useRef(0)
  useEffect(() => {
    if (!state || state.zoomAsked === zoomAsked.current) return
    zoomAsked.current = state.zoomAsked
    canvas.current?.zoomTo(state.zoom / 100)
  }, [state?.zoomAsked, state?.zoom])

  // View › Show › Formula Bar is the person's (Excel's and WPS's application setting): the engine
  // has it for agents (`view.formulaBar`), and the editor keeps it for the next time, giving it
  // to each workbook it opens before the bar shows.
  const storedBar = () => readKitStore<boolean>('sheet', 'formulaBar', (raw) => (typeof raw === 'boolean' ? raw : undefined))
  const barSynced = useRef<unknown>(null)
  useEffect(() => {
    if (!doc || !state) return
    if (barSynced.current !== doc) {
      const wanted = storedBar()
      if (wanted !== undefined && wanted !== state.formulaBar) {
        void editor.run('view.formulaBar', { on: wanted })
        return
      }
      barSynced.current = doc
      return
    }
    writeKitStore('sheet', 'formulaBar', state.formulaBar)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- per workbook and per change
  }, [doc, state?.formulaBar])
  const formulaBar = state ? (barSynced.current === doc ? state.formulaBar : (storedBar() ?? state.formulaBar)) : false
  const showFormulaBar = (on: boolean) => {
    writeKitStore('sheet', 'formulaBar', on)
    void editor.run('view.formulaBar', { on })
  }

  // View › 高亮行列 is the person's too (on or off, and its colour): kept for the next time, and
  // given to each workbook opened before it shows.
  type Focus = { on: boolean; color: string }
  const storedFocus = () =>
    readKitStore<Focus>('sheet', 'focusCell', (raw) => {
      const value = raw as Partial<Focus> | null
      return value && typeof value.on === 'boolean' && typeof value.color === 'string' ? { on: value.on, color: value.color } : undefined
    })
  const focusSynced = useRef<unknown>(null)
  useEffect(() => {
    if (!doc || !state) return
    if (focusSynced.current !== doc) {
      focusSynced.current = doc
      const wanted = storedFocus()
      if (wanted && (wanted.on !== state.focusCell || wanted.color.toUpperCase() !== state.focusColor.toUpperCase())) void editor.run('view.focusCell', { on: wanted.on, color: wanted.color })
      return
    }
    writeKitStore('sheet', 'focusCell', { on: state.focusCell, color: state.focusColor })
    // eslint-disable-next-line react-hooks/exhaustive-deps -- per workbook and per change
  }, [doc, state?.focusCell, state?.focusColor])

  // When the person's active cell moves (an arrow, Enter, Tab, Ctrl+arrow, the Name Box, Go To;
  // an agent's selection is its own and moves neither, KHP C3b) the view scrolls just enough to
  // show it, past the frozen rows and columns, as WPS does; a click on a cell in view leaves the view where it is (one partly in view is brought in
  // whole). Showing another sheet keeps the place that sheet was left at.
  const followed = useRef<{ sheet: string; active: string } | null>(null)
  useEffect(() => {
    if (!state?.activeRect || !doc) return
    const before = followed.current
    followed.current = { sheet: state.sheet, active: state.active }
    if (!before || before.sheet !== state.sheet || before.active === state.active) return
    const view = views.get()
    if (!view) return
    const [x0, y0, x1, y1] = state.activeRect
    const { left, top } = doc.pinned ?? { left: 0, top: 0 }
    const width = view.width / view.zoom
    const height = view.height / view.zoom
    let { x, y } = view
    if (x0 >= left) {
      if (x0 - x < left) x = x0 - left
      else if (x1 - x > width) x = Math.min(x0 - left, x1 - width)
    }
    if (y0 >= top) {
      if (y0 - y < top) y = y0 - top
      else if (y1 - y > height) y = Math.min(y0 - top, y1 - height)
    }
    if (x !== view.x || y !== view.y) canvas.current?.panTo(Math.max(0, x), Math.max(0, y))
    // eslint-disable-next-line react-hooks/exhaustive-deps -- when the active cell moves
  }, [state?.sheet, state?.active, state?.activeRect?.join()])

  // Names the engine makes up follow the editor's language: a table column with an empty header
  // is 列1 in Chinese (WPS), Column1 in English.
  useEffect(() => {
    if (doc) void editor.run('editor.wording', { columnPrefix: t('tableColumnPrefix') })
    // eslint-disable-next-line react-hooks/exhaustive-deps -- per workbook
  }, [doc])

  // The host's color scheme: the engine draws the grid in it (dark cells, gridlines and headings,
  // automatic text light; the workbook's own colours kept), at open and whenever the host changes
  // it, without a reload. A view setting: never saved, never a change of the workbook.
  const scheme = useColorScheme()
  useEffect(() => {
    if (doc) void editor.run('editor.scheme', { scheme }).catch(() => undefined)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- per workbook and scheme
  }, [doc, scheme])

  // A dialog a command asked for (a menu item run without the values it needs) opens here, once
  // per request: Format Cells, Insert Function and Unhide Sheet are the kit's own dialogs; any
  // other (Sort, Define Name, Data Validation, Hyperlink, New Formatting Rule, Picture) is the
  // command's parameters dialog. Opening and saving files are the host's.
  const dialogSeen = useRef(0)
  // The notice that comes with a validation message is not shown again as a toast.
  const validationShown = useRef({ seq: 0, text: '' })
  useEffect(() => {
    const asked = state?.dialog
    if (!asked) return
    if (asked.seq <= dialogSeen.current) {
      // Another workbook's engine counts from the start again.
      if (asked.seq < dialogSeen.current) dialogSeen.current = asked.seq
      return
    }
    dialogSeen.current = asked.seq
    switch (asked.name) {
      case 'formatCells':
        openFormatCells()
        break
      case 'insertFunction':
        openInsertFunction({
          onChoose: async (name) => {
            await editor.run('formulas.insertFunction', { name })
            backToGrid()
          },
          onCancel: backToGrid,
        })
        break
      case 'unhideSheet':
        openUnhideSheets()
        break
      // A dragged block dropped onto cells that hold something (WPS asks first).
      case 'moveCells': {
        const params = asked.params
        askConfirm({ title: t('moveTitle'), message: t('moveReplace'), testId: 'sheet-move-confirm', onOk: () => void editor.run('edit.moveCells', params).then(backToGrid) })
        break
      }
      // A filter button pressed: its list under it.
      case 'filterMenu':
        openFilterMenu(asked.params as unknown as FilterAsk)
        break
      // Alt+↓ (Option+↓) on a cell with a list: its entries.
      case 'validationList':
        openValidationList(true)
        break
      // A value its cell's validation refused: WPS's message, with its answers.
      case 'validationError':
        validationShown.current = { seq: (state?.notice?.seq ?? 0), text: String(asked.params.message ?? '') }
        askValidation(asked.params as unknown as ValidationAsk)
        break
      case 'dataValidation':
        openValidationDialog()
        break
      // A formula that made a new circular reference (WPS warns once; the cells show 0).
      case 'circularReference':
        tell(t('circularTitle'), t('circularMessage'), 'sheet-circular')
        break
      case 'open':
      case 'saveAs':
      case 'saveChanges':
        break
      default:
        editor.ask(asked.command, asked.params)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per request
  }, [state?.dialog?.seq])

  // What the engine tells the person (a checkbox typed in: 「复选框不可直接编辑，请删除后再插入」),
  // once each; the same words again within a moment (every key typed on it) show once.
  const noticeSeen = useRef({ seq: 0, text: '', at: 0 })
  useEffect(() => {
    const notice = state?.notice
    if (!notice) return
    const seen = noticeSeen.current
    if (notice.seq <= seen.seq) {
      if (notice.seq < seen.seq) seen.seq = notice.seq
      return
    }
    const now = Date.now()
    const asked = validationShown.current
    const inDialog = notice.text === asked.text && notice.seq <= asked.seq + 1
    if (!inDialog && !(notice.text === seen.text && now - seen.at < 800)) toast('info', phrase(notice.text))
    noticeSeen.current = { seq: notice.seq, text: notice.text, at: now }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per notice
  }, [state?.notice?.seq])

  // A right-click on the grid: the engine selects what was pressed (unless it is selected) and
  // says whether it was a cell, a row or column heading, or the corner; its menu opens there.
  const gridMenu = async (event: MouseEvent<HTMLDivElement>) => {
    const surface = event.target instanceof HTMLCanvasElement ? event.target.closest('.kui-canvas')?.querySelector('canvas') : null
    const view = views.get()
    if (!surface || !view || !doc || host.readOnly) return
    event.preventDefault()
    const box = surface.getBoundingClientRect()
    const sx = (event.clientX - box.left) / view.zoom
    const sy = (event.clientY - box.top) / view.zoom
    const pins = doc.pinned
    const at = { x: event.clientX, y: event.clientY }
    const answer = await editor.run('editor.contextMenu', { x: sx < pins.left ? sx : view.x + sx, y: sy < pins.top ? sy : view.y + sy })
    const target = answer ? (JSON.parse(answer) as { target?: string | null }).target : null
    if (target === 'cell' || target === 'rows' || target === 'cols' || target === 'all') showGridMenu({ at, target })
  }

  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const command = event.metaKey || event.ctrlKey
    const key = event.key.toLowerCase()
    // Ctrl+Page Down and Ctrl+Page Up show the next and the previous sheet (Cmd on a Mac too).
    if (command && !event.shiftKey && !event.altKey && (event.key === 'PageDown' || event.key === 'PageUp')) {
      void editor.run(event.key === 'PageDown' ? 'sheet.next' : 'sheet.previous', {})
      return true
    }
    // Cmd+5 (Ctrl+5) strikes the selection through, as WPS and Excel for Windows have it (the
    // engine's own shortcut, Cmd+Shift+X, still works).
    if (command && !event.shiftKey && !event.altKey && event.key === '5') {
      void editor.run('home.strikethrough', {})
      return true
    }
    // Ctrl+; types today's date and Ctrl+Shift+; the time (WPS, Excel): at the caret while a cell
    // is typed, else in place of the active cell (a checkbox cell refuses it).
    if (event.ctrlKey && !event.metaKey && !event.altKey && (event.key === ';' || event.key === ':' || event.code === 'Semicolon')) {
      const now = new Date()
      const text = event.shiftKey ? `${now.getHours()}:${String(now.getMinutes()).padStart(2, '0')}` : `${now.getFullYear()}/${now.getMonth() + 1}/${now.getDate()}`
      void editor.run('editor.insertText', { text })
      return true
    }
    // Alt+↓ (Option+↓) opens the active cell's list of entries (its data validation's arrow).
    if (event.altKey && !command && event.key === 'ArrowDown' && state?.validation?.list && !state.editing) {
      openValidationList(true)
      return true
    }
    // Cmd+1 (Ctrl+1) opens Format Cells, as in WPS and Excel.
    if (command && !event.shiftKey && !event.altKey && event.key === '1' && state && !state.editing) {
      openFormatCells()
      return true
    }
    // F4 while a formula is typed in the cell cycles the reference at the caret ($A$1, A$1, $A1, A1).
    if (event.key === 'F4' && !command && !event.altKey && state?.editing && state.editText?.startsWith('=')) {
      void editor.run('editor.cycleReference', {})
      return true
    }
    // Ctrl+Shift+= inserts and Ctrl+- deletes the selected rows, or the selected columns when whole
    // columns are selected (WPS, Excel; Cmd+- and Cmd+= zoom the view).
    if (event.ctrlKey && !event.metaKey && !event.altKey && state && !state.editing && !(state.lines.wholeRows && state.lines.wholeCols)) {
      const columns = state.lines.wholeCols
      if (event.shiftKey && (event.key === '=' || event.key === '+')) {
        void editor.run(columns ? 'home.insertColumns' : 'home.insertRows', {})
        return true
      }
      if (!event.shiftKey && (event.key === '-' || event.key === '_')) {
        void editor.run(columns ? 'home.deleteColumns' : 'home.deleteRows', {})
        return true
      }
    }
    if (command && !event.shiftKey && !event.altKey && (key === 'c' || key === 'x' || key === 'v') && !state?.editing) {
      void clipboard(key === 'c' ? 'copy' : key === 'x' ? 'cut' : 'paste')
      return true
    }
    return false
  }

  const ribbons: Record<string, ReactNode> = {
    file: (
      <>
        <CommandTab menu="File" exclude={[...HOST_FILE_COMMANDS, ...EXPORT_COMMANDS]} primary={SHEET_PRIMARY.File} />
        <ExportGroup sheet={state?.sheet ?? ''} sheets={state?.sheets ?? []} />
      </>
    ),
    home: state ? <HomeTab info={state} onClipboard={(action) => void clipboard(action)} /> : <CommandTab menu="Home" />,
    view: state ? <ViewTab info={state} formulaBar={formulaBar} onFormulaBar={showFormulaBar} /> : <CommandTab menu="View" />,
    formulas: (
      <>
        {/* Insert Function opens the kit's dialog; the function chosen starts the formula in the cell. */}
        <RibbonGroup label={phrase('Insert Function')} icon={FunctionSquare}>
          <RibbonButton
            icon={FunctionSquare}
            label={phrase('Insert Function')}
            size="large"
            primary
            disabled={!editor.enabled('formulas.insertFunction')}
            onClick={() =>
              openInsertFunction({
                onChoose: async (name) => {
                  await editor.run('formulas.insertFunction', { name })
                  backToGrid()
                },
                onCancel: backToGrid,
              })
            }
            data-testid="sheet-insert-function-button"
          />
        </RibbonGroup>
        <CommandTab menu="Formulas" exclude={['formulas.insertFunction']} primary={SHEET_PRIMARY.Formulas} />
      </>
    ),
    insert: (
      <>
        {/* The engine's Illustrations (Pictures, Icons) join the kit's own; its Shapes is the kit's. */}
        <DrawingInsertGroup testId="sheet-drawing" merge="Insert/Illustrations" exclude={['insert.shape']} primary={['insert.picture', 'insert.icons']} />
        <RibbonGroup label={t('chartInsert')} icon={ChartColumn}>
          <ChartInsertMenu testId="sheet-chart" primary />
        </RibbonGroup>
        {/* Charts and Text Box are the shared controls'; Symbol is the kit's panel (WPS's). */}
        <CommandTab menu="Insert" exclude={['insert.chart', 'insert.textBox', 'insert.symbol']} omit={['Illustrations']} primary={SHEET_PRIMARY.Insert} />
        <SymbolGroup />
      </>
    ),
    data: (
      <>
        <CommandTab menu="Data" exclude={['data.validation']} primary={SHEET_PRIMARY.Data} />
        {/* Data Validation is the kit's dialog (WPS's 设置, 输入信息, 出错警告). */}
        <RibbonGroup label={phrase('Data Validation')} icon={ShieldCheckIcon}>
          <RibbonButton icon={ShieldCheckIcon} label={phrase('Data Validation')} size="large" primary disabled={!editor.enabled('data.validation')} onClick={openValidationDialog} data-testid="sheet-data-validation-button" />
        </RibbonGroup>
      </>
    ),
    draw: (
      <>
        <InkGroup testId="sheet-ink" />
        <CommandTab menu="Draw" exclude={['draw.select', 'draw.pen', 'draw.pencil', 'draw.highlighter', 'draw.eraser', 'draw.state']} primary={SHEET_PRIMARY.Draw} />
      </>
    ),
  }
  if (comments.creates) {
    ribbons.review = (
      <>
        <HostCommentGroup onCreate={() => void comments.create()} busy={comments.busy} />
        <CommandTab menu="Review" omit={['Comments']} primary={SHEET_PRIMARY.Review} />
      </>
    )
  }
  const tabs: FrameTab[] = [
    // A document opened to view (WPS's preview) keeps File (exporting, a template), Review and View.
    ...TABS.map(([id, menu]) => ({ id, label: phrase(menu), view: id === 'file' || id === 'review' || id === 'view', ribbon: ribbons[id] ?? <CommandTab menu={menu} primary={SHEET_PRIMARY[menu]} /> })),
    ...(state?.table ? [{ id: 'tableDesign', label: t('tableTools'), ribbon: <TableDesignTab info={state} /> }] : []),
    ...(state?.shape ? [{ id: 'shapeFormat', label: t('drawingShapeFormat'), ribbon: <DrawingFormatGroup ids={state.shapes.length ? state.shapes : [state.shape]} testId="sheet-drawing" /> }] : []),
    ...(state?.chart
      ? [
          {
            id: 'chartDesign',
            label: phrase('Chart Design'),
            ribbon: (
              <>
                {/* A chart shows the sheet's cells: its data are edited there (Select Data). */}
                <ChartDesignGroups id={state.chart} testId="sheet-chart" dataEditable={false} formats={['axes', 'labelPosition', 'lineWidth', 'marker', 'smooth', 'trends', 'combo']} />
                {/* Chart Elements is the shared controls' (title, axis titles, legend, labels, gridlines). */}
                <CommandTab menu="Chart Design" exclude={['chart.set', 'chart.changeType', 'chart.addElement']} primary={SHEET_PRIMARY['Chart Design']} />
              </>
            ),
          },
        ]
      : []),
  ]
  const activeTab = tabs.some((item) => item.id === tab) ? tab : 'home'

  return (
    <KitEditorProvider editor={editor} tools={tools}>
      <EditorFrame
        tabs={tabs}
        tab={activeTab}
        onTab={setTab}
        canvas={canvas}
        zoom={zoom}
        onKeyDown={keyDown}
        statusEnd={state ? <FocusCellButton info={state} place="status" /> : null}
        status={
          state ? (
            <>
              <span data-testid="sheet-status">{t('sheetSelection', { sheet: state.sheet, selection: state.selection })}</span>
              <SelectionFigures stats={state.stats} t={t} />
            </>
          ) : null
        }
      >
        {state && formulaBar ? (
          <FrameChrome>
            <FormulaBar info={state} />
          </FrameChrome>
        ) : null}
        <div className="kui-row" onContextMenu={(event) => void gridMenu(event)}>
          <KitDocumentCanvas canvas={canvas} viewMemory={viewMemory} framed={false} decorate={comments.decorate} onView={(view) => {
            views.set(view)
            setZoom(view.zoom)
            if (shown.current) sheetViews.current.set(shown.current.sheet, { x: view.x, y: view.y })
            if (!doc) return
            if (reach.current.doc !== doc) reach.current = { doc, right: 0, bottom: 0 }
            const right = Math.max(0, view.x) + view.width / view.zoom
            const bottom = Math.max(0, view.y) + view.height / view.zoom
            if (right > reach.current.right || bottom > reach.current.bottom) {
              reach.current = { doc, right: Math.max(right + 512, reach.current.right), bottom: Math.max(bottom + 600, reach.current.bottom) }
              void editor.run('editor.viewport', { right: reach.current.right, bottom: reach.current.bottom })
            }
            // Where the view is scrolled to, once it rests: a freeze starts at the cell shown at its
            // top-left (WPS, Excel).
            window.clearTimeout(scrolled.current.timer)
            const at = { x: Math.max(0, view.x), y: Math.max(0, view.y) }
            scrolled.current.timer = window.setTimeout(() => {
              if (at.x === scrolled.current.x && at.y === scrolled.current.y) return
              scrolled.current = { ...scrolled.current, ...at }
              void editor.run('editor.viewport', { right: reach.current.right, bottom: reach.current.bottom, ...at })
            }, 120)
          }}>
            {state?.formulaHint && doc ? (
              <CellFormulaHints hint={state.formulaHint} views={views} pins={doc.pinned} onPick={(name) => void editor.run('editor.acceptFunction', { name })} />
            ) : null}
            {state && doc ? <ValidationOverlay info={state} views={views} pins={doc.pinned} /> : null}
            {doc ? <FilterMenu views={views} pins={doc.pinned} /> : null}
            {comments.marks && doc ? (
              <CommentMarkerTargets markers={comments.markers} views={views} pins={doc.pinned} onActivate={host.activateComment} label={(count) => t('commentThread', { count })} />
            ) : null}
          </KitDocumentCanvas>
        </div>
        {state ? (
          <FrameChrome>
            <SheetTabs info={state} />
          </FrameChrome>
        ) : null}
        {state ? <SheetDialogs info={state} /> : null}
        {state ? <GridMenu info={state} onClipboard={(action) => void clipboard(action)} /> : null}
        <InsertFunctionDialog />
        <ValidationAlert />
        {state ? <ValidationDialog info={state} /> : null}
        <SymbolDialog />
        {state ? <FormatCellsDialog info={state} /> : null}
        {state ? <TableStyleDialogs info={state} /> : null}
      </EditorFrame>
    </KitEditorProvider>
  )
}

/** A figure as the status bar shows it: up to 15 significant digits, without float noise. */
function figure(value: number) {
  return String(Number(value.toPrecision(15)))
}

/**
 * The status bar's sum, average and count of the selected cells, when more than one cell holds
 * something; the sum and average when some of them are numbers.
 */
function SelectionFigures({ stats, t }: { stats: SelectionStats | null; t: (key: string, params?: Record<string, string | number>) => string }) {
  if (!stats || stats.count < 2) return null
  return (
    <span className="sheet-figures" data-testid="sheet-stats">
      {stats.numeric > 0 ? <span data-testid="sheet-stat-sum">{t('statSum', { value: figure(stats.sum) })}</span> : null}
      {stats.numeric > 0 && stats.average != null ? <span data-testid="sheet-stat-average">{t('statAverage', { value: figure(stats.average) })}</span> : null}
      <span data-testid="sheet-stat-count">{t('statCount', { value: stats.count })}</span>
    </span>
  )
}
