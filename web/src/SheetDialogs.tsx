import { useEffect, useState } from 'react'
import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, ModalTitle, useKitEditor } from '@kition/kit-editor'
import { backToGrid, type SheetInfo } from './sheet'

/**
 * The sheet dialog showing: Delete's confirmation for a sheet, Unhide's list of hidden sheets, or
 * a question before something that loses data (merging cells with values, a table's header
 * pictures).
 */
type Confirm = { title: string; message: string; testId: string; onOk: () => void; /** Only 确定 (a warning to acknowledge). */ alone?: boolean }
type SheetDialog = { kind: 'delete'; sheet: string } | { kind: 'unhide' } | ({ kind: 'confirm' } & Confirm) | null

let current: SheetDialog = null
const listeners = new Set<(dialog: SheetDialog) => void>()
const show = (dialog: SheetDialog) => {
  current = dialog
  for (const listener of listeners) listener(dialog)
}

/** Asks whether to delete `sheet` (the sheet menu's Delete). */
export const askDeleteSheet = (sheet: string) => show({ kind: 'delete', sheet })
/** Asks `message` under `title` (确定 and 取消); OK runs `onOk`. */
export const askConfirm = (confirm: Confirm) => show({ kind: 'confirm', ...confirm })
/** Tells `message` under `title`, with 确定 only. */
export const tell = (title: string, message: string, testId: string) => show({ kind: 'confirm', title, message, testId, onOk: () => undefined, alone: true })
/** Lists the hidden sheets to show one again (Home › Format › Unhide Sheet…, as `sheet.unhide` asks with no sheet). */
export const openUnhideSheets = () => show({ kind: 'unhide' })

/**
 * The sheet dialogs, over the page (a press outside does not close them; Escape and the corner's
 * close do): Delete Sheet asks before a sheet goes, as WPS does (「确认要删除“工作表1”吗？」 with
 * 确认删除 and 取消), and Unhide lists the hidden sheets to choose one.
 */
export function SheetDialogs({ info }: { info: SheetInfo }) {
  const { run, t } = useKitEditor()
  const [dialog, setDialog] = useState<SheetDialog>(current)
  const [chosen, setChosen] = useState<string | null>(null)
  useEffect(() => {
    listeners.add(setDialog)
    return () => {
      listeners.delete(setDialog)
    }
  }, [])
  const hidden = info.tabs.filter((tab) => tab.hidden).map((tab) => tab.name)
  // Unhide opens with the first hidden sheet chosen.
  useEffect(() => {
    if (dialog?.kind === 'unhide') setChosen((was) => (was && hidden.includes(was) ? was : (hidden[0] ?? null)))
  }, [dialog, hidden.join('\u0000')])
  // A command that fails says why in a notice (the editor's `run`).
  const close = () => show(null)
  // Closed, the dialog gives the keyboard back to the grid (it opened from a menu that is gone).
  const refocus = (event: Event) => {
    event.preventDefault()
    backToGrid()
  }

  if (dialog?.kind === 'delete') {
    const confirm = () => {
      close()
      void run('home.deleteSheet', { sheet: dialog.sheet })
    }
    return (
      <Modal open onOpenChange={(open) => (open ? undefined : close())}>
        <ModalContent size="sm" dismissible={false} aria-describedby={undefined} onCloseAutoFocus={refocus} data-testid="sheet-delete-dialog">
          <ModalHeader>
            <ModalTitle>{t('deleteSheetTitle')}</ModalTitle>
          </ModalHeader>
          <ModalBody>
            <p data-testid="sheet-delete-message">{t('deleteSheetConfirm', { sheet: dialog.sheet })}</p>
          </ModalBody>
          <ModalFooter className="sheet-dialog-actions">
            <Button variant="primary" onClick={confirm} data-testid="sheet-delete-confirm">
              {t('deleteSheetOk')}
            </Button>
            <Button variant="secondary" onClick={close} data-testid="sheet-delete-cancel">
              {t('cancel')}
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    )
  }
  if (dialog?.kind === 'confirm') {
    const ok = () => {
      close()
      dialog.onOk()
    }
    return (
      <Modal open onOpenChange={(open) => (open ? undefined : close())}>
        <ModalContent size="sm" dismissible={false} aria-describedby={undefined} onCloseAutoFocus={refocus} data-testid={dialog.testId}>
          <ModalHeader>
            <ModalTitle>{dialog.title}</ModalTitle>
          </ModalHeader>
          <ModalBody>
            <p data-testid={`${dialog.testId}-message`}>{dialog.message}</p>
          </ModalBody>
          <ModalFooter className="sheet-dialog-actions">
            <Button variant="primary" onClick={ok} data-testid={`${dialog.testId}-ok`}>
              {t('ok')}
            </Button>
            {dialog.alone ? null : (
              <Button variant="secondary" onClick={close} data-testid={`${dialog.testId}-cancel`}>
                {t('cancel')}
              </Button>
            )}
          </ModalFooter>
        </ModalContent>
      </Modal>
    )
  }
  if (dialog?.kind === 'unhide') {
    const unhide = () => {
      close()
      if (chosen) void run('sheet.unhide', { sheet: chosen })
    }
    return (
      <Modal open onOpenChange={(open) => (open ? undefined : close())}>
        <ModalContent size="sm" dismissible={false} aria-describedby={undefined} onCloseAutoFocus={refocus} data-testid="sheet-unhide-dialog">
          <ModalHeader>
            <ModalTitle>{t('unhideSheetTitle')}</ModalTitle>
          </ModalHeader>
          <ModalBody>
            <p>{t('unhideSheetPrompt')}</p>
            <div className="sheet-unhide-list" role="listbox" aria-label={t('unhideSheetPrompt')}>
              {hidden.map((name) => (
                <button
                  key={name}
                  type="button"
                  role="option"
                  className="sheet-unhide-option"
                  aria-selected={name === chosen}
                  onClick={() => setChosen(name)}
                  onDoubleClick={() => {
                    close()
                    void run('sheet.unhide', { sheet: name })
                  }}
                  data-testid={`sheet-unhide-option-${name}`}
                >
                  {name}
                </button>
              ))}
            </div>
          </ModalBody>
          <ModalFooter className="sheet-dialog-actions">
            <Button variant="primary" disabled={!chosen} onClick={unhide} data-testid="sheet-unhide-ok">
              {t('ok')}
            </Button>
            <Button variant="secondary" onClick={close} data-testid="sheet-unhide-cancel">
              {t('cancel')}
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    )
  }
  return null
}
