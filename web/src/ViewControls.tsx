import { useEffect, useRef } from 'react'
import { ChevronDownIcon as ChevronDown, ChevronUpIcon as ChevronUp, CrosshairIcon as Crosshair, Grid3x3Icon as Grid3x3, SnowflakeIcon as Snowflake } from '@kition/tessera-icons/react'
import { ColorGrid, MenuButton, MenuItem, MenuSeparator, RibbonButton, RibbonMenu, useKitEditor, useReadOnly } from '@kition/kit-editor'
import { FOCUS_COLORS, GRIDLINE_COLORS, backToGrid, cellParts, columnLetters, type SheetInfo } from './sheet'

/**
 * Freeze Panes as WPS has it (开始 › 冻结窗格, 视图 › 冻结窗格): 取消冻结 first while panes are
 * frozen, then 冻结至第 4 行 B 列, 冻结至第 4 行 and 冻结至第 B 列 for the active cell (each only
 * when it would freeze something), 冻结首行 and 冻结首列. Pointing at a choice previews it on the
 * grid (an engine overlay); each choice replaces the frozen panes.
 */
export function FreezeMenu({ info, size = 'large' }: { info: SheetInfo; size?: 'large' | 'small' }) {
  const { enabled, t } = useKitEditor()
  const readOnly = useReadOnly()
  return (
    <RibbonMenu icon={Snowflake} label={t('freezePanes')} size={size} disabled={readOnly || !enabled('view.freezeTopRow')} testId="sheet-freeze" primary>
      <FreezeItems info={info} />
    </RibbonMenu>
  )
}

function FreezeItems({ info }: { info: SheetInfo }) {
  const { run, t } = useKitEditor()
  const { row, colIndex } = cellParts(info.active)
  const above = row - 1
  const left = colIndex - 1
  const frozen = info.freeze[0] > 0 || info.freeze[1] > 0
  // The preview shown (rows, columns), so each change is asked of the engine once.
  const shownPreview = useRef<string | null>(null)
  const preview = (rows: number, cols: number) => {
    const key = rows || cols ? `${rows},${cols}` : null
    if (key === shownPreview.current) return
    shownPreview.current = key
    void run('editor.freezePreview', key ? { rows, cols } : {})
  }
  // The preview goes when the menu closes, whatever closed it.
  const runner = useRef(run)
  runner.current = run
  useEffect(
    () => () => {
      if (shownPreview.current) void runner.current('editor.freezePreview', {})
    },
    [],
  )
  // A choice ends the preview in the engine itself.
  const choose = (command: string, params: Record<string, unknown> = {}) => {
    shownPreview.current = null
    void run(command, params).finally(backToGrid)
  }
  const item = (label: string, rows: number, cols: number, command: string, params: Record<string, unknown>, testId: string) => (
    <div key={testId} onPointerEnter={() => preview(rows, cols)} onPointerLeave={() => shownPreview.current != null && preview(0, 0)} onFocus={() => preview(rows, cols)}>
      <MenuItem label={label} onSelect={() => choose(command, params)} testId={testId} />
    </div>
  )
  const cell = info.active
  return (
    <>
      {frozen ? (
        <>
          <MenuItem label={t('unfreezePanes')} onSelect={() => choose('view.unfreezePanes')} testId="sheet-freeze-off" />
          <MenuSeparator />
        </>
      ) : null}
      {above > 0 && left > 0 ? item(t('freezeAt', { row: above, col: columnLetters(left) }), above, left, 'view.freezePanes', { cell }, 'sheet-freeze-at') : null}
      {above > 0 ? item(t('freezeRowsAt', { row: above }), above, 0, 'view.freezeRows', { cell }, 'sheet-freeze-rows') : null}
      {left > 0 ? item(t('freezeColsAt', { col: columnLetters(left) }), 0, left, 'view.freezeColumns', { cell }, 'sheet-freeze-cols') : null}
      {item(t('freezeTopRow'), 1, 0, 'view.freezeTopRow', {}, 'sheet-freeze-top-row')}
      {item(t('freezeFirstColumn'), 0, 1, 'view.freezeFirstColumn', {}, 'sheet-freeze-first-column')}
    </>
  )
}

/** The highlight colours, two rows of six; the colour in use is ringed. Choosing one keeps the highlight on or off. */
function FocusPalette({ info, testId }: { info: SheetInfo; testId: string }) {
  const { run, t } = useKitEditor()
  return (
    <div className="sheet-focus-palette" role="group" aria-label={t('focusCellColors')} data-testid={testId}>
      {FOCUS_COLORS.map((color, index) => {
        const chosen = color.toUpperCase() === info.focusColor.toUpperCase()
        return (
          <button
            key={color}
            type="button"
            className="sheet-focus-swatch"
            style={{ background: color }}
            aria-pressed={chosen}
            data-selected={chosen || undefined}
            aria-label={t('focusCellColor', { index: index + 1 })}
            title={color}
            onClick={() => void run('view.focusCell', { color }).finally(backToGrid)}
            data-testid={`${testId}-${index}`}
          />
        )
      })}
    </div>
  )
}

/**
 * 高亮行列 as a split button: the upper part turns the highlight on and off (pressed while on), the
 * arrow below opens the colours. `place` is where it shows: the View tab or the status bar.
 */
export function FocusCellButton({ info, place }: { info: SheetInfo; place: 'ribbon' | 'status' }) {
  const { run, t } = useKitEditor()
  const ribbon = place === 'ribbon'
  const testId = ribbon ? 'sheet-focus' : 'sheet-status-focus'
  const toggle = () => void run('view.focusCell', { on: !info.focusCell }).finally(backToGrid)
  return (
    <span className={ribbon ? 'sheet-focus-split' : 'sheet-focus-status'} data-on={info.focusCell || undefined}>
      {ribbon ? (
        <RibbonButton icon={Crosshair} label={t('focusCell')} size="large" pressed={info.focusCell} onClick={toggle} data-testid={testId} primary />
      ) : (
        <button type="button" className="sheet-focus-icon" aria-pressed={info.focusCell} aria-label={t('focusCell')} title={t('focusCell')} onClick={toggle} data-testid={testId}>
          <Crosshair size={14} aria-hidden="true" />
        </button>
      )}
      <MenuButton
        placement={ribbon ? 'below' : 'auto'}
        trigger={({ open, toggle: flip }) => (
          <button type="button" className="sheet-focus-arrow" aria-expanded={open} aria-label={t('focusCellColors')} title={t('focusCellColors')} onClick={flip} data-testid={`${testId}-arrow`} data-open={open || undefined}>
            {open ? <ChevronUp size={11} aria-hidden="true" /> : <ChevronDown size={11} aria-hidden="true" />}
          </button>
        )}
      >
        <FocusPalette info={info} testId={`${testId}-colors`} />
      </MenuButton>
    </span>
  )
}

/** The sheet's gridline colour: automatic, or one of the colours an .xlsx keeps. */
export function GridlineColorMenu({ info }: { info: SheetInfo }) {
  const { run, enabled, t } = useKitEditor()
  const readOnly = useReadOnly()
  return (
    <RibbonMenu icon={Grid3x3} label={t('gridlineColor')} swatch={info.gridColor ?? undefined} disabled={readOnly || !enabled('view.gridlineColor')} testId="sheet-gridline-color">
      <MenuItem label={t('gridlineAuto')} checked={info.gridColor == null} onSelect={() => void run('view.gridlineColor', { color: 'auto' }).finally(backToGrid)} testId="sheet-gridline-color-auto" />
      <ColorGrid colors={GRIDLINE_COLORS} onPick={(color) => void run('view.gridlineColor', { color }).finally(backToGrid)} testId="sheet-gridline-colors" />
    </RibbonMenu>
  )
}
