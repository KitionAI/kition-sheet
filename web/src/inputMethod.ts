/**
 * Input methods (Chinese pinyin, wubi, Japanese, Korean) in the grid (plan stage 3; walkthrough
 * D13; WPS W-T1–W-T4). kit-editor's text input (`.kui-text-sink`), a hidden text field, is where
 * the input method composes; it hands the engine only the chosen text. Here:
 *
 * - what is being composed goes to the engine through the editor's ordered queue
 *   (`editor.preedit`): the first of it starts typing in the active cell, and it shows in the cell
 *   at the caret, underlined, without changing the workbook; an empty composition (Escape in the
 *   input method) takes it away, the chosen text (typed by the text input) ends it;
 * - the text input sits at the caret of the cell being typed (`inspect().caretRect`), so the input
 *   method's candidate window opens beside the cell, in frozen panes and at any zoom and scroll.
 *
 * The keys of a composition (Enter, Escape, the arrows, Backspace) are the input method's: kit-editor
 * keeps them from the tool and the commands, the editor's key table (keys.ts) and the keyboard's
 * safety net (focus.ts) leave them alone.
 */
import { useEffect, useRef } from 'react'
import { toScreen } from './commentAnchors'
import type { ViewStore } from './HostComments'

/** The longest composition sent to the engine, in characters. */
const LONGEST = 1000

type Run = (command: string, params?: Record<string, unknown>) => Promise<unknown>

/** The grid's text input an event came from, if it did. */
function sinkOf(target: EventTarget | null): HTMLTextAreaElement | null {
  return target instanceof HTMLTextAreaElement && target.classList.contains('kui-text-sink') ? target : null
}

/**
 * The input method's cursor in the composition `text`, in characters (the engine's unit): where the
 * field has it once its value ends with the composition, else at its end (a composition event can
 * come before the field shows what it composes).
 */
function cursorIn(field: HTMLTextAreaElement, text: string) {
  const length = Array.from(text).length
  const start = field.value.length - text.length
  if (start < 0 || field.value.slice(start) !== text) return length
  const end = Math.min(Math.max((field.selectionEnd ?? field.value.length) - start, 0), text.length)
  return Math.min(Array.from(text.slice(0, end)).length, length)
}

/** Sends compositions in the grid's text input to the engine, one at a time, the newest waiting. */
export function useCompositions(run: Run) {
  const latest = useRef(run)
  latest.current = run
  useEffect(() => {
    let sending = false
    let next: { text: string; cursor: number } | null = null
    const send = () => {
      if (sending || !next) return
      const params = next
      next = null
      sending = true
      void latest.current('editor.preedit', params)
        .catch(() => undefined)
        .finally(() => {
          sending = false
          send()
        })
    }
    const update = (event: CompositionEvent) => {
      const field = sinkOf(event.target)
      if (!field) return
      const text = Array.from(event.data ?? '').slice(0, LONGEST).join('')
      // A composition starts empty: nothing to show yet.
      if (event.type === 'compositionstart' && !text) return
      next = { text, cursor: cursorIn(field, text) }
      send()
    }
    const end = (event: CompositionEvent) => {
      if (!sinkOf(event.target)) return
      // A commit types the chosen text (the text input sends it), which ends the composition; one
      // cancelled ends with nothing to type, so it is taken away here.
      next = event.data ? null : { text: '', cursor: 0 }
      send()
    }
    document.addEventListener('compositionstart', update, true)
    document.addEventListener('compositionupdate', update, true)
    document.addEventListener('compositionend', end, true)
    return () => {
      document.removeEventListener('compositionstart', update, true)
      document.removeEventListener('compositionupdate', update, true)
      document.removeEventListener('compositionend', end, true)
    }
  }, [])
}

/**
 * Puts the grid's text input at `caret` (document points, `inspect().caretRect`) while a cell is
 * typed, after every change of it and every move of the view: the input method opens its
 * candidate window under the field, so beside the cell.
 */
export function useInputAtCaret(caret: number[] | null, views: ViewStore, pins: { left: number; top: number } | null) {
  useEffect(() => {
    const place = () => {
      const view = views.get()
      const field = document.querySelector<HTMLTextAreaElement>('textarea.kui-text-sink')
      const surface = document.querySelector('[data-testid="kit-canvas-surface"]')
      if (!caret || caret.length < 4 || !view || !pins || !field || !surface) return
      const box = surface.getBoundingClientRect()
      const holder = (field.offsetParent ?? document.body).getBoundingClientRect()
      const at = toScreen(view, pins, caret[0], caret[1])
      const height = Math.max(1, (caret[3] - caret[1]) * view.zoom)
      const x = box.left + at.x
      const y = box.top + at.y
      if (![x, y, height].every(Number.isFinite)) return
      field.style.left = `${Math.round(x - holder.left)}px`
      field.style.top = `${Math.round(y - holder.top)}px`
      field.style.bottom = 'auto'
      field.style.height = `${Math.round(height)}px`
      // Over the grid now: a press at the cell's edge still reaches the canvas.
      field.style.pointerEvents = 'none'
    }
    place()
    const stop = views.subscribe(place)
    return () => {
      stop()
    }
  }, [caret?.join(), views, pins?.left, pins?.top])
}
