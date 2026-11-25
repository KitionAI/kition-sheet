import { useEffect, useMemo, useState } from 'react'
import { Button, ColorGrid, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, ModalTitle, toast, useKitEditor } from '@kition/sdk/editor'
import { FONTS, SIZES, backToGrid, type SheetInfo } from './sheet'

type Color = 'Auto' | { Rgb: number } | { Theme: unknown } | null | undefined
type Line = { style?: string; color?: Color }
/** The active cell's whole format, as `cell.get` answers it. */
type Full = {
  font?: { name?: string; size?: number; bold?: boolean; italic?: boolean; underline?: string; strike?: boolean; color?: Color; vert?: string }
  fill?: { pattern?: string; fg?: Color; bg?: Color }
  borders?: Record<string, Line>
  align?: { h?: string; v?: string; wrap?: boolean; shrink?: boolean; indent?: number; rotation?: number }
  num_fmt?: string
  protection?: { locked?: boolean; hidden?: boolean }
}

let opened = 0
const listeners = new Set<(count: number) => void>()
/** Opens Format Cells (Cmd/Ctrl+1, Home › Format › Format Cells…, the cell menu, a command's request). */
export const openFormatCells = () => {
  opened += 1
  for (const listener of listeners) listener(opened)
}

const hexOf = (color: Color) => (color && typeof color === 'object' && 'Rgb' in color ? `#${color.Rgb.toString(16).padStart(6, '0').toUpperCase()}` : undefined)

const TABS = ['number', 'alignment', 'font', 'border', 'fill', 'protection'] as const
type Tab = (typeof TABS)[number]

/** Number's categories, as WPS lists them, and the format each makes. */
const CATEGORIES = ['general', 'number', 'currency', 'accounting', 'date', 'time', 'percent', 'fraction', 'scientific', 'text', 'custom'] as const
type Category = (typeof CATEGORIES)[number]
const DATES = ['yyyy/m/d', 'yyyy"年"m"月"d"日"', 'yyyy"年"m"月"', 'm"月"d"日"', 'yyyy-mm-dd', 'm/d/yy', 'd-mmm-yy', 'aaaa']
const TIMES = ['h:mm:ss', 'h:mm', 'h"时"mm"分"ss"秒"', 'h:mm AM/PM', 'yyyy/m/d h:mm']
const FRACTIONS = ['# ?/?', '# ??/??', '# ?/2', '# ?/4', '# ?/10']
const CUSTOM = ['General', '0', '0.00', '#,##0', '#,##0.00', '0%', '0.00%', '0.00E+00', '@', 'yyyy/m/d', 'h:mm:ss', '[Red]-0.00']
const SYMBOLS = ['¥', '$', '€', '£', '']

/** Which category a format code belongs to (Custom when none makes it). */
function categoryOf(code: string): Category {
  if (!code || code === 'General') return 'general'
  if (code === '@') return 'text'
  if (code.startsWith('_(')) return 'accounting'
  if (/E\+/.test(code)) return 'scientific'
  if (code.includes('%')) return 'percent'
  if (FRACTIONS.includes(code)) return 'fraction'
  if (DATES.includes(code)) return 'date'
  if (TIMES.includes(code)) return 'time'
  if (/^"?[¥$€£]"?#,##0/.test(code)) return 'currency'
  if (/^(#,##)?0(\.0+)?(;\[Red\]-(#,##)?0(\.0+)?)?$/.test(code)) return 'number'
  return 'custom'
}
const decimalsOf = (code: string) => /\.(0+)/.exec(code)?.[1].length ?? 0
const zeros = (n: number) => (n > 0 ? `.${'0'.repeat(n)}` : '')

const H_ALIGN = ['General', 'Left', 'Center', 'Right', 'Fill', 'Justify', 'CenterAcross', 'Distributed']
const V_ALIGN = ['Top', 'Center', 'Bottom', 'Justify', 'Distributed']
const UNDERLINES = ['None', 'Single', 'Double', 'SingleAccounting', 'DoubleAccounting']
const LINES = ['thin', 'medium', 'thick', 'dashed', 'dotted', 'double', 'hair']
const EDGES = ['top', 'bottom', 'left', 'right', 'insideHorizontal', 'insideVertical', 'diagonalDown', 'diagonalUp'] as const
type Edge = (typeof EDGES)[number]

/**
 * Format Cells, as WPS's 单元格格式 dialog: Number (a category, its decimal places, thousands
 * separator, currency symbol or format list, and a sample of the active cell), Alignment
 * (horizontal, vertical, indent, rotation, wrap, shrink, merge), Font (font, style, size,
 * underline, colour, strikethrough, superscript, subscript, preview), Border (none, outline,
 * inside, each edge, line style and colour), Fill (no colour, the palette, More Colors) and
 * Protection (locked, hidden). OK sets what changed, in one undo step.
 */
export function FormatCellsDialog({ info }: { info: SheetInfo }) {
  const { run, query, t } = useKitEditor()
  const [count, setCount] = useState(opened)
  const [shown, setShown] = useState(false)
  const [tab, setTab] = useState<Tab>('number')
  const [start, setStart] = useState<Full>({})
  // Number.
  const [category, setCategory] = useState<Category>('general')
  const [decimals, setDecimals] = useState(2)
  const [thousands, setThousands] = useState(false)
  const [redNegative, setRedNegative] = useState(false)
  const [symbol, setSymbol] = useState('¥')
  const [listed, setListed] = useState('')
  const [custom, setCustom] = useState('General')
  const [sample, setSample] = useState('')
  // Alignment.
  const [h, setH] = useState('General')
  const [v, setV] = useState('Bottom')
  const [indent, setIndent] = useState(0)
  const [rotation, setRotation] = useState(0)
  const [wrap, setWrap] = useState(false)
  const [shrink, setShrink] = useState(false)
  const [merge, setMerge] = useState(false)
  // Font.
  const [font, setFont] = useState('')
  const [bold, setBold] = useState(false)
  const [italic, setItalic] = useState(false)
  const [size, setSize] = useState(11)
  const [underline, setUnderline] = useState('None')
  const [fontColor, setFontColor] = useState<string | undefined>()
  const [strike, setStrike] = useState(false)
  const [vert, setVert] = useState('Baseline')
  // Border.
  const [line, setLine] = useState('thin')
  const [lineColor, setLineColor] = useState('#000000')
  const [edges, setEdges] = useState<Array<{ preset: string; style: string; color: string }>>([])
  const [drawn, setDrawn] = useState<Partial<Record<Edge, boolean>>>({})
  // Fill.
  const [fill, setFill] = useState<string | null | undefined>(undefined)
  // Protection.
  const [locked, setLocked] = useState(true)
  const [hidden, setHidden] = useState(false)

  useEffect(() => {
    listeners.add(setCount)
    return () => {
      listeners.delete(setCount)
    }
  }, [])
  // Each opening reads the active cell's format afresh.
  useEffect(() => {
    if (count === 0) return
    setShown(true)
    setTab('number')
    void query<{ style?: Full }>('cell.get', { cell: info.active }).then((cell) => {
      const st = cell?.style ?? {}
      setStart(st)
      const code = st.num_fmt ?? 'General'
      const cat = categoryOf(code)
      setCategory(cat)
      setDecimals(cat === 'general' ? 2 : decimalsOf(code))
      setThousands(code.includes('#,##0'))
      setRedNegative(code.includes('[Red]'))
      setListed(code)
      setCustom(code)
      setH(st.align?.h ?? 'General')
      setV(st.align?.v ?? 'Bottom')
      setIndent(st.align?.indent ?? 0)
      setRotation(st.align?.rotation === 255 ? 0 : (st.align?.rotation ?? 0))
      setWrap(st.align?.wrap === true)
      setShrink(st.align?.shrink === true)
      setMerge(false)
      setFont(st.font?.name ?? '')
      setBold(st.font?.bold === true)
      setItalic(st.font?.italic === true)
      setSize(st.font?.size ?? 11)
      setUnderline(st.font?.underline ?? 'None')
      setFontColor(hexOf(st.font?.color))
      setStrike(st.font?.strike === true)
      setVert(st.font?.vert ?? 'Baseline')
      setEdges([])
      setDrawn(Object.fromEntries(EDGES.map((edge) => [edge, false])))
      setFill(undefined)
      setLocked(st.protection?.locked !== false)
      setHidden(st.protection?.hidden === true)
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per opening
  }, [count])

  const code = useMemo(() => {
    const sep = thousands ? '#,##0' : '0'
    switch (category) {
      case 'general':
        return 'General'
      case 'number':
        return redNegative ? `${sep}${zeros(decimals)};[Red]-${sep}${zeros(decimals)}` : `${sep}${zeros(decimals)}`
      case 'currency':
        return `${symbol ? `"${symbol}"` : ''}#,##0${zeros(decimals)}`
      case 'accounting':
        return `_(${symbol ? `"${symbol}"` : ''}* #,##0${zeros(decimals)}_);_(${symbol ? `"${symbol}"` : ''}* (#,##0${zeros(decimals)});_(${symbol ? `"${symbol}"` : ''}* "-"??_);_(@_)`
      case 'percent':
        return `0${zeros(decimals)}%`
      case 'scientific':
        return `0${zeros(decimals)}E+00`
      case 'text':
        return '@'
      case 'date':
        return DATES.includes(listed) ? listed : DATES[0]
      case 'time':
        return TIMES.includes(listed) ? listed : TIMES[0]
      case 'fraction':
        return FRACTIONS.includes(listed) ? listed : FRACTIONS[0]
      default:
        return custom.trim() || 'General'
    }
  }, [category, custom, decimals, listed, redNegative, symbol, thousands])
  // The active cell's value as the format shows it.
  useEffect(() => {
    if (!shown) return
    const quoted = code.replace(/"/g, '""')
    void query<unknown>('formula.evaluate', { formula: `=TEXT(${info.active},"${quoted}")` }).then((value) => setSample(value == null ? '' : String(value)))
  }, [code, info.active, query, shown])

  if (!shown) return null
  const close = () => {
    setShown(false)
    backToGrid()
  }
  const ok = async () => {
    const style: Record<string, unknown> = {}
    if (code !== (start.num_fmt ?? 'General')) style.num_fmt = code
    const align: Record<string, unknown> = {}
    if (h !== (start.align?.h ?? 'General')) align.h = h
    if (v !== (start.align?.v ?? 'Bottom')) align.v = v
    if (indent !== (start.align?.indent ?? 0)) align.indent = indent
    if (rotation !== (start.align?.rotation === 255 ? 0 : (start.align?.rotation ?? 0))) align.rotation = Math.max(-90, Math.min(90, rotation))
    if (wrap !== (start.align?.wrap === true)) align.wrap = wrap
    if (shrink !== (start.align?.shrink === true)) align.shrink = shrink
    if (Object.keys(align).length) style.align = align
    const f: Record<string, unknown> = {}
    if (font && font !== start.font?.name) f.name = font
    if (bold !== (start.font?.bold === true)) f.bold = bold
    if (italic !== (start.font?.italic === true)) f.italic = italic
    if (size !== (start.font?.size ?? 11)) f.size = size
    if (underline !== (start.font?.underline ?? 'None')) f.underline = underline
    if (fontColor !== hexOf(start.font?.color)) f.color = fontColor ?? 'auto'
    if (strike !== (start.font?.strike === true)) f.strike = strike
    if (vert !== (start.font?.vert ?? 'Baseline')) f.vert = vert
    if (Object.keys(f).length) style.font = f
    if (fill !== undefined) style.fill = fill ?? 'none'
    if (locked !== (start.protection?.locked !== false) || hidden !== (start.protection?.hidden === true)) style.protection = { locked, hidden }
    const params: Record<string, unknown> = {}
    if (Object.keys(style).length) params.style = style
    if (edges.length) params.edges = edges
    if (merge) params.merge = true
    close()
    if (Object.keys(params).length) await run('format.cells', params)
  }
  const preset = (name: string) => {
    if (name === 'none') {
      setEdges([{ preset: 'none', style: line, color: lineColor }])
      setDrawn(Object.fromEntries(EDGES.map((edge) => [edge, false])))
      return
    }
    setEdges((was) => [...was.filter((e) => e.preset !== 'none'), { preset: name, style: line, color: lineColor }])
    const on: Edge[] = name === 'outside' ? ['top', 'bottom', 'left', 'right'] : ['insideHorizontal', 'insideVertical']
    setDrawn((was) => ({ ...was, ...Object.fromEntries(on.map((edge) => [edge, true])) }))
  }
  const toggleEdge = (edge: Edge) => {
    const on = !drawn[edge]
    setDrawn((was) => ({ ...was, [edge]: on }))
    // An edge turned off is drawn with no line.
    setEdges((was) => [...was, { preset: edge, style: on ? line : 'none', color: lineColor }])
  }

  const decimalsField = (
    <label className="sheet-dialog-row">
      <span>{t('fcDecimals')}</span>
      <input className="kui-field" type="number" min={0} max={30} style={{ width: 56 }} value={decimals} onChange={(event) => setDecimals(Math.max(0, Math.min(30, Math.round(Number(event.target.value) || 0))))} data-testid="fc-decimals" />
    </label>
  )
  const list = (codes: string[]) => (
    <ul className="sheet-function-list sheet-fc-list" role="listbox" aria-label={t('fcType')} data-testid="fc-list">
      {codes.map((item) => (
        <li key={item} role="option" aria-selected={item === code} onClick={() => setListed(item)} data-code={item}>
          {item}
        </li>
      ))}
    </ul>
  )

  return (
    <Modal open onOpenChange={(open) => (open ? undefined : close())}>
      <ModalContent size="md" dismissible={false} aria-describedby={undefined} onCloseAutoFocus={(event) => event.preventDefault()} data-testid="sheet-format-cells">
        <ModalHeader>
          <ModalTitle>{t('formatCellsTitle')}</ModalTitle>
        </ModalHeader>
        <ModalBody>
          <div className="sheet-fc-tabs" role="tablist">
            {TABS.map((id) => (
              <button key={id} type="button" role="tab" aria-selected={tab === id} onClick={() => setTab(id)} data-testid={`fc-tab-${id}`}>
                {t(`fcTab:${id}`)}
              </button>
            ))}
          </div>
          <div className="sheet-fc-page" role="tabpanel" data-testid={`fc-page-${tab}`}>
            {tab === 'number' ? (
              <div className="sheet-fc-number">
                <ul className="sheet-function-list sheet-fc-categories" role="listbox" aria-label={t('fcCategory')} data-testid="fc-categories">
                  {CATEGORIES.map((id) => (
                    <li key={id} role="option" aria-selected={category === id} onClick={() => setCategory(id)} data-testid={`fc-cat-${id}`}>
                      {t(`fcCat:${id}`)}
                    </li>
                  ))}
                </ul>
                <div className="sheet-dialog-form">
                  <div className="sheet-dialog-row">
                    <span>{t('fcSample')}</span>
                    <output className="sheet-fc-sample" data-testid="fc-sample">
                      {sample}
                    </output>
                  </div>
                  {category === 'number' ? (
                    <>
                      {decimalsField}
                      <label className="sheet-dialog-choice">
                        <input type="checkbox" checked={thousands} onChange={(event) => setThousands(event.target.checked)} data-testid="fc-thousands" />
                        {t('fcThousands')}
                      </label>
                      <label className="sheet-dialog-choice">
                        <input type="checkbox" checked={redNegative} onChange={(event) => setRedNegative(event.target.checked)} data-testid="fc-red-negative" />
                        {t('fcRedNegative')}
                      </label>
                    </>
                  ) : null}
                  {category === 'currency' || category === 'accounting' ? (
                    <>
                      {decimalsField}
                      <label className="sheet-dialog-row">
                        <span>{t('fcSymbol')}</span>
                        <select className="kui-field" value={symbol} onChange={(event) => setSymbol(event.target.value)} data-testid="fc-symbol">
                          {SYMBOLS.map((item) => (
                            <option key={item} value={item}>
                              {item || t('fcNoSymbol')}
                            </option>
                          ))}
                        </select>
                      </label>
                    </>
                  ) : null}
                  {category === 'percent' || category === 'scientific' ? decimalsField : null}
                  {category === 'date' ? list(DATES) : null}
                  {category === 'time' ? list(TIMES) : null}
                  {category === 'fraction' ? list(FRACTIONS) : null}
                  {category === 'custom' ? (
                    <>
                      <label className="sheet-dialog-row">
                        <span>{t('fcType')}</span>
                        <input className="kui-field" value={custom} spellCheck={false} onChange={(event) => setCustom(event.target.value)} data-testid="fc-code" />
                      </label>
                      <ul className="sheet-function-list sheet-fc-list" role="listbox" aria-label={t('fcType')}>
                        {CUSTOM.map((item) => (
                          <li key={item} role="option" aria-selected={item === custom} onClick={() => setCustom(item)}>
                            {item}
                          </li>
                        ))}
                      </ul>
                    </>
                  ) : null}
                  <div className="sheet-fc-note" data-testid="fc-format">
                    {code}
                  </div>
                </div>
              </div>
            ) : null}
            {tab === 'alignment' ? (
              <div className="sheet-dialog-form">
                <label className="sheet-dialog-row">
                  <span>{t('fcHorizontal')}</span>
                  <select className="kui-field" value={h} onChange={(event) => setH(event.target.value)} data-testid="fc-h">
                    {H_ALIGN.map((item) => (
                      <option key={item} value={item}>
                        {t(`fcH:${item}`)}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="sheet-dialog-row">
                  <span>{t('fcVertical')}</span>
                  <select className="kui-field" value={v} onChange={(event) => setV(event.target.value)} data-testid="fc-v">
                    {V_ALIGN.map((item) => (
                      <option key={item} value={item}>
                        {t(`fcV:${item}`)}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="sheet-dialog-row">
                  <span>{t('fcIndent')}</span>
                  <input className="kui-field" type="number" min={0} max={15} style={{ width: 56 }} value={indent} onChange={(event) => setIndent(Math.max(0, Math.min(15, Math.round(Number(event.target.value) || 0))))} data-testid="fc-indent" />
                </label>
                <label className="sheet-dialog-row">
                  <span>{t('fcRotation')}</span>
                  <input className="kui-field" type="number" min={-90} max={90} style={{ width: 56 }} value={rotation} onChange={(event) => setRotation(Math.round(Number(event.target.value) || 0))} data-testid="fc-rotation" />
                </label>
                <label className="sheet-dialog-choice">
                  <input type="checkbox" checked={wrap} onChange={(event) => setWrap(event.target.checked)} data-testid="fc-wrap" />
                  {t('fcWrap')}
                </label>
                <label className="sheet-dialog-choice">
                  <input type="checkbox" checked={shrink} onChange={(event) => setShrink(event.target.checked)} data-testid="fc-shrink" />
                  {t('fcShrink')}
                </label>
                <label className="sheet-dialog-choice">
                  <input type="checkbox" checked={merge} onChange={(event) => setMerge(event.target.checked)} data-testid="fc-merge" />
                  {t('fcMerge')}
                </label>
              </div>
            ) : null}
            {tab === 'font' ? (
              <div className="sheet-dialog-form">
                <label className="sheet-dialog-row">
                  <span>{t('fcFont')}</span>
                  <select className="kui-field" value={font} onChange={(event) => setFont(event.target.value)} data-testid="fc-font">
                    {[...(font && !FONTS.some((item) => item.name === font) ? [font] : []), ...FONTS.map((item) => item.name)].map((name) => (
                      <option key={name} value={name}>
                        {name}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="sheet-dialog-row">
                  <span>{t('fcFontStyle')}</span>
                  <select
                    className="kui-field"
                    value={`${bold ? 'b' : ''}${italic ? 'i' : ''}`}
                    onChange={(event) => {
                      setBold(event.target.value.includes('b'))
                      setItalic(event.target.value.includes('i'))
                    }}
                    data-testid="fc-font-style"
                  >
                    <option value="">{t('fcRegular')}</option>
                    <option value="i">{t('fcItalic')}</option>
                    <option value="b">{t('fcBold')}</option>
                    <option value="bi">{t('fcBoldItalic')}</option>
                  </select>
                </label>
                <label className="sheet-dialog-row">
                  <span>{t('fcSize')}</span>
                  <input className="kui-field" list="sheet-fc-sizes" style={{ width: 64 }} value={size} onChange={(event) => setSize(Number(event.target.value) || size)} data-testid="fc-size" />
                  <datalist id="sheet-fc-sizes">
                    {SIZES.map((item) => (
                      <option key={item} value={item} />
                    ))}
                  </datalist>
                </label>
                <label className="sheet-dialog-row">
                  <span>{t('fcUnderline')}</span>
                  <select className="kui-field" value={underline} onChange={(event) => setUnderline(event.target.value)} data-testid="fc-underline">
                    {UNDERLINES.map((item) => (
                      <option key={item} value={item}>
                        {t(`fcU:${item}`)}
                      </option>
                    ))}
                  </select>
                </label>
                <div className="sheet-dialog-row">
                  <span>{t('fcColor')}</span>
                  <DialogColor value={fontColor} onChange={setFontColor} none={t('colorAutomatic')} testId="fc-font-color" />
                </div>
                <label className="sheet-dialog-choice">
                  <input type="checkbox" checked={strike} onChange={(event) => setStrike(event.target.checked)} data-testid="fc-strike" />
                  {t('fcStrike')}
                </label>
                <label className="sheet-dialog-choice">
                  <input type="checkbox" checked={vert === 'Superscript'} onChange={(event) => setVert(event.target.checked ? 'Superscript' : 'Baseline')} data-testid="fc-super" />
                  {t('fcSuper')}
                </label>
                <label className="sheet-dialog-choice">
                  <input type="checkbox" checked={vert === 'Subscript'} onChange={(event) => setVert(event.target.checked ? 'Subscript' : 'Baseline')} data-testid="fc-sub" />
                  {t('fcSub')}
                </label>
                <div
                  className="sheet-fc-preview"
                  data-testid="fc-preview"
                  style={{
                    fontFamily: FONTS.find((item) => item.name === font)?.css ?? font,
                    fontWeight: bold ? 700 : 400,
                    fontStyle: italic ? 'italic' : 'normal',
                    fontSize: Math.min(36, size * 1.33),
                    color: fontColor ?? undefined,
                    textDecoration: `${underline !== 'None' ? 'underline' : ''} ${strike ? 'line-through' : ''}`.trim() || 'none',
                  }}
                >
                  {t('fcPreviewText')}
                </div>
              </div>
            ) : null}
            {tab === 'border' ? (
              <div className="sheet-dialog-form">
                <div className="sheet-dialog-row">
                  <span>{t('fcPresets')}</span>
                  <button type="button" className="kui-ribbon-button" onClick={() => preset('none')} data-testid="fc-border-none">
                    {t('fcBorderNone')}
                  </button>
                  <button type="button" className="kui-ribbon-button" onClick={() => preset('outside')} data-testid="fc-border-outside">
                    {t('fcBorderOutside')}
                  </button>
                  <button type="button" className="kui-ribbon-button" onClick={() => preset('inside')} data-testid="fc-border-inside">
                    {t('fcBorderInside')}
                  </button>
                </div>
                <div className="sheet-dialog-row">
                  <span>{t('fcBorderEdges')}</span>
                  <span className="sheet-fc-edges">
                    {EDGES.map((edge) => (
                      <button key={edge} type="button" className="kui-ribbon-button" aria-pressed={drawn[edge] === true} onClick={() => toggleEdge(edge)} data-testid={`fc-edge-${edge}`}>
                        {t(`fcEdge:${edge}`)}
                      </button>
                    ))}
                  </span>
                </div>
                <label className="sheet-dialog-row">
                  <span>{t('fcLineStyle')}</span>
                  <select className="kui-field" value={line} onChange={(event) => setLine(event.target.value)} data-testid="fc-line-style">
                    {LINES.map((item) => (
                      <option key={item} value={item}>
                        {t(`fcLine:${item}`)}
                      </option>
                    ))}
                  </select>
                </label>
                <div className="sheet-dialog-row">
                  <span>{t('fcColor')}</span>
                  <DialogColor value={lineColor} onChange={(color) => setLineColor(color ?? '#000000')} none={t('colorAutomatic')} testId="fc-line-color" />
                </div>
                <div className="sheet-fc-border-preview" data-testid="fc-border-preview">
                  <span
                    style={{
                      borderTop: drawn.top ? `2px solid ${lineColor}` : undefined,
                      borderBottom: drawn.bottom ? `2px solid ${lineColor}` : undefined,
                      borderLeft: drawn.left ? `2px solid ${lineColor}` : undefined,
                      borderRight: drawn.right ? `2px solid ${lineColor}` : undefined,
                    }}
                  >
                    {t('fcText')}
                  </span>
                </div>
              </div>
            ) : null}
            {tab === 'fill' ? (
              <div className="sheet-dialog-form">
                <button type="button" className="kui-menu-item" onClick={() => setFill(null)} data-testid="fc-fill-none">
                  {t('fcNoColor')}
                </button>
                <ColorGrid onPick={(color) => setFill(color.toUpperCase())} testId="fc-fill-colors" />
                <div className="sheet-dialog-row">
                  <span>{t('colorMore')}</span>
                  <DialogColor value={fill ?? undefined} onChange={(color) => setFill(color ?? null)} none={t('fcNoColor')} testId="fc-fill-more" />
                </div>
                <div className="sheet-dialog-row">
                  <span>{t('fcSample')}</span>
                  <span className="sheet-fc-swatch" style={{ background: fill ?? (fill === null ? '#FFFFFF' : (hexOf(start.fill?.fg) ?? '#FFFFFF')) }} data-testid="fc-fill-sample" data-color={fill ?? ''} />
                </div>
              </div>
            ) : null}
            {tab === 'protection' ? (
              <div className="sheet-dialog-form">
                <label className="sheet-dialog-choice">
                  <input type="checkbox" checked={locked} onChange={(event) => setLocked(event.target.checked)} data-testid="fc-locked" />
                  {t('fcLocked')}
                </label>
                <label className="sheet-dialog-choice">
                  <input type="checkbox" checked={hidden} onChange={(event) => setHidden(event.target.checked)} data-testid="fc-hidden" />
                  {t('fcHidden')}
                </label>
                <p className="sheet-fc-note">{t('fcProtectionNote')}</p>
              </div>
            ) : null}
          </div>
        </ModalBody>
        <ModalFooter className="sheet-dialog-actions">
          <Button variant="secondary" onClick={close} data-testid="fc-cancel">
            {t('cancel')}
          </Button>
          <Button variant="primary" onClick={() => void ok()} data-testid="fc-ok">
            {t('ok')}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}

/** A colour in the dialog: a swatch that opens No Color, the palette and More Colors (#RRGGBB). */
function DialogColor({ value, onChange, none, testId }: { value: string | undefined; onChange: (color: string | undefined) => void; none: string; testId: string }) {
  const { t } = useKitEditor()
  const [open, setOpen] = useState(false)
  const [hex, setHex] = useState('')
  return (
    <span className="sheet-color-field">
      <button type="button" className="sheet-color-value" aria-expanded={open} onClick={() => setOpen((was) => !was)} data-testid={testId} data-color={value ?? ''}>
        <span style={{ background: value ?? 'transparent' }} className={value ? undefined : 'sheet-color-empty'} />
        {value ?? none}
      </button>
      {open ? (
        <span className="sheet-color-pop">
          <button
            type="button"
            className="kui-menu-item"
            onClick={() => {
              onChange(undefined)
              setOpen(false)
            }}
            data-testid={`${testId}-none`}
          >
            {none}
          </button>
          <ColorGrid
            onPick={(color) => {
              onChange(color.toUpperCase())
              setOpen(false)
            }}
            testId={`${testId}-colors`}
          />
          <span className="sheet-color-more">
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
