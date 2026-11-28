import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, ModalTitle, useKitEditor } from '@kition/sdk/editor'
import { backToGrid } from './sheet'

/**
 * Paste Special (plan "Sheet usable by hand" stage 4B; walkthrough J5; Excel's Paste Special, the
 * items of WPS's paste options W-F9): what to paste, an operation with the cells pasted onto, skip
 * blanks, transpose; OK runs one `edit.pasteSpecial` of the sheet's copy at the active cell. The
 * keys ⌃⌘V (Ctrl+Alt+V elsewhere), the cell menu and Home › Paste open it; it works on a copy made
 * in the sheet (the command's own condition), not after a cut (Excel).
 *
 * Two hooks the older gate tests drive it by, kept on purpose: the "Paste" list is
 * `<select id="param-what">` with the engine's values (gate-insert.mjs INSC-C262 chooses
 * `values`), and OK is `data-testid="command-dialog-run"` (that test and tests/hand/menus.mjs).
 */
const WHAT = ['all', 'formulas', 'values', 'formats', 'comments', 'allExceptBorders', 'columnWidths', 'formulasAndNumberFormats', 'valuesAndNumberFormats'] as const
const OPERATIONS = ['none', 'add', 'subtract', 'multiply', 'divide'] as const
/** The engine applies an operation only when these are pasted. */
const OPERATES = new Set<string>(['all', 'values', 'valuesAndNumberFormats', 'formulas'])

let open = false
const listeners = new Set<(open: boolean) => void>()
const show = (value: boolean) => {
  open = value
  for (const listener of listeners) listener(value)
}
/** Opens Paste Special (the caller checks that a paste special can run). */
export const openPasteSpecial = () => show(true)

export function PasteSpecialDialog() {
  const { run, t } = useKitEditor()
  const [shown, setShown] = useState(open)
  const [what, setWhat] = useState<string>('all')
  const [operation, setOperation] = useState<string>('none')
  const [skipBlanks, setSkipBlanks] = useState(false)
  const [transpose, setTranspose] = useState(false)
  const busy = useRef(false)
  const first = useRef<HTMLSelectElement>(null)
  useEffect(() => {
    listeners.add(setShown)
    return () => {
      listeners.delete(setShown)
    }
  }, [])
  // Each opening starts from All (Excel).
  useEffect(() => {
    if (!shown) return
    setWhat('all')
    setOperation('none')
    setSkipBlanks(false)
    setTranspose(false)
  }, [shown])

  if (!shown) return null
  const close = () => show(false)
  const operates = OPERATES.has(what)
  const paste = async (params: Record<string, unknown>) => {
    if (busy.current) return
    busy.current = true
    try {
      const answer = await run('edit.pasteSpecial', params)
      if (answer != null) close()
    } finally {
      busy.current = false
    }
  }
  const ok = () => paste({ what, ...(operates && operation !== 'none' ? { operation } : {}), ...(skipBlanks ? { skipBlanks: true } : {}), ...(transpose ? { transpose: true } : {}) })
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'Enter' || event.nativeEvent.isComposing || event.keyCode === 229 || event.target instanceof HTMLButtonElement) return
    event.preventDefault()
    void ok()
  }

  return (
    <Modal open onOpenChange={(value) => (value ? undefined : close())}>
      <ModalContent
        size="sm"
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
        data-testid="sheet-paste-special"
      >
        <ModalHeader>
          <ModalTitle>{t('pasteSpecialTitle')}</ModalTitle>
        </ModalHeader>
        <ModalBody>
          <div className="sheet-dialog-form">
            <label className="sheet-dialog-row">
              <span>{t('pasteWhat')}</span>
              {/* id="param-what": the hook gate-insert.mjs INSC-C262 chooses Values by. */}
              <select ref={first} id="param-what" className="kui-field" value={what} onChange={(event) => setWhat(event.target.value)} data-testid="sheet-paste-what">
                {WHAT.map((value) => (
                  <option key={value} value={value}>
                    {t(`pasteWhat:${value}`)}
                  </option>
                ))}
              </select>
            </label>
            <label className="sheet-dialog-row">
              <span>{t('pasteOperation')}</span>
              <select className="kui-field" value={operates ? operation : 'none'} disabled={!operates} onChange={(event) => setOperation(event.target.value)} data-testid="sheet-paste-operation">
                {OPERATIONS.map((value) => (
                  <option key={value} value={value}>
                    {t(`pasteOp:${value}`)}
                  </option>
                ))}
              </select>
            </label>
            <label className="sheet-dialog-choice">
              <input type="checkbox" checked={skipBlanks} onChange={(event) => setSkipBlanks(event.target.checked)} data-testid="sheet-paste-skip-blanks" />
              {t('pasteSkipBlanks')}
            </label>
            <label className="sheet-dialog-choice">
              <input type="checkbox" checked={transpose} onChange={(event) => setTranspose(event.target.checked)} data-testid="sheet-paste-transpose" />
              {t('pasteTranspose')}
            </label>
          </div>
        </ModalBody>
        <ModalFooter className="sheet-dialog-actions">
          <Button variant="secondary" className="sheet-paste-link" onClick={() => void paste({ link: true })} data-testid="sheet-paste-link">
            {t('pasteLink')}
          </Button>
          {/* data-testid="command-dialog-run": the hook INSC-C262 and tests/hand/menus.mjs press OK by. */}
          <Button variant="primary" onClick={() => void ok()} data-testid="command-dialog-run">
            {t('ok')}
          </Button>
          <Button variant="secondary" onClick={close} data-testid="sheet-paste-special-cancel">
            {t('cancel')}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
