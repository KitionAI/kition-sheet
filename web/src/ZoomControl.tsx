/**
 * The zoom of the grid in the status bar and in View › Zoom (plan stage 0A; walkthrough A8, A9,
 * A10, A11; WPS W-S2, W-R4): − and + go 10 points at a time to a multiple of 10, the slider covers
 * 10%–400%, and the percentage opens the zoom choices: 50% to 200%, a percentage typed in, and Zoom
 * to Selection. There is no Fit: a sheet has no page to fit, and fitting the whole sheet made it a
 * dot (walkthrough A8).
 */
import { useState } from 'react'
import { ZoomInIcon as ZoomIn } from '@kition/tessera/icons/react'
import { MenuButton, MenuItem, MenuSeparator, RibbonMenu, useKitEditor } from '@kition/sdk/editor'
import { MAX_ZOOM, MIN_ZOOM, clampZoom, stepZoom } from './zoom'
import { backToGrid } from './sheet'

/** The zoom presets the choices offer, in percent. */
const PRESETS = [50, 75, 100, 125, 150, 200]

export type ZoomActions = {
  /** The view's zoom now (1 is 100%). */
  zoom: number
  /** Zooms the view to `zoom` (clamped to 10%–400%). */
  zoomTo: (zoom: number) => void
  /** Zooms the view so the selection fills it. */
  toSelection: () => void
}

const toSlider = (zoom: number) => Math.log(zoom / MIN_ZOOM) / Math.log(MAX_ZOOM / MIN_ZOOM)
const fromSlider = (value: number) => MIN_ZOOM * Math.pow(MAX_ZOOM / MIN_ZOOM, value)

/** The choices: the presets, a percentage typed in, and Zoom to Selection. */
function ZoomChoices({ zoom, zoomTo, toSelection, testId }: ZoomActions & { testId: string }) {
  const { t } = useKitEditor()
  const percent = Math.round(zoom * 100)
  const [typed, setTyped] = useState(String(percent))
  const apply = () => {
    const value = Number(typed.replace('%', '').trim())
    if (Number.isFinite(value) && value > 0) zoomTo(value / 100)
    backToGrid()
  }
  return (
    <>
      {PRESETS.map((value) => (
        <MenuItem key={value} label={`${value}%`} checked={percent === value} onSelect={() => { zoomTo(value / 100); backToGrid() }} testId={`${testId}-${value}`} />
      ))}
      <MenuSeparator />
      <div className="sheet-zoom-custom">
        <span>{t('zoomCustom')}</span>
        <input
          className="kui-ribbon-field sheet-zoom-field"
          inputMode="numeric"
          aria-label={t('zoomCustom')}
          value={typed}
          onFocus={(event) => event.currentTarget.select()}
          onChange={(event) => setTyped(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== 'Escape') event.stopPropagation()
            if (event.key === 'Enter') {
              event.preventDefault()
              event.currentTarget.parentElement?.querySelector<HTMLButtonElement>('button')?.click()
            }
          }}
          data-testid={`${testId}-field`}
        />
        <span>%</span>
        <button type="button" className="kui-button" data-variant="ghost" onClick={apply} data-testid={`${testId}-apply`}>
          {t('zoomApply')}
        </button>
      </div>
      <MenuSeparator />
      <MenuItem label={t('zoomToSelection')} onSelect={() => { toSelection(); backToGrid() }} testId={`${testId}-selection`} />
    </>
  )
}

/** The status bar's zoom: −, the slider, +, and the percentage that opens the choices. */
export function ZoomControl(actions: ZoomActions) {
  const { t } = useKitEditor()
  const { zoom, zoomTo } = actions
  const shown = clampZoom(zoom)
  return (
    <div className="kui-zoom sheet-zoom" data-testid="sheet-zoom">
      <button type="button" className="kui-icon-button" aria-label={t('zoomOut')} title={t('zoomOut')} onClick={() => zoomTo(stepZoom(shown, -1))} data-testid="sheet-zoom-out">
        −
      </button>
      <input type="range" min={0} max={1} step={0.001} value={toSlider(shown)} onChange={(event) => zoomTo(fromSlider(Number(event.target.value)))} onPointerUp={backToGrid} aria-label={t('zoomMenu')} data-testid="sheet-zoom-slider" />
      <button type="button" className="kui-icon-button" aria-label={t('zoomIn')} title={t('zoomIn')} onClick={() => zoomTo(stepZoom(shown, 1))} data-testid="sheet-zoom-in">
        +
      </button>
      <MenuButton
        placement="auto"
        align="end"
        trigger={({ open, toggle }) => (
          <button type="button" className="kui-zoom-value sheet-zoom-value" aria-expanded={open} aria-haspopup="menu" title={t('zoomMenu')} onClick={toggle} data-testid="kit-zoom">
            {Math.round(zoom * 100)}%
          </button>
        )}
      >
        <ZoomChoices {...actions} testId="sheet-zoom-menu" />
      </MenuButton>
    </div>
  )
}

/** View › Zoom: the same choices as the status bar's percentage. */
export function ZoomMenu(actions: ZoomActions & { primary?: boolean }) {
  const { t } = useKitEditor()
  return (
    <RibbonMenu icon={ZoomIn} label={t('zoomMenu')} size="large" testId="sheet-view-zoom" primary>
      <ZoomChoices {...actions} testId="sheet-view-zoom-menu" />
    </RibbonMenu>
  )
}
