import { useEffect, useState, type CSSProperties, type MouseEvent as ReactMouseEvent } from 'react'
import { Button, ColorGrid, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, ModalTitle, RibbonGroup, RibbonMenu, RibbonStack, toast, useEngineQuery, useKitEditor } from '@kition/sdk/editor'
import { CheckIcon as Check, Table2Icon as Table2, TablePropertiesIcon as TableProperties, XIcon as X } from '@kition/tessera/icons/react'
import { backToGrid, cellParts, columnLetters, type SheetInfo } from './sheet'
import { askConfirm } from './SheetDialogs'

/** A thumbnail cell: fill, text, then the top, bottom, left and right lines (`#RRGGBB` or null). */
type PreviewCell = [string | null, string | null, string | null, string | null, string | null, string | null]
type Preview = PreviewCell[][]
/** A table part's format as `table.newStyle` takes it. */
type Part = { fill?: string; color?: string; bold?: boolean; italic?: boolean; border?: string; lines?: string; size?: number }
type Parts = Record<string, Part>
/** The gallery, as `table.styles` answers it. */
type Gallery = {
  slot: number
  colors: string[]
  groups: Array<{ id: string; styles: Array<{ name: string; preview: Preview }> }>
  custom: Array<{ name: string; preview: Preview; elements: Parts }>
  default: string
  current: string | null
  table: string | null
  range: string | null
}

/** A style's thumbnail: a header and four banded rows, five columns, drawn from its preview. */
export function Thumbnail({ preview }: { preview: Preview }) {
  return (
    <span className="sheet-table-thumb" aria-hidden="true">
      {preview.flatMap((row, r) =>
        row.map(([fill, text, top, bottom, left, right], c) => {
          const style: CSSProperties = {
            background: fill ?? '#FFFFFF',
            borderTopColor: top ?? 'transparent',
            borderBottomColor: bottom ?? 'transparent',
            borderLeftColor: left ?? 'transparent',
            borderRightColor: right ?? 'transparent',
          }
          return (
            <span key={`${r}-${c}`} style={style}>
              <i style={{ background: text ?? '#404040' }} />
            </span>
          )
        }),
      )}
    </span>
  )
}

// ———— dialogs opened from the gallery and the Table Design tab ————

type Dialog =
  | { kind: 'format'; style: string; range: string }
  | { kind: 'new'; replace?: string; name: string; parts: Parts; isDefault: boolean }
  | { kind: 'resize'; range: string }
  | null
let current: Dialog = null
const listeners = new Set<(dialog: Dialog) => void>()
const show = (dialog: Dialog) => {
  current = dialog
  for (const listener of listeners) listener(dialog)
}
/** Format as Table's dialog (WPS's 套用表格样式): the range, and only the look or a table. */
export const openFormatAsTable = (style: string, range: string) => show({ kind: 'format', style, range })
/** New Table Style (or Modify, with the style's name and parts). */
export const openNewTableStyle = (from?: { name: string; parts: Parts; isDefault: boolean }) =>
  show(from ? { kind: 'new', replace: from.name, name: from.name, parts: from.parts, isDefault: from.isDefault } : { kind: 'new', name: '', parts: {}, isDefault: false })
/** Resize Table: the table's new range. */
export const openResizeTable = (range: string) => show({ kind: 'resize', range })

/** `A1:D10` as WPS shows a dialog's range (`=$A$1:$D$10`), and back. */
const absolute = (range: string) => `=${range.replace(/([A-Z]+)(\d+)/g, '$$$1$$$2')}`
const plain = (text: string) => text.trim().replace(/^=/, '').replace(/\$/g, '')

/** The table parts New Table Style lists, in WPS's order. */
const PARTS = ['wholeTable', 'headerRow', 'totalRow', 'firstColumn', 'lastColumn', 'firstRowStripe', 'secondRowStripe', 'firstColumnStripe', 'secondColumnStripe']

/**
 * A thumbnail of a style from its parts, before it is saved (New Table Style's preview): each
 * cell's fill and text colour, the parts laid in the engine's order.
 */
function partsPreview(parts: Parts): Preview {
  const order = ['wholeTable', 'firstColumnStripe', 'secondColumnStripe', 'firstRowStripe', 'secondRowStripe', 'headerRow']
  return Array.from({ length: 5 }, (_, r) =>
    Array.from({ length: 5 }, (_, c) => {
      let fill: string | null = null
      let text: string | null = null
      let border: string | null = null
      for (const kind of order) {
        const part = parts[kind]
        if (!part) continue
        const applies =
          kind === 'wholeTable' ||
          (kind === 'headerRow' && r === 0) ||
          (r > 0 && kind === 'firstRowStripe' && (r - 1) % 2 === 0) ||
          (r > 0 && kind === 'secondRowStripe' && (r - 1) % 2 === 1)
        if (!applies) continue
        fill = part.fill ?? fill
        text = part.color ?? text
        border = part.border ?? border
      }
      return [fill, text, r === 0 ? border : null, r === 4 ? border : null, c === 0 ? border : null, c === 4 ? border : null] as PreviewCell
    }),
  )
}

/** A colour choice in a dialog: No Color, the palette, and More Colors (any `#RRGGBB`). */
function ColorField({ value, onChange, testId, label }: { value: string | undefined; onChange: (color: string | undefined) => void; testId: string; label: string }) {
  const { t } = useKitEditor()
  const [open, setOpen] = useState(false)
  const [hex, setHex] = useState('')
  return (
    <span className="sheet-color-field">
      <button type="button" className="sheet-color-value" aria-label={label} aria-expanded={open} onClick={() => setOpen((was) => !was)} data-testid={testId} data-color={value ?? ''}>
        <span style={{ background: value ?? 'transparent' }} className={value ? undefined : 'sheet-color-empty'} />
        {value ?? t('tableNoColor')}
      </button>
      {open ? (
        <span className="sheet-color-pop" data-testid={`${testId}-panel`}>
          <button
            type="button"
            className="kui-menu-item"
            onClick={() => {
              onChange(undefined)
              setOpen(false)
            }}
            data-testid={`${testId}-none`}
          >
            {t('tableNoColor')}
          </button>
          <ColorGrid
            onPick={(color) => {
              onChange(color.toUpperCase())
              setOpen(false)
            }}
            testId={`${testId}-colors`}
          />
          <span className="sheet-font-heading">{t('colorMore')}</span>
          <span className="sheet-color-more">
            <input type="color" aria-label={t('colorMore')} value={/^#[0-9A-F]{6}$/i.test(hex) ? hex : (value ?? '#000000')} onChange={(event) => setHex(event.target.value.toUpperCase())} />
            <input className="kui-field" style={{ width: 84 }} value={hex} placeholder="#3C5593" aria-label={t('colorHex')} onChange={(event) => setHex(event.target.value)} data-testid={`${testId}-hex`} />
            <button
              type="button"
              className="kui-ribbon-button"
              onClick={() => {
                const text = hex.trim().startsWith('#') ? hex.trim() : `#${hex.trim()}`
                if (!/^#[0-9A-F]{6}$/i.test(text)) {
                  toast('error', t('colorBad'))
                  return
                }
                onChange(text.toUpperCase())
                setHex('')
                setOpen(false)
              }}
              data-testid={`${testId}-hex-ok`}
            >
              {t('colorApply')}
            </button>
          </span>
        </span>
      ) : null}
    </span>
  )
}

/**
 * The gallery's dialogs: Format as Table (WPS's 套用表格样式: where the data is, Only Format As
 * Table with its title rows, or Convert to a table with headers and filter buttons), New Table
 * Style (a name, each part's fill, text, bold, italic, border and inside lines, a preview, Set as
 * default) and Resize Table.
 */
export function TableStyleDialogs({ info }: { info: SheetInfo }) {
  const { run, query, t } = useKitEditor()
  const [dialog, setDialog] = useState<Dialog>(current)
  useEffect(() => {
    listeners.add(setDialog)
    return () => {
      listeners.delete(setDialog)
    }
  }, [])
  // Format as Table's choices.
  const [range, setRange] = useState('')
  const [mode, setMode] = useState<'format' | 'table'>('format')
  const [titleRows, setTitleRows] = useState(1)
  const [header, setHeader] = useState(true)
  const [filter, setFilter] = useState(true)
  // New Table Style's.
  const [name, setName] = useState('')
  const [parts, setParts] = useState<Parts>({})
  const [part, setPart] = useState('wholeTable')
  const [isDefault, setIsDefault] = useState(false)
  useEffect(() => {
    if (dialog?.kind === 'format' || dialog?.kind === 'resize') setRange(absolute(dialog.range))
    if (dialog?.kind === 'format') {
      setMode('format')
      setTitleRows(1)
      setHeader(true)
      setFilter(true)
    }
    if (dialog?.kind === 'new') {
      setName(dialog.name)
      setParts(dialog.parts)
      setPart('wholeTable')
      setIsDefault(dialog.isDefault)
    }
  }, [dialog])
  // Whether a picture sits in the first row of `range` (A1:D10; at most 256 columns looked at).
  const headerHasPictures = async (range: string) => {
    const match = /^(?:.*!)?([A-Z]+\d+)(?::([A-Z]+\d+))?$/.exec(range)
    if (!match) return false
    const start = cellParts(match[1])
    const end = cellParts(match[2] ?? match[1])
    for (let col = start.colIndex; col <= Math.min(end.colIndex, start.colIndex + 255); col += 1) {
      const cell = await query<{ picture?: unknown }>('cell.get', { cell: `${columnLetters(col)}${start.row}` })
      if (cell?.picture) return true
    }
    return false
  }
  if (!dialog) return null
  const close = () => {
    show(null)
    backToGrid()
  }
  const refocus = (event: Event) => event.preventDefault()

  if (dialog.kind === 'format') {
    const ok = async () => {
      const params =
        mode === 'format'
          ? { range: plain(range), style: dialog.style, mode, titleRows }
          : { range: plain(range), style: dialog.style, mode, header, filter, columnPrefix: t('tableColumnPrefix') }
      close()
      // A header row can't hold pictures: WPS asks, then clears them (the header is named 列1…).
      if (mode === 'table' && header && (await headerHasPictures(plain(range)))) {
        askConfirm({ title: t('tablePicturesTitle'), message: t('tablePicturesConfirm'), testId: 'sheet-table-pictures', onOk: () => void run('home.formatAsTable', { ...params, clearPictures: true }) })
        return
      }
      await run('home.formatAsTable', params)
    }
    return (
      <Modal open onOpenChange={(open) => (open ? undefined : close())}>
        <ModalContent size="sm" dismissible={false} aria-describedby={undefined} onCloseAutoFocus={refocus} data-testid="sheet-fat">
          <ModalHeader>
            <ModalTitle>{t('formatAsTableTitle')}</ModalTitle>
          </ModalHeader>
          <ModalBody>
            <div className="sheet-dialog-form">
              <label className="sheet-dialog-row">
                <span>{t('tableDataSource')}</span>
                <input className="kui-field" value={range} spellCheck={false} onChange={(event) => setRange(event.target.value)} data-testid="sheet-fat-range" />
              </label>
              <label className="sheet-dialog-choice">
                <input type="radio" name="sheet-fat-mode" checked={mode === 'format'} onChange={() => setMode('format')} data-testid="sheet-fat-format" />
                {t('tableOnlyFormat')}
              </label>
              <label className="sheet-dialog-row sheet-dialog-indent">
                <span>{t('tableTitleRows')}</span>
                <input
                  className="kui-field"
                  type="number"
                  min={0}
                  max={9}
                  style={{ width: 56 }}
                  disabled={mode !== 'format'}
                  value={titleRows}
                  onChange={(event) => setTitleRows(Math.max(0, Math.min(9, Math.round(Number(event.target.value) || 0))))}
                  data-testid="sheet-fat-title-rows"
                />
              </label>
              <label className="sheet-dialog-choice">
                <input type="radio" name="sheet-fat-mode" checked={mode === 'table'} onChange={() => setMode('table')} data-testid="sheet-fat-table" />
                {t('tableConvert')}
              </label>
              <label className="sheet-dialog-choice sheet-dialog-indent">
                <input type="checkbox" disabled={mode !== 'table'} checked={header} onChange={(event) => setHeader(event.target.checked)} data-testid="sheet-fat-header" />
                {t('tableHasHeaders')}
              </label>
              <label className="sheet-dialog-choice sheet-dialog-indent">
                <input type="checkbox" disabled={mode !== 'table'} checked={filter} onChange={(event) => setFilter(event.target.checked)} data-testid="sheet-fat-filter" />
                {t('tableFilterButtons')}
              </label>
            </div>
          </ModalBody>
          <ModalFooter className="sheet-dialog-actions">
            <Button variant="secondary" onClick={close} data-testid="sheet-fat-cancel">
              {t('cancel')}
            </Button>
            <Button variant="primary" onClick={() => void ok()} data-testid="sheet-fat-ok">
              {t('ok')}
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    )
  }

  if (dialog.kind === 'resize') {
    const ok = async () => {
      const done = await run('table.resize', { range: plain(range) })
      // A range the table can't take says why and leaves the table as it was.
      if (done != null) close()
      else if (info.tableInfo) setRange(absolute(info.tableInfo.range))
    }
    return (
      <Modal open onOpenChange={(open) => (open ? undefined : close())}>
        <ModalContent size="sm" dismissible={false} aria-describedby={undefined} onCloseAutoFocus={refocus} data-testid="sheet-resize">
          <ModalHeader>
            <ModalTitle>{t('resizeTableTitle')}</ModalTitle>
          </ModalHeader>
          <ModalBody>
            <label className="sheet-dialog-row">
              <span>{t('resizeTableRange')}</span>
              <input
                className="kui-field"
                value={range}
                autoFocus
                spellCheck={false}
                onChange={(event) => setRange(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') {
                    event.preventDefault()
                    void ok()
                  }
                }}
                data-testid="sheet-resize-range"
              />
            </label>
          </ModalBody>
          <ModalFooter className="sheet-dialog-actions">
            <Button variant="secondary" onClick={close} data-testid="sheet-resize-cancel">
              {t('cancel')}
            </Button>
            <Button variant="primary" onClick={() => void ok()} data-testid="sheet-resize-ok">
              {t('ok')}
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    )
  }

  const value = parts[part] ?? {}
  const set = (change: Partial<Part>) =>
    setParts((was) => {
      const next: Part = { ...(was[part] ?? {}), ...change }
      for (const key of Object.keys(next) as Array<keyof Part>) if (next[key] === undefined || next[key] === false) delete next[key]
      const all = { ...was, [part]: next }
      if (!Object.keys(next).length) delete all[part]
      return all
    })
  const ok = async () => {
    if (!name.trim()) {
      toast('error', t('tableStyleNameEmpty'))
      return
    }
    const done = await run('table.newStyle', { name: name.trim(), elements: parts, default: isDefault, ...(dialog.replace ? { replace: dialog.replace } : {}) })
    if (done != null) close()
  }
  return (
    <Modal open onOpenChange={(open) => (open ? undefined : close())}>
      <ModalContent size="lg" dismissible={false} aria-describedby={undefined} onCloseAutoFocus={refocus} data-testid="sheet-nts">
        <ModalHeader>
          <ModalTitle>{t(dialog.replace ? 'modifyTableStyleTitle' : 'newTableStyleTitle')}</ModalTitle>
        </ModalHeader>
        <ModalBody>
          <div className="sheet-dialog-form">
            <label className="sheet-dialog-row">
              <span>{t('tableStyleName')}</span>
              <input className="kui-field" value={name} autoFocus spellCheck={false} onChange={(event) => setName(event.target.value)} data-testid="sheet-nts-name" />
            </label>
            <div className="sheet-nts-body">
              <div>
                <div className="sheet-font-heading">{t('tableElement')}</div>
                <ul className="sheet-function-list sheet-nts-parts" role="listbox" aria-label={t('tableElement')}>
                  {PARTS.map((kind) => (
                    <li key={kind} role="option" aria-selected={kind === part} data-set={parts[kind] ? 'true' : undefined} onClick={() => setPart(kind)} data-testid={`sheet-nts-el-${kind}`}>
                      {t(`tablePart:${kind}`)}
                    </li>
                  ))}
                </ul>
              </div>
              <div className="sheet-nts-format">
                <div className="sheet-dialog-row">
                  <span>{t('tablePartFill')}</span>
                  <ColorField value={value.fill} onChange={(fill) => set({ fill })} label={t('tablePartFill')} testId="sheet-nts-fill" />
                </div>
                <div className="sheet-dialog-row">
                  <span>{t('tablePartText')}</span>
                  <ColorField value={value.color} onChange={(color) => set({ color })} label={t('tablePartText')} testId="sheet-nts-color" />
                </div>
                <div className="sheet-dialog-row">
                  <span>{t('tablePartBorder')}</span>
                  <ColorField value={value.border} onChange={(border) => set({ border })} label={t('tablePartBorder')} testId="sheet-nts-border" />
                </div>
                <div className="sheet-dialog-row">
                  <span>{t('tablePartLines')}</span>
                  <ColorField value={value.lines} onChange={(lines) => set({ lines })} label={t('tablePartLines')} testId="sheet-nts-lines" />
                </div>
                <label className="sheet-dialog-choice">
                  <input type="checkbox" checked={value.bold === true} onChange={(event) => set({ bold: event.target.checked })} data-testid="sheet-nts-bold" />
                  {t('tablePartBold')}
                </label>
                <label className="sheet-dialog-choice">
                  <input type="checkbox" checked={value.italic === true} onChange={(event) => set({ italic: event.target.checked })} data-testid="sheet-nts-italic" />
                  {t('tablePartItalic')}
                </label>
                {part.endsWith('Stripe') ? (
                  <label className="sheet-dialog-row">
                    <span>{t('tableStripeSize')}</span>
                    <input className="kui-field" type="number" min={1} max={9} style={{ width: 56 }} value={value.size ?? 1} onChange={(event) => set({ size: Math.max(1, Math.min(9, Math.round(Number(event.target.value) || 1))) })} data-testid="sheet-nts-size" />
                  </label>
                ) : null}
                <button type="button" className="kui-ribbon-button" onClick={() => setParts((was) => Object.fromEntries(Object.entries(was).filter(([kind]) => kind !== part)))} data-testid="sheet-nts-clear-part">
                  {t('tableClearPart')}
                </button>
              </div>
              <div>
                <div className="sheet-font-heading">{t('tablePreview')}</div>
                <span data-testid="sheet-nts-preview">
                  <Thumbnail preview={partsPreview(parts)} />
                </span>
              </div>
            </div>
            <label className="sheet-dialog-choice">
              <input type="checkbox" checked={isDefault} onChange={(event) => setIsDefault(event.target.checked)} data-testid="sheet-nts-default" />
              {t('tableSetDefault')}
            </label>
          </div>
        </ModalBody>
        <ModalFooter className="sheet-dialog-actions">
          <Button variant="secondary" onClick={close} data-testid="sheet-nts-cancel">
            {t('cancel')}
          </Button>
          <Button variant="primary" onClick={() => void ok()} data-testid="sheet-nts-ok">
            {t('ok')}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}

/**
 * The table style gallery, as WPS for Mac has it (开始 › 套用表格样式): Preset Styles, the theme
 * colour row (Text 1 and six accents; the styles below follow the colour chosen), the light,
 * medium and dark styles, the workbook's own styles (a press on the corner's × deletes one), and
 * New Table Style. A style chosen outside a table opens Format as Table's dialog; inside one it
 * restyles that table, and Convert to Range and Clear Table Style join the list. A right-click on a
 * style offers Set As Default (and Modify and Delete for the workbook's own).
 */
export function TableStyleGallery({ info, testId = 'sheet-table' }: { info: SheetInfo; testId?: string }) {
  const { run, t } = useKitEditor()
  const [slot, setSlot] = useState<number | null>(null)
  const [menu, setMenu] = useState<{ name: string; custom: boolean } | null>(null)
  const gallery = useEngineQuery<Gallery>('table.styles', slot == null ? {} : { slot }, info.tableInfo?.style ?? null)
  if (!gallery) return <div className="sheet-table-gallery" data-testid={`${testId}-gallery`} />
  const inTable = gallery.table != null
  const choose = (name: string) => {
    setMenu(null)
    if (inTable) void run('table.style', { style: name }).finally(backToGrid)
    else openFormatAsTable(name, gallery.range ?? info.selection)
  }
  const right = (event: ReactMouseEvent, name: string, custom: boolean) => {
    event.preventDefault()
    event.stopPropagation()
    setMenu({ name, custom })
  }
  const item = (name: string, preview: Preview, custom: boolean) => (
    <span key={name} className="sheet-table-item" data-default={gallery.default === name || undefined}>
      <button
        type="button"
        className="sheet-table-style"
        title={name}
        aria-label={name}
        aria-pressed={gallery.current === name}
        onClick={() => choose(name)}
        onContextMenu={(event) => right(event, name, custom)}
        data-testid={`${testId}-${name}`}
      >
        <Thumbnail preview={preview} />
      </button>
      {gallery.default === name ? (
        <span className="sheet-table-default" title={t('tableIsDefault')} data-testid={`${testId}-default-mark`}>
          ★
        </span>
      ) : null}
      {custom ? (
        <button
          type="button"
          className="sheet-table-delete"
          aria-label={t('tableDeleteStyle')}
          title={t('tableDeleteStyle')}
          onClick={(event) => {
            event.stopPropagation()
            void run('table.deleteStyle', { name })
          }}
          data-testid={`${testId}-delete-${name}`}
        >
          <X size={10} aria-hidden="true" />
        </button>
      ) : null}
    </span>
  )
  const own = gallery.custom.find((style) => style.name === menu?.name)
  return (
    <div className="sheet-table-gallery" data-testid={`${testId}-gallery`} onClick={(event) => (menu && !(event.target as HTMLElement).closest('.sheet-table-menu') ? setMenu(null) : undefined)}>
      <div className="sheet-table-scroll">
        <div className="sheet-table-title">{t('tablePresetStyles')}</div>
        <div className="sheet-table-colors">
          <span>{t('tableThemeColor')}</span>
          <span className="sheet-table-swatches" role="radiogroup" aria-label={t('tableThemeColor')}>
            {gallery.colors.map((color, index) => (
              <span
                key={index}
                role="radio"
                tabIndex={0}
                aria-checked={gallery.slot === index}
                className="sheet-table-swatch"
                style={{ background: color }}
                title={color}
                onClick={(event) => {
                  event.stopPropagation()
                  setSlot(index)
                }}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault()
                    setSlot(index)
                  }
                }}
                data-testid={`${testId}-color-${index}`}
              >
                {gallery.slot === index ? <Check size={11} aria-hidden="true" /> : null}
              </span>
            ))}
          </span>
        </div>
        {gallery.groups.map((group) => (
          <div key={group.id} className="sheet-table-group" data-testid={`${testId}-group-${group.id}`}>
            <div className="sheet-font-heading">{t(`tableGroup:${group.id}`)}</div>
            <div className="sheet-table-grid">{group.styles.map((style) => item(style.name, style.preview, false))}</div>
          </div>
        ))}
        {gallery.custom.length ? (
          <div className="sheet-table-group" data-testid={`${testId}-group-custom`}>
            <div className="sheet-font-heading">{t('tableGroup:custom')}</div>
            <div className="sheet-table-grid">{gallery.custom.map((style) => item(style.name, style.preview, true))}</div>
          </div>
        ) : null}
      </div>
      {menu ? (
        <div className="sheet-table-menu kui-popover" role="menu" data-testid={`${testId}-menu`}>
          <button type="button" className="kui-menu-item" role="menuitem" onClick={() => void run('table.setDefaultStyle', { name: menu.name })} data-testid={`${testId}-menu-default`}>
            {t('tableSetAsDefault')}
          </button>
          {menu.custom && own ? (
            <>
              <button type="button" className="kui-menu-item" role="menuitem" onClick={() => openNewTableStyle({ name: own.name, parts: own.elements, isDefault: gallery.default === own.name })} data-testid={`${testId}-menu-modify`}>
                {t('tableModifyStyle')}
              </button>
              <button type="button" className="kui-menu-item" role="menuitem" onClick={() => void run('table.deleteStyle', { name: menu.name })} data-testid={`${testId}-menu-delete`}>
                {t('tableDeleteStyle')}
              </button>
            </>
          ) : null}
        </div>
      ) : null}
      <div className="sheet-table-actions">
        {inTable ? (
          <>
            <button type="button" className="kui-menu-item" role="menuitem" onClick={() => void run('table.convertToRange', {}).finally(backToGrid)} data-testid={`${testId}-convert`}>
              {t('tableConvertToRange')}
            </button>
            <button type="button" className="kui-menu-item sheet-danger" role="menuitem" onClick={() => void run('table.style', { style: 'None' }).finally(backToGrid)} data-testid={`${testId}-clear`}>
              {t('tableClearStyle')}
            </button>
          </>
        ) : null}
        <button type="button" className="kui-menu-item" role="menuitem" onClick={() => openNewTableStyle()} data-testid={`${testId}-new`}>
          {t('tableNewStyle')}
        </button>
      </div>
    </div>
  )
}

/** Home › 套用表格样式: the gallery under its button (greyed for several areas or a protected sheet). */
export function FormatAsTableMenu({ info }: { info: SheetInfo }) {
  const { enabled, phrase: p } = useKitEditor()
  return (
    <RibbonMenu icon={Table2} label={p('Format as Table')} size="large" testId="sheet-format-table" disabled={!enabled('home.formatAsTable')}>
      <TableStyleGallery info={info} />
    </RibbonMenu>
  )
}

/**
 * The Table Design tab (WPS's 表格工具): the table's name and Resize Table, Remove Duplicates and
 * Convert to Range, the style options (each takes effect at once), and the style gallery.
 */
export function TableDesignTab({ info }: { info: SheetInfo }) {
  const { run, t, phrase: p } = useKitEditor()
  const table = info.tableInfo
  const [name, setName] = useState(table?.name ?? '')
  useEffect(() => setName(table?.name ?? ''), [table?.name])
  if (!table) return null
  const option = (key: keyof typeof table, command: string, label: string, testId: string) => (
    <label className="sheet-dialog-choice sheet-table-option">
      {/* Toggles at once; the engine's answer sets it again (each change of the option remounts it). */}
      <input key={`${String(key)}-${String(table[key])}`} type="checkbox" defaultChecked={table[key] === true} onChange={(event) => void run(command, { on: event.target.checked }).finally(backToGrid)} data-testid={testId} />
      {label}
    </label>
  )
  const rename = () => {
    if (name.trim() && name.trim() !== table.name) void run('table.rename', { name: name.trim() }).then((done) => (done == null ? setName(table.name) : undefined))
  }
  return (
    <>
      <RibbonGroup label={p('Properties')} icon={TableProperties}>
        <RibbonStack>
          <label className="sheet-dialog-row">
            <span>{t('tableName')}</span>
            <input
              className="kui-ribbon-field"
              style={{ width: 96 }}
              value={name}
              onChange={(event) => setName(event.target.value)}
              onBlur={rename}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault()
                  rename()
                  backToGrid()
                }
              }}
              data-testid="sheet-table-name"
            />
          </label>
          <button type="button" className="kui-ribbon-button" onClick={() => openResizeTable(table.range)} data-testid="sheet-table-resize">
            {p('Resize Table')}
          </button>
        </RibbonStack>
        <RibbonStack>
          <button type="button" className="kui-ribbon-button" onClick={() => void run('table.removeDuplicates', {})} data-testid="sheet-table-dedupe">
            {p('Remove Duplicates')}
          </button>
          <button type="button" className="kui-ribbon-button" onClick={() => void run('table.convertToRange', {}).finally(backToGrid)} data-testid="sheet-table-to-range">
            {p('Convert to Range')}
          </button>
        </RibbonStack>
      </RibbonGroup>
      <RibbonGroup label={p('Table Style Options')} icon={Table2}>
        <div className="sheet-table-options">
          {option('header', 'table.headerRow', p('Header Row'), 'sheet-table-opt-header')}
          {option('totals', 'table.totalRow', p('Total Row'), 'sheet-table-opt-totals')}
          {option('bandedRows', 'table.bandedRows', p('Banded Rows'), 'sheet-table-opt-banded-rows')}
          {option('firstCol', 'table.firstColumn', p('First Column'), 'sheet-table-opt-first-col')}
          {option('lastCol', 'table.lastColumn', p('Last Column'), 'sheet-table-opt-last-col')}
          {option('bandedCols', 'table.bandedColumns', p('Banded Columns'), 'sheet-table-opt-banded-cols')}
          {option('filter', 'table.filterButton', p('Filter Button'), 'sheet-table-opt-filter')}
        </div>
      </RibbonGroup>
      <RibbonGroup label={p('Table Styles')} icon={Table2}>
        <RibbonMenu icon={Table2} label={p('Table Styles')} size="large" testId="sheet-table-design-styles">
          <TableStyleGallery info={info} testId="sheet-table-design" />
        </RibbonMenu>
      </RibbonGroup>
    </>
  )
}
