use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use explorer_extension_host::{
    FeatureRuntimeFactV1, NativeCallOperationV1, NativeCallTerminalV1, NativeExtensionLifecycleV1,
    NativeFeatureStateV1, NativeLifecycleConfigV1, NativeLifecycleErrorV1,
    NativeLoaderDiagnosticCodeV1, NativeRestartReasonV1, NativeSafeModeIncidentKindV1,
    NativeSafeModeIncidentV1, NativeStartupAdmissionV1, PackageResolverV1,
    PackageValidationRequestV1, PackageValidatorV1, SealedPackageStoreV1,
    TrustedPublisherKeyStoreV1, TrustedPublisherKeyV1,
};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const CALLBACK_MARKER_ENV: &str = "EXTENSION_DLL_LOADER_CONTRACT_MARKER";
const STATE_DIR_ENV: &str = "EXTENSION_DLL_LOADER_CONTRACT_STATE_DIR";
const FIXTURE_PACKAGE_ID: &str = "fixture.loader";
const FIXTURE_ENTRYPOINT_ID: &str = "data";
const FIXTURE_ROOT_MODULE_ID: &str = "root-contract-v1";
const FIXTURE_INTERFACE_NAMESPACE: u32 = 0x5345_0001;
const FIXTURE_PRIMARY_INTERFACE_VALUE: u64 = 201;
const DEFAULT_NATIVE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("extension DLL loader contract: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let fingerprint = env::var("SUPEREXPLORER_UI_ABI_FINGERPRINT")
        .map_err(|_| "canonical fingerprint environment missing")?;
    let state_dir = env::var_os(STATE_DIR_ENV)
        .map(PathBuf::from)
        .ok_or("dedicated lifecycle state directory environment missing")?;
    match arguments.as_slice() {
        [scenario, first] if scenario == "data" => {
            expect_load(
                &fingerprint,
                false,
                None,
                &[("data", Path::new(first))],
                &state_dir,
                &["register:primary"],
            )
        }
        [scenario, first] if scenario == "gpui-exact" => {
            expect_load(
                &fingerprint,
                true,
                Some(&fingerprint),
                &[("gpui", Path::new(first))],
                &state_dir,
                &["register:primary"],
            )
        }
        [scenario, first] if scenario == "gpui-missing-binary" => expect_reject(
            &fingerprint,
            true,
            Some(&fingerprint),
            &[("missing", Path::new(first))],
            NativeLoaderDiagnosticCodeV1::MissingBinaryUiFingerprint,
            &state_dir,
        ),
        [scenario, first] if scenario == "gpui-wrong-binary" => expect_reject(
            &fingerprint,
            true,
            Some(&fingerprint),
            &[("wrong", Path::new(first))],
            NativeLoaderDiagnosticCodeV1::BinaryUiFingerprintMismatch,
            &state_dir,
        ),
        [scenario, first] if scenario == "gpui-wrong-manifest" => {
            let wrong_fingerprint = "0".repeat(64);
            expect_reject(
                &fingerprint,
                true,
                Some(&wrong_fingerprint),
                &[("gpui", Path::new(first))],
                NativeLoaderDiagnosticCodeV1::GpuiFingerprintMismatch,
                &state_dir,
            )
        }
        [scenario, first] if scenario == "raw-abort" => {
            // The fixture DLL aborts the process after its registrar marker is
            // durable. Reaching this return is itself a contract failure.
            let _ = load(
                &fingerprint,
                false,
                None,
                &[("data", Path::new(first))],
                &state_dir,
            )
            .map_err(|error| error.to_string())?;
            Err("raw-abort fixture unexpectedly returned from the registrar".to_owned())
        }
        [scenario, first] if scenario == "safe-mode-blocked" => expect_safe_mode_blocked(
            &fingerprint,
            Path::new(first),
            &state_dir,
        ),
        [scenario, first] if scenario == "safe-mode-confirm" => expect_safe_mode_confirmed(
            &fingerprint,
            Path::new(first),
            &state_dir,
        ),
        [scenario, first] if scenario == "slow" => {
            expect_slow_callback(&fingerprint, Path::new(first), &state_dir)
        }
        [scenario, first] if scenario == "drain-timeout" => {
            expect_drain_timeout_is_sticky(&fingerprint, Path::new(first), &state_dir)
        }
        [scenario, first, second] if scenario == "two-roots" => {
            let admission = load(
                &fingerprint,
                false,
                None,
                &[("first", Path::new(first)), ("second", Path::new(second))],
                &state_dir,
            )
            .map_err(|error| error.to_string())?;
            if admission.root_count != 2 {
                return Err("startup admission did not retain both distinct DLL roots".to_owned());
            }
            assert_callback_marker(&["register:primary", "register:alternate"])?;
            assert_call_markers_empty(&state_dir)
        }
        [scenario, first, second] if scenario == "batch-invalid" => expect_reject(
            &fingerprint,
            false,
            None,
            &[("first", Path::new(first)), ("invalid", Path::new(second))],
            NativeLoaderDiagnosticCodeV1::InvalidAbiRoot,
            &state_dir,
        ),
        _ => Err("usage: runner <data|gpui-exact|gpui-missing-binary|gpui-wrong-binary|gpui-wrong-manifest|raw-abort|safe-mode-blocked|safe-mode-confirm|slow|drain-timeout> <dll> | <two-roots|batch-invalid> <dll> <dll>".to_owned()),
    }
}

fn expect_load(
    fingerprint: &str,
    gpui: bool,
    manifest_fingerprint: Option<&str>,
    dlls: &[(&str, &Path)],
    state_dir: &Path,
    expected_callbacks: &[&str],
) -> Result<(), String> {
    let admission = load(fingerprint, gpui, manifest_fingerprint, dlls, state_dir)
        .map_err(|error| error.to_string())?;
    if admission.root_count != dlls.len() {
        return Err("startup admission returned a partial root set".to_owned());
    }
    assert_callback_marker(expected_callbacks)?;
    assert_call_markers_empty(state_dir)
}

fn expect_reject(
    fingerprint: &str,
    gpui: bool,
    manifest_fingerprint: Option<&str>,
    dlls: &[(&str, &Path)],
    expected_diagnostic: NativeLoaderDiagnosticCodeV1,
    state_dir: &Path,
) -> Result<(), String> {
    match load(fingerprint, gpui, manifest_fingerprint, dlls, state_dir) {
        Ok(_) => return Err("invalid package unexpectedly loaded".to_owned()),
        Err(LoadFailure::Lifecycle(NativeLifecycleErrorV1::LoaderRejected { diagnostic }))
            if diagnostic == expected_diagnostic => {}
        Err(LoadFailure::Lifecycle(NativeLifecycleErrorV1::LoaderRejected { diagnostic })) => {
            return Err(format!(
                "expected loader diagnostic {expected_diagnostic:?}, got {diagnostic:?}"
            ));
        }
        Err(LoadFailure::Lifecycle(error)) => {
            return Err(format!(
                "expected loader rejection, got lifecycle error: {error}"
            ));
        }
        Err(LoadFailure::Setup(error)) => {
            return Err(format!(
                "fixture setup failed before lifecycle admission: {error}"
            ));
        }
    }
    assert_callback_marker_absent()?;
    assert_call_markers_empty(state_dir)
}

fn expect_safe_mode_blocked(fingerprint: &str, dll: &Path, state_dir: &Path) -> Result<(), String> {
    match load_with_lifecycle(
        fingerprint,
        false,
        None,
        &[("data", dll)],
        state_dir,
        Duration::from_secs(1),
        |lifecycle| assert_fixture_registrar_incident(lifecycle, state_dir).map(|_| ()),
        |_| Ok(()),
    ) {
        Err(LoadFailure::Lifecycle(NativeLifecycleErrorV1::SafeModeDenied)) => {}
        Err(error) => {
            return Err(format!(
                "expected recovered Safe Mode denial before callback, got {error}"
            ));
        }
        Ok(_) => {
            return Err("Safe Mode unexpectedly admitted the crash residue callback".to_owned());
        }
    }
    assert_callback_marker_absent()
}

fn expect_safe_mode_confirmed(
    fingerprint: &str,
    dll: &Path,
    state_dir: &Path,
) -> Result<(), String> {
    let admission = load_with_lifecycle(
        fingerprint,
        false,
        None,
        &[("data", dll)],
        state_dir,
        Duration::from_secs(1),
        |lifecycle| {
            let incident = assert_fixture_registrar_incident(lifecycle, state_dir)?;
            lifecycle
                .confirm_safe_mode_incident(incident.incident_id())
                .map_err(LoadFailure::Lifecycle)
        },
        |lifecycle| {
            if lifecycle.safe_mode_denies_all() || !lifecycle.safe_mode_incidents().is_empty() {
                return Err(LoadFailure::Setup(
                    "scoped Safe Mode confirmation did not clear its incident".to_owned(),
                ));
            }
            Ok(())
        },
    )
    .map_err(|error| error.to_string())?;
    if admission.root_count != 1 {
        return Err("Safe Mode confirmation did not admit exactly one root".to_owned());
    }
    assert_callback_marker(&["register:primary"])?;
    assert_call_markers_empty(state_dir)
}

fn expect_slow_callback(fingerprint: &str, dll: &Path, state_dir: &Path) -> Result<(), String> {
    let admission = load_with_lifecycle(
        fingerprint,
        false,
        None,
        &[("data", dll)],
        state_dir,
        Duration::from_millis(10),
        |_| Ok(()),
        |lifecycle| {
            let timings = lifecycle.native_call_timings();
            let [timing] = timings.as_slice() else {
                return Err(LoadFailure::Setup(
                    "expected exactly one bounded native timing record".to_owned(),
                ));
            };
            if timing.package_id != FIXTURE_PACKAGE_ID
                || timing.primary_interface_namespace != FIXTURE_INTERFACE_NAMESPACE
                || timing.primary_interface_value != FIXTURE_PRIMARY_INTERFACE_VALUE
                || timing.operation != NativeCallOperationV1::Registrar
                || !timing.slow
                || timing.terminal != NativeCallTerminalV1::Accepted
                || timing.elapsed < Duration::from_millis(50)
            {
                return Err(LoadFailure::Setup(
                    "slow callback timing did not retain the expected accepted registrar identity"
                        .to_owned(),
                ));
            }
            assert_path_free_debug(&format!("{timing:?}"), state_dir, "timing")?;
            Ok(())
        },
    )
    .map_err(|error| error.to_string())?;
    if admission.root_count != 1 {
        return Err("slow callback did not admit exactly one root".to_owned());
    }
    assert_callback_marker(&["register:primary"])?;
    assert_call_markers_empty(state_dir)
}

fn expect_drain_timeout_is_sticky(
    fingerprint: &str,
    dll: &Path,
    state_dir: &Path,
) -> Result<(), String> {
    let (lifecycle, admission) = load_started(
        fingerprint,
        false,
        None,
        &[("data", dll)],
        state_dir,
        Duration::from_secs(1),
        Duration::from_millis(40),
        |_| Ok(()),
    )
    .map_err(|error| error.to_string())?;
    let identity = lifecycle
        .install_integration_test_dispatch_gate(&admission)
        .map_err(|error| error.to_string())?;
    if !lifecycle
        .integration_test_has_resident_validated_root(&identity)
        .map_err(|error| error.to_string())?
    {
        return Err("synthetic dispatch gate lost its validated resident DLL root".to_owned());
    }
    let lease = lifecycle
        .try_enter(&identity)
        .map_err(|error| error.to_string())?
        .ok_or("synthetic dispatch gate did not issue the initial native lease")?;
    thread::scope(|scope| {
        let disable = scope.spawn(|| lifecycle.disable(&identity));
        let mut closed = false;
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            match lifecycle
                .try_enter(&identity)
                .map_err(|error| error.to_string())?
            {
                None => {
                    closed = true;
                    break;
                }
                Some(transient) => drop(transient),
            }
            thread::yield_now();
        }
        if !closed {
            return Err("disable did not close the native dispatch gate promptly".to_owned());
        }
        match disable
            .join()
            .map_err(|_| "drain-timeout disable helper panicked".to_owned())?
            .map_err(|error| error.to_string())?
        {
            NativeFeatureStateV1::PendingRestart {
                primary_reason: NativeRestartReasonV1::DrainTimedOut,
            } => Ok(()),
            _ => Err("drain timeout did not become a pending restart".to_owned()),
        }
    })?;
    if lifecycle
        .runtime_fact(&identity)
        .map_err(|error| error.to_string())?
        != FeatureRuntimeFactV1::PendingRestart
    {
        return Err("drain timeout did not retain the pending-restart runtime fact".to_owned());
    }
    if !lifecycle
        .restart_reasons(&identity)
        .map_err(|error| error.to_string())?
        .contains(&NativeRestartReasonV1::DrainTimedOut)
    {
        return Err("drain timeout restart reason was not retained".to_owned());
    }
    if !lifecycle
        .integration_test_has_resident_validated_root(&identity)
        .map_err(|error| error.to_string())?
    {
        return Err("drain timeout unloaded the validated resident DLL root".to_owned());
    }
    drop(lease);
    if lifecycle
        .runtime_fact(&identity)
        .map_err(|error| error.to_string())?
        != FeatureRuntimeFactV1::PendingRestart
    {
        return Err("late native lease drop cleared the pending-restart runtime fact".to_owned());
    }
    if !matches!(
        lifecycle.enable(&identity),
        Err(NativeLifecycleErrorV1::RestartRequired {
            reason: NativeRestartReasonV1::DrainTimedOut,
            ..
        })
    ) {
        return Err("drain-timeout feature enable did not require process restart".to_owned());
    }
    assert_callback_marker(&["register:primary"])?;
    assert_call_markers_empty(state_dir)
}

fn assert_fixture_registrar_incident(
    lifecycle: &NativeExtensionLifecycleV1,
    state_dir: &Path,
) -> Result<NativeSafeModeIncidentV1, LoadFailure> {
    if lifecycle.safe_mode_denies_all() {
        return Err(LoadFailure::Setup(
            "fixture Safe Mode incident unexpectedly escalated to global denial".to_owned(),
        ));
    }
    let incidents = lifecycle.safe_mode_incidents();
    let [incident] = incidents.as_slice() else {
        return Err(LoadFailure::Setup(
            "expected exactly one recovered path-free Safe Mode incident".to_owned(),
        ));
    };
    if incident.kind() != NativeSafeModeIncidentKindV1::RegistrarInProgress {
        return Err(LoadFailure::Setup(
            "recovered Safe Mode incident was not registrar-in-progress".to_owned(),
        ));
    }
    let NativeSafeModeIncidentV1::RegistrarInProgress {
        package_id,
        entrypoint_id,
        root_module_id,
        primary_interface_namespace,
        primary_interface_value,
        operation,
        ..
    } = incident
    else {
        return Err(LoadFailure::Setup(
            "recovered Safe Mode incident had an unexpected shape".to_owned(),
        ));
    };
    if package_id != FIXTURE_PACKAGE_ID
        || entrypoint_id != FIXTURE_ENTRYPOINT_ID
        || root_module_id != FIXTURE_ROOT_MODULE_ID
        || *primary_interface_namespace != FIXTURE_INTERFACE_NAMESPACE
        || *primary_interface_value != FIXTURE_PRIMARY_INTERFACE_VALUE
        || *operation != NativeCallOperationV1::Registrar
    {
        return Err(LoadFailure::Setup(
            "recovered Safe Mode incident did not match the fixture registrar identity".to_owned(),
        ));
    }
    assert_path_free_debug(&format!("{incident:?}"), state_dir, "Safe Mode incident")?;
    Ok(incident.clone())
}

fn assert_path_free_debug(debug: &str, state_dir: &Path, subject: &str) -> Result<(), LoadFailure> {
    if debug.contains(state_dir.to_string_lossy().as_ref()) {
        return Err(LoadFailure::Setup(format!(
            "{subject} diagnostics exposed application state paths"
        )));
    }
    Ok(())
}

fn callback_marker_path() -> Result<PathBuf, String> {
    env::var_os(CALLBACK_MARKER_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| "callback marker environment missing".to_owned())
}

fn assert_callback_marker(expected: &[&str]) -> Result<(), String> {
    let marker = callback_marker_path()?;
    let contents = fs::read_to_string(&marker)
        .map_err(|error| format!("required registrar callback marker missing: {error}"))?;
    let actual = contents.lines().collect::<Vec<_>>();
    if actual != expected {
        return Err(format!(
            "unexpected registrar callback marker sequence: expected {expected:?}, got {actual:?}"
        ));
    }
    Ok(())
}

fn assert_callback_marker_absent() -> Result<(), String> {
    match env::var_os(CALLBACK_MARKER_ENV) {
        Some(marker) if Path::new(&marker).exists() => Err(
            "registrar callback marker exists although loader must not dispatch callbacks"
                .to_owned(),
        ),
        _ => Ok(()),
    }
}

fn assert_call_markers_empty(state_dir: &Path) -> Result<(), String> {
    let marker_dir = state_dir.join("native-call-markers-v1");
    let entries = fs::read_dir(&marker_dir)
        .map_err(|_| "host call-marker directory was not readable".to_owned())?;
    let launches = entries
        .map(|entry| entry.map_err(|_| "host call-marker directory was not readable".to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    if launches.is_empty() {
        return Ok(());
    }
    let [launch] = launches.as_slice() else {
        return Err(
            "host call-marker directory did not contain exactly one active namespace".to_owned(),
        );
    };
    if !launch
        .file_type()
        .map_err(|_| "host call-marker namespace was not readable".to_owned())?
        .is_dir()
        || !launch.file_name().to_string_lossy().starts_with("launch-")
    {
        return Err("host call-marker directory retained an unexpected namespace entry".to_owned());
    }
    let entries = fs::read_dir(launch.path())
        .map_err(|_| "host active marker namespace was not readable".to_owned())?
        .map(|entry| entry.map_err(|_| "host active marker namespace was not readable".to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    if entries.len() != 1 || entries[0].file_name() != "owner.lease" {
        return Err("host active marker namespace retained callback residue".to_owned());
    }
    Ok(())
}

fn load(
    fingerprint: &str,
    gpui: bool,
    manifest_fingerprint: Option<&str>,
    dlls: &[(&str, &Path)],
    state_dir: &Path,
) -> Result<NativeStartupAdmissionV1, LoadFailure> {
    load_with_lifecycle(
        fingerprint,
        gpui,
        manifest_fingerprint,
        dlls,
        state_dir,
        Duration::from_secs(1),
        |_| Ok(()),
        |_| Ok(()),
    )
}

fn load_with_lifecycle<Before, After>(
    _fingerprint: &str,
    gpui: bool,
    manifest_fingerprint: Option<&str>,
    dlls: &[(&str, &Path)],
    state_dir: &Path,
    slow_callback_threshold: Duration,
    before_admission: Before,
    after_admission: After,
) -> Result<NativeStartupAdmissionV1, LoadFailure>
where
    Before: FnOnce(&NativeExtensionLifecycleV1) -> Result<(), LoadFailure>,
    After: FnOnce(&NativeExtensionLifecycleV1) -> Result<(), LoadFailure>,
{
    let (lifecycle, admission) = load_started(
        _fingerprint,
        gpui,
        manifest_fingerprint,
        dlls,
        state_dir,
        slow_callback_threshold,
        DEFAULT_NATIVE_DRAIN_TIMEOUT,
        before_admission,
    )?;
    after_admission(&lifecycle)?;
    Ok(admission)
}

fn load_started<Before>(
    _fingerprint: &str,
    gpui: bool,
    manifest_fingerprint: Option<&str>,
    dlls: &[(&str, &Path)],
    state_dir: &Path,
    slow_callback_threshold: Duration,
    drain_timeout: Duration,
    before_admission: Before,
) -> Result<(NativeExtensionLifecycleV1, NativeStartupAdmissionV1), LoadFailure>
where
    Before: FnOnce(&NativeExtensionLifecycleV1) -> Result<(), LoadFailure>,
{
    let package = state_dir.join("contract-package");
    let key_path = state_dir.join("contract-signing-public-key.bin");
    let public_key = if package.exists() {
        fs::read(&key_path).map_err(|error| LoadFailure::Setup(error.to_string()))?
    } else {
        prepare_package(&package, &key_path, gpui, manifest_fingerprint, dlls)?
    };
    let trusted = TrustedPublisherKeyStoreV1::new([TrustedPublisherKeyV1::new(
        "fixture.signing".to_owned(),
        "fixture.publisher".to_owned(),
        &public_key,
    )
    .map_err(|error| LoadFailure::Setup(error.to_string()))?])
    .map_err(|error| LoadFailure::Setup(error.to_string()))?;
    let seal = tempfile::tempdir().map_err(|error| LoadFailure::Setup(error.to_string()))?;
    let validator = PackageValidatorV1::new(
        trusted,
        SealedPackageStoreV1::new(seal.path())
            .map_err(|error| LoadFailure::Setup(error.to_string()))?,
    );
    let request = PackageValidationRequestV1::new(package);
    let validated = validator
        .validate(&request)
        .map_err(|error| LoadFailure::Setup(error.to_string()))?;
    let validated_packages = [validated];
    let resolution = PackageResolverV1::resolve(&validated_packages);
    let resolved = resolution
        .resolved_packages()
        .first()
        .ok_or_else(|| LoadFailure::Setup("validated package was blocked".to_owned()))?;
    let lifecycle_config = NativeLifecycleConfigV1::new(state_dir.to_path_buf())
        .with_slow_callback_threshold(slow_callback_threshold)
        .with_integration_test_drain_timeout(drain_timeout);
    let mut lifecycle =
        NativeExtensionLifecycleV1::acquire(lifecycle_config).map_err(LoadFailure::Lifecycle)?;
    before_admission(&lifecycle)?;
    let mut startup = lifecycle.begin_startup().map_err(LoadFailure::Lifecycle)?;
    let admission = startup
        .admit_resolved_package(resolved)
        .map_err(LoadFailure::Lifecycle)?;
    startup.seal().map_err(LoadFailure::Lifecycle)?;
    Ok((lifecycle, admission))
}

fn prepare_package(
    package: &Path,
    key_path: &Path,
    gpui: bool,
    manifest_fingerprint: Option<&str>,
    dlls: &[(&str, &Path)],
) -> Result<Vec<u8>, LoadFailure> {
    fs::create_dir_all(package.join("native"))
        .map_err(|error| LoadFailure::Setup(error.to_string()))?;
    let key = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| LoadFailure::Setup("key generation failed".to_owned()))?;
    let key = Ed25519KeyPair::from_pkcs8(key.as_ref())
        .map_err(|_| LoadFailure::Setup("key decode failed".to_owned()))?;
    let mut payloads = Vec::new();
    let mut rust = Vec::new();
    for (id, source) in dlls {
        let path = format!("native/{id}.dll");
        let destination = package.join(&path);
        fs::copy(source, &destination).map_err(|error| LoadFailure::Setup(error.to_string()))?;
        let bytes =
            fs::read(&destination).map_err(|error| LoadFailure::Setup(error.to_string()))?;
        payloads.push(
            json!({"path":path,"size":bytes.len(),"sha256":sha256_hex(&bytes),"kind":"rust_dll"}),
        );
        rust.push(json!({"id":id,"entrypoint":format!("native/{id}.dll"),"root_contract_id":{"namespace":1_397_030_913_u64,"value":1_u64},"sdk_major":1}));
    }
    let mut value = json!({"manifest_version":1,"package":{"id":"fixture.loader","version":"1.0.0"},"publisher":{"id":"fixture.publisher","display_name":"Fixture Publisher","contacts":[{"kind":"email","value":"support@example.invalid","purposes":["support"]}]},"sdk":{"bundle_id":"fixture", "target":"x86_64-pc-windows-msvc","abi_schema":1,"gpui":gpui,"ui_abi_fingerprint":manifest_fingerprint},"rust":rust,"lua":[],"skins":[],"locales":[],"tools":[],"features":[{"id":"fixture","capabilities":[],"dependencies":[]}],"dependencies":[],"payloads":payloads,"signature":{"kind":"ed25519","key_id":"fixture.signing","signature":""},"data_version":1});
    let parsed = explorer_extension_host::PackageManifestV1::parse_json(&value.to_string())
        .map_err(|error| LoadFailure::Setup(error.to_string()))?;
    value["signature"]["signature"] = Value::String(
        STANDARD.encode(
            key.sign(
                &parsed
                    .canonical_ed25519_signing_bytes()
                    .map_err(|error| LoadFailure::Setup(error.to_string()))?,
            )
            .as_ref(),
        ),
    );
    fs::write(package.join("manifest.json"), value.to_string())
        .map_err(|error| LoadFailure::Setup(error.to_string()))?;
    let public_key = key.public_key().as_ref().to_vec();
    fs::write(key_path, &public_key).map_err(|error| LoadFailure::Setup(error.to_string()))?;
    Ok(public_key)
}

enum LoadFailure {
    Setup(String),
    Lifecycle(NativeLifecycleErrorV1),
}

impl fmt::Display for LoadFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup(error) => write!(formatter, "fixture setup failed: {error}"),
            Self::Lifecycle(error) => write!(formatter, "native lifecycle failed: {error}"),
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
