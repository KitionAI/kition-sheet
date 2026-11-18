# Roadmap

> **Last updated:** 2026-11-17 · **Source:** `features.yaml` (updated 2026-11-17) and the engine's command list

Open work on Kition Sheet, in order. Each item names where its detail lives.

1. **Prove the P0 features end to end:** `sheet.agent.chart-premises`, `sheet.agent.env-cells`, `sheet.agent.env-replay`, `sheet.agent.format-premises`, `sheet.agent.formula-metadata`, `sheet.agent.independent-reading`, `sheet.agent.named-destination`, `sheet.agent.precise-families`, `sheet.agent.premises`: each needs a case that shows it working in the editor and surviving save and reopen before it is `has`. Detail in [gaps.md](gaps.md).
2. **Then the P1 features:** `sheet.agent.axis-premises`, `sheet.cells.selection`, `sheet.charts.design`, `sheet.data.import`, `sheet.data.sort-filter`, `sheet.data.tools`, `sheet.data.what-if`, `sheet.drawing.objects`, `sheet.edit.clipboard`, `sheet.edit.fill-clear`, `sheet.edit.find-replace`, `sheet.file.export-print`, `sheet.file.workbooks`, `sheet.format.conditional`, `sheet.formulas.auditing`, `sheet.formulas.functions`, `sheet.insert.objects`, `sheet.page.setup`, `sheet.pivot.tables`, `sheet.review.comments`, `sheet.review.protect-proof`, `sheet.tables.design`.
3. **Then the P2 features:** `sheet.automate.scripts`, `sheet.draw.ink-tools`, `sheet.insert.smart-graphic`, `sheet.page.themes`.
4. **Keep the catalogue complete:** every new engine command joins a feature in `features.yaml` with a case.
