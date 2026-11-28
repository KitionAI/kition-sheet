/**
 * Where the keyboard goes when nothing in the editor holds it (plan stage 0A; walkthrough K1, K2).
 * Typing reaches a cell through kit-editor's text input (`.kui-text-sink`), which takes the
 * keyboard back after a press on the grid, but only while this page has the focus. A workbook just
 * opened, the Name Box's Enter, a closed dialog or a panel that went away left the focus on the
 * page's `<body>`, and no key worked until a cell was clicked.
 */
import { useEffect } from 'react'

/** kit-editor's text input: typing goes into the active cell from here. */
export function gridInput(): HTMLElement | null {
  return document.querySelector<HTMLElement>('.kui-text-sink')
}

/** Whether no element of the editor holds the keyboard (the page's body has it). */
export function keyboardLost() {
  const active = document.activeElement
  return !active || active === document.body || active === document.documentElement || !active.isConnected
}

/** Gives the keyboard to the grid; answers whether it has it now. */
export function focusGrid() {
  const input = gridInput()
  if (!input) return false
  input.focus({ preventScroll: true })
  return document.activeElement === input
}

/** A control a person types into: the keyboard is theirs there, never taken from it. */
function typingIn(element: Element | null) {
  if (!element) return false
  if (element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement) return true
  if (element instanceof HTMLElement && element.isContentEditable) return true
  return element instanceof HTMLInputElement && !['button', 'submit', 'reset', 'checkbox', 'radio', 'file', 'range', 'color', 'image'].includes(element.type)
}

/**
 * The workbook just opened: the person means to work in it, so the grid takes the keyboard, as a
 * desktop spreadsheet does with the window it opened. Not while the host has someone typing in one
 * of its own fields (its search box); a host on another origin is asked through this page's focus.
 */
export function takeKeyboardOnOpen() {
  if (!keyboardLost()) return
  let hostIdle = document.hasFocus()
  if (!hostIdle) {
    try {
      const host = window.parent !== window ? window.parent.document : null
      hostIdle = host !== null && !typingIn(host.activeElement)
    } catch {
      hostIdle = false
    }
  }
  if (hostIdle) focusGrid()
}

/**
 * Closes the Single line group panel a choice was made in (the More panels: Freeze Panes, Format
 * Painter): the panel closes on Escape, which is all it listens to besides a press outside it.
 */
export function closeLinePanels() {
  if (!document.querySelector('.kui-drop-panel')) return
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', cancelable: true }))
}

/** Whether the key types a character (a letter, a digit, a space), not a command. */
const typesText = (event: KeyboardEvent) => event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey

/**
 * The safety net, a capture listener on the document before any other: a key pressed while the
 * body has the keyboard gives the grid the keyboard and the key itself, once. A typed character
 * goes into the grid's input as the browser types it (the focus moved before it lands); any other
 * key is handed to that input as it was pressed, so the editor's keys see it in their order. A
 * blur with nowhere to go (a field left by script, a panel closed) gives the grid the keyboard back
 * when nothing else takes it; a press on the grid does that through kit-editor already.
 */
export function useKeyboardFallback() {
  useEffect(() => {
    let pressed = false
    const down = () => {
      pressed = true
    }
    const up = () => {
      pressed = false
    }
    const keyDown = (event: KeyboardEvent) => {
      if (!event.isTrusted || event.defaultPrevented || event.isComposing || event.keyCode === 229 || !keyboardLost()) return
      if (event.target !== document.body && event.target !== document.documentElement) return
      if (!focusGrid()) return
      if (typesText(event)) return
      const input = gridInput()
      if (!input) return
      event.preventDefault()
      event.stopImmediatePropagation()
      input.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: event.key,
          code: event.code,
          location: event.location,
          repeat: event.repeat,
          shiftKey: event.shiftKey,
          altKey: event.altKey,
          ctrlKey: event.ctrlKey,
          metaKey: event.metaKey,
          bubbles: true,
          cancelable: true,
          composed: true,
        }),
      )
    }
    const focusOut = (event: FocusEvent) => {
      if (event.relatedTarget !== null) return
      window.setTimeout(() => {
        if (!pressed && keyboardLost() && document.hasFocus()) focusGrid()
      }, 0)
    }
    window.addEventListener('pointerdown', down, true)
    window.addEventListener('pointerup', up, true)
    window.addEventListener('pointercancel', up, true)
    document.addEventListener('keydown', keyDown, true)
    document.addEventListener('focusout', focusOut)
    return () => {
      window.removeEventListener('pointerdown', down, true)
      window.removeEventListener('pointerup', up, true)
      window.removeEventListener('pointercancel', up, true)
      document.removeEventListener('keydown', keyDown, true)
      document.removeEventListener('focusout', focusOut)
    }
  }, [])
}
