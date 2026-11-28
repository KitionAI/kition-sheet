import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { Button, Dialog, useKitEditor, useReadOnly } from '@kition/sdk/editor'
import { keyMatches } from './keys'
import { backToGrid } from './sheet'

/**
 * Find and Replace (plan "Sheet usable by hand" stage 4A; walkthrough Q1, Q2, C10; WPS W-O1–W-O4):
 * a small window beside the grid that leaves the sheet usable, with three tabs. Find goes on by
 * rows from the active cell (Enter: Find Next; Shift+Enter: Find Previous), the cell found becomes
 * the active cell and the view follows it; Replace changes the active cell's match and finds the
 * next, Replace All changes every one in one undo step; Go To selects a reference or a name and
 * closes. Escape closes it and gives the grid the keyboard back, the active cell where it was
 * found. Cmd+F, Ctrl+H and Ctrl+G open it at their tab, from the grid (keys.ts) or in it.
 *
 * The window only fills in the commands' values (`edit.find`, `edit.replace`, `edit.goTo`): any
 * entry that asks for one of them (a menu, a ribbon tab's command list, an agent's request for its
 * dialog) opens this window instead of the command's parameters dialog (App.tsx).
 */
export type FindTab = 'find' | 'replace' | 'goTo'
const TABS: FindTab[] = ['find', 'replace', 'goTo']

type Request = { tab: FindTab; seq: number } | null
let current: Request = null
let seq = 0
const listeners = new Set<(request: Request) => void>()
const show = (request: Request) => {
  current = request
  for (const listener of listeners) listener(request)
}
/** Opens the window at `tab`, or brings the open one to it, its first field chosen. */
export const openFind = (tab: FindTab) => show({ tab, seq: ++seq })

/** What the window holds between openings in one session (Excel keeps the last search). */
type Fields = { what: string; with: string; reference: string; matchCase: boolean; wholeCell: boolean; within: 'sheet' | 'workbook'; byColumns: boolean; lookIn: 'formulas' | 'values' }
let kept: Fields = { what: '', with: '', reference: '', matchCase: false, wholeCell: false, within: 'sheet', byColumns: false, lookIn: 'formulas' }
/** Where the window was dragged to (this session only). */
let place: { x: number; y: number } | null = null

type Hit = { sheet: string | null; cell: string; value: string }

/** The window's first place: the grid's top right corner, 12 px in. */
const firstPlace = () => {
  const grid = document.querySelector('.sheet-grid-viewport')?.getBoundingClientRect()
  const width = 470
  if (!grid) return { x: Math.max(0, window.innerWidth - width - 12), y: 96 }
  return { x: Math.max(0, Math.round(grid.right - width - 12)), y: Math.max(0, Math.round(grid.top + 12)) }
}

const composing = (event: KeyboardEvent) => event.nativeEvent.isComposing || event.keyCode === 229

export function FindReplaceDialog() {
  const { session, run, query, t } = useKitEditor()
  const readOnly = useReadOnly()
  const [request, setRequest] = useState<Request>(current)
  const [tab, setTab] = useState<FindTab>('find')
  const [fields, setFields] = useState<Fields>(kept)
  const [status, setStatus] = useState('')
  const [hits, setHits] = useState<Hit[] | null>(null)
  const [at, setAt] = useState(place)
  const busy = useRef(false)
  const box = useRef<HTMLDivElement>(null)
  useEffect(() => {
    listeners.add(setRequest)
    return () => {
      listeners.delete(setRequest)
    }
  }, [])
  const set = (change: Partial<Fields>) =>
    setFields((was) => {
      kept = { ...was, ...change }
      return kept
    })

  // Each request: its tab, and its first field chosen (after the menu that asked has closed and
  // handed the keyboard back to where it was).
  const choose = (which: FindTab) => {
    const focus = () => {
      const field = box.current?.querySelector<HTMLInputElement>(which === 'goTo' ? '[data-testid=sheet-goto-reference]' : '[data-testid=sheet-find-what]')
      field?.focus()
      field?.select()
    }
    window.requestAnimationFrame(focus)
    window.setTimeout(() => {
      if (!box.current?.contains(document.activeElement)) focus()
    }, 80)
  }
  useEffect(() => {
    if (!request) return
    setTab(request.tab)
    setStatus('')
    if (!at) setAt(firstPlace())
    choose(request.tab)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per request
  }, [request?.seq])

  if (!request) return null

  const close = () => {
    show(null)
    setHits(null)
    setStatus('')
    backToGrid()
  }
  const switchTo = (which: FindTab) => {
    setTab(which)
    setStatus('')
    choose(which)
  }
  const findParams = () => ({ what: fields.what, matchCase: fields.matchCase, wholeCell: fields.wholeCell, within: fields.within, ...(tab === 'find' ? { byColumns: fields.byColumns, lookIn: fields.lookIn } : {}) })
  /** One step at a time: a second press waits for the first's answer. */
  const once = async (work: () => Promise<void>) => {
    if (busy.current) return
    busy.current = true
    try {
      await work()
    } finally {
      busy.current = false
    }
  }
  const find = (backward: boolean) =>
    once(async () => {
      if (!fields.what) return
      try {
        const raw = await session.run('edit.find', { ...findParams(), ...(backward ? { backward: true } : {}) })
        const answer = raw ? (JSON.parse(raw) as { count?: number }) : {}
        setStatus(t('findCount', { count: answer.count ?? 1 }))
      } catch {
        setStatus(t('findNone', { what: fields.what }))
      }
    })
  const findAll = () =>
    once(async () => {
      if (!fields.what) return
      const answer = await query<{ count: number; results: Hit[] }>('edit.find', { ...findParams(), all: true })
      setHits(answer?.results ?? [])
      setStatus(answer ? t('findCount', { count: answer.count }) : t('findNone', { what: fields.what }))
    })
  const replace = (all: boolean) =>
    once(async () => {
      if (!fields.what || readOnly) return
      const raw = await run('edit.replace', { what: fields.what, with: fields.with, matchCase: fields.matchCase, wholeCell: fields.wholeCell, within: fields.within, all })
      const replaced = raw ? ((JSON.parse(raw) as { replaced?: number }).replaced ?? 0) : null
      if (replaced == null) return
      if (all) {
        setStatus(replaced > 0 ? t('replaceDone', { count: replaced }) : t('replaceNone'))
        return
      }
      // Replace changes the active cell's match, then goes on to the next one (Excel).
      try {
        const next = await session.run('edit.find', findParams())
        const answer = next ? (JSON.parse(next) as { count?: number }) : {}
        setStatus(replaced > 0 ? t('replaceDone', { count: replaced }) : t('findCount', { count: answer.count ?? 1 }))
      } catch {
        setStatus(replaced > 0 ? t('replaceDone', { count: replaced }) : t('findNone', { what: fields.what }))
      }
    })
  const goTo = () =>
    once(async () => {
      const reference = fields.reference.trim()
      if (!reference) return
      try {
        await session.run('edit.goTo', { reference })
        close()
      } catch {
        setStatus(t('goToInvalid'))
      }
    })
  const pick = (hit: Hit) => void run('select.set', { range: hit.cell, ...(hit.sheet ? { sheet: hit.sheet } : {}) })

  // Keys in the window: its own Cmd+F, Ctrl+H and Ctrl+G (the editor's keys skip a dialog, and the
  // browser's own Find must not open), Enter and Shift+Enter in the fields. An input method's
  // Enter is the input method's.
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (composing(event)) return
    const asked = keyMatches('Cmd+F', event) ? 'find' : keyMatches('Ctrl+H', event) ? 'replace' : keyMatches('Ctrl+G', event) ? 'goTo' : null
    if (asked) {
      event.preventDefault()
      event.stopPropagation()
      switchTo(asked)
      return
    }
    if (event.key !== 'Enter' || event.metaKey || event.ctrlKey || event.altKey || !(event.target instanceof HTMLInputElement) || event.target.type !== 'text') return
    event.preventDefault()
    if (tab === 'goTo') void goTo()
    else void find(event.shiftKey)
  }

  const check = (key: keyof Fields, label: string, testId: string) => (
    <label className="sheet-dialog-choice">
      <input type="checkbox" checked={fields[key] === true} onChange={(event) => set({ [key]: event.target.checked } as Partial<Fields>)} data-testid={testId} />
      {label}
    </label>
  )
  const empty = !fields.what
  const actions =
    tab === 'goTo' ? (
      <>
        <Button variant="primary" disabled={!fields.reference.trim()} onClick={() => void goTo()} data-testid="sheet-goto-ok">
          {t('goToOk')}
        </Button>
        <Button variant="secondary" onClick={close} data-testid="sheet-find-close">
          {t('findClose')}
        </Button>
      </>
    ) : (
      <>
        {tab === 'replace' ? (
          <>
            <Button variant="secondary" disabled={empty || readOnly} onClick={() => void replace(true)} data-testid="sheet-replace-all">
              {t('replaceAll')}
            </Button>
            <Button variant="secondary" disabled={empty || readOnly} onClick={() => void replace(false)} data-testid="sheet-replace-one">
              {t('replaceOne')}
            </Button>
          </>
        ) : (
          <Button variant="secondary" disabled={empty} onClick={() => void findAll()} data-testid="sheet-find-all">
            {t('findAll')}
          </Button>
        )}
        <Button variant="secondary" disabled={empty} onClick={() => void find(true)} data-testid="sheet-find-previous">
          {t('findPrevious')}
        </Button>
        <Button variant="primary" disabled={empty} onClick={() => void find(false)} data-testid="sheet-find-next">
          {t('findNext')}
        </Button>
        <Button variant="secondary" onClick={close} data-testid="sheet-find-close">
          {t('findClose')}
        </Button>
      </>
    )

  return (
    <div className="sheet-find-host" onKeyDown={keyDown}>
      <Dialog
        modal={false}
        title={t('findTitle')}
        testId="sheet-find"
        position={at}
        onMove={(moved) => {
          place = moved
          setAt(moved)
        }}
        onClose={close}
        onEscape={(event) => {
          event.preventDefault()
          close()
        }}
        actions={actions}
      >
        <div className="sheet-find" ref={box}>
          <div className="sheet-fc-tabs" role="tablist">
            {TABS.map((id) => (
              <button key={id} type="button" role="tab" aria-selected={tab === id} onClick={() => switchTo(id)} data-testid={`sheet-find-tab-${id}`}>
                {t(`findTab:${id}`)}
              </button>
            ))}
          </div>
          {tab === 'goTo' ? (
            <div className="sheet-dialog-form" role="tabpanel">
              <label className="sheet-dialog-row">
                <span>{t('goToReference')}</span>
                <input
                  className="kui-field sheet-find-field"
                  type="text"
                  value={fields.reference}
                  placeholder={t('goToHint')}
                  spellCheck={false}
                  onChange={(event) => {
                    set({ reference: event.target.value })
                    setStatus('')
                  }}
                  data-testid="sheet-goto-reference"
                />
              </label>
            </div>
          ) : (
            <div className="sheet-dialog-form" role="tabpanel">
              <label className="sheet-dialog-row">
                <span>{t('findWhat')}</span>
                <input
                  className="kui-field sheet-find-field"
                  type="text"
                  value={fields.what}
                  spellCheck={false}
                  onChange={(event) => {
                    set({ what: event.target.value })
                    setStatus('')
                    setHits(null)
                  }}
                  data-testid="sheet-find-what"
                />
              </label>
              {tab === 'replace' ? (
                <label className="sheet-dialog-row">
                  <span>{t('findWith')}</span>
                  <input className="kui-field sheet-find-field" type="text" value={fields.with} spellCheck={false} onChange={(event) => set({ with: event.target.value })} data-testid="sheet-find-with" />
                </label>
              ) : null}
              <div className="sheet-find-options">
                <label className="sheet-dialog-row">
                  <span>{t('findWithin')}</span>
                  <select className="kui-field" value={fields.within} onChange={(event) => set({ within: event.target.value === 'workbook' ? 'workbook' : 'sheet' })} data-testid="sheet-find-within">
                    <option value="sheet">{t('findWithinSheet')}</option>
                    <option value="workbook">{t('findWithinWorkbook')}</option>
                  </select>
                </label>
                {tab === 'find' ? (
                  <>
                    <label className="sheet-dialog-row">
                      <span>{t('findOrder')}</span>
                      <select className="kui-field" value={fields.byColumns ? 'columns' : 'rows'} onChange={(event) => set({ byColumns: event.target.value === 'columns' })} data-testid="sheet-find-order">
                        <option value="rows">{t('findByRows')}</option>
                        <option value="columns">{t('findByColumns')}</option>
                      </select>
                    </label>
                    <label className="sheet-dialog-row">
                      <span>{t('findLookIn')}</span>
                      <select className="kui-field" value={fields.lookIn} onChange={(event) => set({ lookIn: event.target.value === 'values' ? 'values' : 'formulas' })} data-testid="sheet-find-look-in">
                        <option value="formulas">{t('findFormulas')}</option>
                        <option value="values">{t('findValues')}</option>
                      </select>
                    </label>
                  </>
                ) : null}
                {check('matchCase', t('findCase'), 'sheet-find-case')}
                {check('wholeCell', t('findWhole'), 'sheet-find-whole')}
              </div>
              {tab === 'find' && hits && hits.length > 0 ? (
                <ul className="sheet-function-list sheet-find-results" role="listbox" data-testid="sheet-find-results">
                  {hits.map((hit) => (
                    <li key={`${hit.sheet}!${hit.cell}`} role="option" aria-selected={false} onClick={() => pick(hit)}>
                      <span>{hit.sheet}</span>
                      <span>{hit.cell}</span>
                      <span>{hit.value}</span>
                    </li>
                  ))}
                </ul>
              ) : null}
            </div>
          )}
          <p className="sheet-find-status" aria-live="polite" data-testid="sheet-find-status">
            {status}
          </p>
        </div>
      </Dialog>
    </div>
  )
}
