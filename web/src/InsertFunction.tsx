import { useEffect, useMemo, useRef, useState } from 'react'
import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, ModalTitle, useKitEditor } from '@kition/sdk/editor'
import { backToGrid } from './sheet'
import { functionText } from './strings'

/** A function as `formula.functions` lists it. */
type Listed = { name: string; category: string; signature: string; description: string }

/** What the dialog does with the function chosen, and when it is closed without one. */
type Request = { onChoose: (name: string) => void | Promise<void>; onCancel: () => void }

let current: Request | null = null
const listeners = new Set<(request: Request | null) => void>()
const show = (request: Request | null) => {
  current = request
  for (const listener of listeners) listener(request)
}

/** Opens Insert Function (fx). */
export const openInsertFunction = (request: Request = { onChoose: () => undefined, onCancel: backToGrid }) => show(request)

/** The functions offered first, as WPS's 常用函数 (Most Used). */
const COMMON = ['SUM', 'AVERAGE', 'IF', 'COUNT', 'MAX', 'MIN', 'VLOOKUP', 'SUMIF', 'COUNTIF', 'ROUND', 'IFERROR', 'TODAY']
const CATEGORIES = ['Financial', 'DateTime', 'MathTrig', 'Statistical', 'Lookup', 'Database', 'Text', 'Logical', 'Information', 'Engineering', 'Compatibility', 'Web', 'Cube']

/**
 * Insert Function, as WPS has it: search for a function or choose a category, choose a function
 * (its syntax and what it does show under the list) and OK puts it in the formula at the caret.
 */
export function InsertFunctionDialog() {
  const { query, t } = useKitEditor()
  const [request, setRequest] = useState<Request | null>(current)
  const [all, setAll] = useState<Listed[]>([])
  const [search, setSearch] = useState('')
  const [category, setCategory] = useState('common')
  const [chosen, setChosen] = useState<string | null>(null)
  const list = useRef<HTMLUListElement>(null)
  useEffect(() => {
    listeners.add(setRequest)
    return () => {
      listeners.delete(setRequest)
    }
  }, [])
  useEffect(() => {
    if (!request) return
    setSearch('')
    setCategory('common')
    if (!all.length) void query<Listed[]>('formula.functions', {}).then((found) => setAll(Array.isArray(found) ? found : []))
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per opening
  }, [request])
  const shown = useMemo(() => {
    const needle = search.trim().toLowerCase()
    if (needle) return all.filter((f) => f.name.toLowerCase().includes(needle) || functionText(t, f.name, f.description).toLowerCase().includes(needle))
    if (category === 'common') return COMMON.map((name) => all.find((f) => f.name === name)).filter((f): f is Listed => f != null)
    if (category === 'all') return all
    return all.filter((f) => f.category === category)
  }, [all, category, search, t])
  // The first function is chosen when the list changes.
  useEffect(() => {
    setChosen((was) => (was && shown.some((f) => f.name === was) ? was : (shown[0]?.name ?? null)))
  }, [shown])
  const picked = shown.find((f) => f.name === chosen) ?? null

  if (!request) return null
  const close = () => {
    show(null)
    request.onCancel()
  }
  const choose = (name: string | null) => {
    if (!name) return
    show(null)
    void request.onChoose(name)
  }
  const move = (by: number) => {
    if (!shown.length) return
    const at = Math.max(0, Math.min(shown.length - 1, shown.findIndex((f) => f.name === chosen) + by))
    setChosen(shown[at]?.name ?? null)
    list.current?.querySelector(`[data-index="${at}"]`)?.scrollIntoView({ block: 'nearest' })
  }

  return (
    <Modal open onOpenChange={(open) => (open ? undefined : close())}>
      <ModalContent size="md" dismissible={false} aria-describedby={undefined} onCloseAutoFocus={(event) => event.preventDefault()} data-testid="sheet-insert-function">
        <ModalHeader>
          <ModalTitle>{t('insertFunction')}</ModalTitle>
        </ModalHeader>
        <ModalBody>
          <div className="sheet-function-dialog">
            <label className="sheet-function-row">
              <span>{t('functionSearch')}</span>
              <input
                className="kui-field"
                value={search}
                autoFocus
                spellCheck={false}
                onChange={(event) => setSearch(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
                    event.preventDefault()
                    move(event.key === 'ArrowDown' ? 1 : -1)
                  } else if (event.key === 'Enter') {
                    event.preventDefault()
                    choose(chosen)
                  }
                }}
                data-testid="sheet-insert-function-search"
              />
            </label>
            <label className="sheet-function-row">
              <span>{t('functionCategory')}</span>
              <select className="kui-field" value={category} disabled={search.trim() !== ''} onChange={(event) => setCategory(event.target.value)} data-testid="sheet-insert-function-category">
                {['common', 'all', ...CATEGORIES].map((id) => (
                  <option key={id} value={id}>
                    {t(`fnCategory:${id}`)}
                  </option>
                ))}
              </select>
            </label>
            <span>{t('functionPick')}</span>
            <ul ref={list} className="sheet-function-list" role="listbox" aria-label={t('functionPick')} data-testid="sheet-insert-function-list">
              {shown.map((f, index) => (
                <li key={f.name} role="option" aria-selected={f.name === chosen} data-name={f.name} data-index={index} onClick={() => setChosen(f.name)} onDoubleClick={() => choose(f.name)}>
                  {f.name}
                </li>
              ))}
              {!shown.length && all.length ? <li aria-disabled="true">{t('functionNone')}</li> : null}
            </ul>
            <div className="sheet-function-about" data-testid="sheet-insert-function-about">
              {picked ? (
                <>
                  <b>{picked.signature}</b>
                  <div>{functionText(t, picked.name, picked.description)}</div>
                </>
              ) : null}
            </div>
          </div>
        </ModalBody>
        <ModalFooter className="sheet-dialog-actions">
          <Button variant="primary" disabled={!picked} onClick={() => choose(chosen)} data-testid="sheet-insert-function-ok">
            {t('ok')}
          </Button>
          <Button variant="secondary" onClick={close} data-testid="sheet-insert-function-cancel">
            {t('cancel')}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
