import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from 'react'
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
  type RasterPins,
  type RasterView,
} from '@kition/sdk/editor'
import { ChartColumnIcon as ChartColumn, ShieldCheckIcon, SquareFunctionIcon as FunctionSquare } from '@kition/tessera/icons/react'
import { engineManifest } from './engine'
import { repaintFor, type Drawn, type Handed } from './repaint'
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
import { FindReplaceDialog, openFind, type FindTab } from './FindReplace'
import { SortDialog, openSortDialog } from './SortDialog'
import { PasteSpecialDialog, openPasteSpecial } from './PasteSpecial'
import { backToGrid } from './sheet'
import { ExportGroup } from './ribbon/ExportGroup'
import { CommentMarkerTargets, HostCommentGroup, createViewStore, useHostComments } from './HostComments'
import { SheetScrollbars } from './SheetScrollbars'
import { CellFormulaHints } from './FormulaHints'
import { GridMenu, showGridMenu } from './LineMenus'
import { FocusCellButton } from './ViewControls'
import { usePendingToggle } from './pendingToggle'
import { SymbolDialog, SymbolGroup } from './Symbols'
import { useColorScheme } from './colorScheme'
import { gridInput, takeKeyboardOnOpen, useKeyboardFallback } from './focus'
import { pressKey, type KeyContext } from './keys'
import { useCompositions, useInputAtCaret } from './inputMethod'
import { steppedCanvas, stepZoom, zoomKeepingCorner } from './zoom'
import { ZoomControl, type ZoomActions } from './ZoomControl'
import { useSheetWheel } from './useSheetWheel'
import { edgeScrolling } from './edgeScroll'

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

/**
 * An agent's step that moves neither the engine's revision nor its `inspect()` (a cell read, the
 * agent selecting) does not repaint the grid, so the person's paint in progress lands. The engine
 * keeps that safe: every agent step that changes what the grid draws moves the revision
 * (`bindings/wasm/src/drawn.rs`, checked command by command by `tests/agent_paint.rs`). Its "paint
 * nothing" must not replace a repaint the person's edit still owes when React takes both in one
 * render: `repaintFor` (`src/repaint.ts`) sees to that.
 *
 * A workbook changed outside Kition opens with its stored replica whatever the file's stamp (Kit
 * UI protocol UI-R1): the engine records what the file changed as one `import` change onto the
 * replica (`open_with_replica`), so the sheets, rows, columns and cells keep their object ids and
 * the history goes on (tests/reopen-changed.mjs).
 */
const SESSION = { agentSkipsUnchangedPaint: true } as const

export function App({ host: given, phrase }: { host: HostConnection; phrase: Phrases }) {
  const t = useMemo(() => createKitStrings(CATALOGS, given.init.locale), [given])
  // A workbook that does not open says why in the editor's language too: the host hears the
  // engine's English reason translated (kit-editor hands the host the engine's error as it is).
  const host = useMemo<HostConnection>(() => ({ ...given, failed: (error: unknown) => given.failed(new Error(localizeError(phrase, t, error))) }), [given, phrase, t])
  const manifest = useMemo(engineManifest, [])
  const opened = useKitSession(host, manifest, describe, SESSION)
  // The canvas repaints what the session's changes taken together owe (`src/repaint.ts`): kit-editor
  // keeps only the last change's area, so a checkbox's press (repaint) and release (nothing more)
  // taken in one render repainted nothing.
  const handed = useRef<Handed | null>(null)
  const drawnNow = (): Drawn => ({ doc: opened.doc, revision: opened.info?.revision ?? 0, inspected: JSON.stringify(opened.info?.inspected ?? null) })
  // eslint-disable-next-line react-hooks/exhaustive-deps -- the info and the document come with the change
  const owed = useMemo(() => repaintFor(handed.current, opened.change, drawnNow()), [opened.change])
  // eslint-disable-next-line react-hooks/exhaustive-deps -- as above
  useLayoutEffect(() => { handed.current = { change: opened.change.revision, drawn: drawnNow() } }, [opened.change])
  // The editor shows why a workbook did not open in its own language too.
  const session = useMemo(() => {
    const shown = owed === opened.change ? opened : { ...opened, change: owed }
    return opened.failure ? { ...shown, failure: localizeError(phrase, t, opened.failure) } : shown
  }, [opened, owed, phrase, t])
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
  // kit-editor's frame zooms through this canvas (its Fit and zoom keys): 100% and 10-point steps
  // within 10%–400%, never the whole sheet fitted into the window (zoom.ts).
  const frameCanvas = useMemo(() => steppedCanvas(canvas), [])
  // The workbook just opened has the keyboard, once kit-editor's text input is there (walkthrough
  // K1); a key that still finds the page's body is handed to it (focus.ts).
  useKeyboardFallback()
  useEffect(() => {
    if (!doc) return
    let frame = 0
    let tries = 0
    const take = () => {
      if (gridInput()) takeKeyboardOnOpen()
      else if (tries++ < 120) frame = window.requestAnimationFrame(take)
    }
    take()
    return () => window.cancelAnimationFrame(frame)
  }, [doc])
  // A sheet opens at its top-left cell at 1:1, as spreadsheets do.
  const viewMemory = useRef<RasterView | null>({ x: 0, y: 0, zoom: 1 })
  const [zoom, setZoom] = useState(1)
  const [tab, setTab] = useState('home')
  const reach = useRef({ doc, right: 0, bottom: 0 })
  const scrolled = useRef({ x: 0, y: 0, timer: 0 })
  // Comment threads the host keeps (Kit UI protocol section 7k): marked here, created there.
  const comments = useHostComments(host, doc, change, state?.sheet, t('commentFailed'))
  const views = useMemo(createViewStore, [])
  // Input methods compose in the cell being typed, their candidate window beside it (inputMethod.ts):
  // the text input sits at the caret, else at the active cell's bottom-left, where typing starts.
  useCompositions((command, params) => editor.run(command, params))
  const active = state?.activeRect
  useInputAtCaret(state?.caretRect ?? (active ? [active[0], active[1], active[0] + 1, active[3]] : null), views, doc?.pinned ?? null)
  const gridId = useId()
  // The overlays follow what the engine draws them from: the cell being typed, the highlight of the
  // selection's rows and columns, the frozen panes and the freeze being previewed, the marching
  // ants of the person's copy.
  const copied = state?.copied ? `${state.copied.sheet}!${state.copied.range}` : null
  useEffect(() => { tools.refresh() }, [state?.formulaText, doc?.info.width, doc?.info.height, state?.focusCell, state?.focusColor, state?.freeze.join(), state?.freezePreview?.join(), state?.freezeShift.join(), state?.formatPainter, copied, tools.refresh])

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
  // Paste Special works on the sheet's own copy (the command's condition), not after a cut (Excel).
  const pasteSpecialOff = !editor.enabled('edit.pasteSpecial') || state?.copied?.cut === true
  const pasteSpecial = () => {
    if (!pasteSpecialOff) openPasteSpecial()
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
      // ⌃⌘V is Paste Special's (keys.ts), not a paste.
      if (command && !(event.metaKey && event.ctrlKey) && !event.shiftKey && !event.altKey && event.key.toLowerCase() === 'v' && inSink(event.target) && !latest.current.editing) event.stopPropagation()
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

  // An agent's View › Zoom (`view.zoom`, `view.zoomToSelection`) zooms the view; Zoom to
  // Selection shows the selection from its top-left.
  const zoomAsked = useRef(0)
  const selectionZoom = useRef(false)
  useEffect(() => {
    if (!state || state.zoomAsked === zoomAsked.current) return
    zoomAsked.current = state.zoomAsked
    if (canvas.current) zoomKeepingCorner(canvas.current, state.zoom / 100)
    if (!selectionZoom.current || !state.activeRect || !doc) return
    selectionZoom.current = false
    const [x0, y0] = state.activeRect
    const { left, top } = doc.pinned
    const view = canvas.current?.view()
    if (view) canvas.current?.panTo(x0 >= left ? Math.max(0, x0 - left) : view.x, y0 >= top ? Math.max(0, y0 - top) : view.y)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- when the zoom is asked for
  }, [state?.zoomAsked, state?.zoom])

  // The person's zoom (plan 0A; WPS W-S1, W-S2): the status bar, View › Zoom, Cmd+= and the wheel
  // with Ctrl or Cmd zoom the view only, as Excel and WPS do (no undo step, no change of the
  // workbook), within 10%–400%.
  const zoomTo = (value: number) => {
    if (canvas.current) zoomKeepingCorner(canvas.current, value)
  }
  const zoomStep = (direction: 1 | -1) => {
    const view = canvas.current?.view()
    if (view) zoomTo(stepZoom(view.zoom, direction))
  }
  const zoomActions: ZoomActions = {
    zoom,
    zoomTo,
    toSelection: () => {
      const view = canvas.current?.view()
      if (!view || !doc) return
      selectionZoom.current = true
      void editor.run('view.zoomToSelection', { viewWidth: Math.max(1, view.width - doc.pinned.left * view.zoom), viewHeight: Math.max(1, view.height - doc.pinned.top * view.zoom) })
    },
  }
  const gridRegion = useRef<HTMLDivElement>(null)
  const pinned = useRef<RasterPins | null>(null)
  pinned.current = doc?.pinned ?? null
  const pins = useCallback(() => pinned.current, [])
  useSheetWheel(gridRegion, canvas, pins)
  // A drag past the grid's edge scrolls the view (edgeScroll.ts), whichever gesture the engine's
  // tool runs; it reads the tool's cursor and the state as they are at each frame.
  const edge = useRef({ cursor: tools.canvasTool.cursor, drawing: false, cell: [69, 19] as [number, number] })
  edge.current = { cursor: tools.canvasTool.cursor, drawing: state?.drawTool != null, cell: state?.defaultSize ?? [69, 19] }
  const gridTool = useMemo(() => edgeScrolling(tools.canvasTool, () => {
    const view = canvas.current?.view()
    return { ...edge.current, pins: pinned.current, size: view ? { width: view.width, height: view.height } : null }
  }), [tools.canvasTool])

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
  // The box shows what the person last asked for until the engine answers; the bar follows the
  // engine.
  const [formulaBarBox, clickFormulaBar] = usePendingToggle('view.formulaBar', formulaBar, (on) => {
    writeKitStore('sheet', 'formulaBar', on)
    return editor.run('view.formulaBar', { on })
  })
  const showFormulaBar = () => void clickFormulaBar()

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
  const reveal = (rect: [number, number, number, number]) => {
    const view = views.get()
    if (!view || !doc) return
    const [x0, y0, x1, y1] = rect
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
  }
  // Page Down and Page Up move the view as far as the active cell moved, so it stays where it was
  // on the screen (WPS W-B12); the key's request, if the engine answers it within a moment.
  const paging = useRef<{ before: [number, number, number, number]; view: { x: number; y: number }; at: number } | null>(null)
  useEffect(() => {
    if (!state?.activeRect || !doc) return
    const before = followed.current
    followed.current = { sheet: state.sheet, active: state.active }
    const paged = paging.current
    paging.current = null
    if (!before || before.sheet !== state.sheet || before.active === state.active) return
    if (paged && performance.now() - paged.at < 2000) {
      const [x0, y0] = state.activeRect
      canvas.current?.panTo(Math.max(0, paged.view.x + x0 - paged.before[0]), Math.max(0, paged.view.y + y0 - paged.before[1]))
      return
    }
    reveal(state.activeRect)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- when the active cell moves
  }, [state?.sheet, state?.active, state?.activeRect?.join()])
  // Typing in (or F2 on) an active cell scrolled out of view brings it back first, as WPS and Excel
  // do (walkthrough B15): the first character shows where it goes.
  const typing = useRef(false)
  useEffect(() => {
    const was = typing.current
    typing.current = state?.editing === true
    if (!state?.editing || was || !state.activeRect) return
    if (state.editCell && state.editCell.sheet !== state.sheet) return
    reveal(state.activeRect)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- when typing starts
  }, [state?.editing])
  /** Page Down / Up (Alt: across): a screenful of whole rows or columns at the sheet's size. */
  const page = (direction: 1 | -1, across: boolean, extend: boolean) => {
    const view = canvas.current?.view()
    if (!state || !doc || !view) return
    const { left, top } = doc.pinned
    const [width, height] = state.defaultSize
    const count = across ? Math.floor((view.width / view.zoom - left) / width) : Math.floor((view.height / view.zoom - top) / height)
    const by = direction * Math.max(1, count)
    if (extend) {
      canvas.current?.panTo(Math.max(0, across ? view.x + by * width : view.x), Math.max(0, across ? view.y : view.y + by * height))
    } else if (state.activeRect) {
      paging.current = { before: state.activeRect, view: { x: view.x, y: view.y }, at: performance.now() }
    }
    void editor.run('select.move', across ? { dr: 0, dc: by, extend } : { dr: by, dc: 0, extend })
  }

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
      // Sort with no keys (Custom Sort): the range and the header guess are the engine's.
      case 'sort':
        openSortDialog({ range: String(asked.params.range ?? state?.selection ?? ''), header: asked.params.header !== false, active: state?.active ?? '' })
        break
      case 'insertFunction':
        openInsertFunction({
          onChoose: async (name) => {
            await editor.run('formula.insertFunction', { name })
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

  // The commands the editor has its own windows for never show their parameters dialog: whatever
  // asks for one (a menu item, a ribbon tab's command list, the engine's request above) opens the
  // window instead, before the dialog paints.
  // Custom Sort asks the engine for its range first (its `sort` dialog above); Paste Special opens
  // its own dialog.
  const ownWindows: Record<string, FindTab> = { 'edit.find': 'find', 'edit.replace': 'replace', 'edit.goTo': 'goTo' }
  const askedFor = editor.dialog?.command.id
  useLayoutEffect(() => {
    if (!askedFor) return
    const tab = ownWindows[askedFor]
    if (tab) {
      editor.setDialog(null)
      openFind(tab)
    } else if (askedFor === 'data.sort') {
      editor.setDialog(null)
      void editor.run('data.sort', {})
    } else if (askedFor === 'edit.pasteSpecial') {
      editor.setDialog(null)
      pasteSpecial()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- per dialog asked for
  }, [editor.dialog])

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

  // The editor's own keys are a table (keys.ts): Page Down and Up, Cmd+Shift+Z, Cmd+0, Find, the
  // clipboard and the rest, each through the editor's ordered runs.
  const keyContext: KeyContext | null = state
    ? {
        state,
        run: (command, params = {}) => void editor.run(command, params),
        answer: async (command, params = {}) => {
          const raw = await editor.run(command, params)
          return raw ? (JSON.parse(raw) as Record<string, unknown>) : null
        },
        openFind,
        openPasteSpecial: pasteSpecial,
        clipboard: (action) => void clipboard(action),
        page,
        zoomStep,
        openFormatCells,
        openValidationList: () => openValidationList(true),
      }
    : null
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => pressKey(event, keyContext)
  // The keys taken before kit-editor sees them (keys.ts, `capture`): Shift+Space would type a
  // space into the cell, a Mac's Ctrl+Enter would fill the selection. Only from the grid.
  const capturedKeys = useRef(keyContext)
  capturedKeys.current = keyContext
  useEffect(() => {
    const listen = (event: globalThis.KeyboardEvent) => {
      if (event.defaultPrevented || !(event.target instanceof Element) || !event.target.closest('.kui-text-sink, .kui-canvas')) return
      if (!pressKey(event, capturedKeys.current, true)) return
      event.preventDefault()
      event.stopImmediatePropagation()
    }
    document.addEventListener('keydown', listen, true)
    return () => document.removeEventListener('keydown', listen, true)
  }, [])

  const ribbons: Record<string, ReactNode> = {
    file: (
      <>
        <CommandTab menu="File" exclude={[...HOST_FILE_COMMANDS, ...EXPORT_COMMANDS]} primary={SHEET_PRIMARY.File} />
        <ExportGroup sheet={state?.sheet ?? ''} sheets={state?.sheets ?? []} />
      </>
    ),
    home: state ? <HomeTab info={state} onClipboard={(action) => void clipboard(action)} onPasteSpecial={pasteSpecial} pasteSpecialOff={pasteSpecialOff} /> : <CommandTab menu="Home" />,
    view: state ? <ViewTab info={state} formulaBar={formulaBarBox} onFormulaBar={showFormulaBar} zoom={zoomActions} /> : <CommandTab menu="View" />,
    formulas: (
      <>
        {/* Insert Function opens the kit's dialog; the function chosen starts the formula in the cell. */}
        <RibbonGroup label={phrase('Insert Function')} icon={FunctionSquare}>
          <RibbonButton
            icon={FunctionSquare}
            label={phrase('Insert Function')}
            size="large"
            primary
            disabled={!editor.enabled('formula.insertFunction')}
            onClick={() =>
              openInsertFunction({
                onChoose: async (name) => {
                  await editor.run('formula.insertFunction', { name })
                  backToGrid()
                },
                onCancel: backToGrid,
              })
            }
            data-testid="sheet-insert-function-button"
          />
        </RibbonGroup>
        <CommandTab menu="Formulas" exclude={['formula.insertFunction']} primary={SHEET_PRIMARY.Formulas} />
      </>
    ),
    insert: (
      <>
        {/* The engine's Illustrations (Pictures, Icons) join the kit's own; its Shapes is the kit's. */}
        <DrawingInsertGroup testId="sheet-drawing" merge="Insert/Illustrations" exclude={['shape.insert']} primary={['image.insert', 'icon.insert']} />
        <RibbonGroup label={t('chartInsert')} icon={ChartColumn}>
          <ChartInsertMenu testId="sheet-chart" primary />
        </RibbonGroup>
        {/* Charts and Text Box are the shared controls'; Symbol is the kit's panel (WPS's). */}
        <CommandTab menu="Insert" exclude={['chart.insert', 'textBox.insert', 'symbol.insert']} omit={['Illustrations']} primary={SHEET_PRIMARY.Insert} />
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
        <CommandTab menu="Draw" exclude={['ink.select', 'ink.pen', 'ink.pencil', 'ink.highlighter', 'ink.eraser', 'ink.state']} primary={SHEET_PRIMARY.Draw} />
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
        canvas={frameCanvas}
        onKeyDown={keyDown}
        statusEnd={
          state ? (
            <>
              <FocusCellButton info={state} place="status" />
              <ZoomControl {...zoomActions} />
            </>
          ) : null
        }
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
          <div className="sheet-grid-workspace" data-testid="sheet-grid-workspace">
          <div className="sheet-grid-viewport" id={gridId} ref={gridRegion}>
          <KitDocumentCanvas canvas={canvas} viewMemory={viewMemory} framed={false} tool={gridTool} decorate={comments.decorate} onView={(view) => {
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
              // Back from far away (Cmd+↓ to the last row and up again), the sheet's reach comes
              // back to what the view needs, so the scrollbar's thumb is not left at its smallest
              // (walkthrough A6); a little past the view, as when it grows.
              const shrink = (now: number, need: number) => (now > need * 4 + 10_000 ? need : now)
              reach.current = { doc, right: shrink(reach.current.right, right + 512), bottom: shrink(reach.current.bottom, bottom + 600) }
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
          {doc && !session.failure ? <SheetScrollbars views={views} canvas={canvas} width={doc.width} height={doc.height} controls={gridId} vertical={t('verticalScroll')} horizontal={t('horizontalScroll')} /> : null}
          </div>
        </div>
        {state ? (
          <FrameChrome>
            <SheetTabs info={state} />
          </FrameChrome>
        ) : null}
        {state ? <SheetDialogs info={state} /> : null}
        {state ? <GridMenu info={state} onClipboard={(action) => void clipboard(action)} onPasteSpecial={pasteSpecial} pasteSpecialOff={pasteSpecialOff} /> : null}
        <InsertFunctionDialog />
        {state ? <FindReplaceDialog /> : null}
        {state ? <SortDialog /> : null}
        {state ? <PasteSpecialDialog /> : null}
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
