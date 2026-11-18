# Function parity: worksheet functions and formulas

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft Excel (Microsoft 365), Excel for Mac 16.113.4; WPS Office for Mac 7.5.1 for results

Excel's worksheet functions, category by category, against Kition Sheet's function library. Part of
[target-app-parity.md](target-app-parity.md); gaps feed [gaps.md](../gaps.md).

## How this is measured

- **Excel's list (523):** Microsoft's "Excel functions (alphabetical)" support page, fetched and
  parsed (506 rows with their category and the Excel version that introduced
  them), plus 14 text functions the page lists under combined entries (FIND/FINDB, LEFT/LEFTB,
  LEN/LENB, MID/MIDB, REPLACE/REPLACEB, RIGHT/RIGHTB, SEARCH/SEARCHB), BETA.INV, and COPILOT and
  PY (Microsoft 365 functions not on that page when fetched).
- **Ours (501):** every function spec in `crates/engine/functions/src/*.rs` (`f!("NAME", …)`) plus the
  evaluator's special forms in `crates/engine/calc/src/eval.rs` (`SPECIAL_FUNCTIONS`: IF, IFS, CHOOSE,
  OFFSET, INDIRECT, INDEX, SUBTOTAL, AGGREGATE, TEXT, LET, LAMBDA, MAP, REDUCE, SCAN, BYROW, BYCOL,
  MAKEARRAY…), excluding the test-only `ADD2`. Every one is an Excel name.
- **Working** excludes stubs that return an error by design: the 7 CUBE functions (`#N/A`, no
  cube connections) and WEBSERVICE and FILTERXML (`#VALUE!`). Excel for Mac also lacks
  WEBSERVICE and FILTERXML; Excel for Windows has them.
- The measurement is **presence**. Depth (edge cases matching Excel) is estimated below.

## Coverage

**Measured: 501 / 523 by name (95.8%); 492 / 523 working (94.1%).**

| Category | Excel | Ours | Working | % working | Missing |
|---|---|---|---|---|---|
| Statistical | 111 | 107 | 107 | 96% | FORECAST.ETS, FORECAST.ETS.CONFINT, FORECAST.ETS.SEASONALITY, FORECAST.ETS.STAT |
| Math and trigonometry | 80 | 79 | 79 | 99% | PERCENTOF |
| Financial | 55 | 51 | 51 | 93% | ODDFPRICE, ODDFYIELD, ODDLPRICE, ODDLYIELD |
| Engineering | 54 | 54 | 54 | 100% | — |
| Text | 50 | 47 | 47 | 94% | BAHTTEXT, DETECTLANGUAGE, TRANSLATE |
| Lookup and reference | 40 | 36 | 36 | 90% | GETPIVOTDATA, IMAGE, RTD, TRIMRANGE |
| Compatibility | 40 | 40 | 40 | 100% | — |
| Date and time | 25 | 25 | 25 | 100% | — |
| Information | 22 | 21 | 21 | 95% | STOCKHISTORY |
| Logical | 19 | 19 | 19 | 100% | — |
| Database | 12 | 12 | 12 | 100% | — |
| Cube | 7 | 7 | 0 | 0% | (all 7 are `#N/A` stubs) |
| Web | 3 | 3 | 1 | 33% | (WEBSERVICE, FILTERXML are stubs; ENCODEURL works) |
| Add-in and Automation | 3 | 0 | 0 | 0% | CALL, EUROCONVERT, REGISTER.ID |
| AI and Python | 2 | 0 | 0 | 0% | COPILOT, PY |
| **Total** | **523** | **501** | **492** | **94.1%** | 22 missing, 9 stubs |

By the Excel version that introduced them (working / total): before 2010 342 / 360, 2010
56 / 56, 2013 49 / 51, 2016 1 / 5, 2019 6 / 6, 2021 11 / 11, 2024 22 / 23, Microsoft 365 5 / 11.
Dynamic-array and LAMBDA-era functions (FILTER, SORT, UNIQUE, XLOOKUP, VSTACK, TAKE, GROUPBY,
PIVOTBY, REGEXTEST…) are all present.

### Scope of what's missing

| Function(s) | In scope? | Estimate | Note |
|---|---|---|---|
| FORECAST.ETS family (4) | yes | 4–6 h | AAA exponential smoothing with seasonality detection; needed for Forecast Sheet too |
| ODDFPRICE, ODDFYIELD, ODDLPRICE, ODDLYIELD | yes | 2–4 h | Odd first/last period bond maths |
| GETPIVOTDATA | yes | 2–3 h | Needs the pivot engine's result cache; also the "Generate GetPivotData" option |
| TRIMRANGE, PERCENTOF, BAHTTEXT | yes | 1–2 h | TRIMRANGE also implies the `.:` trim-ref operators |
| IMAGE | yes | 3–5 h | Pictures in cells (see [cell-pictures.md](../features/cell-pictures.md)); fetching URLs needs a privacy decision |
| EUROCONVERT | yes | 0.5 h | Fixed conversion table |
| CUBE* (7), WEBSERVICE, FILTERXML | partly | 4–8 h | FILTERXML can work offline; CUBE needs an OLAP/data-model source |
| TRANSLATE, DETECTLANGUAGE, STOCKHISTORY, COPILOT, PY | no (cloud) | — | Microsoft cloud services; open equivalents only by maintainer decision |
| CALL, REGISTER.ID, RTD | no | — | Native DLL/COM calls; not portable or safe |

## Formula language

| Feature | Status |
|---|---|
| A1 and R1C1 references, absolute/relative/mixed, F4 cycling | done |
| 3-D references (`Sheet1:Sheet3!A1`) | done |
| Structured references (`Table1[Col]`, `[#Totals]`, `@`) | done |
| Defined names (workbook and sheet scope), Create from Selection | done |
| Dynamic arrays, spill, `#SPILL!`, spill operator `A1#` | done |
| Implicit intersection `@`, legacy CSE array formulas | done; a legacy array formula fills its range on recalculation |
| LET, LAMBDA, named LAMBDAs, ISOMITTED, MAP/REDUCE/SCAN/BYROW/BYCOL/MAKEARRAY | done |
| Iterative calculation for circular references | done: with the setting on, a circular reference is evaluated again from the previous round's values up to the maximum iterations or until no value moves more than the maximum change (saved in `calcPr`); with it off, the cells show 0 as in WPS and Excel and Error Checking lists them |
| 15-significant-digit semantics | done: comparisons at 15 digits; `+`/`-`, SUM and AVERAGE give exactly 0 for binary noise (`0.1+0.2-0.3`); INT, MOD, TRUNC, ROUNDUP… round their argument to 15 digits first (`INT(4.35*100)` is 435); typed numbers keep 15 digits; `&` writes numbers as WPS does |
| External workbook references (`[Book.xlsx]Sheet1!A1`) | parsed; links dropped on open, no update |
| Localized function names and `;` separators | **missing**; en-US only |
| Error values | the seven classic errors plus `#SPILL!`, `#CALC!`, `#GETTING_DATA` done; `#FIELD!`, `#BLOCKED!`, `#CONNECT!`, `#BUSY!`, `#UNKNOWN!`, `#PYTHON!` missing (minor: they come from data types, cloud and Python features) |

## Depth (measured against WPS)

**~92% ready.** `crates/engine/engine/tests/formula_oracle.rs` checks 994 formulas over 245
functions (aggregation, conditional sums and counts, rounding, logic, lookups, text including the
DBCS byte functions and Chinese text, dates, statistics, finance, dynamic arrays, LET/LAMBDA) and the
coercion, precision and error rules against WPS for Mac 7.5.1: the formulas were written to a
workbook without cached values, recalculated and saved by WPS, and its cached values committed as
our own data (`tests/data/formula_oracle.tsv`). All of them match. 39 rows are annotated: functions WPS for Mac 7.5 lacks (CEILING.MATH, ISOWEEKNUM,
QUARTILE.INC, EFFECT…) carry Microsoft's documented result; results that depend on the Chinese
locale (`2026年11月10日` as a date, `aaaa`, `[DBNum1]`) carry the zh-CN result, the oracle's WPS ran in
an English region; RATE/IRR allow 1e-12 (WPS stops a few units short of the last digit).

Deliberate differences from WPS, each a row of the data: an empty FILTER is `#CALC!` (WPS 7.5:
`#VALUE!`); a zero-interest RATE is 0 (WPS: `#NUM!`); EDATE/EOMONTH land on the fictitious
29 February 1900 as Excel does (WPS: 28 February). Followed WPS where Excel differs: DATEDIF with
the start after the end counts backwards (Excel `#NUM!`), an odd root of a negative number is real
(`(-8)^(1/3)` is -2), a negative number formatted to all zeros shows no minus sign.

Still to measure against WPS: the long tail of statistical distributions, engineering and
database functions (covered by engine tests only), and the locale-dependent rows in a Chinese
region.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
