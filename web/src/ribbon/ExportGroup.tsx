import { useEffect, useRef, useState } from 'react'
import {
  FileOutputIcon as FileOutput,
  FileSpreadsheetIcon as FileSpreadsheet,
  GlobeIcon as Globe,
  ImageDownIcon as ImageOut,
  LayoutTemplateIcon as Template,
  TableIcon as Workbook,
} from '@kition/tessera/icons/react'
import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, ModalTitle, RibbonButton, RibbonGroup, toast, useKitEditor } from '@kition/sdk/editor'
import { cleanName, exportThroughHost, MAX_NAME, type ExportFormat } from '../exportFile'
import { backToGrid } from '../sheet'

/**
 * File › Export (WPS's 输出为PDF, 输出为图片 and 另存为): the whole workbook as PDF (as Print lays it
 * out), the active sheet as a picture, CSV or a web page, and the workbook as an Excel workbook (a
 * copy: an .xls or .csv opened here becomes one) — written beside the workbook as a new file by
 * the host (`exportFile.ts`). File › Template › Save as Template (保存为模板) names the template
 * and writes it as an .xltx. Hidden where the host withholds exporting.
 */
export function ExportGroup({ sheet, sheets }: { sheet: string; sheets: readonly string[] }) {
  const { phrase: p, t, host, session } = useKitEditor()
  const [busy, setBusy] = useState(false)
  const [naming, setNaming] = useState(false)
  if (!host.permissions.export) return null
  const run = async (format: ExportFormat, name: string | null = null, extra: Record<string, unknown> = {}) => {
    const doc = session.doc
    if (!doc || busy) return
    setBusy(true)
    try {
      const written = await exportThroughHost((id, params) => doc.execute(id, params), host, format, sheet, sheets, name, extra)
      toast('success', format === 'xltx' ? t('templateSaved') : t('exported'), written)
    } catch (error) {
      toast('error', format === 'xltx' ? t('templateFailed') : t('exportFailed'), p(error instanceof Error ? error.message : String(error)))
    } finally {
      setBusy(false)
    }
  }
  const off = busy || !session.doc
  const own = host.init.document.name || host.init.document.path.slice(host.init.document.path.lastIndexOf('/') + 1)
  return (
    <>
      <RibbonGroup label={p('Export')} icon={FileOutput}>
        <RibbonButton icon={FileOutput} size="large" label={p('Export as PDF')} disabled={off} onClick={() => void run('pdf')} data-testid="sheet-export-pdf" primary />
        <RibbonButton icon={ImageOut} size="large" label={p('Export as Picture')} disabled={off} onClick={() => void run('png')} data-testid="sheet-export-png" primary />
        <RibbonButton icon={Workbook} size="large" label={p('Export as Excel Workbook')} disabled={off} onClick={() => void run('xlsx')} data-testid="sheet-export-xlsx" primary />
        <RibbonButton icon={FileSpreadsheet} size="large" label={p('Export as CSV')} disabled={off} onClick={() => void run('csv')} data-testid="sheet-export-csv" primary />
        {/* CSV in UTF-8 with a byte-order mark, as Excel's and WPS's 「CSV UTF-8」 write it: Excel for
            Windows then reads its Chinese text rightly (golden case EXP-C69). */}
        <RibbonButton icon={FileSpreadsheet} size="large" label={p('Export as CSV UTF-8')} disabled={off} onClick={() => void run('csv', null, { bom: true })} data-testid="sheet-export-csv-bom" primary />
        <RibbonButton icon={Globe} size="large" label={p('Export as Web Page')} disabled={off} onClick={() => void run('html')} data-testid="sheet-export-html" primary />
      </RibbonGroup>
      <RibbonGroup label={p('Template')} icon={Template}>
        <RibbonButton icon={Template} size="large" label={p('Save as Template')} disabled={off} onClick={() => setNaming(true)} data-testid="sheet-save-template" primary />
      </RibbonGroup>
      {naming ? (
        <TemplateNameDialog
          initial={own.replace(/\.[^.]+$/, '')}
          onCancel={() => setNaming(false)}
          onSave={(name) => {
            setNaming(false)
            void run('xltx', name)
          }}
        />
      ) : null}
    </>
  )
}

/**
 * Save as Template's name (WPS 保存为模板): the workbook's name to start with, selected so typing
 * replaces it; OK is off while the name is empty or only spaces; Escape, the corner's close and
 * 取消 save nothing.
 */
function TemplateNameDialog({ initial, onCancel, onSave }: { initial: string; onCancel: () => void; onSave: (name: string) => void }) {
  const { t } = useKitEditor()
  const [name, setName] = useState(initial)
  const field = useRef<HTMLInputElement>(null)
  useEffect(() => {
    field.current?.focus()
    field.current?.select()
  }, [])
  const clean = cleanName(name)
  const save = () => {
    if (clean) onSave(clean)
  }
  return (
    <Modal open onOpenChange={(open) => (open ? undefined : onCancel())}>
      <ModalContent
        size="sm"
        dismissible={false}
        aria-describedby={undefined}
        onCloseAutoFocus={(event) => {
          event.preventDefault()
          backToGrid()
        }}
        data-testid="sheet-template-dialog"
      >
        <ModalHeader>
          <ModalTitle>{t('templateTitle')}</ModalTitle>
        </ModalHeader>
        <ModalBody>
          <label className="sheet-template-field">
            <span>{t('templateName')}</span>
            <input
              ref={field}
              value={name}
              maxLength={MAX_NAME + 20}
              onChange={(event) => setName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault()
                  save()
                }
              }}
              data-testid="sheet-template-name"
            />
          </label>
          {clean ? null : (
            <p className="sheet-template-hint" role="alert" data-testid="sheet-template-required">
              {t('templateNameRequired')}
            </p>
          )}
        </ModalBody>
        <ModalFooter className="sheet-dialog-actions">
          <Button variant="primary" disabled={!clean} onClick={save} data-testid="sheet-template-ok">
            {t('ok')}
          </Button>
          <Button variant="secondary" onClick={onCancel} data-testid="sheet-template-cancel">
            {t('cancel')}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
