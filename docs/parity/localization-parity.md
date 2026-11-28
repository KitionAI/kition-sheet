# Localization parity

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-28 · **Target:** Microsoft Excel (Microsoft 365), Excel for Mac 16.113.4

Per-language status against Excel, which ships its UI in dozens of languages (Windows language
packs cover 100+), with localized function names, argument separators, number and date formats,
right-to-left sheets and full complex-script support. Part of
[target-app-parity.md](target-app-parity.md). **Localization: ~10% ready, 100–165 h** (estimated).

## Where we are (measured from source)

- **Catalogs for the ribbon's labels.** The kit's editor (`web/`) takes its language from the
  host (`host.init.locale`) and loads English-keyed label catalogs for Korean, Russian, Brazilian
  Portuguese and Simplified Chinese (`web/src/i18n/upstream/*.rs`, 266 labels each), plus its own
  `en-US` and `zh-CN` strings (`web/src/strings.ts`). There is no Japanese catalog. Untranslated
  labels fall back to English.
- **About 2,200 user-visible strings** in all (heuristic source scan: ~1,700 UI and command
  strings plus ~500 function descriptions), so each shipped language covers **~12%** (measured
  count / estimated total).
- **en-US only formulas and formats.** Function names, `,` argument separator, `.` decimal
  separator, en-US number and date formats (`crates/engine/numfmt` says "en-US only"). Users ask for
  localized function names and `;` separators.
- **Script support.** The web build has no system fonts: `web/scripts/build-engine.sh` copies
  Droid Sans Fallback from a kition-fonts checkout, and the engine adds it at run time for
  characters its built-in faces lack (Chinese text). There is no complex-text shaping: Arabic and
  Devanagari don't join or reorder, and there is no bidirectional layout. **Sheet Right-to-Left**
  is a stored flag (`view.rightToLeft`) but the grid doesn't mirror. An input method composes
  in the cell being typed, underlined at the caret, with its candidate window beside the cell
  (`sheet.editor.input-method`, web/tests/hand/ime.mjs, as WPS shows it); checked with Chromium's
  composition only, a real Chinese, Japanese or Korean input method not yet by hand.
- **PDF export** writes Latin text in the 14 standard PDF fonts, and other text in a subset of a
  font the host added; scripts without a host font drop out.

## Languages

UI strings translated are measured counts against the ~2,200 estimate. Estimates to `full`
include the rest of the catalog work below (10–20 h, counted once), localized function names,
separators, number/date formats, machine translation with agent review, and script work.

| Language | Code | UI strings translated | Dialogs / tooltips / help | Script support | Native review | Status | Estimate to full |
|---|---|---|---|---|---|---|---|
| English | en | ~2,200 / ~2,200 (100%, source) | yes | Latin: yes | yes | full | — |
| Simplified Chinese (Mandarin) | zh-Hans | 266 (~12%) | no | Droid Sans Fallback added at run time; IME composes in the cell (real input methods not yet tried by hand); PDF with the host's font | no | menus only | 8–13 h |
| Spanish | es | 0 (0%) | no | Latin: yes | no | none | 6–10 h |
| Hindi | hi | 0 (0%) | no | Devanagari: **no shaping** | no | none | 12–18 h |
| Arabic | ar | 0 (0%) | no | **no shaping, no RTL UI, RTL sheet flag not rendered** | no | none | 25–35 h |
| French | fr | 0 (0%) | no | Latin: yes | no | none | 6–10 h |
| Portuguese | pt-BR | 266 (~12%) | no | Latin: yes | no | menus only | 4–8 h |
| Indonesian | id | 0 (0%) | no | Latin: yes | no | none | 5–8 h |
| Japanese | ja | 0 (0%; no Japanese catalog in `web/`) | no | Droid Sans Fallback added at run time, unverified for Japanese; IME unverified; no furigana display | no | none | 8–12 h |
| German | de | 0 (0%) | no | Latin: yes | no | none | 6–10 h |
| Korean | ko | 266 (~12%) | no | Droid Sans Fallback added at run time, unverified for Hangul; IME unverified | no | menus only | 6–10 h |
| Vietnamese | vi | 0 (0%) | no | Latin with stacked diacritics: unverified | no | none | 6–10 h |

Other shipped languages: Russian (ru), 266 labels (~12%), menus only.

**Total:** catalog work 10–20 h + languages 92–144 h ≈ **100–165 h**. Parallelizes almost fully
once the catalog covers dialogs and commands. **Needs a human:** native-speaker review of every
language (none has had one) and of localized function names.

## Infrastructure (do first)

1. Extend the catalog from the ribbon to every user-visible string (dialogs, menus, task panes,
   status and error messages, command labels, function descriptions), with a missing-key check
   in `cargo xtask ci`. The editor already follows the host's language (`host.init.locale`).
2. Locale-aware formulas: localized function-name tables and separators at the formula-bar
   boundary only (files keep en-US invariant names), locale number and date formats, list
   separators in CSV.
3. Text shaping and fallback: a shaping engine for grid, editor and PDF text (Arabic, Devanagari,
   Thai), system font fallback for CJK, bidirectional runs, and a real right-to-left sheet layout;
   PDF text in the scripts the host adds no font for.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
