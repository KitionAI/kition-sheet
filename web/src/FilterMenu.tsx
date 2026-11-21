import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react'
import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, ModalTitle, useKitEditor } from '@kition/sdk/editor'
import { toScreen, type Pins } from './commentAnchors'
import type { ViewStore } from './HostComments'
import { backToGrid } from './sheet'

/** A filter button pressed (`dialog` `filterMenu`): its header cell and the button's rectangle (document pixels). */
export type FilterAsk = { cell: string; rect: [number, number, number, number]; cellRect: [number, number, number, number] }

/** What `data.filterValues` answers for the column. */
type FilterColumn = {
  range: string
  table: string | null
  column: string
  header: string
  kind: 'text' | 'number' | 'date'
  values: Array<{ text: string; count: number; checked: boolean }>
  blanks: { count: number; checked: boolean } | null
  criterion: { type: string; op?: string; value?: string; op2?: string | null; value2?: string | null; and?: boolean } | null
}

/** A comparison of the text, number or date conditions (`data.filterBy`'s `custom` operators). */
type Condition = { key: string; op: string; op2?: string }
const TEXT_CONDITIONS: Condition[] = [
  { key: 'filterEquals', op: '=' },
  { key: 'filterNotEquals', op: '<>' },
  { key: 'filterBeginsWith', op: 'beginsWith' },
  { key: 'filterEndsWith', op: 'endsWith' },
  { key: 'filterContains', op: 'contains' },
  { key: 'filterNotContains', op: 'notContains' },
]
const NUMBER_CONDITIONS: Condition[] = [
  { key: 'filterEquals', op: '=' },
  { key: 'filterNotEquals', op: '<>' },
  { key: 'filterGreater', op: '>' },
  { key: 'filterGreaterOrEqual', op: '>=' },
  { key: 'filterLess', op: '<' },
  { key: 'filterLessOrEqual', op: '<=' },
  { key: 'filterBetween', op: '>=', op2: '<=' },
]
const OPERATORS = ['=', '<>', '>', '>=', '<', '<=', 'beginsWith', 'endsWith', 'contains', 'notContains']
const OPERATOR_KEYS: Record<string, string> = {
  '=': 'filterEquals',
  '<>': 'filterNotEquals',
  '>': 'filterGreater',
  '>=': 'filterGreaterOrEqual',
  '<': 'filterLess',
  '<=': 'filterLessOrEqual',
  beginsWith: 'filterBeginsWith',
  endsWith: 'filterEndsWith',
  contains: 'filterContains',
  notContains: 'filterNotContains',
}

let asked: FilterAsk | null = null
const listeners = new Set<(ask: FilterAsk | null) => void>()
/** Opens a filter button's list (or closes it with `null`). */
export const openFilterMenu = (ask: FilterAsk | null) => {
  asked = ask
  for (const listener of listeners) listener(ask)
}

type Custom = { op: string; value: string; op2: string; value2: string; and: boolean }
type Top = { bottom: boolean; count: string; percent: boolean }

/**
 * A filter button's list, as WPS shows it under the button: sort the filter's rows by the column
 * (升序, 降序), tick the values to show (with a search box, 全选 and the blanks), or pick a text,
 * number or date condition (a dialog for its value; 前十项; above or below the average); 清空条件
 * shows every row again. The sheet's AutoFilter and a table's header buttons alike.
 */
export function FilterMenu({ views, pins }: { views: ViewStore; pins: Pins }) {
  const { run, t } = useKitEditor()
  const view = useSyncExternalStore(views.subscribe, views.get)
  const [ask, setAsk] = useState<FilterAsk | null>(asked)
  const [column, setColumn] = useState<FilterColumn | null>(null)
  const [checked, setChecked] = useState<Set<string>>(new Set())
  const [blanks, setBlanks] = useState(true)
  const [search, setSearch] = useState('')
  const [conditions, setConditions] = useState(false)
  const [custom, setCustom] = useState<Custom | null>(null)
  const [top, setTop] = useState<Top | null>(null)
  const panel = useRef<HTMLDivElement>(null)

  useEffect(() => {
    listeners.add(setAsk)
    return () => {
      listeners.delete(setAsk)
    }
  }, [])
  // The column's values, each time a button opens the list.
  useEffect(() => {
    setColumn(null)
    setSearch('')
    setConditions(false)
    if (!ask) return
    let live = true
    void run('data.filterValues', { cell: ask.cell }).then((raw) => {
      if (!live || !raw) return
      const answer = JSON.parse(raw) as FilterColumn
      setColumn(answer)
      setChecked(new Set(answer.values.filter((value) => value.checked).map((value) => value.text)))
      setBlanks(answer.blanks?.checked ?? true)
    })
    return () => {
      live = false
    }
  }, [ask, run])
  // A press outside the list, or Escape, closes it.
  useEffect(() => {
    if (!ask || custom || top) return
    const away = (event: PointerEvent) => {
      if (panel.current && event.target instanceof Node && !panel.current.contains(event.target)) close()
    }
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        close()
      }
    }
    window.addEventListener('pointerdown', away, true)
    window.addEventListener('keydown', key, true)
    return () => {
      window.removeEventListener('pointerdown', away, true)
      window.removeEventListener('keydown', key, true)
    }
  })

  const shown = useMemo(() => {
    if (!column) return []
    const q = search.trim().toLowerCase()
    return q ? column.values.filter((value) => value.text.toLowerCase().includes(q)) : column.values
  }, [column, search])

  function close() {
    openFilterMenu(null)
    backToGrid()
  }
  const filterBy = async (params: Record<string, unknown>) => {
    if (!ask) return
    close()
    await run('data.filterBy', { cell: ask.cell, ...params })
  }
  const sort = async (order: 'asc' | 'desc') => {
    if (!column) return
    close()
    await run('data.sort', { range: column.range, header: true, keys: [{ column: column.column, order }] })
  }
  const apply = () => {
    if (!column) return
    const searching = search.trim() !== ''
    const values = shown.filter((value) => checked.has(value.text)).map((value) => value.text)
    const withBlanks = column.blanks != null && blanks && !searching
    const all = !searching && values.length === column.values.length && (column.blanks == null || blanks)
    void filterBy(all ? { clear: true } : { values, blanks: withBlanks })
  }

  if (!ask || !view) return null
  const at = toScreen(view, pins, ask.cellRect[0], ask.cellRect[3])
  const allShown = shown.length > 0 && shown.every((value) => checked.has(value.text)) && (search.trim() !== '' || column?.blanks == null || blanks)
  const toggleAll = () => {
    const next = new Set(checked)
    for (const value of shown) {
      if (allShown) next.delete(value.text)
      else next.add(value.text)
    }
    setChecked(next)
    if (!search.trim()) setBlanks(!allShown)
  }
  const kindLabel = column?.kind === 'number' ? t('filterNumber') : column?.kind === 'date' ? t('filterDate') : t('filterText')
  const choices = column?.kind === 'text' ? TEXT_CONDITIONS : NUMBER_CONDITIONS
  const label = (value: string) => (value === '' ? t('filterEmptyText') : value)
  return (
    <>
      <div
        ref={panel}
        className="sheet-filter-menu"
        style={{ left: Math.max(0, at.x), top: at.y + 1 }}
        role="dialog"
        aria-label={t('filterTitle')}
        data-testid="sheet-filter-menu"
        onPointerDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && !(event.target instanceof HTMLButtonElement)) apply()
          event.stopPropagation()
        }}
      >
        <div className="sheet-filter-sort">
          <button type="button" onClick={() => void sort('asc')} data-testid="sheet-filter-asc">
            {t('filterAscending')}
          </button>
          <button type="button" onClick={() => void sort('desc')} data-testid="sheet-filter-desc">
            {t('filterDescending')}
          </button>
        </div>
        <div className="sheet-filter-tabs">
          <span className="sheet-filter-tab" aria-current="true">
            {t('filterContent')}
          </span>
          <button type="button" className="sheet-filter-tab" aria-expanded={conditions} onClick={() => setConditions(!conditions)} data-testid="sheet-filter-conditions">
            {kindLabel} ▾
          </button>
        </div>
        {conditions ? (
          <div className="sheet-filter-conditions" role="menu" data-testid="sheet-filter-condition-list">
            {choices.map((choice) => (
              <button
                key={choice.key}
                type="button"
                role="menuitem"
                onClick={() => {
                  setConditions(false)
                  setCustom({ op: choice.op, value: '', op2: choice.op2 ?? '=', value2: '', and: true })
                }}
                data-testid={`sheet-filter-cond-${choice.key}`}
              >
                {t(choice.key)}…
              </button>
            ))}
            {column?.kind !== 'text' ? (
              <>
                <button type="button" role="menuitem" onClick={() => setTop({ bottom: false, count: '10', percent: false })} data-testid="sheet-filter-cond-top">
                  {t('filterTop')}…
                </button>
                <button type="button" role="menuitem" onClick={() => void filterBy({ aboveAverage: true })} data-testid="sheet-filter-cond-above">
                  {t('filterAboveAverage')}
                </button>
                <button type="button" role="menuitem" onClick={() => void filterBy({ aboveAverage: false })} data-testid="sheet-filter-cond-below">
                  {t('filterBelowAverage')}
                </button>
              </>
            ) : null}
            <button type="button" role="menuitem" onClick={() => setCustom({ op: '=', value: '', op2: '=', value2: '', and: true })} data-testid="sheet-filter-cond-custom">
              {t('filterCustom')}…
            </button>
          </div>
        ) : null}
        <input
          className="sheet-filter-search"
          type="search"
          value={search}
          placeholder={t('filterSearch')}
          onChange={(event) => setSearch(event.target.value)}
          data-testid="sheet-filter-search"
        />
        <div className="sheet-filter-list" role="listbox" aria-multiselectable="true" data-testid="sheet-filter-list">
          {column ? (
            <>
              <label className="sheet-filter-item">
                <input type="checkbox" checked={allShown} onChange={toggleAll} data-testid="sheet-filter-all" />
                {search.trim() ? t('filterAllResults') : t('filterAll')}
              </label>
              {shown.map((value) => (
                <label key={value.text} className="sheet-filter-item">
                  <input
                    type="checkbox"
                    checked={checked.has(value.text)}
                    onChange={() => {
                      const next = new Set(checked)
                      if (next.has(value.text)) next.delete(value.text)
                      else next.add(value.text)
                      setChecked(next)
                    }}
                    data-testid={`sheet-filter-item-${value.text}`}
                  />
                  <span className="sheet-filter-text">{label(value.text)}</span>
                  <span className="sheet-filter-count">({value.count})</span>
                </label>
              ))}
              {column.blanks && !search.trim() ? (
                <label className="sheet-filter-item">
                  <input type="checkbox" checked={blanks} onChange={() => setBlanks(!blanks)} data-testid="sheet-filter-blanks" />
                  <span className="sheet-filter-text">{t('filterBlanks')}</span>
                  <span className="sheet-filter-count">({column.blanks.count})</span>
                </label>
              ) : null}
              {!shown.length && search.trim() ? <div className="sheet-filter-none">{t('filterNoMatch')}</div> : null}
            </>
          ) : null}
        </div>
        <div className="sheet-filter-actions">
          <button type="button" className="sheet-filter-clear" onClick={() => void filterBy({ clear: true })} disabled={!column?.criterion} data-testid="sheet-filter-clear">
            {t('filterClear')}
          </button>
          <span className="sheet-filter-spacer" />
          <Button variant="primary" onClick={apply} disabled={!column} data-testid="sheet-filter-ok">
            {t('ok')}
          </Button>
          <Button variant="secondary" onClick={close} data-testid="sheet-filter-cancel">
            {t('cancel')}
          </Button>
        </div>
      </div>
      {custom ? (
        <Modal open onOpenChange={(open) => (open ? undefined : setCustom(null))}>
          <ModalContent size="sm" dismissible={false} aria-describedby={undefined} data-testid="sheet-filter-custom">
            <ModalHeader>
              <ModalTitle>{t('filterCustomTitle')}</ModalTitle>
            </ModalHeader>
            <ModalBody>
              <p className="sheet-filter-custom-label">{t('filterShowRows', { column: column?.header || column?.column || '' })}</p>
              <div className="sheet-filter-custom-row">
                <select value={custom.op} onChange={(event) => setCustom({ ...custom, op: event.target.value })} data-testid="sheet-filter-custom-op">
                  {OPERATORS.map((op) => (
                    <option key={op} value={op}>
                      {t(OPERATOR_KEYS[op] ?? 'filterEquals')}
                    </option>
                  ))}
                </select>
                <input value={custom.value} onChange={(event) => setCustom({ ...custom, value: event.target.value })} autoFocus data-testid="sheet-filter-custom-value" />
              </div>
              <div className="sheet-filter-custom-join">
                <label>
                  <input type="radio" checked={custom.and} onChange={() => setCustom({ ...custom, and: true })} data-testid="sheet-filter-custom-and" />
                  {t('filterAnd')}
                </label>
                <label>
                  <input type="radio" checked={!custom.and} onChange={() => setCustom({ ...custom, and: false })} data-testid="sheet-filter-custom-or" />
                  {t('filterOr')}
                </label>
              </div>
              <div className="sheet-filter-custom-row">
                <select value={custom.op2} onChange={(event) => setCustom({ ...custom, op2: event.target.value })} data-testid="sheet-filter-custom-op2">
                  {OPERATORS.map((op) => (
                    <option key={op} value={op}>
                      {t(OPERATOR_KEYS[op] ?? 'filterEquals')}
                    </option>
                  ))}
                </select>
                <input value={custom.value2} onChange={(event) => setCustom({ ...custom, value2: event.target.value })} data-testid="sheet-filter-custom-value2" />
              </div>
            </ModalBody>
            <ModalFooter className="sheet-dialog-actions">
              <Button
                variant="primary"
                disabled={!custom.value.trim()}
                onClick={() => {
                  const second = custom.value2.trim() ? { op2: custom.op2, value2: custom.value2.trim() } : {}
                  setCustom(null)
                  void filterBy({ custom: { op: custom.op, value: custom.value.trim(), ...second, and: custom.and } })
                }}
                data-testid="sheet-filter-custom-ok"
              >
                {t('ok')}
              </Button>
              <Button variant="secondary" onClick={() => setCustom(null)} data-testid="sheet-filter-custom-cancel">
                {t('cancel')}
              </Button>
            </ModalFooter>
          </ModalContent>
        </Modal>
      ) : null}
      {top ? (
        <Modal open onOpenChange={(open) => (open ? undefined : setTop(null))}>
          <ModalContent size="sm" dismissible={false} aria-describedby={undefined} data-testid="sheet-filter-top">
            <ModalHeader>
              <ModalTitle>{t('filterTopTitle')}</ModalTitle>
            </ModalHeader>
            <ModalBody>
              <div className="sheet-filter-custom-row">
                <select value={top.bottom ? 'bottom' : 'top'} onChange={(event) => setTop({ ...top, bottom: event.target.value === 'bottom' })} data-testid="sheet-filter-top-which">
                  <option value="top">{t('filterLargest')}</option>
                  <option value="bottom">{t('filterSmallest')}</option>
                </select>
                <input value={top.count} inputMode="numeric" onChange={(event) => setTop({ ...top, count: event.target.value.replace(/[^0-9]/g, '') })} data-testid="sheet-filter-top-count" />
                <select value={top.percent ? 'percent' : 'items'} onChange={(event) => setTop({ ...top, percent: event.target.value === 'percent' })} data-testid="sheet-filter-top-unit">
                  <option value="items">{t('filterItems')}</option>
                  <option value="percent">{t('filterPercent')}</option>
                </select>
              </div>
            </ModalBody>
            <ModalFooter className="sheet-dialog-actions">
              <Button
                variant="primary"
                disabled={!Number(top.count)}
                onClick={() => {
                  setTop(null)
                  void filterBy({ top: { count: Math.min(500, Number(top.count)), bottom: top.bottom, percent: top.percent } })
                }}
                data-testid="sheet-filter-top-ok"
              >
                {t('ok')}
              </Button>
              <Button variant="secondary" onClick={() => setTop(null)} data-testid="sheet-filter-top-cancel">
                {t('cancel')}
              </Button>
            </ModalFooter>
          </ModalContent>
        </Modal>
      ) : null}
    </>
  )
}
