import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent as ReactMouseEvent } from 'react'
import { CommandItem, ContextMenu, MenuItem, MenuSeparator, SubMenu, useKitEditor, useReadOnly, type ContextMenuPoint } from '@kition/sdk/editor'
import { backToGrid, type SheetInfo } from './sheet'
import { askConfirm } from './SheetDialogs'

/** The most rows and columns one insert adds (the count box takes no more, as the case sheets ask). */
export const MAX_INSERT_ROWS = 1000
export const MAX_INSERT_COLS = 100

/**
 * A menu row with a count box (向上插入 [1] 行): a press anywhere on the row but the box runs it
 * with the count shown, and Enter in the box does too. The box takes digits only: a value with
 * anything else (a letter, a sign, a decimal point, Chinese) is refused whole and the box keeps
 * what it had; a number over `max` shows as `max`. Empty or 0, the row does nothing.
 */
export function CountItem({ before, after, initial, max, disabled, onRun, testId }: { before: string; after: string; initial: number; max: number; disabled?: boolean; onRun: (count: number) => void; testId: string }) {
  const [text, setText] = useState(String(Math.max(1, Math.min(initial, max))))
  // A hidden button the row presses after running: the menu around it closes on a button's click.
  const closer = useRef<HTMLButtonElement>(null)
  const count = Number(text)
  const usable = !disabled && Number.isInteger(count) && count >= 1
  const run = () => {
    if (!usable) return
    onRun(count)
    closer.current?.click()
  }
  return (
    <div
      className="kui-menu-item sheet-count-item"
      role="menuitem"
      tabIndex={-1}
      aria-disabled={disabled || undefined}
      data-disabled={disabled || undefined}
      onClick={(event: ReactMouseEvent<HTMLDivElement>) => {
        if ((event.target as HTMLElement).closest('input, button')) return
        run()
      }}
      onKeyDown={(event: ReactKeyboardEvent<HTMLDivElement>) => {
        if (event.target === event.currentTarget && (event.key === 'Enter' || event.key === ' ')) {
          event.preventDefault()
          event.stopPropagation()
          run()
        }
      }}
      data-testid={testId}
    >
      <span style={{ width: 15 }} />
      <span>{before}</span>
      <input
        className="sheet-count-field"
        inputMode="numeric"
        value={text}
        disabled={disabled}
        aria-label={`${before} ${after}`}
        onChange={(event) => {
          const value = event.target.value
          if (!/^\d*$/.test(value)) return
          setText(value === '' ? '' : String(Math.min(Number(value), max)))
        }}
        onKeyDown={(event) => {
          // The box keeps its keys from the menu (Home, End, the arrows move in the text).
          if (event.key === 'Enter') {
            event.preventDefault()
            run()
          }
          if (event.key !== 'Escape') event.stopPropagation()
        }}
        data-testid={`${testId}-count`}
      />
      <span>{after}</span>
      <button ref={closer} type="button" tabIndex={-1} aria-hidden="true" style={{ display: 'none' }} />
    </div>
  )
}

/** What the insert and delete menus can do with the selection, and whether they may. */
function useLines(info: SheetInfo) {
  const { run, enabled, t } = useKitEditor()
  const readOnly = useReadOnly()
  const { lines } = info
  const all = lines.wholeRows && lines.wholeCols
  // WPS greys inserting and deleting while a cell is typed, and rows and columns with the whole sheet selected.
  const blocked = readOnly || info.editing
  const go = (command: string, params: Record<string, unknown>) => {
    void run(command, params).finally(backToGrid)
  }
  return {
    t,
    lines,
    rowsOff: blocked || all || !enabled('row.insert'),
    colsOff: blocked || all || !enabled('column.insert'),
    deleteRowsOff: blocked || all || !enabled('row.delete'),
    deleteColsOff: blocked || all || !enabled('column.delete'),
    cellsOff: blocked || all || !enabled('cell.insert'),
    deleteCellsOff: blocked || all || !enabled('cell.delete'),
    sheetOff: readOnly || info.editing || !enabled('sheet.insert'),
    insertRows: (count: number, below: boolean) => go('row.insert', below ? { count, below } : { count }),
    insertColumns: (count: number, right: boolean) => go('column.insert', right ? { count, right } : { count }),
    deleteRows: () => go('row.delete', {}),
    deleteColumns: () => go('column.delete', {}),
    insertCells: (shift: 'right' | 'down') => go('cell.insert', { shift }),
    deleteCells: (shift: 'left' | 'up') => go('cell.delete', { shift }),
    insertSheet: () => go('sheet.insert', {}),
  }
}

/** The four count rows: rows above and below, columns left and right. */
function LineCountItems({ info, testId, rows = true, cols = true }: { info: SheetInfo; testId: string; rows?: boolean; cols?: boolean }) {
  const l = useLines(info)
  const { t } = l
  return (
    <>
      {rows ? (
        <>
          <CountItem key={`a${l.lines.rows}`} before={t('insertAbove')} after={t('insertAboveAfter')} initial={l.lines.rows} max={MAX_INSERT_ROWS} disabled={l.rowsOff} onRun={(n) => l.insertRows(n, false)} testId={`${testId}-rows-above`} />
          <CountItem key={`b${l.lines.rows}`} before={t('insertBelow')} after={t('insertBelowAfter')} initial={l.lines.rows} max={MAX_INSERT_ROWS} disabled={l.rowsOff} onRun={(n) => l.insertRows(n, true)} testId={`${testId}-rows-below`} />
        </>
      ) : null}
      {cols ? (
        <>
          <CountItem key={`l${l.lines.cols}`} before={t('insertLeft')} after={t('insertLeftAfter')} initial={l.lines.cols} max={MAX_INSERT_COLS} disabled={l.colsOff} onRun={(n) => l.insertColumns(n, false)} testId={`${testId}-cols-left`} />
          <CountItem key={`r${l.lines.cols}`} before={t('insertRight')} after={t('insertRightAfter')} initial={l.lines.cols} max={MAX_INSERT_COLS} disabled={l.colsOff} onRun={(n) => l.insertColumns(n, true)} testId={`${testId}-cols-right`} />
        </>
      ) : null}
    </>
  )
}

/**
 * Home › Insert, as the case sheets list it: rows above and below, columns left and right (each
 * with its count, the selection's rows or columns at first), cells shifted right or down, a sheet.
 */
export function InsertMenuItems({ info, testId }: { info: SheetInfo; testId: string }) {
  const l = useLines(info)
  return (
    <>
      <LineCountItems info={info} testId={testId} />
      <MenuItem label={l.t('insertCellsRight')} disabled={l.cellsOff} onSelect={() => l.insertCells('right')} testId={`${testId}-cells-right`} />
      <MenuItem label={l.t('insertCellsDown')} disabled={l.cellsOff} onSelect={() => l.insertCells('down')} testId={`${testId}-cells-down`} />
      <MenuItem label={l.t('insertSheetItem')} disabled={l.sheetOff} onSelect={l.insertSheet} testId={`${testId}-sheet`} />
    </>
  )
}

/** Home › Delete: cells shifted left or up, the selected rows, the selected columns (WPS's order), and the sheet. */
export function DeleteMenuItems({ info, testId, sheet = false }: { info: SheetInfo; testId: string; sheet?: boolean }) {
  const l = useLines(info)
  return (
    <>
      <MenuItem label={l.t('deleteCellsLeft')} disabled={l.deleteCellsOff} onSelect={() => l.deleteCells('left')} testId={`${testId}-cells-left`} />
      <MenuItem label={l.t('deleteCellsUp')} disabled={l.deleteCellsOff} onSelect={() => l.deleteCells('up')} testId={`${testId}-cells-up`} />
      <MenuItem label={l.t('deleteRowsItem')} disabled={l.deleteRowsOff} onSelect={l.deleteRows} testId={`${testId}-rows`} />
      <MenuItem label={l.t('deleteColsItem')} disabled={l.deleteColsOff} onSelect={l.deleteColumns} testId={`${testId}-cols`} />
      {sheet ? <CommandItem command="sheet.delete" label={l.t('deleteSheetItem')} /> : null}
    </>
  )
}

type GridMenuState = { at: ContextMenuPoint; target: 'cell' | 'rows' | 'cols' | 'all' }
let openMenu: (state: GridMenuState | null) => void = () => undefined

/** Opens the grid's right-click menu for what the engine says was pressed (`editor.contextMenu`). */
export const showGridMenu = (state: GridMenuState | null) => openMenu(state)

/**
 * The grid's right-click menus, as WPS has them: a cell's (cut, copy, paste, Paste Special; Delete
 * and Insert submenus; Clear Contents with all, formats, contents, comments and links; Sort;
 * Merge and Unmerge; Insert Comment, Hyperlink, Format Cells), a row heading's (delete, hide,
 * unhide, rows above and below with their counts, row height) and a column heading's (the same
 * for columns).
 */
export function GridMenu({ info, onClipboard }: { info: SheetInfo; onClipboard: (action: 'cut' | 'copy' | 'paste') => void }) {
  const { t, enabled, run, ask } = useKitEditor()
  const [menu, setMenu] = useState<GridMenuState | null>(null)
  useEffect(() => {
    openMenu = setMenu
    return () => {
      openMenu = () => undefined
    }
  }, [])
  useEffect(() => {
    if (!menu) return undefined
    const onBlur = () => setMenu(null)
    window.addEventListener('blur', onBlur)
    return () => window.removeEventListener('blur', onBlur)
  }, [menu])
  const l = useLines(info)
  const close = () => {
    setMenu(null)
    backToGrid()
  }
  const clipboard = (
    <>
      <MenuItem label={t('menuCut')} disabled={!enabled('edit.cut')} onSelect={() => onClipboard('cut')} testId="sheet-grid-menu-cut" />
      <MenuItem label={t('menuCopy')} disabled={!enabled('edit.copy')} onSelect={() => onClipboard('copy')} testId="sheet-grid-menu-copy" />
      <MenuItem label={t('menuPaste')} disabled={!enabled('edit.paste')} onSelect={() => onClipboard('paste')} testId="sheet-grid-menu-paste" />
      {menu?.target === 'cell' || menu?.target === 'all' ? <MenuItem label={t('pasteSpecialItem')} disabled={!enabled('edit.pasteSpecial')} onSelect={() => ask('edit.pasteSpecial')} testId="sheet-grid-menu-paste-special" /> : null}
      <MenuSeparator />
    </>
  )
  const rows = menu?.target === 'rows'
  const go = (command: string, params: Record<string, unknown> = {}) => void run(command, params).finally(backToGrid)
  // Merging keeps only the upper-left value: asked first when other cells hold something (WPS).
  const merge = () => {
    const filled = info.stats?.count ?? 0
    if (filled > 1) askConfirm({ title: t('mergeTitle'), message: t('mergeConfirm'), testId: 'sheet-merge-confirm', onOk: () => go('cell.merge') })
    else go('cell.merge')
  }
  return (
    <ContextMenu at={menu?.at ?? null} onClose={close} label={t('gridMenu')} testId="sheet-grid-menu">
      {menu?.target === 'cell' || menu?.target === 'all' ? (
        <>
          {clipboard}
          <SubMenu label={t('deleteMenu')} path="Grid/Delete" testId="sheet-grid-menu-delete">
            <DeleteMenuItems info={info} testId="sheet-grid-delete" />
          </SubMenu>
          <SubMenu label={t('insertMenu')} path="Grid/Insert" testId="sheet-grid-menu-insert">
            {/* WPS for Mac's order: the cells first, then the rows and columns. */}
            <MenuItem label={t('insertCellsRight')} disabled={l.cellsOff} onSelect={() => l.insertCells('right')} testId="sheet-grid-insert-cells-right" />
            <MenuItem label={t('insertCellsDown')} disabled={l.cellsOff} onSelect={() => l.insertCells('down')} testId="sheet-grid-insert-cells-down" />
            <LineCountItems info={info} testId="sheet-grid-insert" />
            <MenuSeparator />
            <MenuItem label={t('insertSheetItem')} disabled={l.sheetOff} onSelect={l.insertSheet} testId="sheet-grid-insert-sheet" />
          </SubMenu>
          <MenuSeparator />
          <SubMenu label={t('clearMenu')} path="Grid/Clear" testId="sheet-grid-menu-clear">
            <MenuItem label={t('clearAllItem')} disabled={!enabled('edit.clearAll')} onSelect={() => go('edit.clearAll')} testId="sheet-grid-clear-all" />
            <MenuItem label={t('clearFormatsItem')} disabled={!enabled('edit.clearFormats')} onSelect={() => go('edit.clearFormats')} testId="sheet-grid-clear-formats" />
            <MenuItem label={t('clearContentsItem')} disabled={!enabled('edit.clearContents')} onSelect={() => go('edit.clearContents')} testId="sheet-grid-clear-contents" />
            <MenuItem label={t('clearCommentsItem')} disabled={!enabled('edit.clearComments')} onSelect={() => go('edit.clearComments')} testId="sheet-grid-clear-comments" />
            <MenuItem label={t('clearLinksItem')} disabled={!enabled('edit.clearHyperlinks')} onSelect={() => go('edit.clearHyperlinks')} testId="sheet-grid-clear-links" />
          </SubMenu>
          <SubMenu label={t('sortMenu')} path="Grid/Sort" testId="sheet-grid-menu-sort">
            <MenuItem label={t('sortAscendingItem')} disabled={!enabled('data.sortAscending')} onSelect={() => go('data.sortAscending')} testId="sheet-grid-sort-ascending" />
            <MenuItem label={t('sortDescendingItem')} disabled={!enabled('data.sortDescending')} onSelect={() => go('data.sortDescending')} testId="sheet-grid-sort-descending" />
          </SubMenu>
          <MenuItem label={t('mergeCellsItem')} disabled={!enabled('cell.merge')} onSelect={merge} testId="sheet-grid-menu-merge" />
          <MenuItem label={t('unmergeCellsItem')} disabled={!enabled('cell.unmerge')} onSelect={() => go('cell.unmerge')} testId="sheet-grid-menu-unmerge" />
          <MenuSeparator />
          <MenuItem label={t('insertNoteItem')} disabled={!enabled('note.add')} onSelect={() => ask('note.add')} testId="sheet-grid-menu-note" />
          {/* A checkbox cell holds no link (WPS greys it). */}
          <MenuItem label={t('linkItem')} disabled={info.checkbox || !enabled('link.insert')} onSelect={() => go('link.insert')} testId="sheet-grid-menu-link" />
          <CommandItem command="format.cells" label={t('formatCellsItem')} />
        </>
      ) : null}
      {menu?.target === 'rows' || menu?.target === 'cols' ? (
        <>
          {clipboard}
          <MenuItem label={t('deleteMenu')} disabled={rows ? l.deleteRowsOff : l.deleteColsOff} onSelect={rows ? l.deleteRows : l.deleteColumns} testId="sheet-grid-menu-delete" />
          <CommandItem command={rows ? 'row.hide' : 'column.hide'} label={t('hideLines')} />
          <CommandItem command={rows ? 'row.show' : 'column.show'} label={t('unhideLines')} />
          <LineCountItems info={info} testId="sheet-grid-insert" rows={rows} cols={!rows} />
          <CommandItem command={rows ? 'row.setHeight' : 'column.setWidth'} ask label={t(rows ? 'rowHeightItem' : 'columnWidthItem')} />
          <MenuSeparator />
          <CommandItem command="edit.clearContents" label={t('clearContents')} />
          <CommandItem command="format.cells" label={t('formatCellsItem')} />
        </>
      ) : null}
    </ContextMenu>
  )
}
