import { useEffect, useRef, useState, useSyncExternalStore } from 'react'
import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, ModalTitle, useKitEditor } from '@kition/sdk/editor'
import { toScreen, type Pins } from './commentAnchors'
import type { ViewStore } from './HostComments'
import { backToGrid, type SheetInfo, type ValidationRule } from './sheet'

/** What the editor asks about a value its cell's validation refused (`dialog` `validationError`). */
export type ValidationAsk = { style: 'stop' | 'warning' | 'information'; title: string; message: string; cell: string }

type Store<T> = { value: T; listeners: Set<(value: T) => void> }
const store = <T,>(value: T): Store<T> => ({ value, listeners: new Set() })
const set = <T,>(s: Store<T>, value: T) => {
  s.value = value
  for (const listener of s.listeners) listener(value)
}
const useStore = <T,>(s: Store<T>) => {
  const [value, setValue] = useState(s.value)
  useEffect(() => {
    s.listeners.add(setValue)
    return () => {
      s.listeners.delete(setValue)
    }
  }, [s])
  return value
}

const alert = store<ValidationAsk | null>(null)
const list = store<boolean>(false)
const dialog = store<boolean>(false)

/** Shows the message for a value its cell's validation refused. */
export const askValidation = (ask: ValidationAsk) => set(alert, ask)
/** Opens (or closes) the active cell's list of entries (its dropdown arrow, Alt+↓). */
export const openValidationList = (open = true) => set(list, open)
/** Opens Data › Data Validation for the selection. */
export const openValidationDialog = () => set(dialog, true)

/**
 * The active cell's data validation on the grid, as WPS shows it: a list's arrow at the right of
 * the cell (its entries drop down under the cell; one chosen goes in), and the rule's input
 * message beside the cell while it is selected.
 */
export function ValidationOverlay({ info, views, pins }: { info: SheetInfo; views: ViewStore; pins: Pins }) {
  const { run, t } = useKitEditor()
  const view = useSyncExternalStore(views.subscribe, views.get)
  const open = useStore(list)
  const panel = useRef<HTMLDivElement>(null)
  const validation = info.validation
  // Another cell, or typing, closes the list.
  useEffect(() => {
    if (open && (!validation?.list || info.editing)) openValidationList(false)
  }, [info.active, info.editing, info.sheet])
  useEffect(() => {
    if (!open) return
    const away = (event: PointerEvent) => {
      if (panel.current && event.target instanceof Node && !panel.current.contains(event.target)) openValidationList(false)
    }
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        openValidationList(false)
        backToGrid()
      }
    }
    window.addEventListener('pointerdown', away, true)
    window.addEventListener('keydown', key, true)
    return () => {
      window.removeEventListener('pointerdown', away, true)
      window.removeEventListener('keydown', key, true)
    }
  }, [open])
  if (!validation || !view || info.editing || info.shape) return null
  const [x0, y0, x1, y1] = validation.rect
  const topLeft = toScreen(view, pins, x0, y0)
  const bottomRight = toScreen(view, pins, x1, y1)
  if (bottomRight.x < 0 || bottomRight.y < 0 || topLeft.x > view.width || topLeft.y > view.height) return null
  const height = Math.max(12, bottomRight.y - topLeft.y)
  const choose = async (entry: string) => {
    openValidationList(false)
    await run('cell.set', { cell: validation.cell, input: entry })
    backToGrid()
  }
  return (
    <>
      {validation.list ? (
        <button
          type="button"
          className="sheet-validation-arrow"
          style={{ left: bottomRight.x + 1, top: topLeft.y, height }}
          aria-label={t('validationPick')}
          aria-expanded={open}
          onPointerDown={(event) => event.stopPropagation()}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => openValidationList(!open)}
          data-testid="sheet-validation-arrow"
        >
          <svg width="8" height="5" viewBox="0 0 8 5" aria-hidden="true">
            <path d="M0 0h8L4 5z" fill="currentColor" />
          </svg>
        </button>
      ) : null}
      {open && validation.list ? (
        <div
          ref={panel}
          className="sheet-validation-list"
          role="listbox"
          style={{ left: topLeft.x, top: bottomRight.y + 1, minWidth: Math.max(60, bottomRight.x - topLeft.x + 18) }}
          onPointerDown={(event) => event.stopPropagation()}
          onMouseDown={(event) => event.preventDefault()}
          data-testid="sheet-validation-list"
        >
          {validation.list.map((entry, index) => (
            <button key={`${index}:${entry}`} type="button" role="option" aria-selected={info.content === entry} onClick={() => void choose(entry)} data-testid={`sheet-validation-option-${entry}`}>
              {entry}
            </button>
          ))}
        </div>
      ) : null}
      {validation.prompt && !open ? (
        <div className="sheet-validation-prompt" style={{ left: bottomRight.x + (validation.list ? 20 : 4), top: bottomRight.y + 4 }} role="tooltip" data-testid="sheet-validation-prompt">
          {validation.prompt.title ? <strong>{validation.prompt.title}</strong> : null}
          {validation.prompt.message ? <span>{validation.prompt.message}</span> : null}
        </div>
      ) : null}
    </>
  )
}

/**
 * The message for a value its cell's validation refused, as WPS asks: a Stop rule's with 重试 and
 * 取消, a Warning's (「是否继续？」) with 是, 否 and 取消, an Information's with 确定 and 取消.
 */
export function ValidationAlert() {
  const { run, t, phrase } = useKitEditor()
  const ask = useStore(alert)
  if (!ask) return null
  const answer = (choice: 'retry' | 'accept' | 'cancel') => {
    set(alert, null)
    void run('editor.validation', { answer: choice }).then(() => backToGrid())
  }
  const message = phrase(ask.message)
  const title = ask.title || t(ask.style === 'stop' ? 'validationStopTitle' : ask.style === 'warning' ? 'validationWarningTitle' : 'validationInfoTitle')
  return (
    <Modal open onOpenChange={(open) => (open ? undefined : answer('cancel'))}>
      <ModalContent size="sm" dismissible={false} aria-describedby={undefined} onCloseAutoFocus={(event) => event.preventDefault()} data-testid="sheet-validation-alert" data-style={ask.style}>
        <ModalHeader>
          <ModalTitle>{title}</ModalTitle>
        </ModalHeader>
        <ModalBody>
          <p className={`sheet-validation-message sheet-validation-${ask.style}`} data-testid="sheet-validation-message">
            {message}
          </p>
          {ask.style === 'warning' ? <p>{t('validationContinue')}</p> : null}
        </ModalBody>
        <ModalFooter className="sheet-dialog-actions">
          {ask.style === 'stop' ? (
            <>
              <Button variant="primary" onClick={() => answer('retry')} data-testid="sheet-validation-retry">
                {t('validationRetry')}
              </Button>
              <Button variant="secondary" onClick={() => answer('cancel')} data-testid="sheet-validation-cancel">
                {t('cancel')}
              </Button>
            </>
          ) : ask.style === 'warning' ? (
            <>
              <Button variant="primary" onClick={() => answer('accept')} data-testid="sheet-validation-yes">
                {t('validationYes')}
              </Button>
              <Button variant="secondary" onClick={() => answer('retry')} data-testid="sheet-validation-no">
                {t('validationNo')}
              </Button>
              <Button variant="secondary" onClick={() => answer('cancel')} data-testid="sheet-validation-cancel">
                {t('cancel')}
              </Button>
            </>
          ) : (
            <>
              <Button variant="primary" onClick={() => answer('accept')} data-testid="sheet-validation-ok">
                {t('ok')}
              </Button>
              <Button variant="secondary" onClick={() => answer('cancel')} data-testid="sheet-validation-cancel">
                {t('cancel')}
              </Button>
            </>
          )}
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}

const KINDS = ['any', 'whole', 'decimal', 'list', 'date', 'time', 'textLength', 'custom'] as const
const OPERATORS = ['between', 'notBetween', 'equal', 'notEqual', 'greater', 'less', 'greaterOrEqual', 'lessOrEqual'] as const
const BLANK_RULE: ValidationRule = {
  range: '',
  type: 'any',
  operator: 'between',
  formula1: '',
  formula2: null,
  ignoreBlank: true,
  dropdown: true,
  showInput: true,
  inputTitle: '',
  inputMessage: '',
  showError: true,
  errorStyle: 'stop',
  errorTitle: '',
  errorMessage: '',
}

/**
 * Data › Data Validation, as WPS lays it out: 设置 (allow, data, the values or the list's source,
 * ignore blanks, the in-cell arrow), 输入信息 (the message shown when the cell is selected) and
 * 出错警告 (the style, title and message for a value refused); 全部清除, 确定 and 取消. It edits the
 * active cell's rule, for the whole selection.
 */
export function ValidationDialog({ info }: { info: SheetInfo }) {
  const { run, t } = useKitEditor()
  const open = useStore(dialog)
  const [tab, setTab] = useState<'settings' | 'input' | 'error'>('settings')
  const [rule, setRule] = useState<ValidationRule>(BLANK_RULE)
  useEffect(() => {
    if (!open) return
    setTab('settings')
    setRule(info.validation?.rule ? { ...BLANK_RULE, ...info.validation.rule } : BLANK_RULE)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per opening
  }, [open])
  if (!open) return null
  const close = () => {
    set(dialog, false)
    backToGrid()
  }
  const change = (part: Partial<ValidationRule>) => setRule({ ...rule, ...part })
  const two = rule.operator === 'between' || rule.operator === 'notBetween'
  const compares = rule.type === 'whole' || rule.type === 'decimal' || rule.type === 'date' || rule.type === 'time' || rule.type === 'textLength'
  const ok = async () => {
    const range = info.selection
    close()
    if (rule.type === 'any' && !rule.inputTitle && !rule.inputMessage) {
      await run('data.validation', { range, clear: true })
      return
    }
    await run('data.validation', {
      range,
      type: rule.type,
      operator: rule.operator,
      formula1: rule.formula1,
      ...(compares && two && rule.formula2 ? { formula2: rule.formula2 } : {}),
      ignoreBlank: rule.ignoreBlank,
      dropdown: rule.dropdown,
      showInput: rule.showInput,
      inputTitle: rule.inputTitle,
      inputMessage: rule.inputMessage,
      showError: rule.showError,
      errorStyle: rule.errorStyle,
      errorTitle: rule.errorTitle,
      errorMessage: rule.errorMessage,
    })
  }
  const field = (label: string, value: string, onChange: (value: string) => void, testId: string) => (
    <label className="sheet-validation-field">
      <span>{label}</span>
      <input value={value} onChange={(event) => onChange(event.target.value)} data-testid={testId} />
    </label>
  )
  return (
    <Modal open onOpenChange={(next) => (next ? undefined : close())}>
      <ModalContent size="md" dismissible={false} aria-describedby={undefined} onCloseAutoFocus={(event) => event.preventDefault()} data-testid="sheet-validation-dialog">
        <ModalHeader>
          <ModalTitle>{t('validationTitle')}</ModalTitle>
        </ModalHeader>
        <ModalBody>
          <div className="sheet-fc-tabs" role="tablist">
            {(['settings', 'input', 'error'] as const).map((id) => (
              <button key={id} type="button" role="tab" aria-selected={tab === id} onClick={() => setTab(id)} data-testid={`sheet-validation-tab-${id}`}>
                {t(id === 'settings' ? 'validationSettings' : id === 'input' ? 'validationInputTab' : 'validationErrorTab')}
              </button>
            ))}
          </div>
          {tab === 'settings' ? (
            <div className="sheet-validation-pane">
              <p className="sheet-validation-heading">{t('validationCriteria')}</p>
              <label className="sheet-validation-field">
                <span>{t('validationAllow')}</span>
                <select value={rule.type} onChange={(event) => change({ type: event.target.value as ValidationRule['type'], formula1: '', formula2: null })} data-testid="sheet-validation-type">
                  {KINDS.map((kind) => (
                    <option key={kind} value={kind}>
                      {t(`validationKind-${kind}`)}
                    </option>
                  ))}
                </select>
              </label>
              {compares ? (
                <>
                  <label className="sheet-validation-field">
                    <span>{t('validationData')}</span>
                    <select value={rule.operator} onChange={(event) => change({ operator: event.target.value })} data-testid="sheet-validation-operator">
                      {OPERATORS.map((op) => (
                        <option key={op} value={op}>
                          {t(`validationOp-${op}`)}
                        </option>
                      ))}
                    </select>
                  </label>
                  {field(two ? t('validationMinimum') : t('validationValue'), rule.formula1, (value) => change({ formula1: value }), 'sheet-validation-formula1')}
                  {two ? field(t('validationMaximum'), rule.formula2 ?? '', (value) => change({ formula2: value }), 'sheet-validation-formula2') : null}
                </>
              ) : null}
              {rule.type === 'list' ? field(t('validationSource'), rule.formula1, (value) => change({ formula1: value }), 'sheet-validation-source') : null}
              {rule.type === 'custom' ? field(t('validationFormula'), rule.formula1, (value) => change({ formula1: value }), 'sheet-validation-formula') : null}
              {rule.type !== 'any' ? (
                <label className="sheet-validation-check">
                  <input type="checkbox" checked={rule.ignoreBlank} onChange={(event) => change({ ignoreBlank: event.target.checked })} data-testid="sheet-validation-ignore-blank" />
                  {t('validationIgnoreBlank')}
                </label>
              ) : null}
              {rule.type === 'list' ? (
                <label className="sheet-validation-check">
                  <input type="checkbox" checked={rule.dropdown} onChange={(event) => change({ dropdown: event.target.checked })} data-testid="sheet-validation-dropdown" />
                  {t('validationDropdown')}
                </label>
              ) : null}
            </div>
          ) : tab === 'input' ? (
            <div className="sheet-validation-pane">
              <label className="sheet-validation-check">
                <input type="checkbox" checked={rule.showInput} onChange={(event) => change({ showInput: event.target.checked })} data-testid="sheet-validation-show-input" />
                {t('validationShowInput')}
              </label>
              {field(t('validationTitleField'), rule.inputTitle, (value) => change({ inputTitle: value }), 'sheet-validation-input-title')}
              <label className="sheet-validation-field">
                <span>{t('validationInputMessage')}</span>
                <textarea value={rule.inputMessage} onChange={(event) => change({ inputMessage: event.target.value })} rows={4} data-testid="sheet-validation-input-message" />
              </label>
            </div>
          ) : (
            <div className="sheet-validation-pane">
              <label className="sheet-validation-check">
                <input type="checkbox" checked={rule.showError} onChange={(event) => change({ showError: event.target.checked })} data-testid="sheet-validation-show-error" />
                {t('validationShowError')}
              </label>
              <label className="sheet-validation-field">
                <span>{t('validationStyle')}</span>
                <select value={rule.errorStyle} onChange={(event) => change({ errorStyle: event.target.value as ValidationRule['errorStyle'] })} data-testid="sheet-validation-style">
                  <option value="stop">{t('validationStyleStop')}</option>
                  <option value="warning">{t('validationStyleWarning')}</option>
                  <option value="information">{t('validationStyleInfo')}</option>
                </select>
              </label>
              {field(t('validationTitleField'), rule.errorTitle, (value) => change({ errorTitle: value }), 'sheet-validation-error-title')}
              <label className="sheet-validation-field">
                <span>{t('validationErrorMessage')}</span>
                <textarea value={rule.errorMessage} onChange={(event) => change({ errorMessage: event.target.value })} rows={4} data-testid="sheet-validation-error-message" />
              </label>
            </div>
          )}
        </ModalBody>
        <ModalFooter className="sheet-dialog-actions">
          <Button variant="secondary" onClick={() => setRule({ ...BLANK_RULE })} data-testid="sheet-validation-clear-all">
            {t('validationClearAll')}
          </Button>
          <span className="sheet-filter-spacer" />
          <Button variant="primary" onClick={() => void ok()} disabled={rule.type !== 'any' && !rule.formula1.trim()} data-testid="sheet-validation-ok">
            {t('ok')}
          </Button>
          <Button variant="secondary" onClick={close} data-testid="sheet-validation-dialog-cancel">
            {t('cancel')}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
