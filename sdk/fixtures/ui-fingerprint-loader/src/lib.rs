use std::sync::atomic::{AtomicUsize, Ordering};
use superexplorer_ui_abi_fingerprint::{
    CompatibilityDiagnostic, UiAbiFingerprint, compare_before_callback,
};
pub struct Loader {
    callback_count: AtomicUsize,
}
impl Default for Loader {
    fn default() -> Self {
        Self::new()
    }
}
impl Loader {
    pub fn new() -> Self {
        Self {
            callback_count: AtomicUsize::new(0),
        }
    }
    pub fn load(
        &self,
        host: &UiAbiFingerprint,
        plugin: &UiAbiFingerprint,
    ) -> Result<(), CompatibilityDiagnostic> {
        compare_before_callback(host, plugin)?;
        self.callback_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    pub fn callbacks(&self) -> usize {
        self.callback_count.load(Ordering::SeqCst)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn f(bundle: &str, hash: &str) -> UiAbiFingerprint {
        UiAbiFingerprint {
            bundle_id: bundle.into(),
            fingerprint: hash.into(),
        }
    }
    #[test]
    fn mismatch_never_calls() {
        let l = Loader::new();
        let d = l.load(&f("host", "a"), &f("plugin", "b")).unwrap_err();
        assert_eq!(l.callbacks(), 0);
        assert_eq!(d.host_bundle_id, "host");
        assert_eq!(d.plugin_bundle_id, "plugin");
    }
    #[test]
    fn same_fingerprint_different_bundles_calls() {
        let l = Loader::new();
        l.load(&f("host", "a"), &f("plugin", "a")).unwrap();
        assert_eq!(l.callbacks(), 1);
    }
}
