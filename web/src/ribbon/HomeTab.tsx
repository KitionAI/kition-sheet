import { AArrowDownIcon as AArrowDown, AArrowUpIcon as AArrowUp, TextAlignCenterIcon as AlignCenter, TextAlignStartIcon as AlignLeft, TextAlignEndIcon as AlignRight, AlignVerticalJustifyCenterIcon as AlignVerticalJustifyCenter, AlignVerticalJustifyEndIcon as AlignVerticalJustifyEnd, AlignVerticalJustifyStartIcon as AlignVerticalJustifyStart, ArrowDownWideNarrowIcon as ArrowDownWideNarrow, BaselineIcon as Baseline, BoldIcon as Bold, ClipboardPasteIcon as ClipboardPaste, CopyIcon as Copy, DecimalsArrowLeftIcon as DecimalsArrowLeft, DecimalsArrowRightIcon as DecimalsArrowRight, DollarSignIcon as DollarSign, EraserIcon as Eraser, Grid2x2Icon as Grid2x2, ListIndentDecreaseIcon as IndentDecrease, ListIndentIncreaseIcon as IndentIncrease, ItalicIcon as Italic, PaintBucketIcon as PaintBucket, PercentIcon as Percent, Rows3Icon as Rows3, ScissorsIcon as Scissors, SearchIcon as Search, SigmaIcon as Sigma, StrikethroughIcon as Strikethrough, TableCellsMergeIcon as TableCellsMerge, TablePropertiesIcon as TableProperties, UnderlineIcon as Underline, TextWrapIcon as WrapText, ArrowDownToLineIcon as ArrowDownToLine, PaletteIcon as Palette, SquarePlusIcon as SquarePlus, SquareMinusIcon as SquareMinus, SquareSplitVerticalIcon as SquareSplitVertical, RotateCcwIcon as RotateCcw } from '@kition/tessera/icons/react'
import { CommandButton, CommandItem, MenuItem, MenuSeparator, RibbonButton, RibbonGroup, RibbonItem, RibbonMenu, RibbonRow, RibbonStack, useKitEditor } from '@kition/sdk/editor'
import { NUMBER_FORMATS, type SheetInfo } from '../sheet'
import { ColorSplit, FontNamePicker, FontSizePicker, FormatPainterButton, shortcutKey } from '../FontControls'
import { openUnhideSheets } from '../SheetDialogs'
import { DeleteMenuItems, InsertMenuItems } from '../LineMenus'
import { FreezeMenu } from '../ViewControls'
import { FormatAsTableMenu } from '../TableStyles'

const BORDERS: Array<[string, string]> = [
  ['Bottom Border', 'bottom'],
  ['Top Border', 'top'],
  ['Left Border', 'left'],
  ['Right Border', 'right'],
  ['No Border', 'none'],
  ['All Borders', 'all'],
  ['Outside Borders', 'outside'],
  ['Thick Outside Borders', 'thickOutside'],
  ['Bottom Double Border', 'doubleBottom'],
  ['Thick Bottom Border', 'thickBottom'],
  ['Top and Bottom Border', 'topBottom'],
]

const CELL_STYLES = ['Normal', 'Good', 'Bad', 'Neutral', 'Title', 'Heading 1', 'Heading 2', 'Total', 'Accent1']

export function HomeTab({ info, onClipboard }: { info: SheetInfo; onClipboard: (action: 'cut' | 'copy' | 'paste') => void }) {
  const { run, enabled, phrase: p, t, testId } = useKitEditor()
  const { font = {}, align = {} } = info.style
  const key = shortcutKey()
  const formats = info.style.num_fmt && !NUMBER_FORMATS.includes(info.style.num_fmt) ? [...NUMBER_FORMATS, info.style.num_fmt] : NUMBER_FORMATS

  return (
    <>
      <RibbonGroup label={p('Clipboard')} icon={ClipboardPaste}>
        {/* WPS puts Format Painter first, left of Paste (in Single line it waits in the group's popover, so Home fits at 1024 px). */}
        <FormatPainterButton info={info} />
        <RibbonMenu icon={ClipboardPaste} label={p('Paste')} size="large" testId={`${testId}-paste`} primary>
          <MenuItem label={p('Paste')} disabled={!enabled('edit.paste')} onSelect={() => onClipboard('paste')} testId={`${testId}-paste-text`} />
          <CommandItem command="edit.pasteSpecial" label={p('Paste Special')} />
        </RibbonMenu>
        <RibbonStack>
          <RibbonButton icon={Scissors} label={p('Cut')} disabled={!enabled('edit.cut')} onClick={() => onClipboard('cut')} data-testid={`${testId}-cut`} />
          <RibbonButton icon={Copy} label={p('Copy')} disabled={!enabled('edit.copy')} onClick={() => onClipboard('copy')} data-testid={`${testId}-copy`} />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Font')} icon={Baseline}>
        <RibbonStack>
          <RibbonRow>
            <RibbonItem primary>
              <FontNamePicker info={info} />
            </RibbonItem>
            <RibbonItem primary>
              <FontSizePicker info={info} />
            </RibbonItem>
            <CommandButton command="format.increaseFontSize" icon={AArrowUp} size="icon" label={p('Increase Font Size')} title={p('Increase Font Size')} />
            <CommandButton command="format.decreaseFontSize" icon={AArrowDown} size="icon" label={p('Decrease Font Size')} title={p('Decrease Font Size')} />
          </RibbonRow>
          <RibbonRow>
            <CommandButton command="format.bold" primary icon={Bold} size="icon" pressed={font.bold} label={p('Bold')} title={t('boldTip', { key })} />
            <CommandButton command="format.italic" primary icon={Italic} size="icon" pressed={font.italic} label={p('Italic')} title={t('italicTip', { key })} />
            <CommandButton command="format.underline" primary icon={Underline} size="icon" pressed={font.underline != null && font.underline !== 'None'} label={p('Underline')} title={t('underlineTip', { key })} />
            <CommandButton command="format.strikethrough" icon={Strikethrough} size="icon" pressed={font.strike} label={p('Strikethrough')} title={t('strikeTip', { key })} />
            <RibbonMenu icon={Grid2x2} label={p('Borders')} size="icon" testId="sheet-borders">
              {BORDERS.map(([label, preset]) => (
                <CommandItem key={preset} command="format.borders" params={{ preset }} label={p(label)} />
              ))}
            </RibbonMenu>
            <ColorSplit primary command="format.fillColor" none="none" noneLabel={t('colorNoFill')} icon={PaintBucket} label={p('Fill Color')} store="fillColor" initial="#FFFF00" testId="sheet-fill" />
            <ColorSplit primary command="format.color" none="auto" noneLabel={t('colorAutomatic')} icon={Baseline} label={p('Font Color')} store="fontColor" initial="#FF0000" testId="sheet-font-color" />
          </RibbonRow>
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Alignment')} icon={AlignLeft}>
        <RibbonStack>
          <RibbonRow>
            <CommandButton command="format.alignTop" icon={AlignVerticalJustifyStart} size="icon" pressed={align.v === 'Top'} label={p('Top Align')} title={p('Top Align')} />
            <CommandButton command="format.alignMiddle" icon={AlignVerticalJustifyCenter} size="icon" pressed={align.v === 'Center'} label={p('Middle Align')} title={p('Middle Align')} />
            <CommandButton command="format.alignBottom" icon={AlignVerticalJustifyEnd} size="icon" pressed={align.v === 'Bottom'} label={p('Bottom Align')} title={p('Bottom Align')} />
            <RibbonMenu icon={RotateCcw} label={p('Orientation')} size="icon">
              <CommandItem command="format.orientation" params={{ angle: 'counterclockwise' }} label={p('Angle Counterclockwise')} />
              <CommandItem command="format.orientation" params={{ angle: 'clockwise' }} label={p('Angle Clockwise')} />
              <CommandItem command="format.orientation" params={{ angle: 'vertical' }} label={p('Vertical Text')} />
              <CommandItem command="format.orientation" params={{ angle: 'up' }} label={p('Rotate Text Up')} />
              <CommandItem command="format.orientation" params={{ angle: 'down' }} label={p('Rotate Text Down')} />
            </RibbonMenu>
            <CommandButton command="format.wrapText" icon={WrapText} pressed={align.wrap} label={p('Wrap Text')} />
          </RibbonRow>
          <RibbonRow>
            <CommandButton command="format.alignLeft" icon={AlignLeft} size="icon" pressed={align.h === 'Left'} label={p('Align Left')} title={p('Align Left')} />
            <CommandButton command="format.alignCenter" primary icon={AlignCenter} size="icon" pressed={align.h === 'Center'} label={p('Center')} title={p('Center')} />
            <CommandButton command="format.alignRight" icon={AlignRight} size="icon" pressed={align.h === 'Right'} label={p('Align Right')} title={p('Align Right')} />
            <CommandButton command="format.decreaseIndent" icon={IndentDecrease} size="icon" label={p('Decrease Indent')} title={p('Decrease Indent')} />
            <CommandButton command="format.increaseIndent" icon={IndentIncrease} size="icon" label={p('Increase Indent')} title={p('Increase Indent')} />
            <RibbonMenu icon={TableCellsMerge} label={p('Merge & Center')} testId="sheet-merge" primary>
              <CommandItem command="cell.mergeCenter" label={p('Merge & Center')} />
              <CommandItem command="cell.mergeAcross" label={p('Merge Across')} />
              <CommandItem command="cell.merge" label={p('Merge Cells')} />
              <CommandItem command="cell.unmerge" label={p('Unmerge Cells')} />
            </RibbonMenu>
          </RibbonRow>
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Number')} icon={DollarSign}>
        <RibbonStack>
          <RibbonRow>
            <RibbonItem primary>
              <select className="kui-ribbon-field" style={{ width: 104 }} value={info.style.num_fmt ?? 'General'} aria-label={p('Number Format')} onChange={(event) => void run('format.numberFormat', { format: event.target.value })} data-testid="sheet-number-format">
                {formats.map((format) => (
                  <option key={format} value={format}>
                    {p(format)}
                  </option>
                ))}
              </select>
            </RibbonItem>
          </RibbonRow>
          <RibbonRow>
            <CommandButton command="format.accounting" icon={DollarSign} size="icon" label={p('Accounting Number Format')} title={p('Accounting Number Format')} />
            <CommandButton command="format.percent" icon={Percent} size="icon" label={p('Percent Style')} title={p('Percent Style')} />
            <CommandButton command="format.comma" icon={Rows3} size="icon" label={p('Comma Style')} title={p('Comma Style')} />
            <CommandButton command="format.increaseDecimal" icon={DecimalsArrowRight} size="icon" label={p('Increase Decimal')} title={p('Increase Decimal')} />
            <CommandButton command="format.decreaseDecimal" icon={DecimalsArrowLeft} size="icon" label={p('Decrease Decimal')} title={p('Decrease Decimal')} />
          </RibbonRow>
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Styles')} icon={Palette}>
        <RibbonMenu icon={Palette} label={p('Conditional Formatting')} size="large" testId="sheet-cf">
          <CommandItem command="conditionalFormat.apply" params={{ rule: { type: 'dataBar' } }} label={p('Data Bars')} />
          <CommandItem command="conditionalFormat.apply" params={{ rule: { type: 'colorScale' } }} label={p('Color Scales')} />
          <CommandItem command="conditionalFormat.apply" params={{ rule: { type: 'iconSet' } }} label={p('Icon Sets')} />
          <MenuSeparator />
          <CommandItem command="conditionalFormat.clearRules" label={p('Clear Rules')} />
          <CommandItem command="conditionalFormat.manageRules" label={p('Manage Rules')} />
        </RibbonMenu>
        <FormatAsTableMenu info={info} />
        <RibbonMenu icon={TableProperties} label={p('Cell Styles')} size="large">
          {CELL_STYLES.map((name) => (
            <CommandItem key={name} command="style.apply" params={{ name }} label={p(name)} />
          ))}
        </RibbonMenu>
      </RibbonGroup>

      <RibbonGroup label={p('Cells')} icon={Rows3}>
        {/* Insert and Delete: rows and columns with their counts, cells, and the sheet (module M4). */}
        <RibbonMenu icon={SquarePlus} label={p('Insert')} size="large" testId="sheet-insert-menu">
          <InsertMenuItems info={info} testId="sheet-insert" />
        </RibbonMenu>
        <RibbonMenu icon={SquareMinus} label={p('Delete')} size="large" testId="sheet-delete-menu">
          <DeleteMenuItems info={info} testId="sheet-delete" sheet />
        </RibbonMenu>
        <RibbonMenu icon={SquareSplitVertical} label={p('Format')} size="large" testId="sheet-format-menu">
          <CommandItem command="row.setHeight" ask label={p('Row Height')} />
          <CommandItem command="row.autofitHeight" label={p('AutoFit Row Height')} />
          <CommandItem command="column.setWidth" ask label={p('Column Width')} />
          <CommandItem command="column.autofitWidth" label={p('AutoFit Column Width')} />
          <MenuSeparator />
          {/* Hide & Unhide: Unhide lists the hidden sheets, as `sheet.unhide` asks with no sheet. */}
          <CommandItem command="sheet.hide" label={p('Hide Sheet')} />
          <MenuItem label={p('Unhide Sheet…')} disabled={!info.tabs.some((tab) => tab.hidden)} onSelect={openUnhideSheets} testId="sheet-format-unhide" />
          <MenuSeparator />
          <CommandItem command="format.cells" label={p('Format Cells')} />
        </RibbonMenu>
      </RibbonGroup>

      <RibbonGroup label={p('Editing')} icon={Search}>
        <RibbonStack>
          <RibbonMenu icon={Sigma} label={p('AutoSum')} testId="sheet-autosum" primary>
            <CommandItem command="formula.autoSum" params={{ function: 'SUM' }} label={p('Sum')} />
            <CommandItem command="formula.autoSum" params={{ function: 'AVERAGE' }} label={p('Average')} />
            <CommandItem command="formula.autoSum" params={{ function: 'COUNT' }} label={p('Count Numbers')} />
            <CommandItem command="formula.autoSum" params={{ function: 'MAX' }} label={p('Max')} />
            <CommandItem command="formula.autoSum" params={{ function: 'MIN' }} label={p('Min')} />
          </RibbonMenu>
          <RibbonMenu icon={ArrowDownToLine} label={p('Fill')}>
            <CommandItem command="edit.fillDown" label={p('Down')} />
            <CommandItem command="edit.fillRight" label={p('Right')} />
            <CommandItem command="edit.fillUp" label={p('Up')} />
            <CommandItem command="edit.fillLeft" label={p('Left')} />
            <CommandItem command="edit.fillSeries" ask label={p('Series')} />
            <CommandItem command="edit.smartFill" label={p('Smart fill')} />
          </RibbonMenu>
          <RibbonMenu icon={Eraser} label={p('Clear')}>
            <CommandItem command="edit.clearAll" label={p('Clear All')} />
            <CommandItem command="edit.clearFormats" label={p('Clear Formats')} />
            <CommandItem command="edit.clearContents" label={p('Clear Contents')} />
            <CommandItem command="edit.clearComments" label={p('Clear Comments')} />
            <CommandItem command="edit.clearHyperlinks" label={p('Clear Hyperlinks')} />
          </RibbonMenu>
        </RibbonStack>
        <RibbonStack>
          <RibbonMenu icon={ArrowDownWideNarrow} label={p('Sort & Filter')} testId="sheet-sort" primary>
            <CommandItem command="data.sortAscending" label={p('Sort A to Z')} />
            <CommandItem command="data.sortDescending" label={p('Sort Z to A')} />
            <CommandItem command="data.sort" ask label={p('Custom Sort')} />
            <MenuSeparator />
            <CommandItem command="data.filter" label={p('Filter')} />
            <CommandItem command="data.clearFilter" label={p('Clear')} />
            <CommandItem command="data.reapply" label={p('Reapply')} />
          </RibbonMenu>
          <FreezeMenu info={info} size="small" />
          <RibbonMenu icon={Search} label={p('Find & Select')}>
            <CommandItem command="edit.find" ask label={p('Find')} />
            <CommandItem command="edit.replace" ask label={p('Replace')} />
            <CommandItem command="edit.goTo" ask label={p('Go To')} />
            <CommandItem command="edit.goToSpecial" ask label={p('Go To Special')} />
          </RibbonMenu>
        </RibbonStack>
      </RibbonGroup>
    </>
  )
}
