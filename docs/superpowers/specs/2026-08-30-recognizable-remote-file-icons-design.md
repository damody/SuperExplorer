# Recognizable Remote File Icons Design

## Problem

The first remote icon renderer places every category inside the same white document silhouette and hides its text badge below 24 logical pixels. Details view therefore collapses all categories to an almost identical page with a colored bottom stripe. Color alone is not a sufficient identifier, and Linux dotfiles are visually indistinguishable from ordinary text files.

## Decision

Replace the color-strip renderer with category-specific vector geometry designed first for 16–20 logical pixels. Each category must retain a distinct monochrome silhouette before accent color or text is considered:

- generic file: plain folded page;
- PDF: red page with a central Acrobat-like three-arm mark;
- text: blue page with three horizontal text rules;
- settings/dotfile: slate page with a central gear/ring-and-teeth mark;
- image: green landscape frame with a sun dot and mountain diagonals;
- archive: amber package with a central vertical zipper and alternating teeth;
- audio: magenta musical-note stem and head;
- video: purple rounded frame with a play triangle;
- code: teal opposing angle brackets;
- executable/binary: gray microchip with a central die and edge pins;
- office: blue document with a prominent `W`-like block mark.

Large views may add a short category label, but labels and color are secondary reinforcement. At 16–20px the geometry itself must be visible and distinct.

## Architecture

`RemoteFileIconKind` gains a `Settings` category. The existing filename classifier maps valid single-component dotfiles to `Settings`; ordinary text and configuration extensions remain `Text`. Type labels do not change.

`RemoteFileIconSpec` describes stable accessibility text, accent color, and a geometry discriminator. `remote_file_icon` delegates to small category-specific GPUI primitive renderers rather than sharing one page-and-stripe body. No external assets, font glyphs, Windows registry associations, remote I/O, or file-content inspection are introduced.

The ADB/SFTP-only fallback selection and folder/local precedence remain unchanged.

## Testing

Tests must prove that every category has unique accessible text, color, and geometry; dotfiles select `Settings`; requested formats retain their existing categories; all geometry stays within 16, 20, 64, 256, and 512px hosts; and remote-only/local-preservation selection remains intact. The affected crates must format, compile, and pass focused model/UI tests, followed by strict OpenSpec and evidence reconciliation.

## Rollback

Reverting the `Settings` enum case and the renderer restores the first category implementation without affecting Type labels, remote identities, navigation, or stored data.
