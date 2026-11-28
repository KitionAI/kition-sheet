import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, ModalTitle, useKitEditor } from '@kition/sdk/editor'
import { backToGrid } from './sheet'

/**
 * Custom Sort (plan "Sheet usable by hand" stage 4B; walkthrough S2; Excel for Mac's Sort, WPS's
 * 自定义排序): the range the engine chose (the data round the active cell, editable), whether its
 * first row is headers (the engine's guess), and one row per level, a column (named by its header)
 * and an order; levels are added, deleted and moved; OK runs one `data.sort`, one undo step.
 *
 * Every entry asks the engine first (`data.sort` with no keys): it answers the range and the
 * header guess as its `sort` dialog (App.tsx), so neither is worked out here.
 */
export type SortAsk = { range: string; header: boolean; active: string }
type Level = { column: string; order: 'asc' | 'desc' }

let current: SortAsk | null = null
const listeners = new Set<(ask: SortAsk | null) => void>()
const show = (ask: SortAsk | null) => {
  current = ask
  for (const listener of listeners) listener(ask)
}
export const openSortDialog = (ask: SortAsk) => show(ask)

/** The engine's sort levels are at most 64. */
const MAX_LEVELS = 64

const letters = (col: number) => {
  let out = ''
  for (let n = col + 1; n > 0; n = Math.floor((n - 1) / 26)) out = String.fromCharCode(65 + ((n - 1) % 26)) + out
  return out
}
const colIndex = (name: string) => [...name.toUpperCase()].reduce((n, ch) => n * 26 + ch.charCodeAt(0) - 64, 0) - 1

/** A range's columns and first row (`Sheet1!V1:Y7` → V…Y, 1); null when it is no plain range. */
function shape(range: string): { columns: string[]; row: number } | null {
  const match = /^(?:.*!)?\$?([A-Za-z]{1,3})\$?(\d+)(?::\$?([A-Za-z]{1,3})\$?(\d+))?$/.exec(range.trim())
  if (!match) return null
  const first = colIndex(match[1])
  const last = match[3] ? colIndex(match[3]) : first
  const row = Math.min(Number(match[2]), match[4] ? Number(match[4]) : Number(match[2]))
  const [from, to] = first <= last ? [first, last] : [last, first]
  const columns: string[] = []
  for (let col = from; col <= to && columns.length < 256; col += 1) columns.push(letters(col))
  return { columns, row }
}
const columnOf = (cell: string) => /^\$?([A-Za-z]+)/.exec(cell)?.[1]?.toUpperCase() ?? ''

export function SortDialog() {
  const { run, query, t } = useKitEditor()
  const [ask, setAsk] = useState<SortAsk | null>(current)
  const [range, setRange] = useState('')
  const [header, setHeader] = useState(true)
  const [matchCase, setMatchCase] = useState(false)
  const [levels, setLevels] = useState<Level[]>([])
  const [headers, setHeaders] = useState<Record<string, string>>({})
  const [read, setRead] = useState<{ columns: string[]; row: number } | null>(null)
  const busy = useRef(false)
  const first = useRef<HTMLSelectElement>(null)
  useEffect(() => {
    listeners.add(setAsk)
    return () => {
      listeners.delete(setAsk)
    }
  }, [])

  /** Reads the range's first row: the headers' text, column by column (64 at most; the rest by letter). */
  const readRange = async (value: string) => {
    const found = shape(value)
    setRead(found)
    if (!found) return found
    const named = await Promise.all(
      found.columns.slice(0, MAX_LEVELS).map(async (column) => {
        const cell = await query<{ text?: string; value?: unknown }>('cell.get', { cell: `${column}${found.row}` })
        return [column, typeof cell?.text === 'string' ? cell.text : cell?.value != null ? String(cell.value) : ''] as const
      }),
    )
    setHeaders(Object.fromEntries(named))
    return found
  }
  useEffect(() => {
    if (!ask) return
    setRange(ask.range)
    setHeader(ask.header)
    setMatchCase(false)
    setHeaders({})
    const found = shape(ask.range)
    const active = columnOf(ask.active)
    setLevels([{ column: found?.columns.includes(active) ? active : (found?.columns[0] ?? active), order: 'asc' }])
    void readRange(ask.range)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per opening
  }, [ask])

  if (!ask) return null
  const columns = read?.columns ?? []
  const label = (column: string) => (header && headers[column] ? headers[column] : t('sortColumn', { column }))
  const close = () => show(null)
  const used = new Set<string>()
  const twice = levels.some((level) => (used.has(level.column) ? true : (used.add(level.column), false)))
  const setLevel = (index: number, change: Partial<Level>) => setLevels((was) => was.map((level, at) => (at === index ? { ...level, ...change } : level)))
  const add = () =>
    setLevels((was) => {
      const free = columns.find((column) => !was.some((level) => level.column === column))
      return free && was.length < MAX_LEVELS ? [...was, { column: free, order: 'asc' }] : was
    })
  const remove = (index: number) => setLevels((was) => (was.length > 1 ? was.filter((_, at) => at !== index) : was))
  const swap = (index: number, by: -1 | 1) =>
    setLevels((was) => {
      const other = index + by
      if (other < 0 || other >= was.length) return was
      const next = [...was]
      ;[next[index], next[other]] = [next[other], next[index]]
      return next
    })
  const canAdd = levels.length < MAX_LEVELS && columns.some((column) => !levels.some((level) => level.column === column))
  const ok = async () => {
    if (busy.current || twice || !levels.length || !range.trim()) return
    busy.current = true
    try {
      const answer = await run('data.sort', { range: range.trim(), header, keys: levels.map((level) => ({ column: level.column, order: level.order })), ...(matchCase ? { matchCase: true } : {}) })
      // A sort the engine refused (merged cells of different sizes, a range it cannot read) says
      // why in a notice; the dialog stays.
      if (answer != null) close()
    } finally {
      busy.current = false
    }
  }
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'Enter' || event.nativeEvent.isComposing || event.keyCode === 229 || event.target instanceof HTMLButtonElement) return
    event.preventDefault()
    void ok()
  }

  return (
    <Modal open onOpenChange={(open) => (open ? undefined : close())}>
      <ModalContent
        size="md"
        dismissible={false}
        aria-describedby={undefined}
        onOpenAutoFocus={(event) => {
          event.preventDefault()
          window.requestAnimationFrame(() => first.current?.focus())
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault()
          backToGrid()
        }}
        onKeyDown={keyDown}
        data-testid="sheet-sort-dialog"
      >
        <ModalHeader>
          <ModalTitle>{t('sortTitle')}</ModalTitle>
        </ModalHeader>
        <ModalBody>
          <div className="sheet-dialog-form">
            <label className="sheet-dialog-row">
              <span>{t('sortRange')}</span>
              <input
                className="kui-field"
                type="text"
                value={range}
                spellCheck={false}
                onChange={(event) => setRange(event.target.value)}
                onBlur={() => void readRange(range)}
                data-testid="sheet-sort-range"
              />
            </label>
            <div className="sheet-sort-options">
              <label className="sheet-dialog-choice">
                <input type="checkbox" checked={header} onChange={(event) => setHeader(event.target.checked)} data-testid="sheet-sort-header" />
                {t('sortHeader')}
              </label>
              <label className="sheet-dialog-choice">
                <input type="checkbox" checked={matchCase} onChange={(event) => setMatchCase(event.target.checked)} data-testid="sheet-sort-case" />
                {t('sortCase')}
              </label>
            </div>
            <div className="sheet-sort-levels" role="list">
              <span className="sheet-sort-head" />
              <span className="sheet-sort-head">{t('sortColumnHead')}</span>
              <span className="sheet-sort-head">{t('sortOrder')}</span>
              <span className="sheet-sort-head" />
              {levels.map((level, index) => (
                <div className="sheet-sort-level" role="listitem" key={index} data-testid={`sheet-sort-level-${index}`}>
                  <span>{index === 0 ? t('sortBy') : t('sortThenBy')}</span>
                  <select ref={index === 0 ? first : undefined} className="kui-field" value={level.column} aria-label={t('sortColumnHead')} onChange={(event) => setLevel(index, { column: event.target.value })} data-testid={`sheet-sort-column-${index}`}>
                    {(columns.includes(level.column) ? columns : [level.column, ...columns]).map((column) => (
                      <option key={column} value={column}>
                        {label(column)}
                      </option>
                    ))}
                  </select>
                  <select className="kui-field" value={level.order} aria-label={t('sortOrder')} onChange={(event) => setLevel(index, { order: event.target.value === 'desc' ? 'desc' : 'asc' })} data-testid={`sheet-sort-order-${index}`}>
                    <option value="asc">{t('sortAscending')}</option>
                    <option value="desc">{t('sortDescending')}</option>
                  </select>
                  <span className="sheet-sort-moves">
                    <Button variant="secondary" disabled={index === 0} onClick={() => swap(index, -1)} aria-label={t('sortUp')} title={t('sortUp')} data-testid={`sheet-sort-up-${index}`}>
                      ↑
                    </Button>
                    <Button variant="secondary" disabled={index === levels.length - 1} onClick={() => swap(index, 1)} aria-label={t('sortDown')} title={t('sortDown')} data-testid={`sheet-sort-down-${index}`}>
                      ↓
                    </Button>
                    <Button variant="secondary" disabled={levels.length < 2} onClick={() => remove(index)} aria-label={t('sortRemove')} title={t('sortRemove')} data-testid={`sheet-sort-remove-${index}`}>
                      ×
                    </Button>
                  </span>
                </div>
              ))}
            </div>
            <div className="sheet-dialog-row">
              <Button variant="secondary" disabled={!canAdd} onClick={add} data-testid="sheet-sort-add">
                {t('sortAdd')}
              </Button>
              {twice ? (
                <span className="sheet-sort-note" data-testid="sheet-sort-twice">
                  {t('sortSameColumn')}
                </span>
              ) : null}
            </div>
          </div>
        </ModalBody>
        <ModalFooter className="sheet-dialog-actions">
          <Button variant="primary" disabled={twice || !levels.length || !range.trim()} onClick={() => void ok()} data-testid="sheet-sort-ok">
            {t('ok')}
          </Button>
          <Button variant="secondary" onClick={close} data-testid="sheet-sort-cancel">
            {t('cancel')}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
