/**
 * Tessera icons for the Sheet engine's commands that kit-editor's rules cannot read (they draw
 * the neutral command mark otherwise): its own vocabulary, by command id. Registered once, when the
 * editor loads (`main.tsx`); ribbons and menus built from engine commands draw them.
 */
import { registerCommandIcons } from '@kition/sdk/editor'
import { EraserIcon, FileCodeIcon, Grid2x2Icon, LassoIcon, ListChecksIcon, PencilIcon, PlayIcon, RotateCwIcon, SwatchBookIcon, TableCellsSplitIcon, TablePropertiesIcon, TextAlignJustifyIcon, UnderlineIcon } from '@kition/tessera/icons/react'

registerCommandIcons({
  "script.list": FileCodeIcon,
  "script.run": PlayIcon,
  "ink.eraser": EraserIcon,
  "ink.lasso": LassoIcon,
  "format.borders": Grid2x2Icon,
  "conditionalFormat.apply": SwatchBookIcon,
  "format.doubleUnderline": UnderlineIcon,
  "format.drawBorder": PencilIcon,
  "format.cells": TablePropertiesIcon,
  "format.justify": TextAlignJustifyIcon,
  "conditionalFormat.manageRules": ListChecksIcon,
  "format.orientation": RotateCwIcon,
  "cell.unmerge": TableCellsSplitIcon,
})
