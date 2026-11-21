/**
 * Tessera icons for the Sheet engine's commands that kit-editor's rules cannot read (they draw
 * the neutral command mark otherwise): its own vocabulary, by command id. Registered once, when the
 * editor loads (`main.tsx`); ribbons and menus built from engine commands draw them.
 */
import { registerCommandIcons } from '@kition/sdk/editor'
import { EraserIcon, FileCodeIcon, Grid2x2Icon, LassoIcon, ListChecksIcon, PencilIcon, PlayIcon, RotateCwIcon, SwatchBookIcon, TableCellsSplitIcon, TablePropertiesIcon, TextAlignJustifyIcon, UnderlineIcon } from '@kition/tessera/icons/react'

registerCommandIcons({
  "automate.allScripts": FileCodeIcon,
  "automate.runScript": PlayIcon,
  "draw.eraser": EraserIcon,
  "draw.lasso": LassoIcon,
  "home.borders": Grid2x2Icon,
  "home.conditionalFormat": SwatchBookIcon,
  "home.doubleUnderline": UnderlineIcon,
  "home.drawBorder": PencilIcon,
  "home.formatCells": TablePropertiesIcon,
  "home.justify": TextAlignJustifyIcon,
  "home.manageRules": ListChecksIcon,
  "home.orientation": RotateCwIcon,
  "home.unmergeCells": TableCellsSplitIcon,
})
