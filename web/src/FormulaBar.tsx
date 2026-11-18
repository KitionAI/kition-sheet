import { useEffect, useRef, useState } from 'react'
import { CheckIcon as Check, ChevronDownIcon as ChevronDown, ChevronUpIcon as ChevronUp, SquareFunctionIcon as FunctionSquare, XIcon as X } from '@kition/tessera-icons/react'
import { IconButton, useKitEditor, useReadOnly } from '@kition/kit-editor'
import { backToGrid, type SheetInfo } from './sheet'
import { FormulaText } from './FormulaText'
import { HintPanel, type FormulaHint } from './FormulaHints'
import { openInsertFunction } from './InsertFunction'

/** Cmd/Ctrl+B, I and U in the formula field: the character format they toggle. */
const FORMAT_SHORTCUTS: Record<string, string> = { b: 'bold', i: 'italic', u: 'underline' }

/** Characters (as the engine counts them) in the first `units` UTF-16 units of `text`. */
const characters = (text: string, units: number) => Array.from(text.slice(0, units)).length

/** What `editor.formula` answers: the cell being typed, and the hint at its caret. */
type BarState = { editing: boolean; pointing: boolean; text: string | null; start: number; end: number; hint: FormulaHint | null }

/** Where Enter, Shift+Enter, Tab, Shift+Tab and ✓ leave the selection. */
type Leave = 'down' | 'up' | 'right' | 'left' | 'stay'

const FIELD = '[data-testid="sheet-formula"]'

/** Keeps a caret at the end of the text in view (WebKit leaves a long line where it was). */
const revealEnd = (target: HTMLTextAreaElement) =>
  requestAnimationFrame(() => {
    if (target.selectionEnd !== target.value.length) return
    target.scrollLeft = target.scrollWidth
    target.scrollTop = target.scrollHeight
  })

/**
 * The formula bar, as WPS and Excel have it: the Name Box (go to a cell or range), ✕, ✓, fx
 * (Insert Function) and the field with the active cell's formula or text, and a button that
 * expands the field to several lines.
 *
 * What is typed in the field is the cell being typed (`editor.formula`): the cell shows it as it
 * is typed, a click on a cell while a formula waits for a reference inserts that reference at the
 * field's caret (on another sheet too) and the field takes the keyboard back to go on typing.
 * Enter enters it and moves down (Shift+Enter up, Tab right, Shift+Tab left), ✓ enters it and
 * stays, Escape and ✕ drop it, Alt+Enter (Option+Return) breaks the line. Leaving the field for
 * the grid keeps the typing in the cell. Cmd/Ctrl+B, I or U with characters of a text selected
 * formats just them (rich text, `cell.formatText`).
 */
export function FormulaBar({ info }: { info: SheetInfo }) {
  const { run, t } = useKitEditor()
  // A viewer can go to cells and read formulas, not enter them.
  const viewer = useReadOnly()
  // A checkbox cell is not typed in (WPS): the field shows its TRUE or FALSE and stays shut.
  const readOnly = viewer || (info.checkbox && !info.editing)
  const [reference, setReference] = useState<string | null>(null)
  const field = useRef<HTMLTextAreaElement>(null)
  // What the person is typing here, while the field has the keyboard; otherwise the field shows
  // the cell being typed (`editText`, which pointing changes) or the active cell's content.
  const [draft, setDraft] = useState<string | null>(null)
  const focused = useRef(false)
  // The typing began (or went on) in the field: after pointing at a cell the field takes the
  // keyboard back. Typing in the cell, or the typing ending, ends it.
  const owns = useRef(false)
  const [expanded, setExpanded] = useState(false)
  const text = draft ?? (info.editing ? (info.editText ?? '') : info.content)
  const references = info.formulaText === text ? info.formulaReferences : []
  const [scroll, setScroll] = useState({ left: 0, top: 0 })
  // The function list and argument hint for the formula being typed here, and the function chosen.
  const [hint, setHint] = useState<FormulaHint | null>(null)
  const [pick, setPick] = useState(0)

  /** The field's text and selection become the cell being typed's; the hint follows the caret. */
  const sent = useRef('')
  const send = (value: string, start: number, end: number) => {
    owns.current = true
    const key = `${start}:${end}:${value}`
    if (key === sent.current) return
    sent.current = key
    void run('editor.formula', { text: value, start, end }).then((answer) => {
      if (!focused.current) return
      const state = answer ? (JSON.parse(answer) as BarState) : null
      setHint(state?.hint ?? null)
    })
  }
  /** The text and the caret set together (F4, a function from the list, Alt+Enter). */
  const replaceDraft = (target: HTMLTextAreaElement, value: string, caret: number) => {
    // The field changes at once, so a key typed straight after goes in at the new caret.
    target.value = value
    target.setSelectionRange(caret, caret)
    revealEnd(target)
    setDraft(value)
    setPick(0)
    send(value, caret, caret)
  }
  /** `NAME(` in place of the function name being typed. */
  const acceptFunction = (target: HTMLTextAreaElement, name: string) => {
    if (!hint?.replace) return
    const [from, to] = hint.replace
    const rest = target.value.slice(to)
    replaceDraft(target, target.value.slice(0, from) + name + '(' + (rest.startsWith('(') ? rest.slice(1) : rest), from + name.length + 1)
  }
  /** Ends here: the keyboard goes back to the grid. */
  const release = () => {
    owns.current = false
    sent.current = ''
    setDraft(null)
    setHint(null)
    field.current?.blur()
    backToGrid()
  }
  /** Enters the typing (or, with nothing typed, moves as Enter does). */
  const finish = (leave: Leave) => {
    const target = field.current
    const params: Record<string, unknown> = { enter: leave }
    if (owns.current && draft != null && target) Object.assign(params, { text: draft, start: target.selectionStart, end: target.selectionEnd })
    release()
    void run('editor.formula', params)
  }
  const cancel = () => {
    release()
    void run('editor.formula', {})
  }
  /** Takes the keyboard back to go on typing the cell, the caret where the engine has it. */
  const resume = (value: string, start: number, end: number) => {
    const target = field.current
    if (!target) return
    owns.current = true
    sent.current = `${start}:${end}:${value}`
    target.value = value
    target.focus({ preventScroll: true })
    target.setSelectionRange(start, end)
    revealEnd(target)
    setDraft(value)
  }

  // A press on the grid while the field had the keyboard: once released, if it pointed at a cell
  // for the formula, the field takes the keyboard back.
  const pointer = useRef({ down: false, grid: false })
  const [released, setReleased] = useState(0)
  useEffect(() => {
    const down = (event: PointerEvent) => {
      pointer.current = { down: true, grid: event.target instanceof Element && event.target.closest('[data-testid="sheet-canvas-region"]') != null }
    }
    const up = () => {
      const { grid } = pointer.current
      pointer.current = { down: false, grid }
      if (grid) setReleased((n) => n + 1)
    }
    // Typing in the cell (the editor's text sink) makes it the cell's typing again.
    const key = (event: KeyboardEvent) => {
      if (event.target instanceof HTMLTextAreaElement && event.target.classList.contains('kui-text-sink')) owns.current = false
    }
    window.addEventListener('pointerdown', down, true)
    window.addEventListener('pointerup', up, true)
    window.addEventListener('keydown', key, true)
    return () => {
      window.removeEventListener('pointerdown', down, true)
      window.removeEventListener('pointerup', up, true)
      window.removeEventListener('keydown', key, true)
    }
  }, [])
  // After the press (and its drag) reached the engine: a reference pointed at goes on in the field.
  useEffect(() => {
    if (!released || !owns.current || focused.current) return
    void run('editor.formula', { peek: true }).then((answer) => {
      const state = answer ? (JSON.parse(answer) as BarState) : null
      if (!state?.editing) owns.current = false
      else if (state.pointing && owns.current && !focused.current && !pointer.current.down) resume(state.text ?? '', state.end, state.end)
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per press on the grid
  }, [released])
  // The typing ended elsewhere (entered or dropped from the cell).
  useEffect(() => {
    if (!info.editing && !focused.current && !pointer.current.down) owns.current = false
  }, [info.editing])

  /** Toggles `toggle` on the selected characters, entering what was typed first. */
  const formatCharacters = async (target: HTMLTextAreaElement, toggle: string) => {
    const [from, to] = [target.selectionStart ?? 0, target.selectionEnd ?? 0]
    const value = target.value
    if (readOnly || from >= to || value.startsWith('=')) return
    const cell = info.editCell?.cell ?? info.active
    const sheet = info.editCell?.sheet ?? info.sheet
    if (owns.current || info.editing) {
      setDraft(null)
      owns.current = false
      await run('editor.formula', { text: value, enter: 'stay' })
    }
    await run('cell.formatText', { cell, sheet, start: characters(value, from), end: characters(value, to), toggle })
    requestAnimationFrame(() => {
      target.focus()
      target.setSelectionRange(from, to)
    })
  }

  /** fx: Insert Function; the function chosen goes in at the caret, and typing goes on here. */
  const insertFunction = () => {
    const target = field.current
    // The selection the function goes over, for a cell not yet being typed.
    if (target && focused.current && draft != null) send(draft, target.selectionStart ?? draft.length, target.selectionEnd ?? draft.length)
    const was = owns.current || info.editing
    openInsertFunction({
      onChoose: async (name) => {
        await run('formulas.insertFunction', { name })
        const answer = await run('editor.formula', { peek: true })
        const state = answer ? (JSON.parse(answer) as BarState) : null
        if (state?.editing) resume(state.text ?? '', state.end, state.end)
      },
      onCancel: () => {
        if (!was) return backToGrid()
        void run('editor.formula', { peek: true }).then((answer) => {
          const state = answer ? (JSON.parse(answer) as BarState) : null
          if (state?.editing) resume(state.text ?? '', state.start, state.end)
          else backToGrid()
        })
      },
    })
  }

  // The Name Box shows the cell the formula goes in while another sheet's cells are pointed at.
  const place = info.editCell && info.editCell.sheet !== info.sheet ? info.editCell.cell : info.selection === info.active ? info.active : info.selection
  const editing = draft != null || info.editing

  return (
    <div className="kui-bar sheet-formula-bar" data-expanded={expanded} data-testid="sheet-formula-bar">
      <input
        className="kui-field sheet-name-box"
        value={reference ?? place}
        aria-label={t('nameBox')}
        spellCheck={false}
        onFocus={(event) => event.target.select()}
        onChange={(event) => setReference(event.target.value)}
        onBlur={() => setReference(null)}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && reference) {
            // Going elsewhere enters what is being typed first, as a click on another cell does.
            if (info.editing) void run('editor.formula', { enter: 'stay' })
            void run('edit.goTo', { reference: reference.trim() })
            setReference(null)
            event.currentTarget.blur()
          } else if (event.key === 'Escape') {
            // What this Escape cancelled: the reference being typed.
            event.preventDefault()
            setReference(null)
            event.currentTarget.blur()
          }
        }}
        data-testid="sheet-name-box"
      />
      {/* These keep the keyboard in the formula field. */}
      <IconButton icon={X} label={t('cancel')} disabled={!editing || readOnly} onMouseDown={(event) => event.preventDefault()} onClick={cancel} data-testid="sheet-formula-cancel" />
      <IconButton icon={Check} label={t('enterFormula')} disabled={!editing || readOnly} onMouseDown={(event) => event.preventDefault()} onClick={() => finish('stay')} data-testid="sheet-formula-enter" />
      <IconButton icon={FunctionSquare} label={t('insertFunction')} disabled={readOnly} onMouseDown={(event) => event.preventDefault()} onClick={insertFunction} data-testid="sheet-formula-fx" />
      <div className="sheet-formula-field" data-colored={references.length > 0}>
        <div className="sheet-formula-colors" aria-hidden="true">
          <span style={{ transform: `translate(${-scroll.left}px, ${-scroll.top}px)` }}>
            <FormulaText text={text} references={references} />
          </span>
        </div>
        <textarea
          ref={field}
          className="kui-field sheet-formula"
          value={text}
          rows={1}
          wrap={expanded ? 'soft' : 'off'}
          aria-label={t('formula')}
          spellCheck={false}
          readOnly={readOnly}
          onMouseDown={(event) => {
            if (info.checkbox && !info.editing && !viewer) event.preventDefault()
          }}
          onFocus={() => {
            focused.current = true
            if (readOnly) return
            // The cell being typed goes on here.
            if (info.editing) {
              owns.current = true
              setDraft(info.editText ?? '')
            }
          }}
          onBlur={() => {
            // Leaving the field keeps the typing in the cell (a click on the grid points at a
            // cell, or enters it).
            focused.current = false
            sent.current = ''
            setDraft(null)
            setHint(null)
          }}
          onScroll={(event) => setScroll({ left: event.currentTarget.scrollLeft, top: event.currentTarget.scrollTop })}
          onChange={(event) => {
            if (readOnly) return
            const target = event.target
            setDraft(target.value)
            setPick(0)
            revealEnd(target)
            send(target.value, target.selectionStart ?? target.value.length, target.selectionEnd ?? target.value.length)
          }}
          // The caret moved (a click, Home, the arrows): the cell's caret and the hint follow it.
          onSelect={(event) => {
            const target = event.currentTarget
            if (owns.current && draft != null && focused.current) send(target.value, target.selectionStart ?? 0, target.selectionEnd ?? 0)
          }}
          onKeyDown={(event) => {
            // While an input method composes, it owns every key (Enter ends the composition).
            if (event.nativeEvent.isComposing || event.key === 'Process') return
            const command = event.metaKey || event.ctrlKey
            const toggle = command && !event.altKey && !event.shiftKey ? FORMAT_SHORTCUTS[event.key.toLowerCase()] : undefined
            const target = event.currentTarget
            const listed = hint?.functions.length ?? 0
            if (toggle) {
              event.preventDefault()
              void formatCharacters(target, toggle)
            } else if (command && event.shiftKey && event.key.toLowerCase() === 'u') {
              // Ctrl+Shift+U expands or collapses the formula bar, as in Excel.
              event.preventDefault()
              setExpanded((open) => !open)
            } else if (readOnly) {
              if (event.key === 'Escape' || event.key === 'Enter') {
                event.preventDefault()
                release()
              }
            } else if (event.key === 'F4' && target.value.startsWith('=')) {
              // F4 cycles the reference at the caret: A1, $A$1, A$1, $A1.
              event.preventDefault()
              const [start, end] = [target.selectionStart ?? 0, target.selectionEnd ?? 0]
              void run('editor.cycleReference', { text: target.value, start, end }).then((answer) => {
                const next = answer ? (JSON.parse(answer) as { text: string; start: number }) : null
                if (next && next.text !== target.value) replaceDraft(target, next.text, next.start)
              })
            } else if (listed && (event.key === 'ArrowDown' || event.key === 'ArrowUp') && !event.shiftKey) {
              event.preventDefault()
              setPick((at) => Math.max(0, Math.min(listed - 1, at + (event.key === 'ArrowDown' ? 1 : -1))))
            } else if (listed && event.key === 'Tab' && !event.shiftKey) {
              event.preventDefault()
              const chosen = hint?.functions[Math.min(pick, listed - 1)]
              if (chosen) acceptFunction(target, chosen.name)
            } else if (event.key === 'Enter' && (event.altKey || event.ctrlKey)) {
              // Alt+Enter (Option+Return on a Mac; Ctrl+Option+Return too): a line break.
              event.preventDefault()
              const [start, end] = [target.selectionStart ?? 0, target.selectionEnd ?? 0]
              replaceDraft(target, target.value.slice(0, start) + '\n' + target.value.slice(end), start + 1)
            } else if (event.key === 'Enter' && !command) {
              event.preventDefault()
              finish(event.shiftKey ? 'up' : 'down')
            } else if (event.key === 'Tab' && !command) {
              event.preventDefault()
              finish(event.shiftKey ? 'left' : 'right')
            } else if (event.key === 'Escape') {
              // What this Escape cancelled: the formula being typed.
              event.preventDefault()
              cancel()
            }
          }}
          data-testid="sheet-formula"
        />
        {hint && draft != null ? (
          <HintPanel
            hint={hint}
            pick={pick}
            testId="sheet-formula-hints"
            onPick={(name) => {
              const target = document.querySelector<HTMLTextAreaElement>(FIELD)
              if (target) acceptFunction(target, name)
            }}
          />
        ) : null}
      </div>
      <IconButton
        icon={expanded ? ChevronUp : ChevronDown}
        label={t(expanded ? 'collapseFormulaBar' : 'expandFormulaBar')}
        aria-expanded={expanded}
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => setExpanded((open) => !open)}
        data-testid="sheet-formula-expand"
      />
    </div>
  )
}
