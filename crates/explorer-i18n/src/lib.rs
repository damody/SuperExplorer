#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
//! Fluent localization catalogs and closed `AppLocale` set for `SuperExplorer` and `SuperDesktop`.

mod bundle;
mod locale;

pub use bundle::Catalog;
pub use fluent_bundle::FluentArgs;
pub use locale::AppLocale;
