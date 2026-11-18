import { useEffect, useMemo, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, ModalTitle, RibbonGroup, RibbonMenu, readKitStore, useKitEditor, useReadOnly, writeKitStore } from '@kition/kit-editor'
import { OmegaIcon } from '@kition/tessera-icons/react'
import { FONTS, backToGrid } from './sheet'

/**
 * Insert › 符号 (module M8, as WPS for Mac has it): a panel of the symbols used last and the
 * common symbols (5 rows of 8) over 「更多符号…」, which opens the Symbol dialog (符号 and 特殊字符
 * tabs, a name search, the font and the Unicode subset, the grid, the symbols used last, the
 * character's code and name). A symbol goes in at the caret of what is being typed (the cell, a
 * shape's text); with nothing typed, after the active cell's text with the cell left open, as
 * WPS does (`insert.symbol`, which the editor's engine routes so).
 */

/** The common symbols, row by row (the first two rows as the case sheet lists them; ours after). */
export const COMMON_SYMBOLS = [
  ['＋', '−', '×', '÷', '±', '≡', '≠', '≈'],
  ['≤', '≥', '‰', '¼', '½', 'α', 'β', 'θ'],
  ['℃', '°', '√', '∞', '∑', 'π', 'Ω', 'μ'],
  ['←', '→', '↑', '↓', '①', '②', '③', '№'],
  ['©', '®', '™', '§', '※', '□', '☑', '☒'],
]
/** How many symbols 最近使用符号 keeps (two rows; WPS for Mac keeps more than one row). */
export const RECENT_SYMBOLS = 16

/** The special characters, with the names Word and WPS give them (the 特殊字符 tab). */
const SPECIAL: Array<[string, string]> = [
  ['—', 'specialEmDash'],
  ['–', 'specialEnDash'],
  ['‑', 'specialNbHyphen'],
  [' ', 'specialEmSpace'],
  [' ', 'specialEnSpace'],
  [' ', 'specialQuarterSpace'],
  [' ', 'specialNbSpace'],
  ['©', 'specialCopyright'],
  ['®', 'specialRegistered'],
  ['™', 'specialTrademark'],
  ['§', 'specialSection'],
  ['¶', 'specialParagraph'],
  ['…', 'specialEllipsis'],
  ['‘', 'specialSingleOpen'],
  ['’', 'specialSingleClose'],
  ['“', 'specialDoubleOpen'],
  ['”', 'specialDoubleClose'],
  ['°', 'specialDegree'],
  ['℃', 'specialCelsius'],
  ['‰', 'specialPerMille'],
  ['±', 'specialPlusMinus'],
  ['×', 'specialTimes'],
  ['÷', 'specialDivide'],
  ['√', 'specialRoot'],
  ['∞', 'specialInfinity'],
  ['≈', 'specialAlmostEqual'],
  ['≠', 'specialNotEqual'],
  ['≤', 'specialLessEqual'],
  ['≥', 'specialGreaterEqual'],
  ['€', 'specialEuro'],
  ['¥', 'specialYuan'],
  ['£', 'specialPound'],
  ['№', 'specialNumero'],
]

const recentSymbols = () => readKitStore<string[]>('sheet', 'recentSymbols', (raw) => (Array.isArray(raw) && raw.every((item) => typeof item === 'string') ? (raw as string[]) : undefined)) ?? []
/** Puts `symbol` first among the symbols used last (once, at most `RECENT_SYMBOLS`). */
const rememberSymbol = (symbol: string) => writeKitStore('sheet', 'recentSymbols', [symbol, ...recentSymbols().filter((item) => item !== symbol)].slice(0, RECENT_SYMBOLS))

/** Puts a symbol in (and among those used last); the keyboard goes back to the grid. */
function useInsertSymbol(refocus = true) {
  const { run } = useKitEditor()
  return (symbol: string) => {
    rememberSymbol(symbol)
    const done = run('insert.symbol', { char: symbol })
    return refocus ? done.finally(backToGrid) : done
  }
}

/** Insert › 符号: the panel under its button. */
export function SymbolGroup() {
  const { enabled, phrase: p } = useKitEditor()
  const readOnly = useReadOnly()
  return (
    <RibbonGroup label={p('Symbols')} icon={OmegaIcon}>
      <RibbonMenu icon={OmegaIcon} label={p('Symbol')} size="large" primary testId="sheet-symbol" disabled={readOnly || !enabled('insert.symbol')}>
        <SymbolPanel />
      </RibbonMenu>
    </RibbonGroup>
  )
}

function SymbolPanel() {
  const { t } = useKitEditor()
  const insert = useInsertSymbol()
  const recent = useMemo(recentSymbols, [])
  const cell = (symbol: string, testId: string) => (
    <button key={symbol} type="button" role="menuitem" className="sheet-symbol-cell" onClick={() => void insert(symbol)} data-symbol={symbol} data-testid={testId}>
      {symbol}
    </button>
  )
  return (
    <div className="sheet-symbol-panel" data-testid="sheet-symbol-panel">
      <div className="sheet-symbol-heading" data-testid="sheet-symbol-recent-title">
        {t('symbolRecent')}
      </div>
      <div className="sheet-symbol-grid sheet-symbol-recent" data-testid="sheet-symbol-recent">
        {recent.map((symbol) => cell(symbol, 'sheet-symbol-recent-item'))}
      </div>
      <div className="sheet-symbol-heading" data-testid="sheet-symbol-common-title">
        {t('symbolCommon')}
      </div>
      <div className="sheet-symbol-grid" data-testid="sheet-symbol-common">
        {COMMON_SYMBOLS.flat().map((symbol) => cell(symbol, 'sheet-symbol-item'))}
      </div>
      <div className="kui-menu-separator" role="separator" />
      <button type="button" role="menuitem" className="kui-menu-item" onClick={() => openSymbolDialog()} data-testid="sheet-symbol-more">
        <OmegaIcon size={15} aria-hidden="true" />
        <span>{t('symbolMore')}</span>
      </button>
    </div>
  )
}

let opened = 0
const listeners = new Set<(count: number) => void>()
/** Opens the Symbol dialog (Insert › 符号 › 更多符号…). */
export const openSymbolDialog = () => {
  opened += 1
  for (const listener of listeners) listener(opened)
}

type Names = { names: Map<number, string>; subsets: Array<{ id: string; zh: string; en: string; chars: string[] }> }
let names: Promise<Names> | null = null
/** The subsets' characters and their Unicode names, loaded once when the dialog first opens. */
const loadNames = () => {
  names ??= import('./symbolNames').then(({ NAMES, SUBSETS }) => {
    const map = new Map<number, string>()
    for (const line of NAMES.split('\n')) {
      const space = line.indexOf(' ')
      if (space > 0) map.set(parseInt(line.slice(0, space), 16), line.slice(space + 1))
    }
    const subsets = SUBSETS.map((s) => {
      const chars: string[] = []
      for (let cp = s.from; cp <= s.to; cp += 1) if (map.has(cp)) chars.push(String.fromCodePoint(cp))
      return { id: s.id, zh: s.zh, en: s.en, chars }
    })
    return { names: map, subsets }
  })
  return names
}

const COLUMNS = 16
const code = (symbol: string) => (symbol.codePointAt(0) ?? 0).toString(16).toUpperCase().padStart(4, '0')

/**
 * The Symbol dialog (WPS's 符号): the 符号 tab (a search by name on Enter, the font the grid shows
 * the symbols in, the Unicode subset, a grid of 16 columns, the symbols used last, the chosen
 * symbol's code and name) and the 特殊字符 tab (named characters). A click chooses a symbol, a
 * double-click or 插入 puts it in; the dialog stays open until 关闭 or its ×.
 */
export function SymbolDialog() {
  const { t } = useKitEditor()
  // In the dialog the keyboard stays there; it goes back to the grid when the dialog closes.
  const insert = useInsertSymbol(false)
  const [count, setCount] = useState(opened)
  const [shown, setShown] = useState(false)
  const [tab, setTab] = useState<'symbols' | 'special'>('symbols')
  const [data, setData] = useState<Names | null>(null)
  const [font, setFont] = useState('')
  const [subset, setSubset] = useState('basicLatin')
  const [typed, setTyped] = useState('')
  const [search, setSearch] = useState('')
  const [chosen, setChosen] = useState<string | null>(null)
  const [recent, setRecent] = useState<string[]>([])
  useEffect(() => {
    listeners.add(setCount)
    return () => {
      listeners.delete(setCount)
    }
  }, [])
  useEffect(() => {
    if (count === 0) return
    setShown(true)
    setTab('symbols')
    setFont('')
    setSubset('basicLatin')
    setTyped('')
    setSearch('')
    setChosen(null)
    setRecent(recentSymbols())
    void loadNames().then(setData)
  }, [count])
  const subsetName = (s: { zh: string; en: string }) => (t('symbolSubsetNames') === 'zh' ? s.zh : s.en)
  const grid = useMemo(() => {
    if (!data) return []
    const query = search.trim().toLowerCase()
    if (query) {
      const found: string[] = []
      for (const s of data.subsets) for (const ch of s.chars) if (data.names.get(ch.codePointAt(0) ?? 0)?.toLowerCase().includes(query)) found.push(ch)
      return found
    }
    return data.subsets.find((s) => s.id === subset)?.chars ?? []
  }, [data, subset, search])
  const special = useMemo(() => {
    const query = search.trim().toLowerCase()
    return SPECIAL.filter(([ch, key]) => !query || t(key).toLowerCase().includes(query) || (data?.names.get(ch.codePointAt(0) ?? 0)?.toLowerCase().includes(query) ?? false))
  }, [search, data, t])
  if (!shown) return null
  const close = () => setShown(false)
  const put = async (symbol: string | null) => {
    if (!symbol) return
    await insert(symbol)
    setRecent(recentSymbols())
  }
  const css = FONTS.find((f) => f.name === font)?.css
  const nameOf = (symbol: string) => data?.names.get(symbol.codePointAt(0) ?? 0) ?? ''
  const searchKey = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key !== 'Escape') event.stopPropagation()
    if (event.key === 'Enter') {
      event.preventDefault()
      setSearch(typed)
    }
  }
  const symbolButton = (symbol: string, testId: string) => (
    <button
      key={`${testId}-${symbol}`}
      type="button"
      role="gridcell"
      className="sheet-symbol-cell"
      aria-selected={symbol === chosen}
      title={nameOf(symbol)}
      style={css ? { fontFamily: css } : undefined}
      onClick={() => setChosen(symbol)}
      onDoubleClick={() => void put(symbol)}
      data-symbol={symbol}
      data-testid={testId}
    >
      {symbol}
    </button>
  )
  return (
    <Modal open onOpenChange={(open) => (open ? undefined : close())}>
      <ModalContent size="lg" dismissible={false} aria-describedby={undefined} onCloseAutoFocus={(event) => { event.preventDefault(); backToGrid() }} data-testid="sheet-symbol-dialog">
        <ModalHeader>
          <ModalTitle>{t('symbolTitle')}</ModalTitle>
        </ModalHeader>
        <ModalBody>
          <div className="sheet-fc-tabs" role="tablist">
            {(['symbols', 'special'] as const).map((id) => (
              <button key={id} type="button" role="tab" aria-selected={tab === id} onClick={() => setTab(id)} data-testid={`sheet-symbol-tab-${id}`}>
                {t(id === 'symbols' ? 'symbolTabSymbols' : 'symbolTabSpecial')}
              </button>
            ))}
          </div>
          <div className="sheet-symbol-controls">
            <input
              className="kui-field sheet-symbol-search"
              type="search"
              value={typed}
              placeholder={t('symbolSearch')}
              aria-label={t('symbolSearch')}
              onChange={(event) => {
                setTyped(event.target.value)
                if (!event.target.value) setSearch('')
              }}
              onKeyDown={searchKey}
              data-testid="sheet-symbol-search"
            />
            <span className="sheet-dialog-row">
              <span>{t('symbolFont')}</span>
              <select className="kui-field" value={font} onChange={(event) => setFont(event.target.value)} data-testid="sheet-symbol-font">
                <option value="">{t('symbolNormalText')}</option>
                {FONTS.map((f) => (
                  <option key={f.name} value={f.name}>
                    {f.name}
                  </option>
                ))}
              </select>
            </span>
            <span className="sheet-dialog-row">
              <span>{t('symbolSubset')}</span>
              <select className="kui-field" value={subset} disabled={!data || tab !== 'symbols'} onChange={(event) => {
                setSubset(event.target.value)
                setSearch('')
                setTyped('')
              }} data-testid="sheet-symbol-subset">
                {(data?.subsets ?? [{ id: 'basicLatin', zh: '基本拉丁字母', en: 'Basic Latin' }]).map((s) => (
                  <option key={s.id} value={s.id}>
                    {subsetName(s)}
                  </option>
                ))}
              </select>
            </span>
          </div>
          {tab === 'symbols' ? (
            <div className="sheet-symbol-dialog-grid" role="grid" style={{ gridTemplateColumns: `repeat(${COLUMNS}, 1fr)` }} data-testid="sheet-symbol-grid">
              {grid.map((symbol) => symbolButton(symbol, 'sheet-symbol-grid-item'))}
            </div>
          ) : (
            <div className="sheet-symbol-special" role="listbox" data-testid="sheet-symbol-special">
              {special.map(([symbol, key]) => (
                <button
                  key={key}
                  type="button"
                  role="option"
                  aria-selected={symbol === chosen}
                  className="sheet-symbol-special-row"
                  onClick={() => setChosen(symbol)}
                  onDoubleClick={() => void put(symbol)}
                  data-symbol={symbol}
                  data-testid="sheet-symbol-special-item"
                >
                  <span className="sheet-symbol-special-char" style={css ? { fontFamily: css } : undefined}>
                    {symbol.trim() ? symbol : '·'}
                  </span>
                  <span>{t(key)}</span>
                </button>
              ))}
            </div>
          )}
          <div className="sheet-symbol-heading">{t('symbolRecentDialog')}</div>
          <div className="sheet-symbol-dialog-grid sheet-symbol-recent" style={{ gridTemplateColumns: `repeat(${COLUMNS}, 1fr)` }} data-testid="sheet-symbol-dialog-recent">
            {recent.map((symbol) => symbolButton(symbol, 'sheet-symbol-dialog-recent-item'))}
          </div>
          <div className="sheet-symbol-info" data-testid="sheet-symbol-info">
            <span data-testid="sheet-symbol-name">{chosen ? nameOf(chosen) : ''}</span>
            <span>
              {t('symbolCode')}
              <output data-testid="sheet-symbol-code">{chosen ? code(chosen) : ''}</output>
            </span>
            <span data-testid="sheet-symbol-from">{t('symbolFrom')}</span>
          </div>
        </ModalBody>
        <ModalFooter className="sheet-dialog-actions">
          <Button variant="primary" disabled={!chosen} onClick={() => void put(chosen)} data-testid="sheet-symbol-insert">
            {t('symbolInsert')}
          </Button>
          <Button variant="secondary" onClick={close} data-testid="sheet-symbol-close">
            {t('symbolClose')}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
