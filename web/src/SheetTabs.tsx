import { useCallback, useEffect, useLayoutEffect, useRef, useState, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent } from 'react'
import {
  ChevronDownIcon as ChevronDown,
  ChevronLeftIcon as ChevronLeft,
  ChevronRightIcon as ChevronRight,
  EyeOffIcon as EyeOff,
  ListIcon as List,
  LockIcon as Lock,
  PlusIcon as Plus,
} from '@kition/tessera/icons/react'
import { ColorGrid, ContextMenu, IconButton, MenuItem, MenuSeparator, SubMenu, toast, useKitEditor, useReadOnly, type ContextMenuPoint } from '@kition/sdk/editor'
import type { SheetInfo, SheetTab } from './sheet'
import { askDeleteSheet } from './SheetDialogs'
import { backToGrid } from './sheet'

/** How far the pointer moves before a press on a tab becomes a drag. */
const DRAG_START = 4
/** How far above or below the tabs a dragged tab can still be dropped. */
const DROP_SLACK = 12
/** The band at each end of the tabs that scrolls them while a tab is dragged there. */
const EDGE = 28
/** How far one press of a scroll button moves the tabs. */
const SCROLL_STEP = 160

/** ASCII letters folded to lower case, as sheet names compare (the engine's rule). */
const folded = (name: string) => name.replace(/[A-Z]/g, (letter) => letter.toLowerCase())

/**
 * Why `name` cannot be sheet `from`'s new name (a key of the editor's strings), or null: blank
 * (spaces only too), a character a sheet name cannot hold, an apostrophe at either end, the
 * reserved "History", or another sheet's name. The engine checks the same; this says it in the
 * editor's words before the name is sent.
 */
export function sheetNameProblem(name: string, from: string, names: readonly string[]): string | null {
  const text = name.trim()
  if (!text) return 'sheetNameEmpty'
  if (/[:\\/?*[\]]/.test(text)) return 'sheetNameInvalid'
  if (text.startsWith("'") || text.endsWith("'")) return 'sheetNameApostrophe'
  if (folded(text) === 'history') return 'sheetNameReserved'
  if (names.some((other) => other !== from && folded(other) === folded(text))) return 'sheetNameTaken'
  return null
}

/** The rename field's width for `text`, in `ch`: wide characters count twice, so the tab fits its name. */
const fieldWidth = (text: string) => Math.max(4, [...text].reduce((width, char) => width + ((char.codePointAt(0) ?? 0) > 0x2e7f ? 2 : 1), 0)) + 2

/** A tab being dragged: where it was pressed, where the pointer is, and where it would land. */
type Drag = {
  sheet: string
  pointer: number
  startX: number
  startY: number
  x: number
  y: number
  /** Whether the press has become a drag (it moved far enough). */
  moving: boolean
  /** Pressed in the rename field: a drag starts only once the pointer leaves the tab (inside, it selects text). */
  fromField: boolean
  /** Whether the press may become a drag (not for a viewer, nor with one tab). */
  canDrag: boolean
  /** The tab's box when pressed and where in it the pointer took it, for the copy that follows the pointer. */
  grab: { dx: number; top: number; height: number }
  /** The gap among the visible tabs it would land in (0: before the first), or null where it cannot be dropped. */
  slot: number | null
}

/**
 * The workbook's sheets under the grid, as WPS lays them out: the sheet list and New Sheet at
 * the left, then the tabs, which scroll. A click shows a sheet; a tab is dragged to move its sheet
 * (a mark shows where it lands; Escape, leaving the window, or a drop away from the tabs leaves it
 * where it was); a double-click renames it; the active tab's arrow and any tab's right-click open
 * the sheet menu (New Sheet before or after it, Rename, Move Left/Right, Create a Copy, which
 * opens the copy's name to edit, Tab Color, Hide, Delete after asking). The sheet list shows each
 * sheet's colour and greys a hidden sheet, with its Unhide button. A viewer chooses sheets and
 * nothing else.
 */
export function SheetTabs({ info }: { info: SheetInfo }) {
  const { run, t } = useKitEditor()
  const readOnly = useReadOnly()
  const [renaming, setRenaming] = useState<string | null>(null)
  const [name, setName] = useState('')
  const [menu, setMenu] = useState<{ sheet: string; at: ContextMenuPoint } | null>(null)
  const [list, setList] = useState<ContextMenuPoint | null>(null)
  const [drag, setDrag] = useState<Drag | null>(null)
  const strip = useRef<HTMLDivElement>(null)
  const tabRefs = useRef(new Map<string, HTMLDivElement>())
  const dragRef = useRef<Drag | null>(null)
  const renamingRef = useRef(renaming)
  renamingRef.current = renaming
  /** A drag just ended on a tab: the click that follows is not a choice of that tab. */
  const swallowClick = useRef(false)

  const tabs = info.tabs
  const visible = tabs.filter((tab) => !tab.hidden)
  const indexOf = (sheet: string) => tabs.findIndex((tab) => tab.name === sheet)
  const activate = (sheet: string) => {
    if (sheet !== info.sheet) void run('sheet.activate', { sheet })
  }
  const latest = useRef({ visible, tabs, run, activate })
  latest.current = { visible, tabs, run, activate }

  /** The sheet being renamed, set at once (state lags a render): a rename ends once. */
  const renameOpen = useRef<string | null>(null)
  /**
   * Ends a rename, keeping what was typed: Enter (`stay`) leaves the field open on a name the
   * sheet cannot take, saying why; a click elsewhere puts the old name back. The same name, or one
   * only spaced differently, changes nothing.
   */
  const rename = async (stay = false) => {
    const from = renameOpen.current
    if (!from || from !== renaming) return
    const to = name.trim()
    const problem = to === from ? null : sheetNameProblem(name, from, tabs.map((tab) => tab.name))
    if (problem) {
      toast('error', t(problem))
      if (stay) return
    }
    renameOpen.current = null
    setRenaming(null)
    if (stay) backToGrid()
    if (!problem && to !== from) await run('sheet.rename', { sheet: from, name: to })
  }
  const cancelRename = () => {
    renameOpen.current = null
    setRenaming(null)
    backToGrid()
  }
  const startRename = (sheet: string) => {
    if (readOnly) return
    renameOpen.current = sheet
    setName(sheet)
    setRenaming(sheet)
  }
  /** Copies a sheet right after itself; the copy shows with its name open to edit (Enter keeps it). */
  const copySheet = async (sheet: string) => {
    const answer = await run('sheet.move', { sheet, to: indexOf(sheet) + 1, copy: true })
    let copy: unknown = null
    try {
      copy = answer ? (JSON.parse(answer) as { name?: unknown }).name : null
    } catch {
      copy = null
    }
    if (typeof copy === 'string') startRename(copy)
  }
  const latestRename = useRef(rename)
  latestRename.current = rename

  // The active tab stays in view: after a switch, a move, a new sheet, or a rename.
  const order = visible.map((tab) => tab.name).join('\u0000')
  useLayoutEffect(() => {
    const box = strip.current
    const tab = tabRefs.current.get(info.sheet)
    if (!box || !tab) return
    const left = tab.offsetLeft
    const right = left + tab.offsetWidth
    if (left < box.scrollLeft) box.scrollLeft = left
    else if (right > box.scrollLeft + box.clientWidth) box.scrollLeft = right - box.clientWidth
  }, [info.sheet, order])

  /**
   * The gap among the visible tabs that `sheet`, dragged to `x`, `y` (client pixels), would land in
   * (0: before the first tab), counted by the other tabs' middles; null when `x`, `y` is off the tabs.
   */
  const slotAt = useCallback((sheet: string, x: number, y: number): number | null => {
    const box = strip.current?.getBoundingClientRect()
    if (!box || y < box.top - DROP_SLACK || y > box.bottom + DROP_SLACK || x < box.left || x > box.right) return null
    const shown = latest.current.visible
    const at = shown.findIndex((tab) => tab.name === sheet)
    let passed = 0
    for (const tab of shown) {
      if (tab.name === sheet) continue
      const rect = tabRefs.current.get(tab.name)?.getBoundingClientRect()
      if (rect && x > rect.left + rect.width / 2) passed += 1
    }
    return at >= 0 && passed >= at ? passed + 1 : passed
  }, [])

  /** Where the dragged sheet goes for `slot` (the `to` of `sheet.move`), or null when it stays. */
  const target = useCallback((sheet: string, slot: number): number | null => {
    const { visible: shown, tabs: all } = latest.current
    const at = shown.findIndex((tab) => tab.name === sheet)
    if (at < 0 || slot === at || slot === at + 1) return null
    const rest = all.filter((tab) => tab.name !== sheet).map((tab) => tab.name)
    if (slot < shown.length) return rest.indexOf(shown[slot].name)
    return rest.indexOf(shown[shown.length - 1].name) + 1
  }, [])

  const update = (next: Drag | null) => {
    dragRef.current = next
    setDrag(next)
  }

  const endDrag = useCallback((drop: boolean) => {
    const current = dragRef.current
    if (!current) return
    dragRef.current = null
    setDrag(null)
    const element = tabRefs.current.get(current.sheet)
    if (element?.hasPointerCapture(current.pointer)) element.releasePointerCapture(current.pointer)
    // The click that follows the release is this press's, already answered here.
    swallowClick.current = true
    window.setTimeout(() => {
      swallowClick.current = false
    }, 0)
    // A press released without a drag chooses its sheet, wherever the release lands: a rename
    // ended by this press may have moved the tabs under the pointer.
    if (!current.moving) {
      if (drop) latest.current.activate(current.sheet)
      return
    }
    if (!drop || current.slot == null) return
    const to = target(current.sheet, current.slot)
    if (to != null) void latest.current.run('sheet.move', { sheet: current.sheet, to })
  }, [target])

  // While a tab is held: the pointer anywhere moves it, a release drops it, and Escape or the
  // window losing focus puts it back.
  const held = drag != null
  useEffect(() => {
    if (!held) return undefined
    const onMove = (event: PointerEvent) => {
      const current = dragRef.current
      if (!current || event.pointerId !== current.pointer) return
      // The button was let go where this page did not see it (outside the window): the drag ends there.
      if (event.buttons === 0) {
        endDrag(false)
        return
      }
      const far = Math.abs(event.clientX - current.startX) > DRAG_START || Math.abs(event.clientY - current.startY) > DRAG_START
      const box = tabRefs.current.get(current.sheet)?.getBoundingClientRect()
      const outside = !box || event.clientX < box.left || event.clientX > box.right
      const moving = current.moving || (current.canDrag && far && (!current.fromField || outside))
      if (moving && !current.moving) {
        // A drag from the rename field ends the rename first, keeping what was typed.
        if (renamingRef.current === current.sheet) void latestRename.current()
        tabRefs.current.get(current.sheet)?.setPointerCapture(current.pointer)
      }
      update({ ...current, x: event.clientX, y: event.clientY, moving, slot: moving ? slotAt(current.sheet, event.clientX, event.clientY) : null })
    }
    const onUp = (event: PointerEvent) => {
      if (dragRef.current && event.pointerId === dragRef.current.pointer) endDrag(true)
    }
    const onCancel = () => endDrag(false)
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || !dragRef.current?.moving) return
      event.preventDefault()
      event.stopPropagation()
      endDrag(false)
    }
    window.addEventListener('pointermove', onMove, true)
    window.addEventListener('pointerup', onUp, true)
    window.addEventListener('pointercancel', onCancel, true)
    window.addEventListener('keydown', onKey, true)
    window.addEventListener('blur', onCancel)
    return () => {
      window.removeEventListener('pointermove', onMove, true)
      window.removeEventListener('pointerup', onUp, true)
      window.removeEventListener('pointercancel', onCancel, true)
      window.removeEventListener('keydown', onKey, true)
      window.removeEventListener('blur', onCancel)
    }
  }, [held, endDrag, slotAt])

  // A dragged tab held at either end of the tabs scrolls them, bringing the next tabs into view.
  const moving = drag?.moving === true
  useEffect(() => {
    if (!moving) return undefined
    const timer = window.setInterval(() => {
      const current = dragRef.current
      const box = strip.current
      if (!current || !box) return
      const rect = box.getBoundingClientRect()
      if (current.y < rect.top - DROP_SLACK || current.y > rect.bottom + DROP_SLACK) return
      const step = current.x > rect.right - EDGE ? 12 : current.x < rect.left + EDGE ? -12 : 0
      if (!step) return
      const before = box.scrollLeft
      box.scrollLeft = before + step
      if (box.scrollLeft !== before) update({ ...current, slot: slotAt(current.sheet, current.x, current.y) })
    }, 30)
    return () => window.clearInterval(timer)
  }, [moving, slotAt])

  const press = (sheet: string, event: ReactPointerEvent<HTMLElement>) => {
    if (event.button !== 0) return
    const fromField = event.target instanceof HTMLInputElement
    const box = event.currentTarget.getBoundingClientRect()
    const grab = { dx: event.clientX - box.left, top: box.top, height: box.height }
    const canDrag = !readOnly && visible.length > 1
    update({ sheet, pointer: event.pointerId, startX: event.clientX, startY: event.clientY, x: event.clientX, y: event.clientY, moving: false, fromField, canDrag, grab, slot: null })
  }

  const openMenu = (sheet: string, at: ContextMenuPoint) => {
    setList(null)
    setMenu({ sheet, at })
  }

  // The window losing the focus (another browser tab, another window) closes the sheet menu and
  // the sheet list, changing nothing.
  const menusOpen = menu != null || list != null
  useEffect(() => {
    if (!menusOpen) return undefined
    const onBlur = () => {
      setMenu(null)
      setList(null)
    }
    window.addEventListener('blur', onBlur)
    return () => window.removeEventListener('blur', onBlur)
  }, [menusOpen])

  // The sheet menu and the sheet list open above the tab bar, never over the tabs: the tabs stay
  // there to click (a click on another tab closes the menu and shows that sheet).
  const bar = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    const top = bar.current?.getBoundingClientRect().top
    const lift = (testId: string, at: ContextMenuPoint | null, set: (at: ContextMenuPoint) => void) => {
      const element = bar.current?.ownerDocument.querySelector<HTMLElement>(`[data-testid="${testId}"]`)
      if (!at || top == null || !element) return
      const y = Math.max(4, top - element.offsetHeight - 2)
      if (at.y !== y) set({ x: at.x, y })
    }
    lift('sheet-menu', menu?.at ?? null, (at) => setMenu((open) => (open ? { ...open, at } : open)))
    lift('sheet-list', list, setList)
  }, [menu, list])

  // Where the drop mark stands: the gap the dragged tab would land in, when that moves it.
  let mark: number | null = null
  if (drag?.moving && drag.slot != null && target(drag.sheet, drag.slot) != null) {
    const before = visible[drag.slot]
    const after = visible[drag.slot - 1]
    const element = before ? tabRefs.current.get(before.name) : after ? tabRefs.current.get(after.name) : undefined
    if (element) mark = before ? element.offsetLeft - 1 : element.offsetLeft + element.offsetWidth + 1
  }
  const dropOff = drag?.moving === true && drag.slot == null

  const menuSheet = menu ? tabs.find((tab) => tab.name === menu.sheet) ?? null : null
  const menuIndex = menuSheet ? indexOf(menuSheet.name) : -1
  // Move Left and Move Right go by the sheets' real order, hidden sheets included: a move can
  // pass a hidden sheet.
  const left = menuIndex > 0
  const right = menuIndex >= 0 && menuIndex < tabs.length - 1

  return (
    <div ref={bar} className="sheet-tabs" data-testid="sheet-tabs" data-drop={dropOff ? 'none' : undefined}>
      <IconButton
        icon={List}
        label={t('sheetList')}
        onClick={(event) => {
          const box = event.currentTarget.getBoundingClientRect()
          setMenu(null)
          setList({ x: box.left, y: box.top })
        }}
        data-testid="sheet-list-button"
      />
      {/* New Sheet adds a sheet after the last one, as WPS's does. */}
      <IconButton icon={Plus} label={t('addSheet')} disabled={readOnly} onClick={() => void run('sheet.insert', { before: tabs.length })} data-testid="sheet-add" />
      <div
        ref={strip}
        className="sheet-strip"
        role="tablist"
        data-testid="sheet-strip"
        onWheel={(event) => {
          if (Math.abs(event.deltaY) > Math.abs(event.deltaX)) event.currentTarget.scrollLeft += event.deltaY
        }}
      >
        {visible.map((tab) => (
          <SheetTabButton
            key={tab.name}
            tab={tab}
            active={tab.name === info.sheet}
            dragging={drag?.moving === true && drag.sheet === tab.name}
            renaming={renaming === tab.name}
            name={name}
            readOnly={readOnly}
            menuLabel={t('sheetMenu')}
            lockLabel={t('sheetLocked')}
            refFor={(element) => {
              if (element) tabRefs.current.set(tab.name, element)
              else tabRefs.current.delete(tab.name)
            }}
            onName={setName}
            onRename={(stay) => void rename(stay)}
            onCancelRename={cancelRename}
            onPress={(event) => press(tab.name, event)}
            onClick={() => {
              if (swallowClick.current) return
              activate(tab.name)
            }}
            onDoubleClick={() => startRename(tab.name)}
            onContextMenu={(event) => {
              event.preventDefault()
              // A viewer gets no sheet menu, and the right-click changes nothing.
              if (readOnly) return
              activate(tab.name)
              openMenu(tab.name, { x: event.clientX, y: event.clientY })
            }}
            onMenu={(event) => {
              const box = event.currentTarget.getBoundingClientRect()
              openMenu(tab.name, { x: box.left, y: box.top })
            }}
          />
        ))}
        {mark != null ? <div className="sheet-drop-mark" style={{ left: mark }} data-slot={drag?.slot ?? undefined} data-testid="sheet-drop-mark" /> : null}
      </div>
      {drag?.moving ? (
        // The dragged tab, following the pointer above the others (which stay where they are).
        <div className="sheet-tab sheet-tab-ghost" style={{ left: drag.x - drag.grab.dx, top: drag.grab.top, height: drag.grab.height }} aria-hidden="true" data-testid="sheet-drag-ghost">
          <span className="sheet-tab-name">{drag.sheet}</span>
        </div>
      ) : null}
      <IconButton icon={ChevronLeft} label={t('sheetScrollLeft')} onClick={() => strip.current?.scrollBy({ left: -SCROLL_STEP })} data-testid="sheet-scroll-left" />
      <IconButton icon={ChevronRight} label={t('sheetScrollRight')} onClick={() => strip.current?.scrollBy({ left: SCROLL_STEP })} data-testid="sheet-scroll-right" />

      {/* The sheet list: every sheet in order, the current one checked, a tab colour as a bar at the
          row's start, and a hidden sheet greyed with its Unhide button (which leaves the sheet in
          view where it is, and the list open). A row only chooses its sheet. */}
      <ContextMenu at={list} onClose={() => setList(null)} label={t('sheetList')} testId="sheet-list">
        <div className="sheet-list">
          {tabs.map((tab) => (
            <div key={tab.name} className="sheet-list-row" data-hidden={tab.hidden || undefined}>
              <span className="sheet-list-color" style={tab.color ? { background: tab.color } : undefined} data-color={tab.color ?? undefined} data-testid={`sheet-list-color-${tab.name}`} />
              <MenuItem
                label={tab.name}
                shortcut={tab.hidden ? t('sheetHidden') : null}
                checked={tab.name === info.sheet}
                disabled={tab.hidden}
                onSelect={() => activate(tab.name)}
                testId={`sheet-list-item-${tab.name}`}
              />
              {tab.hidden ? (
                <button
                  type="button"
                  className="sheet-list-unhide"
                  title={t('unhideSheet')}
                  aria-label={t('unhideSheet')}
                  onClick={(event) => {
                    event.stopPropagation()
                    if (!readOnly) void run('sheet.unhide', { sheet: tab.name, activate: false })
                  }}
                  data-testid={`sheet-list-unhide-${tab.name}`}
                >
                  <EyeOff size={14} aria-hidden="true" />
                </button>
              ) : null}
            </div>
          ))}
        </div>
      </ContextMenu>

      <ContextMenu at={menu?.at ?? null} onClose={() => setMenu(null)} label={t('sheetMenu')} testId="sheet-menu">
        {menuSheet ? (
          <>
            {readOnly ? (
              <MenuItem label={t('addSheet')} disabled onSelect={() => undefined} testId="sheet-menu-insert" />
            ) : (
              <SubMenu label={t('addSheet')} path="Sheet Tab/New Sheet" testId="sheet-menu-insert">
                <MenuItem label={t('insertSheetBefore')} onSelect={() => void run('sheet.insert', { before: menuIndex })} testId="sheet-menu-insert-before" />
                <MenuItem label={t('insertSheetAfter')} onSelect={() => void run('sheet.insert', { before: menuIndex + 1 })} testId="sheet-menu-insert-after" />
              </SubMenu>
            )}
            <MenuItem label={t('renameSheet')} disabled={readOnly} onSelect={() => startRename(menuSheet.name)} testId="sheet-menu-rename" />
            {readOnly || (!left && !right) ? (
              <MenuItem label={t('moveSheet')} disabled onSelect={() => undefined} testId="sheet-menu-move" />
            ) : (
              <SubMenu label={t('moveSheet')} path="Sheet Tab/Move" testId="sheet-menu-move">
                {left ? <MenuItem label={t('moveSheetLeft')} onSelect={() => void run('sheet.move', { sheet: menuSheet.name, to: menuIndex - 1 })} testId="sheet-menu-move-left" /> : null}
                {right ? <MenuItem label={t('moveSheetRight')} onSelect={() => void run('sheet.move', { sheet: menuSheet.name, to: menuIndex + 1 })} testId="sheet-menu-move-right" /> : null}
              </SubMenu>
            )}
            <MenuItem label={t('copySheet')} disabled={readOnly} onSelect={() => void copySheet(menuSheet.name)} testId="sheet-menu-copy" />
            {readOnly ? (
              <MenuItem label={t('tabColor')} disabled onSelect={() => undefined} testId="sheet-menu-color" />
            ) : (
              // No Fill as a bar on top, then ten columns of five colours; a pick takes effect at once.
              <SubMenu label={t('tabColor')} path="Sheet Tab/Tab Color" testId="sheet-menu-color">
                <MenuItem label={t('noTabColor')} onSelect={() => void run('sheet.tabColor', { sheet: menuSheet.name, color: 'none' })} testId="sheet-menu-no-color" />
                <ColorGrid onPick={(color) => void run('sheet.tabColor', { sheet: menuSheet.name, color })} testId="sheet-menu-colors" />
              </SubMenu>
            )}
            {readOnly || visible.length > 1 ? (
              <MenuItem label={t('hideSheet')} disabled={readOnly} onSelect={() => void run('sheet.hide', { sheet: menuSheet.name })} testId="sheet-menu-hide" />
            ) : (
              // With one sheet showing, Hide is greyed and says why.
              <span className="sheet-menu-hint" title={t('hideNeedsTwo')} data-testid="sheet-menu-hide-hint">
                <MenuItem label={t('hideSheet')} disabled onSelect={() => undefined} testId="sheet-menu-hide" />
              </span>
            )}
            <MenuSeparator />
            {/* Delete asks first; the workbook's only sheet cannot go. */}
            <MenuItem label={t('deleteSheet')} disabled={readOnly || tabs.length < 2} onSelect={() => askDeleteSheet(menuSheet.name)} testId="sheet-menu-delete" />
          </>
        ) : null}
      </ContextMenu>
    </div>
  )
}

type TabProps = {
  tab: SheetTab
  active: boolean
  dragging: boolean
  renaming: boolean
  name: string
  readOnly: boolean
  menuLabel: string
  lockLabel: string
  refFor: (element: HTMLDivElement | null) => void
  onName: (name: string) => void
  /** Ends the rename keeping what was typed; `stay`: Enter, which leaves the field open on a name the sheet cannot take. */
  onRename: (stay: boolean) => void
  onCancelRename: () => void
  onPress: (event: ReactPointerEvent<HTMLElement>) => void
  onClick: () => void
  onDoubleClick: () => void
  onContextMenu: (event: ReactMouseEvent<HTMLElement>) => void
  onMenu: (event: ReactMouseEvent<HTMLButtonElement>) => void
}

/** One sheet's tab: its name (or the rename field), its colour, a lock, and on the active tab the menu arrow. */
function SheetTabButton(props: TabProps) {
  const { tab, active, dragging, renaming } = props
  return (
    <div
      ref={props.refFor}
      role="tab"
      tabIndex={active ? 0 : -1}
      className="sheet-tab"
      aria-selected={active}
      title={tab.name}
      data-color={tab.color ?? undefined}
      data-dragging={dragging || undefined}
      style={tab.color ? { ['--sheet-tab-color' as string]: tab.color } : undefined}
      onPointerDown={props.onPress}
      onClick={props.onClick}
      onDoubleClick={props.onDoubleClick}
      onContextMenu={props.onContextMenu}
      data-testid={`sheet-tab-${tab.name}`}
    >
      {tab.protected ? (
        <span className="sheet-tab-lock" role="img" aria-label={props.lockLabel} title={props.lockLabel} data-testid={`sheet-lock-${tab.name}`}>
          <Lock size={12} aria-hidden="true" />
        </span>
      ) : null}
      {renaming ? (
        // The name opens selected, so what is typed replaces it; the field grows with the name.
        <input
          className="kui-field sheet-tab-rename"
          autoFocus
          maxLength={31}
          value={props.name}
          style={{ width: `${fieldWidth(props.name)}ch` }}
          onFocus={(event) => event.currentTarget.select()}
          onChange={(event) => props.onName(event.target.value)}
          onBlur={() => props.onRename(false)}
          onClick={(event) => event.stopPropagation()}
          onDoubleClick={(event) => event.stopPropagation()}
          // Pressing on the selected name and moving drags the tab, never the text: a native text
          // drag would cancel the pointer (pointercancel) and the tab would not move (Linux).
          onDragStart={(event) => event.preventDefault()}
          onKeyDown={(event) => {
            if (event.key === 'Enter') props.onRename(true)
            // What this Escape cancelled: the rename.
            if (event.key === 'Escape') {
              event.preventDefault()
              event.stopPropagation()
              props.onCancelRename()
            }
          }}
          data-testid="sheet-rename"
        />
      ) : (
        <span className="sheet-tab-name">{tab.name}</span>
      )}
      {active ? (
        <button
          type="button"
          className="sheet-tab-arrow"
          aria-label={props.menuLabel}
          title={props.menuLabel}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => {
            event.stopPropagation()
            props.onMenu(event)
          }}
          data-testid="sheet-menu-arrow"
        >
          <ChevronDown size={12} aria-hidden="true" />
        </button>
      ) : null}
    </div>
  )
}
