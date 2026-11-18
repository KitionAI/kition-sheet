# Importing Excel binary workbooks

Kition opens an `.xlsb` file with this kit as a read-only preview (`kit.json` gives XLSB the
`view` role), and the headless kit executable opens one too. Kition Sheet imports worksheet data
into a new workbook and returns a warning describing the conversion limits.

The import reads worksheet names, visibility, numbers, text, booleans, errors and merged
cells. Formula cells become their **last saved results**, not live formulas. Those results
may be out of date if the source application did not recalculate before saving.

Source formatting is omitted: dates, times, percentages and currencies initially display as
their underlying numbers. The workbook's date system is retained. Charts, images, names,
macros and other workbook features are not imported. Kition Sheet does not write XLSB files.

The source XLSB file is not used as the workbook's save destination: `file.save` asks for a new
file (Save As), and requests to save to `.xlsb` return an error. Save the data as a new `.xlsx`
file instead.
