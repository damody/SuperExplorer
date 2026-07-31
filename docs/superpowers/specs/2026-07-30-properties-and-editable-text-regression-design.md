# Properties and Editable Text Regression Design

## Goal

Restore File Explorer parity for the native Properties command and for pointer editing in the address and search fields. A visible command is not considered functional until a real target-specific result is observed.

## Properties validation and routing

The genuine-pointer UTIT opens the native context menu for an ordinary file, a filesystem folder, and a compatible multi-selection. It selects the live Properties entry and requires the resulting Windows property sheet to identify the expected target and expose real property-page controls. A generic "contents unavailable" dialog, no dialog, the wrong target, or a terminal that reports success without a property sheet fails the case.

Application-owned Properties routing continues to carry the immutable popup selection to the long-lived Shell STA. The disposable worker may discover the canonical verb, but it must not own the resulting property sheet or replace Shell identity with display text or a stale row index.

## Editable text pointer model

Address and search use the same vendored `EditableText` hit-test transform. Pointer coordinates are converted from window space to text-layout space by subtracting the padded inner text origin and current scroll offset exactly once. Mouse down positions the caret, mouse drag extends the selection, and double-click word behavior remains owned by the text control.

Entering address edit mode creates and selects the document once. Later mouse presses while already editing must reuse that entity instead of resetting it and reselecting all text. Search keeps its leading icon padding, but that padding participates in the same inner-bounds coordinate conversion.

## Selection visuals

Focused address and search selections use an Explorer-like strong semantic Highlight color. The editable text renderer paints selection behind glyphs and paints selected glyphs with HighlightText so an opaque high-contrast blue selection remains legible. High-contrast mode uses Windows Highlight and HighlightText roles without alpha dilution.

## Testing

Unit tests cover padded and horizontally scrolled text hit transforms, address-input entity preservation, and semantic selection colors. Headful tests use native pointer input to place carets and drag-select deterministic substrings in both fields, compare UIA selection ranges, capture the strong selection visual, and execute file/folder/multi-selection Properties with real result oracles. The full workspace, architecture, OpenSpec, Release, installer, and installed-path gates remain mandatory.

## Scope

This is a focused regression fix. It does not replace the native Windows property sheet, add rich-text behavior, or change search semantics.
