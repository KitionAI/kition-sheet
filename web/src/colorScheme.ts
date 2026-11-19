/**
 * The host's color scheme (Kit UI protocol `KitTheme.colorScheme`), as kit-editor's `applyTheme`
 * marks it on the root element (`data-color-scheme`): read at first, then followed while the host
 * changes it (no reload). The engine draws the grid in it (`editor.scheme`, a view setting the
 * workbook never holds).
 */
import { useEffect, useState } from 'react'

export type ColorScheme = 'light' | 'dark'

/** The scheme the root element is marked with: light unless it says dark. */
export function colorSchemeOf(root: HTMLElement): ColorScheme {
  return root.dataset.colorScheme === 'dark' ? 'dark' : 'light'
}

/** The host's color scheme now, following its changes. */
export function useColorScheme(root: HTMLElement = document.documentElement): ColorScheme {
  const [scheme, setScheme] = useState<ColorScheme>(() => colorSchemeOf(root))
  useEffect(() => {
    setScheme(colorSchemeOf(root))
    if (typeof MutationObserver === 'undefined') return undefined
    const observer = new MutationObserver(() => setScheme(colorSchemeOf(root)))
    observer.observe(root, { attributes: true, attributeFilter: ['data-color-scheme'] })
    return () => observer.disconnect()
  }, [root])
  return scheme
}
