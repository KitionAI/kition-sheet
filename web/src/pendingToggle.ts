import { useSyncExternalStore } from 'react'

/**
 * An on/off setting the engine keeps (View › Show's Formula Bar, Gridlines and Headings, 高亮行列)
 * shown as a check box or a pressed button. A click asks for the opposite of what the control
 * SHOWS, and the control shows what the person last asked for while their requests are on the
 * way: two clicks before the engine answers the first are two toggles, not the same value sent
 * twice. Once every request has come back (answered, refused or failed, in any order) the control
 * shows the engine's state again, so it never stays on a value the engine did not take, and an
 * agent's or another person's change shows whenever nothing is on the way.
 */
export class PendingToggle {
  /** What the person last asked for, while a request is on the way; null when none is. */
  #asked: boolean | null = null
  #inFlight = 0
  readonly #listeners = new Set<() => void>()

  /** What the control shows, given the engine's state. */
  shown(engine: boolean): boolean {
    return this.#asked ?? engine
  }

  /** The value asked for while a request is on the way, or null. */
  pending = (): boolean | null => this.#asked

  subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener)
    return () => this.#listeners.delete(listener)
  }

  /**
   * A click: sends the opposite of what the control shows (`engine` is the engine's state as last
   * reported) and shows it at once. Returns `send`'s answer, failure included.
   */
  click<T>(engine: boolean, send: (on: boolean) => Promise<T>): Promise<T> {
    const on = !this.shown(engine)
    this.#asked = on
    this.#inFlight += 1
    this.#notify()
    let answer: Promise<T>
    try {
      answer = send(on)
    } catch (error) {
      answer = Promise.reject(error)
    }
    const settle = () => {
      this.#inFlight -= 1
      if (this.#inFlight > 0) return
      this.#asked = null
      this.#notify()
    }
    answer.then(settle, settle)
    return answer
  }

  #notify() {
    for (const listener of this.#listeners) listener()
  }
}

const shared = new Map<string, PendingToggle>()

/**
 * The editor's one PendingToggle for a setting (by its command id), so every control showing it
 * (高亮行列 sits on the View tab and in the status bar) shares what was asked for.
 */
export function pendingToggle(key: string): PendingToggle {
  let toggle = shared.get(key)
  if (!toggle) {
    toggle = new PendingToggle()
    shared.set(key, toggle)
  }
  return toggle
}

/**
 * A control's side of a PendingToggle: what it shows (the value asked for while a request is on
 * the way, else the engine's `engine`) and its click, which sends through `send`.
 */
export function usePendingToggle<T>(key: string, engine: boolean, send: (on: boolean) => Promise<T>): [on: boolean, click: () => Promise<T>] {
  const toggle = pendingToggle(key)
  const asked = useSyncExternalStore(toggle.subscribe, toggle.pending)
  return [asked ?? engine, () => toggle.click(engine, send)]
}
