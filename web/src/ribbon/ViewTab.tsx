import { CrosshairIcon as Crosshair, EyeIcon as Eye, ScanIcon as Scan, SnowflakeIcon as Snowflake, ZoomInIcon as ZoomIn } from '@kition/tessera/icons/react'
import { Checkbox, CommandTab, RibbonButton, RibbonGroup, RibbonItem, RibbonStack, useKitEditor } from '@kition/sdk/editor'
import { SHEET_PRIMARY, type SheetInfo } from '../sheet'
import { usePendingToggle } from '../pendingToggle'
import { FocusCellButton, FreezeMenu, GridlineColorMenu } from '../ViewControls'
import { ZoomMenu, type ZoomActions } from '../ZoomControl'
import { backToGrid } from '../sheet'

/** The freeze commands the View tab's own Freeze Panes menu runs (WPS's choices). */
const FREEZE_COMMANDS = ['view.freezePanes', 'view.freezeRows', 'view.freezeColumns', 'view.freezeTopRow', 'view.freezeFirstColumn', 'view.unfreezePanes']
/** The zoom commands the View tab's own Zoom group replaces (the view's zoom is the person's). */
const ZOOM_COMMANDS = ['view.zoom', 'view.zoomToSelection']

/**
 * The View tab: its Show group as WPS has it, check boxes top to bottom for 显示网格线 (the
 * sheet's gridlines), 显示编辑栏 (the formula bar, the person's) and 显示行号列标 (the headings);
 * then 高亮行列 (the person's highlight of the selection's rows and columns) and Freeze Panes as
 * WPS has them (Freeze Panes in Single line's row itself, walkthrough V2); Zoom (the status bar's
 * choices), 100% and Zoom to Selection on the view; the engine's other View groups after it. Each box shows what the person last
 * asked for until the engine answers (`usePendingToggle`), so quick clicks each toggle.
 */
export function ViewTab({ info, formulaBar, onFormulaBar, zoom }: { info: SheetInfo; formulaBar: boolean; onFormulaBar: () => void; zoom: ZoomActions }) {
  const { run, enabled, t } = useKitEditor()
  const [gridlines, toggleGridlines] = usePendingToggle('view.gridlines', info.gridlines, (on) => run('view.gridlines', { on }))
  const [headings, toggleHeadings] = usePendingToggle('view.headings', info.headings, (on) => run('view.headings', { on }))
  return (
    <>
      <RibbonGroup id="sheet:show" label={t('showGroup')} icon={Eye}>
        <RibbonStack>
          <RibbonItem primary>
            <span className="sheet-show-option">
              <Checkbox label={t('showGridlines')} checked={gridlines} disabled={!enabled('view.gridlines')} onChange={() => void toggleGridlines()} testId="sheet-show-gridlines" />
            </span>
          </RibbonItem>
          <RibbonItem primary>
            <span className="sheet-show-option">
              <Checkbox label={t('showFormulaBar')} checked={formulaBar} onChange={onFormulaBar} testId="sheet-show-formula-bar" />
            </span>
          </RibbonItem>
          <RibbonItem>
            <span className="sheet-show-option">
              <Checkbox label={t('showHeadings')} checked={headings} disabled={!enabled('view.headings')} onChange={() => void toggleHeadings()} testId="sheet-show-headings" />
            </span>
          </RibbonItem>
        </RibbonStack>
        <GridlineColorMenu info={info} />
      </RibbonGroup>
      <RibbonGroup id="sheet:focus" label={t('focusCell')} icon={Crosshair}>
        <FocusCellButton info={info} place="ribbon" />
      </RibbonGroup>
      <RibbonGroup id="sheet:freeze" label={t('freezePanes')} icon={Snowflake}>
        <FreezeMenu info={info} primary />
      </RibbonGroup>
      <RibbonGroup id="sheet:zoom" label={t('zoomMenu')} icon={ZoomIn}>
        <ZoomMenu {...zoom} primary />
        <RibbonButton icon={ZoomIn} label="100%" size="large" onClick={() => { zoom.zoomTo(1); backToGrid() }} data-testid="sheet-view-zoom-100" />
        <RibbonButton icon={Scan} label={t('zoomToSelection')} size="large" onClick={() => { zoom.toSelection(); backToGrid() }} data-testid="sheet-view-zoom-selection" />
      </RibbonGroup>
      <CommandTab menu="View" omit={['Show']} exclude={[...FREEZE_COMMANDS, ...ZOOM_COMMANDS]} primary={SHEET_PRIMARY.View} />
    </>
  )
}
