# SuperExplorer Splash and Application Icon Design

## Goal

Use the user-provided SuperExplorer artwork to add a polished startup splash and a consistent Windows application icon.

## Confirmed Experience

- Use `codex-clipboard-6f60ef50-7d1d-4276-9124-f8e367c16d55.png` as the authoritative source image.
- The upper SuperExplorer wordmark becomes the splash artwork.
- The splash background is transparent while the logo remains crisp and opaque.
- The normal Explorer window starts loading immediately behind the splash.
- The splash remains visible for 1 second, then fades out over 180 milliseconds.
- The largest lower square artwork becomes the application icon.
- The application icon must appear on the executable, taskbar, Alt+Tab surface, and window chrome wherever Windows uses the executable resource.

## Architecture

### Assets

Store final project assets under `crates/explorer-app/assets/`:

- `super-explorer-splash.png`: tightly cropped upper wordmark with an alpha channel.
- `super-explorer.ico`: a multi-image Windows icon containing 16, 24, 32, 48, 64, 128, and 256 pixel entries.

Asset derivation is deterministic. Crop the supplied artwork, convert the near-white background to alpha with a soft edge transition, and preserve the original logo colors. Generate all icon sizes from the largest lower square source crop with high-quality downsampling.

### Windows executable identity

Add the ICO as the application icon resource in `crates/explorer-app/app.rc`. The existing `embed-resource` build step will compile it into `SuperExplorer.exe`. The resource is the authoritative Windows icon for File Explorer, shortcuts, the taskbar, Alt+Tab, and native window surfaces.

### Splash window

Add a focused splash view owned by `explorer-app`, separate from the main Explorer UI. It uses a centered, undecorated, non-resizable GPUI popup with `WindowBackgroundAppearance::Transparent`. The view renders the splash PNG without tinting and exposes only the opacity state needed by the fade.

The existing main window is opened first and begins its normal service and UI initialization. The splash popup is opened immediately afterward and activated so it remains above the main window. The splash does not block background initialization.

After the splash has rendered, an asynchronous GPUI timer holds it for 1 second. A short frame-based animation reduces opacity from 1.0 to 0.0 over 180 milliseconds, then removes the splash window and activates the main window. Closing the splash must not trigger application shutdown while the main window remains open.

## Data and Control Flow

1. Windows loads the executable and its embedded application icon.
2. `ApplicationLifecycle::run_gpui` initializes GPUI and opens the normal Explorer window using the existing path.
3. Production startup opens the transparent splash popup using the packaged PNG.
4. The main Explorer window continues loading services, session state, icons, and content behind the popup.
5. After 1 second, the splash opacity animates to zero over 180 milliseconds.
6. The splash window is removed and focus returns to the Explorer window.

## Exceptional Cases

- If the splash window cannot be created, record a diagnostic warning and continue with the main Explorer window; startup must not fail solely because of branding UI.
- If the splash asset cannot be decoded, the asset validation test fails during development. Production still continues without blocking the Explorer window.
- If the main window cannot be created, preserve the existing controlled launch failure behavior and close any splash that was created.
- Timer or window-close races are treated as no-ops when the target handle is already gone.

## Test and Automation Policy

- Skip the splash when `EXPLORER_VISUAL_FIXTURE` is active so existing deterministic screenshots remain unchanged.
- Skip the splash whenever `EXPLORER_AUTO_CLOSE_MS` is set, which is the existing automated launch path.
- Add unit tests for splash timing constants, skip-policy decisions, asset presence, PNG alpha, and ICO frame sizes.
- Keep existing product identity and window title tests passing.
- Run focused `explorer-app` tests, workspace formatting and checks, and a headful smoke launch that confirms the main window remains usable after the splash closes.

## Scope Boundaries

- No settings toggle, progress text, sound, or loading telemetry is added.
- No changes are made to the supplied logo design beyond cropping, background transparency, and size conversion.
- Existing unrelated working-tree changes remain untouched.
