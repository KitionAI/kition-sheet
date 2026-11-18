# Importing OpenDocument spreadsheets

Kition opens an `.ods` file with this kit as a read-only preview (`kit.json` gives ODS the `view`
role), and the headless kit executable opens one too. Kition Sheet imports worksheet data into a
new workbook and returns a warning describing the conversion limits.

The import reads sheets, numbers, text, booleans, supported dates/times, repeated cells/rows and
merged cells. Formula cells become their **last saved results**, not live formulas. These results
may be out of date if the source application did not recalculate before saving. Unsupported or
missing values retain available display text and produce a warning.

This is data import, not full ODS compatibility. Source formatting, formulas, charts, images,
names and other workbook features are not preserved. Kition Sheet does not write ODS files.

The source ODS file is not used as the workbook's save destination: `file.save` asks for a new
file (Save As), and requests to save to `.ods` return an error. Save the data as a new `.xlsx`
file instead.
