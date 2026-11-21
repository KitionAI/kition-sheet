/** The development host (dev.html): opens `public/sample/Sample.xlsx`, or a file chosen in the page. */
import { startDevHost } from '@kition/sdk/dev-host'

startDevHost({ sample: './sample/Sample.xlsx' })
