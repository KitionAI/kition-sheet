// The engine's messages in the editor's language (module M10): a whole message through PHRASES, a
// message with a varying part (a file's name, a reason) through the catalog's `error…` keys, an
// agent's step prefix dropped, and English left as the engine says it.
import test from 'node:test'
import assert from 'node:assert/strict'
import { CATALOGS, PHRASES, localizeError } from './strings.ts'

/** The editor's `t` and `phrase` for a locale, as kit-editor makes them from the catalogs. */
function strings(locale) {
  const catalog = CATALOGS[locale]
  const t = (key, params = {}) => (key in catalog ? catalog[key].replace(/\{(\w+)\}/g, (_, name) => String(params[name] ?? `{${name}}`)) : key)
  const phrase = (text) => PHRASES[locale]?.[text] ?? text
  return { t, phrase }
}

test('a refusal the engine words whole is shown in Chinese', () => {
  const { t, phrase } = strings('zh-CN')
  assert.equal(localizeError(phrase, t, new Error("A table can't overlap another table.")), '表格不能与另一个表格重叠。')
  assert.equal(localizeError(phrase, t, "This action won't work on multiple selections."), '不能对多重选定区域使用此命令。')
})

test('open failures name the file in Chinese, with what it is', () => {
  const { t, phrase } = strings('zh-CN')
  assert.equal(localizeError(phrase, t, "'空白.xlsx' is empty (0 bytes): there is no workbook in it."), '“空白.xlsx”是空文件（0 字节），其中没有工作簿。')
  assert.equal(localizeError(phrase, t, "'报告.xlsx' is a PDF document, not a spreadsheet."), '“报告.xlsx”是PDF 文档，不是表格文件。')
  assert.match(localizeError(phrase, t, "'预算.xlsx' is password-protected. Kition Sheet can't open encrypted workbooks yet; remove the password in Excel and try again."), /“预算.xlsx”已设置密码保护/)
})

test('a refusal wrapped by the command or an agent step keeps its translated reason', () => {
  const { t, phrase } = strings('zh-CN')
  assert.equal(localizeError(phrase, t, 'command `home.mergeCells` is not available right now: The workbook is protected and cannot be changed.'), '命令现在不可用：工作簿受保护，不能更改。')
  assert.equal(localizeError(phrase, t, 'step 1 (`edit.paste`) failed: Nothing to paste.'), '没有可粘贴的内容。')
})

test('English stays as the engine says it, and an unknown message is left alone', () => {
  const { t, phrase } = strings('en-US')
  const empty = "'Book.xlsx' is empty (0 bytes): there is no workbook in it."
  assert.equal(localizeError(phrase, t, empty), empty)
  assert.equal(localizeError(phrase, t, 'command `x.y` is not available right now: why'), 'command `x.y` is not available right now: why')
  const zh = strings('zh-CN')
  assert.equal(localizeError(zh.phrase, zh.t, 'Something new happened.'), 'Something new happened.')
})
