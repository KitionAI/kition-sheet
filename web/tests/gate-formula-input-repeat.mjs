// FX-C6 again and again (case sheet.editor.formula-input/fx-c6-repeat): choosing SUM from the cell's
// function list and typing `1,2)` and Enter left A10 empty in 2 of about 8 CI runs, gone on rerun.
// Each round runs the formula-input gate (tests/gate-formula-input.mjs) as it runs in CI, in a new
// process, a new browser and a fresh page load: the lead-up FX-C1 to FX-C5, then FX-C6, which
// asserts A10 holds =SUM(1,2) with the value 3 (polling `ai.inspect()` as the gate's helpers do)
// and prints where the keys went when it fails.
// node tests/gate-formula-input-repeat.mjs [chromium|webkit] [rounds, default 20]
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const which = process.argv[2] ?? 'chromium'
const rounds = Number(process.argv[3] ?? 20)
const gate = fileURLToPath(new URL('./gate-formula-input.mjs', import.meta.url))
const wanted = ['PASS FX-C6 单击 SUM：=SUM(，仍在编辑', 'PASS FX-C6 回车后值为 3', 'PASS FX-C6 A10 为 =SUM(1,2)']

/** One round: the gate's lead-up and FX-C6 in a process of its own; answers its exit code and output. */
const round = () =>
  new Promise((resolve) => {
    const child = spawn(process.execPath, [gate, which], { env: { ...process.env, GATE_ONLY: 'C1,C2,C3,C4,C5,C6' }, stdio: ['ignore', 'pipe', 'pipe'] })
    let output = ''
    child.stdout.on('data', (data) => (output += data))
    child.stderr.on('data', (data) => (output += data))
    child.on('close', (code) => resolve({ code, output }))
  })

const started = Date.now()
let failed = 0
for (let n = 1; n <= rounds; n += 1) {
  const at = Date.now()
  const { code, output } = await round()
  const missing = wanted.filter((line) => !output.includes(line))
  const ok = code === 0 && missing.length === 0
  if (!ok) failed += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${which} round ${n}/${rounds} (${((Date.now() - at) / 1000).toFixed(1)} s)`)
  if (!ok) console.log(`exit ${code}; missing ${JSON.stringify(missing)}\n${output}`)
}
console.log(`${which}: FX-C6 ${rounds - failed} of ${rounds} rounds passed in ${((Date.now() - started) / 1000).toFixed(0)} s`)
process.exit(failed ? 1 : 0)
