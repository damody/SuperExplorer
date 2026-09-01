# GPUI Fluent visibility final review

The screenshot regression is closed at the asset contract: every one of the 24 category assets now uses an official Fluent Filled SVG with non-empty path geometry and `currentColor` paint. The unsupported Color SVG gradients/paint servers have been removed from the runtime asset set. PDF and Archive remain on the same visible official Filled assets.

Settings (`.bashrc`, `.profile`, `.sudo_as_admin_successful`), text (`.data`, `.txt`), code (`.py`), archive (`.zip`), PDF, and every other category now traverse the same known-visible GPUI path. A future reintroduction of gradients, `url(#...)`, external references, active content, filters, masks, missing paths, or non-tintable paint fails the compatibility tests.

Classification, 274 suffix rules, Type labels, ADB/SFTP-only selection, directory precedence, local Shell icons, thumbnails, row geometry, remote transfers, and navigation are unchanged. No runtime dependency or network access was introduced.

Current hashes:
- `fluent_assets.rs`: `DEF101976D84A8B0D3701AE886D21B09B8ACD364862AAB52E4429A4EB8338C4E`
- `icons.rs`: `C32ED11A2E8E199150666C82102F6CE4019A70E56CEF8447A5792B25BF24D340`
- `NOTICE.md`: `0F8251812876FC90FD6F3085C4F89C23D5C377BA18EB48FAEC208AD45B989B4E`

Historical Color-asset evidence is retained and points to the corresponding 7.x superseding task. No unresolved scoped P0/P1 issue remains.

