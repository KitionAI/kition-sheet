import { useContext, useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { ChevronDownIcon as ChevronDown, PaintbrushIcon as Paintbrush } from '@kition/tessera/icons/react'
import { ColorGrid, MenuButton, RibbonButton, RibbonPresentationContext, isMac, readKitStore, toast, useKitEditor, useReadOnly, writeKitStore, type IconComponent } from '@kition/sdk/editor'
import { FONTS, SIZES, backToGrid, fontMatches, type FontChoice, type SheetInfo } from './sheet'

/** How many fonts the Font list keeps under Recently Used. */
const RECENT_FONTS = 6
/** How many colours a colour menu keeps under Recent Colors. */
const RECENT_COLORS = 10

const storedList = (key: string) => readKitStore<string[]>('sheet', key, (raw) => (Array.isArray(raw) && raw.every((item) => typeof item === 'string') ? (raw as string[]) : undefined)) ?? []
const remember = (key: string, value: string, most: number) => {
  const list = [value, ...storedList(key).filter((item) => item !== value)].slice(0, most)
  writeKitStore('sheet', key, list)
  return list
}

/** The open stand-ins the engine draws Office's fonts in, so the Font list can show them too. */
const STAND_INS: Array<[string, string]> = [
  ['Carlito', 'Carlito-Regular.ttf'],
  ['Caladea', 'Caladea-Regular.ttf'],
  ['Arimo', 'Arimo-Variable.ttf'],
  ['Tinos', 'Tinos-Regular.ttf'],
  ['Cousine', 'Cousine-Regular.ttf'],
  ['Ma Shan Zheng', 'MaShanZheng-Regular.ttf'],
]
let standInsLoaded = false
/** Loads the stand-ins once, the first time the Font list opens (a face the build lacks is skipped). */
function loadStandIns() {
  if (standInsLoaded || typeof FontFace === 'undefined') return
  standInsLoaded = true
  for (const [family, file] of STAND_INS) {
    const face = new FontFace(family, `url(${new URL(`./fonts/${file}`, document.baseURI).href})`)
    face
      .load()
      .then((loaded) => document.fonts.add(loaded))
      .catch(() => undefined)
  }
}

/**
 * Home › Clipboard › Format Painter, as WPS has it: a click picks up the selection's formats for
 * one paint, a double-click keeps the painter up until it is clicked again or Esc is pressed. It
 * shows pressed while the painter is up, and is unavailable while a cell is typed.
 */
export function FormatPainterButton({ info }: { info: SheetInfo; primary?: boolean }) {
  const { run, enabled, t } = useKitEditor()
  const readOnly = useReadOnly()
  const disabled = readOnly || info.editing || !enabled('edit.formatPainter')
  return (
    <RibbonButton
      icon={Paintbrush}
      label={t('formatPainter')}
      title={t('formatPainterTip')}
      size="large"
      pressed={info.formatPainter != null}
      disabled={disabled}
      onClick={(event) => {
        // The second click of a double-click is the double-click's.
        if (event.detail > 1) return
        void run('edit.formatPainter', {}).finally(backToGrid)
      }}
      onDoubleClick={() => void run('edit.formatPainter', { sticky: true }).finally(backToGrid)}
      data-testid="sheet-format-painter"
      data-sticky={info.formatPainter === 'sticky' || undefined}
    />
  )
}

/** A font's row in the Font list: shown in its own face, checked when it is the cell's. */
function FontRow({ font, current, onPick, testId }: { font: FontChoice; current: string; onPick: (name: string) => void; testId: string }) {
  const checked = font.name === current
  return (
    <button
      type="button"
      role="menuitemcheckbox"
      aria-checked={checked}
      className="kui-menu-item sheet-font-row"
      style={{ fontFamily: font.css }}
      title={font.also[0] && /[^\x00-\x7f]/.test(font.name) ? `${font.name} (${font.also[0]})` : font.name}
      onClick={() => onPick(font.name)}
      data-testid={`${testId}-${font.name}`}
    >
      <span className="sheet-font-check" aria-hidden="true">
        {checked ? '✓' : ''}
      </span>
      <span>{font.name}</span>
    </button>
  )
}

/**
 * Home › Font › Font: a box showing the active cell's font (the selected characters' while a cell
 * is typed; empty when they differ) that takes a typed name, and a list of fonts, each shown in its
 * own face: a search field (a name, its initials or its pinyin), Recently Used (latest first), and
 * All Fonts; the cell's font is checked.
 */
export function FontNamePicker({ info }: { info: SheetInfo }) {
  const { run, enabled, t } = useKitEditor()
  const readOnly = useReadOnly()
  const current = info.style.font?.name ?? ''
  const [typed, setTyped] = useState<string | null>(null)
  // Narrower in Single line's row, so Home fits there.
  const inline = useContext(RibbonPresentationContext) === 'primary'
  const disabled = readOnly || !enabled('format.fontName')
  const apply = (name: string) => {
    const clean = name.trim()
    setTyped(null)
    if (!clean) return
    const known = FONTS.find((font) => [font.name, ...font.also].some((other) => other.toLowerCase() === clean.toLowerCase()))
    const chosen = known?.name ?? clean
    remember('recentFonts', chosen, RECENT_FONTS)
    void run('format.fontName', { name: chosen }).finally(backToGrid)
  }
  const keyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    event.stopPropagation()
    if (event.key === 'Enter') {
      event.preventDefault()
      apply(event.currentTarget.value)
    } else if (event.key === 'Escape') {
      event.preventDefault()
      setTyped(null)
      backToGrid()
    }
  }
  return (
    <span className="sheet-combo" data-primary="true">
      <input
        className="kui-ribbon-field sheet-combo-field"
        style={{ width: inline ? 76 : 118 }}
        value={typed ?? current}
        placeholder={t('fontBox')}
        aria-label={t('fontBox')}
        disabled={disabled}
        onChange={(event) => setTyped(event.target.value)}
        onFocus={(event) => event.currentTarget.select()}
        onBlur={() => setTyped(null)}
        onKeyDown={keyDown}
        data-testid="sheet-font"
      />
      <MenuButton
        trigger={({ toggle, open }) => (
          <button type="button" className="kui-ribbon-button sheet-combo-arrow" data-primary="true" aria-label={t('fontMore')} aria-expanded={open} aria-pressed={open} disabled={disabled} onClick={toggle} data-testid="sheet-font-arrow">
            <ChevronDown size={12} aria-hidden="true" />
          </button>
        )}
      >
        <FontPanel current={current} onPick={apply} />
      </MenuButton>
    </span>
  )
}

function FontPanel({ current, onPick }: { current: string; onPick: (name: string) => void }) {
  const { t } = useKitEditor()
  const [query, setQuery] = useState('')
  const recent = useMemo(() => storedList('recentFonts').map((name) => FONTS.find((font) => font.name === name) ?? { name, also: [], css: `"${name}"` }), [])
  const list = useRef<HTMLDivElement>(null)
  useEffect(loadStandIns, [])
  // The cell's font is brought into view as the list opens (in Recently Used when it is there).
  useEffect(() => {
    const row = list.current?.querySelector<HTMLElement>('[aria-checked="true"]')
    row?.scrollIntoView({ block: 'nearest' })
  }, [])
  const found = query.trim() ? FONTS.filter((font) => fontMatches(font, query)) : null
  return (
    <div className="sheet-font-panel" data-testid="sheet-font-panel">
      <div className="sheet-font-search">
        <input
          className="kui-ribbon-field"
          type="search"
          value={query}
          placeholder={t('fontSearch')}
          aria-label={t('fontSearch')}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== 'Escape' && event.key !== 'ArrowDown') event.stopPropagation()
            if (event.key === 'Enter' && found?.length) {
              event.preventDefault()
              onPick(found[0].name)
            }
          }}
          data-testid="sheet-font-search"
        />
      </div>
      <div className="sheet-font-list" ref={list} data-testid="sheet-font-list">
        {found ? (
          <>
            <div className="sheet-font-heading" data-testid="sheet-font-results">
              {t('fontResults', { count: found.length })}
            </div>
            {found.length ? found.map((font) => <FontRow key={font.name} font={font} current={current} onPick={onPick} testId="sheet-font-result" />) : <div className="sheet-font-empty" data-testid="sheet-font-empty">{t('fontNoResults')}</div>}
          </>
        ) : (
          <>
            {recent.length ? (
              <div data-testid="sheet-font-recent">
                <div className="sheet-font-heading">{t('fontRecent')}</div>
                {recent.map((font) => (
                  <FontRow key={font.name} font={font} current={current} onPick={onPick} testId="sheet-font-recent" />
                ))}
              </div>
            ) : null}
            <div data-testid="sheet-font-all">
              <div className="sheet-font-heading">{t('fontAll')}</div>
              {FONTS.map((font) => (
                <FontRow key={font.name} font={font} current={current} onPick={onPick} testId="sheet-font-item" />
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  )
}

/**
 * Home › Font › Font Size: a box showing the active cell's size that takes a typed size (1–409
 * points; anything else is refused with a message and the box goes back), and WPS's list of
 * sizes, the cell's checked.
 */
export function FontSizePicker({ info }: { info: SheetInfo }) {
  const { run, enabled, t } = useKitEditor()
  const readOnly = useReadOnly()
  const size = info.style.font?.size ?? 11
  const [typed, setTyped] = useState<string | null>(null)
  const inline = useContext(RibbonPresentationContext) === 'primary'
  const disabled = readOnly || !enabled('format.fontSize')
  const apply = (value: string) => {
    setTyped(null)
    const number = Number(value.trim())
    if (!value.trim() || !Number.isFinite(number) || number < 1 || number > 409) {
      toast('error', t('fontSizeRange'))
      backToGrid()
      return
    }
    void run('format.fontSize', { size: number }).finally(backToGrid)
  }
  return (
    <span className="sheet-combo" data-primary="true">
      <input
        className="kui-ribbon-field sheet-combo-field"
        style={{ width: inline ? 24 : 36 }}
        value={typed ?? String(size)}
        aria-label={t('fontSizeBox')}
        inputMode="decimal"
        disabled={disabled}
        onChange={(event) => setTyped(event.target.value)}
        onFocus={(event) => event.currentTarget.select()}
        onBlur={() => setTyped(null)}
        onKeyDown={(event) => {
          event.stopPropagation()
          if (event.key === 'Enter') {
            event.preventDefault()
            apply(event.currentTarget.value)
          } else if (event.key === 'Escape') {
            event.preventDefault()
            setTyped(null)
            backToGrid()
          }
        }}
        data-testid="sheet-size"
      />
      <MenuButton
        trigger={({ toggle, open }) => (
          <button type="button" className="kui-ribbon-button sheet-combo-arrow" data-primary="true" aria-label={t('fontSizeList')} aria-expanded={open} aria-pressed={open} disabled={disabled} onClick={toggle} data-testid="sheet-size-arrow">
            <ChevronDown size={12} aria-hidden="true" />
          </button>
        )}
      >
        <div className="sheet-size-list" data-testid="sheet-size-list">
          {SIZES.map((value) => (
            <button
              key={value}
              type="button"
              role="menuitemcheckbox"
              aria-checked={value === size}
              className="kui-menu-item sheet-font-row"
              onClick={() => void run('format.fontSize', { size: value }).finally(backToGrid)}
              data-testid={`sheet-size-item-${value}`}
            >
              <span className="sheet-font-check" aria-hidden="true">
                {value === size ? '✓' : ''}
              </span>
              <span>{value}</span>
            </button>
          ))}
        </div>
      </MenuButton>
    </span>
  )
}

/** The shortcut key's name in tooltips: ⌘ on a Mac, Ctrl+ elsewhere. */
export const shortcutKey = () => (isMac() ? '⌘' : 'Ctrl+')

/**
 * A colour split button (Font Color, Fill Color), as WPS has them: the button applies the colour
 * last chosen (shown under its icon, remembered for the next time), the arrow opens the colours —
 * Automatic (or No Fill), the palette, Recent Colors, and More Colors (any `#RRGGBB`).
 */
export function ColorSplit({ command, none, noneLabel, icon, label, store, initial, testId }: { primary?: boolean; command: string; none: string; noneLabel: string; icon: IconComponent; label: string; store: string; initial: string; testId: string }) {
  const { run, enabled, t } = useKitEditor()
  const readOnly = useReadOnly()
  const [color, setColor] = useState(() => readKitStore<string>('sheet', store, (raw) => (typeof raw === 'string' && /^#[0-9A-F]{6}$/i.test(raw) ? raw : undefined)) ?? initial)
  const [recent, setRecent] = useState(() => storedList(`${store}Recent`))
  const [hex, setHex] = useState('')
  const disabled = readOnly || !enabled(command)
  const apply = (value: string) => {
    void run(command, { color: value }).finally(backToGrid)
  }
  const choose = (value: string) => {
    const upper = value.toUpperCase()
    setColor(upper)
    writeKitStore('sheet', store, upper)
    setRecent(remember(`${store}Recent`, upper, RECENT_COLORS))
    apply(upper)
  }
  return (
    <span className="sheet-split" data-primary="true">
      <RibbonButton icon={icon} label={label} size="icon" swatch={color} primary disabled={disabled} onClick={() => apply(color)} data-testid={`${testId}-apply`} data-color={color} />
      <MenuButton
        trigger={({ toggle, open }) => (
          <button type="button" className="kui-ribbon-button sheet-split-arrow" data-primary="true" aria-label={`${label} — ${t('colorMenu')}`} aria-expanded={open} aria-pressed={open} disabled={disabled} onClick={toggle} data-testid={testId}>
            <ChevronDown size={11} aria-hidden="true" />
          </button>
        )}
      >
        <button type="button" className="kui-menu-item" role="menuitem" onClick={() => void run(command, { color: none }).finally(backToGrid)} data-testid={`${testId}-none`}>
          <span style={{ width: 15 }} />
          <span>{noneLabel}</span>
        </button>
        <ColorGrid onPick={choose} testId={`${testId}-colors`} />
        {recent.length ? (
          <>
            <div className="sheet-font-heading">{t('colorRecent')}</div>
            <ColorGrid colors={recent} onPick={choose} testId={`${testId}-recent`} />
          </>
        ) : null}
        <div className="sheet-font-heading">{t('colorMore')}</div>
        <div className="sheet-color-more">
          <input
            type="color"
            aria-label={t('colorMore')}
            value={/^#[0-9A-F]{6}$/i.test(hex) ? hex : color}
            onChange={(event) => setHex(event.target.value.toUpperCase())}
            data-testid={`${testId}-picker`}
          />
          <input
            className="kui-ribbon-field"
            style={{ width: 84 }}
            value={hex}
            placeholder="#3C5593"
            aria-label={t('colorHex')}
            onChange={(event) => setHex(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== 'Escape') event.stopPropagation()
              if (event.key === 'Enter') {
                event.preventDefault()
                event.currentTarget.parentElement?.querySelector<HTMLButtonElement>('button')?.click()
              }
            }}
            data-testid={`${testId}-hex`}
          />
          <button
            type="button"
            className="kui-ribbon-button"
            onClick={() => {
              const value = hex.trim().startsWith('#') ? hex.trim() : `#${hex.trim()}`
              if (!/^#[0-9A-F]{6}$/i.test(value)) {
                toast('error', t('colorBad'))
                return
              }
              setHex('')
              choose(value)
            }}
            data-testid={`${testId}-hex-ok`}
          >
            {t('colorApply')}
          </button>
        </div>
      </MenuButton>
    </span>
  )
}
