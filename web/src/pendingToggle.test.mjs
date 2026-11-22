// A setting the engine keeps, toggled by the person: each click toggles what the control shows,
// which is what the person last asked for until the engine answers; then the engine's state again.
import test from 'node:test'
import assert from 'node:assert/strict'
import { PendingToggle } from './pendingToggle.ts'

/** A send whose answers the test releases (or fails) one by one, in any order. */
function heldSend() {
  const sent = []
  const send = (on) =>
    new Promise((resolve, reject) => {
      sent.push({ on, resolve, reject })
    })
  return { sent, send }
}
const tick = () => new Promise((resolve) => setImmediate(resolve))

test('two clicks before the first answer are two toggles, and the control shows each at once', async () => {
  const toggle = new PendingToggle()
  const { sent, send } = heldSend()
  let engine = true
  let notified = 0
  toggle.subscribe(() => (notified += 1))
  void toggle.click(engine, send)
  assert.equal(toggle.shown(engine), false)
  void toggle.click(engine, send)
  assert.equal(toggle.shown(engine), true)
  assert.deepEqual(sent.map((request) => request.on), [false, true])
  // The engine takes both, in order; the control shows the engine's state once both are back.
  engine = false
  sent[0].resolve('ok')
  await tick()
  assert.equal(toggle.shown(engine), true, 'still the value last asked for while one is on the way')
  engine = true
  sent[1].resolve('ok')
  await tick()
  assert.equal(toggle.pending(), null)
  assert.equal(toggle.shown(engine), true)
  assert.ok(notified >= 3)
})

test('answers out of order: the control settles on the engine state once all are back', async () => {
  const toggle = new PendingToggle()
  const { sent, send } = heldSend()
  void toggle.click(false, send)
  void toggle.click(false, send)
  void toggle.click(false, send)
  assert.deepEqual(sent.map((request) => request.on), [true, false, true])
  assert.equal(toggle.shown(false), true)
  sent[2].resolve()
  sent[0].resolve()
  await tick()
  assert.equal(toggle.shown(false), true, 'one is still on the way')
  sent[1].resolve()
  await tick()
  // Whatever order the engine applied them in, the control now shows what it reports.
  assert.equal(toggle.pending(), null)
  assert.equal(toggle.shown(false), false)
  assert.equal(toggle.shown(true), true)
})

test('a failed request settles on the engine state, not the value asked for', async () => {
  const toggle = new PendingToggle()
  const { sent, send } = heldSend()
  const answer = toggle.click(true, send)
  assert.equal(toggle.shown(true), false)
  sent[0].reject(new Error('refused'))
  await assert.rejects(answer, /refused/)
  await tick()
  assert.equal(toggle.pending(), null)
  assert.equal(toggle.shown(true), true)
  // A send that throws before it returns a promise settles the same way.
  const thrown = toggle.click(true, () => {
    throw new Error('closed')
  })
  await assert.rejects(thrown, /closed/)
  await tick()
  assert.equal(toggle.shown(true), true)
})

test('an agent or another person changing the setting while nothing is on the way shows', () => {
  const toggle = new PendingToggle()
  assert.equal(toggle.shown(true), true)
  assert.equal(toggle.shown(false), false)
  assert.equal(toggle.pending(), null)
})

test('the next click after the engine changed the setting toggles from the engine state', async () => {
  const toggle = new PendingToggle()
  const { sent, send } = heldSend()
  void toggle.click(false, send)
  sent[0].resolve()
  await tick()
  // An agent turned it off again meanwhile: the click turns it on.
  void toggle.click(false, send)
  assert.equal(sent[1].on, true)
})
