import type { SheetInfo } from './sheet'

/** Only the engine recognizes references and assigns colors; both editors show its spans. */
export function FormulaText({ text, references }: { text: string; references: SheetInfo['formulaReferences'] }) {
  let at = 0
  const pieces = references.map((reference) => {
    const plain = text.slice(at, reference.start)
    at = reference.end
    const [r, g, b] = reference.color
    return <span key={reference.start}>{plain}<span data-formula-reference={reference.text} style={{ color: `rgb(${r}, ${g}, ${b})` }}>{text.slice(reference.start, reference.end)}</span></span>
  })
  return <>{pieces}{text.slice(at)}</>
}
