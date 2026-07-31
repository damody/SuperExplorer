//! Product branding assets and the bounded startup splash lifecycle.

use std::{borrow::Cow, time::Duration};

use explorer_ui::{ExplorerAssets, ExplorerRoot};
use gpui::{
    App, AssetSource, Bounds, Context, ObjectFit, Render, SharedString, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, div, img,
    prelude::*, px, size,
};

pub const SPLASH_HOLD_DURATION: Duration = Duration::from_secs(1);
pub const SPLASH_FADE_DURATION: Duration = Duration::from_millis(180);
const SPLASH_FADE_STEPS: u32 = 12;
const SPLASH_FADE_FRAME: Duration = Duration::from_millis(15);
const SPLASH_WIDTH: f32 = 940.0;
const SPLASH_HEIGHT: f32 = 237.0;
const SPLASH_ASSET_PATH: &str = "branding/super-explorer-splash.png";
const SPLASH_BYTES: &[u8] = include_bytes!("../assets/super-explorer-splash.png");

/// Application assets layered over the reusable Explorer icon assets.
#[derive(Clone, Copy, Debug, Default)]
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if path == SPLASH_ASSET_PATH {
            return Ok(Some(Cow::Borrowed(SPLASH_BYTES)));
        }
        ExplorerAssets.load(path)
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        if path == "branding" {
            return Ok(vec![SharedString::from(SPLASH_ASSET_PATH)]);
        }
        ExplorerAssets.list(path)
    }
}

/// Returns whether production startup should create a splash window.
pub const fn should_show_splash(has_visual_fixture: bool, has_auto_close: bool) -> bool {
    !has_visual_fixture && !has_auto_close
}

/// Root view for the transparent splash popup.
pub struct SplashView {
    opacity: f32,
}

impl Default for SplashView {
    fn default() -> Self {
        Self { opacity: 1.0 }
    }
}

impl Render for SplashView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().opacity(self.opacity).child(
            img(SPLASH_ASSET_PATH)
                .size_full()
                .object_fit(ObjectFit::Contain),
        )
    }
}

fn splash_window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(SPLASH_WIDTH), px(SPLASH_HEIGHT)),
            cx,
        ))),
        titlebar: None,
        focus: true,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        window_min_size: Some(size(px(SPLASH_WIDTH), px(SPLASH_HEIGHT))),
        ..Default::default()
    }
}

/// Opens the splash after the main window and starts its bounded dismissal task.
pub fn open_splash(cx: &mut App, main_window: WindowHandle<ExplorerRoot>) -> anyhow::Result<()> {
    let splash_window = cx.open_window(splash_window_options(cx), |_window, cx| {
        cx.new(|_| SplashView::default())
    })?;

    cx.spawn(async move |cx| {
        cx.background_executor().timer(SPLASH_HOLD_DURATION).await;
        for step in 1..=SPLASH_FADE_STEPS {
            cx.background_executor().timer(SPLASH_FADE_FRAME).await;
            let opacity = 1.0 - step as f32 / SPLASH_FADE_STEPS as f32;
            let update = cx.update(|cx| {
                splash_window.update(cx, |view, _window, cx| {
                    view.opacity = opacity;
                    cx.notify();
                })
            });
            if update.is_err() {
                return;
            }
        }

        let _ = cx.update(|cx| {
            let _ = splash_window.update(cx, |_view, window, _cx| window.remove_window());
            let _ = main_window.update(cx, |_view, window, _cx| window.activate_window());
        });
    })
    .detach();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_timing_is_exact_and_evenly_divisible() {
        assert_eq!(SPLASH_HOLD_DURATION, Duration::from_millis(1_000));
        assert_eq!(SPLASH_FADE_DURATION, Duration::from_millis(180));
        assert_eq!(SPLASH_FADE_FRAME * SPLASH_FADE_STEPS, SPLASH_FADE_DURATION);
    }

    #[test]
    fn deterministic_automation_modes_skip_the_splash() {
        assert!(should_show_splash(false, false));
        assert!(!should_show_splash(true, false));
        assert!(!should_show_splash(false, true));
        assert!(!should_show_splash(true, true));
    }

    #[test]
    fn app_assets_expose_branding_and_existing_explorer_icons() {
        let assets = AppAssets;
        assert_eq!(
            assets.load(SPLASH_ASSET_PATH).unwrap().unwrap().as_ref(),
            SPLASH_BYTES
        );
        assert!(assets.load("fluent/search.svg").unwrap().is_some());
    }
}
