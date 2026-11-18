/**
 * File › Export: the engine writes the file's bytes (`file.export`, answered as base64) and the
 * host writes them as a new file beside the workbook (`host/write` mode `export`, Kit UI protocol).
 * The editor runs in the host's page, where it cannot write files itself; only the host's answer
 * says the file was written, and the path it answers is the one the host chose (it never replaces
 * a file).
 */

export type ExportFormat = 'pdf' | 'csv' | 'html' | 'png' | 'xlsx' | 'xltx'

/** What `file.export` answers. */
export type ExportAnswer = { format: string; extension: string; mime: string; bytes: number; data: string; pages?: number }

/** What the export needs of the host connection. */
export type ExportHost = {
  init: { document: { path: string; name: string } }
  exportFile: (path: string, bytes: Uint8Array) => Promise<string>
}

/**
 * The engine's `file.export` parameters: the whole workbook as Print lays it out (PDF, as WPS's
 * Export to PDF does by default), a copy of the workbook or the workbook as a template (Excel
 * workbook, template), or one sheet (CSV, web page, picture).
 */
export function exportParams(format: ExportFormat, sheet: string): Record<string, unknown> {
  if (format === 'pdf') return { format, sheets: 'all' }
  return format === 'xlsx' || format === 'xltx' || !sheet ? { format } : { format, sheet }
}

/**
 * The new file's workspace path: in the workbook's folder, named after the workbook; one sheet of
 * a workbook of several says which.
 */
export function exportPath(documentPath: string, documentName: string, extension: string, sheet: string | null = null, name: string | null = null): string {
  const folder = documentPath.includes('/') ? documentPath.slice(0, documentPath.lastIndexOf('/') + 1) : ''
  const own = documentName || documentPath.slice(documentPath.lastIndexOf('/') + 1)
  const stem = cleanName(name ?? own.replace(/\.[^.]+$/, '')) || 'Book'
  const part = sheet ? cleanName(sheet) : ''
  return `${folder}${part ? `${stem} - ${part}` : stem}.${extension}`
}

/** Longest file name (characters, before the extension) a template or an export gets. */
export const MAX_NAME = 80

/** A name as a file name: characters files cannot have become `_`, spaces at the ends go, long names are cut. */
export function cleanName(text: string): string {
  return [...text.replace(/[\\/:*?"<>|]/g, '_').trim()].slice(0, MAX_NAME).join('').trim()
}

/** The file's bytes from the engine's base64. */
export function decodeBase64(text: string): Uint8Array {
  const binary = atob(text)
  const bytes = new Uint8Array(binary.length)
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index)
  return bytes
}

/** Checks the engine's answer: the format asked for, with its bytes. */
export function parseAnswer(text: string, format: ExportFormat): ExportAnswer {
  const answer = JSON.parse(text) as Partial<ExportAnswer> | null
  if (!answer || typeof answer.data !== 'string' || answer.extension !== format) throw new Error(`the engine answered no ${format.toUpperCase()} file`)
  return answer as ExportAnswer
}

/**
 * Exports the workbook: the engine's bytes (`execute`, which throws the engine's error), written
 * by the host (which throws the host's). `sheets` are the workbook's sheets, `sheet` the active
 * one. Answers the path the host wrote.
 */
export async function exportThroughHost(
  execute: (id: string, params: Record<string, unknown>) => Promise<string>,
  host: ExportHost,
  format: ExportFormat,
  sheet: string,
  sheets: readonly string[],
  name: string | null = null,
  extra: Record<string, unknown> = {},
): Promise<string> {
  const answer = parseAnswer(await execute('file.export', { ...exportParams(format, sheet), ...extra }), format)
  const { path, name: own } = host.init.document
  const named = (format === 'csv' || format === 'html' || format === 'png') && sheets.length > 1 ? sheet : null
  return host.exportFile(exportPath(path, own, answer.extension, named, name), decodeBase64(answer.data))
}
