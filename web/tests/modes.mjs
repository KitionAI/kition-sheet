// The editor in each display mode, through the development host: node tests/modes.mjs [chromium|webkit]
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'
import { runModeSmoke } from '@kition/kit-dev-host/mode-smoke'

await runModeSmoke({
  kit: 'kit.sheet',
  config: new URL('../vite.config.ts', import.meta.url).pathname,
  browser: process.argv[2] === 'webkit' ? webkit : chromium,
  createServer,
  line: { tab: 'home', primary: ['sheet-paste', 'sheet-font', 'sheet-size', 'sheet-cmd-home.bold', 'sheet-fill', 'sheet-font-color', 'sheet-merge', 'sheet-number-format', 'sheet-autosum', 'sheet-sort'] },
})
