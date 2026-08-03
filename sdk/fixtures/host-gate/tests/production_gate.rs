use explorer_extension_host::{ExtensionHost, ExtensionHostConfigV1, LocalDeveloperModeV1};

#[test]
fn script_produced_sepack_reaches_production_native_lifecycle() {
    let archive = std::env::var_os("SUPEREXPLORER_TEST_SEPACK_PATH")
        .map(std::path::PathBuf::from)
        .expect("SUPEREXPLORER_TEST_SEPACK_PATH must identify the script-produced archive");
    let config = ExtensionHostConfigV1::default()
        .with_local_developer_mode(LocalDeveloperModeV1::Enabled)
        .with_local_developer_archives([archive]);
    let mut host = ExtensionHost::with_config(config);
    host.start().expect(
        "script-produced package must traverse ExtensionHost import, validation, resolution, and native lifecycle",
    );
    let [admission] = host.startup_admissions() else {
        panic!("script-produced package must produce exactly one host startup admission");
    };
    assert_eq!(admission.root_count, 1);
    assert_eq!(admission.activated_feature_count, 1);
    host.shutdown();
}
