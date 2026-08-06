# Built-in Size one-decimal formatting design

## Goal

Make the built-in Size column use the same one-decimal precision as Folder size so the same recursive byte count is presented consistently.

## Behavior

- Built-in Size formats KB, MB, GB, and TB with exactly one decimal place.
- Examples include `1.0 KB`, `652.6 KB`, `250.5 GB`, and `1.0 TB`.
- Zero remains `0 KB` and sub-kilobyte nonzero files retain the existing minimum `1.0 KB` presentation.
- Folder size data sources remain Host cache and MFT Service only.
- File and folder byte-count semantics, sorting values, cache keys, and invalidation do not change.
- Folder size plugin rendering remains unchanged because it already uses one-decimal precision.

## Verification

- Unit tests cover zero, sub-kilobyte, exact-unit, fractional-unit, large fractional GB, and TB boundaries.
- The test installer is rebuilt and installed.
- A D:\ Details-view screenshot proves built-in Size and Folder size show matching one-decimal values.
