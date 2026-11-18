import { useSyncExternalStore } from 'react'
import { useKitEditor } from '@kition/kit-editor'
import { functionText } from './strings'
import { toScreen, type Pins } from './commentAnchors'
import type { ViewStore } from './HostComments'

/** A function the list offers, as the engine describes it. */
export type FunctionInfo = { name: string; signature: string; description: string }

/**
 * What the engine says to show while a formula is typed (`inspect().formulaHint` for the cell
 * editor, `editor.formula`'s `hint` for the formula bar): the functions whose names start with the
 * word being typed (the chosen one, and where that word is, UTF-16), the call the caret is in with
 * its argument's place in the signature, and for the cell editor the document point under it.
 */
export type FormulaHint = {
  functions: FunctionInfo[]
  pick: number
  replace: [number, number] | null
  call: { name: string; signature: string; description: string; argument: number; bold: [number, number] | null } | null
  at?: [number, number]
}

/** The function list and the argument hint, as WPS shows them under the formula being typed. */
export function HintPanel({ hint, pick, onPick, testId }: { hint: FormulaHint; pick: number; onPick: (name: string) => void; testId: string }) {
  const { t } = useKitEditor()
  const chosen = hint.functions[Math.min(pick, hint.functions.length - 1)]
  const call = hint.call
  return (
    <div className="sheet-formula-hints" data-testid={testId} onPointerDown={(event) => event.stopPropagation()} onMouseDown={(event) => event.preventDefault()}>
      {call && !hint.functions.length ? (
        <div className="sheet-formula-call" data-testid={`${testId}-call`}>
          {call.bold ? (
            <>
              {call.signature.slice(0, call.bold[0])}
              <b>{call.signature.slice(call.bold[0], call.bold[1])}</b>
              {call.signature.slice(call.bold[1])}
            </>
          ) : (
            call.signature
          )}
        </div>
      ) : null}
      {hint.functions.length ? (
        <div className="sheet-formula-list-row">
          <ul className="sheet-formula-list" role="listbox" data-testid={`${testId}-list`}>
            {hint.functions.map((f, index) => (
              <li
                key={f.name}
                role="option"
                aria-selected={f === chosen}
                data-name={f.name}
                ref={f === chosen ? (node) => node?.scrollIntoView({ block: 'nearest' }) : undefined}
                onClick={() => onPick(f.name)}
                data-index={index}
              >
                {f.name}
              </li>
            ))}
          </ul>
          {chosen ? (
            <div className="sheet-formula-doc">
              <div className="sheet-formula-doc-syntax">{chosen.signature}</div>
              <div data-testid={`${testId}-description`}>{functionText(t, chosen.name, chosen.description)}</div>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

/** The cell editor's hints, under the cell being typed wherever the canvas has it. */
export function CellFormulaHints({ hint, views, pins, onPick }: { hint: FormulaHint; views: ViewStore; pins: Pins; onPick: (name: string) => void }) {
  const view = useSyncExternalStore(views.subscribe, views.get)
  if (!view || !hint.at) return null
  const at = toScreen(view, pins, hint.at[0], hint.at[1])
  if (at.x < 0 || at.y < 0 || at.x > view.width || at.y > view.height) return null
  return (
    <div className="sheet-cell-hints" style={{ left: at.x, top: at.y + 2 }}>
      <HintPanel hint={hint} pick={hint.pick} onPick={onPick} testId="sheet-cell-hints" />
    </div>
  )
}
