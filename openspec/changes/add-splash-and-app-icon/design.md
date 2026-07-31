## Context

`explorer-app` is the Windows-only GPUI composition root. Its `app.rc` currently contains version information but no icon, and `ApplicationLifecycle::run_gpui` creates only the main Explorer window. GPUI-CE supports transparent popup windows and asynchronous timers, so the branding flow can remain inside the existing event loop without a second UI framework.

The authoritative visual source is the user-provided 1254×1254 artwork. The working tree contains unrelated modifications, so this change must remain isolated to application startup, resources, new assets, and focused tests.

## Goals / Non-Goals

**Goals:**

- Show a crisp transparent SuperExplorer wordmark above a concurrently loading main window.
- Hold the splash for 1 second and fade it out over 180 milliseconds.
- Embed one consistent multi-resolution icon into the Windows executable.
- Keep startup resilient and automated visual runs deterministic.
- Validate generated assets and startup policy with focused tests.

**Non-Goals:**

- Loading progress, status text, sound, configuration, or telemetry UI.
- Redesigning or regenerating the supplied branding.
- Modifying Explorer content, navigation, or session behavior.
- Adding another runtime UI or image-processing dependency.

## Decisions

### Use deterministic local asset derivation

Crop the upper wordmark and lower largest square icon from the supplied source, convert near-white splash pixels to alpha with a soft matte, and use high-quality resampling for ICO frames. This preserves the approved artwork exactly and makes output reviewable. Generative image editing was rejected because it could alter logo geometry, text, or colors.

### Use a separate transparent GPUI popup

Open the existing main window first, then immediately create a centered `WindowKind::PopUp` with transparent background, no title bar, and fixed bounds. This matches the requested layering while allowing the main UI and services to initialize normally. An in-window overlay was rejected because it would be constrained to main-window chrome; a native Win32 layered window was rejected because GPUI already provides the required transparency and lifecycle integration.

### Keep the splash view application-local

Place the splash view and timing policy in `explorer-app`, not `explorer-ui`. Startup branding is process composition behavior and should not become part of the reusable Explorer content surface. The view owns only opacity and asset rendering.

### Drive dismissal from the GPUI executor

After the first rendered frame, hold for 1 second, then update opacity in bounded animation steps across 180 milliseconds. Remove the splash window after the final step and activate the main window. Window-handle update failures are ignored after recording relevant startup diagnostics because they can legitimately indicate that the user or application already closed a window.

### Embed the ICO through the existing resource build

Add the icon entry to `app.rc` and retain the current `embed-resource` build script. A single multi-image ICO is used rather than independent PNG resources because Windows shell surfaces select the most appropriate embedded icon frame.

### Skip branding in deterministic automation modes

Do not create the splash when a visual fixture is active or `EXPLORER_AUTO_CLOSE_MS` is set. Those paths validate Explorer content and rely on predictable window counts and timing.

## Risks / Trade-offs

- [Transparent edges can show a white halo] → Use a soft alpha matte, inspect the final PNG over light and dark backgrounds, and contract the matte only if needed.
- [Opening the main window first can expose a brief frame] → Create and activate the splash in the same GPUI startup callback immediately after the main window handle is returned.
- [A second window can affect quit behavior] → Retain the existing rule that quits only when no windows remain; removing the splash while the main window exists cannot terminate the app.
- [Windows icon caching can hide a new icon during manual checks] → Validate the executable resource directly and treat shell cache refresh as an external display concern.
- [Animation scheduling can outlive a closed window] → Use fallible window-handle updates and make missing handles a benign terminal state.

## Migration Plan

Add assets, resource metadata, splash module, startup integration, and tests in that order. The change is backward-compatible and requires no user data migration. Rollback consists of removing the splash startup call, icon resource entry, and generated assets.

## Open Questions

None. The source artwork, transparency behavior, layering, hold duration, fade duration, and automation policy are fixed by the approved design.
