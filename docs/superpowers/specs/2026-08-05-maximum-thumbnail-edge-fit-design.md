# Maximum Thumbnail Edge-Fit Design

## Problem

At maximum icon zoom, a successfully loaded landscape thumbnail is constrained to the square icon host. The item cell is wider than that host, so the thumbnail remains visibly inset from the selection border even though more display space is available. The same rule should also make portrait thumbnails use the available height instead of remaining unnecessarily small.

## Approved behavior

- A real thumbnail uses the complete visual region above the independent file-name area.
- The image preserves its source aspect ratio and is never cropped or distorted.
- Landscape thumbnails grow until they touch the left and right edges of the visual region whenever their aspect ratio permits.
- Portrait thumbnails grow until they touch the top and bottom edges of the visual region whenever their aspect ratio permits.
- Square thumbnails use the largest aspect-preserving size that fits the visual region.
- Any unavoidable whitespace remains only on the axis whose aspect ratio cannot be filled without cropping.
- Folder icons, file-type Shell icons, and generic fallback icons retain the current centered icon-sized presentation.

## Architecture

The UI snapshot must preserve whether a rendered texture came from the thumbnail pipeline or the Shell-icon pipeline. The file-view renderer then selects one of two bounded presentation geometries:

1. `Thumbnail`: fit the source into the full cell content width and the existing icon-region height.
2. `Shell icon`: fit the source into the existing square icon host.

The label remains a separate stacked child below the visual region. Selection borders, hit testing, drag behavior, virtualization, and cache admission are unchanged.

The thumbnail host width is derived from the actual spatial cell content width after subtracting horizontal padding. This keeps the image inside the border across DPI scaling and across the grid's existing per-row width adjustment. The host height remains the configured icon size, so the change does not alter row height or scroll geometry.

## Data flow

1. Thumbnail completion inserts the texture into the visible-item cache with thumbnail provenance.
2. Shell-icon completion and compatible-size fallback insert textures with Shell-icon provenance.
3. The render snapshot carries the texture and provenance for each presentation key.
4. The renderer computes the appropriate host dimensions and applies the existing aspect-fit calculation.

If provenance is absent or uncertain, the renderer must use the Shell-icon geometry. This conservative fallback prevents an icon from being stretched across the cell.

## Error handling

- Zero-sized or invalid source images continue to collapse safely inside the bounded host.
- Missing thumbnails continue through the existing Shell-icon/fallback path.
- A failed thumbnail must never cause a Shell icon to receive thumbnail edge-fit geometry.
- DPI changes and view-mode changes use the current spatial metrics; no persisted pixel geometry is reused.

## Testing

- Unit-test landscape, portrait, and square aspect fitting against the full thumbnail visual region.
- Unit-test that Shell icons retain the square icon host.
- Unit-test that the thumbnail region subtracts horizontal cell padding and never exceeds the selection border.
- Extend the maximum-icon UTIT scenario to assert that the real thumbnail approaches the available horizontal edge while the folder icon remains centered and bounded.
- Run formatting, focused tests, `cargo check -p explorer-ui`, `cargo build -p explorer-app`, manifest parsing, and strict OpenSpec validation.

## Out of scope

- Cropping thumbnails to cover the visual region.
- Changing row height, file-name layout, cache capacity, thumbnail extraction size, or Shell icon acquisition.
- Stretching folder icons or file-type icons to fill the item cell.
