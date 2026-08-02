use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use explorer_extension_host::{
    NativeExtensionLifecycleV1, NativeLifecycleConfigV1, NativeLifecycleErrorV1,
    NativeLoaderDiagnosticCodeV1, NativeStartupAdmissionV1, PackageResolverV1,
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
        [scenario, first] if scenario == "old-data" => expect_load(
            &fingerprint,
            false,
            None,
            &[("old", Path::new(first))],
            &state_dir,
            &["old-v1 registrar invoked"],
        ),
        [scenario, first] if scenario == "old-panic" => expect_activation_fault(
            &fingerprint,
            &[("old", Path::new(first))],
            &state_dir,
            "old-v1 registrar entered before translated panic",
        ),
        [scenario, first] if matches!(
            scenario.to_str(),
            Some("old-schema-mismatch" | "old-root-contract-mismatch" | "old-sdk-major-mismatch")
        ) =>
        {
            expect_reject(
                &fingerprint,
                false,
                None,
                &[("old", Path::new(first))],
                NativeLoaderDiagnosticCodeV1::RootContract,
                &state_dir,
            )
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
        _ => Err("usage: runner <data|gpui-exact|gpui-missing-binary|gpui-wrong-binary|gpui-wrong-manifest|old-data|old-panic|old-schema-mismatch|old-root-contract-mismatch|old-sdk-major-mismatch> <dll> | <two-roots|batch-invalid> <dll> <dll>".to_owned()),
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

fn expect_activation_fault(
    fingerprint: &str,
    dlls: &[(&str, &Path)],
    state_dir: &Path,
    expected_callback: &str,
) -> Result<(), String> {
    match load(fingerprint, false, None, dlls, state_dir) {
        Err(LoadFailure::Lifecycle(NativeLifecycleErrorV1::ActivationFaulted)) => {}
        Err(LoadFailure::Lifecycle(error)) => {
            return Err(format!(
                "expected translated panic activation fault, got lifecycle error: {error}"
            ));
        }
        Err(LoadFailure::Setup(error)) => {
            return Err(format!(
                "fixture setup failed before translated panic admission: {error}"
            ));
        }
        Ok(_) => return Err("translated panic unexpectedly admitted the package".to_owned()),
    }
    assert_callback_marker(&[expected_callback])?;
    assert_call_markers_empty(state_dir)
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
        .map_err(|error| format!("host call-marker directory was not readable: {error}"))?;
    let launches = entries
        .map(|entry| entry.map_err(|error| error.to_string()))
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
        .map_err(|error| error.to_string())?
        .is_dir()
        || !launch.file_name().to_string_lossy().starts_with("launch-")
    {
        return Err("host call-marker directory retained an unexpected namespace entry".to_owned());
    }
    let entries = fs::read_dir(launch.path())
        .map_err(|error| format!("host active marker namespace was not readable: {error}"))?
        .map(|entry| entry.map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if entries.len() != 1 || entries[0].file_name() != "owner.lease" {
        return Err("host active marker namespace retained callback residue".to_owned());
    }
    Ok(())
}

fn load(
    _fingerprint: &str,
    gpui: bool,
    manifest_fingerprint: Option<&str>,
    dlls: &[(&str, &Path)],
    state_dir: &Path,
) -> Result<NativeStartupAdmissionV1, LoadFailure> {
    let temp = tempfile::tempdir().map_err(|error| LoadFailure::Setup(error.to_string()))?;
    let package = temp.path().join("package");
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
        rust.push(json!({"id":id,"entrypoint":format!("native/{id}.dll"),"root_module":format!("fixture.{id}"),"sdk_major":1}));
    }
    let mut value = json!({"manifest_version":1,"package":{"id":"fixture.loader","version":"1.0.0"},"publisher":{"id":"fixture.publisher","display_name":"Fixture Publisher","contacts":[{"kind":"email","value":"support@example.invalid","purposes":["support"]}]},"sdk":{"bundle_id":"fixture", "target":"x86_64-pc-windows-msvc","abi_schema":1,"gpui":gpui,"ui_abi_fingerprint":manifest_fingerprint},"rust":rust,"lua":[],"skins":[],"locales":[],"tools":[],"features":[],"dependencies":[],"payloads":payloads,"signature":{"kind":"ed25519","key_id":"fixture.signing","signature":""},"data_version":1});
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
    let trusted = TrustedPublisherKeyStoreV1::new([TrustedPublisherKeyV1::new(
        "fixture.signing".to_owned(),
        "fixture.publisher".to_owned(),
        key.public_key().as_ref(),
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
        .with_slow_callback_threshold(Duration::from_secs(1));
    let mut lifecycle =
        NativeExtensionLifecycleV1::acquire(lifecycle_config).map_err(LoadFailure::Lifecycle)?;
    let mut startup = lifecycle.begin_startup().map_err(LoadFailure::Lifecycle)?;
    let admission = startup
        .admit_resolved_package(resolved)
        .map_err(LoadFailure::Lifecycle)?;
    startup.seal().map_err(LoadFailure::Lifecycle)?;
    Ok(admission)
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
