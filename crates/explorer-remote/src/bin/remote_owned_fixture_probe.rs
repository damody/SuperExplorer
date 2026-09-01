use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result, bail};
use explorer_model::{
    CancellationToken, ConflictDecision, RemoteProviderKind, SftpProfile,
    VirtualLocationDescriptor, remote_container_identity,
};
use explorer_remote::{
    AdbClient, AdbProvider, RemoteProvider, RemoteProviderRegistry, SftpProvider, TransferEngine,
    TransferMode, TransferResult,
};

const MARKER: &str = ".superexplorer-owned-fixture";

fn unique_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("superexplorer-owned-{}-{nanos}", std::process::id())
}

fn child(parent: &VirtualLocationDescriptor, name: &str) -> VirtualLocationDescriptor {
    let mut location = parent.clone();
    location.components.push(name.to_owned());
    location.entry_id = None;
    location
}

fn verify_and_delete(
    provider: &dyn RemoteProvider,
    fixture: &VirtualLocationDescriptor,
    cancellation: &CancellationToken,
) -> Result<()> {
    let entries = provider.list(fixture, cancellation)?;
    if !entries.iter().any(|entry| entry.name == MARKER) {
        bail!("owned remote fixture marker is missing; cleanup refused");
    }
    provider.delete(fixture, true, cancellation)
}

fn run_adb(serial: &str) -> Result<()> {
    let client = AdbClient::discover()?;
    let provider = AdbProvider::new(client);
    let identity = remote_container_identity(RemoteProviderKind::Adb, serial);
    provider.register_device(identity, serial.to_owned())?;
    let parent = VirtualLocationDescriptor {
        provider_id: "adb".to_owned(),
        public_authority: Some(serial.to_owned()),
        container_identity: identity,
        container_generation: 1,
        entry_id: None,
        components: vec!["data".to_owned(), "local".to_owned(), "tmp".to_owned()],
    };
    run_fixture(&provider, parent)
}

fn run_sftp(host: &str, username: &str, fingerprint: &str) -> Result<()> {
    let password = rpassword::read_password()?;
    let identity = remote_container_identity(RemoteProviderKind::Sftp, host);
    let mut profile = SftpProfile::new(
        host.to_owned(),
        host.to_owned(),
        22,
        username.to_owned(),
        identity,
    )?;
    profile.host_key_fingerprint = Some(fingerprint.to_owned());
    let provider = SftpProvider::new()?;
    provider.register_profile(profile, password)?;
    let parent = VirtualLocationDescriptor {
        provider_id: "sftp".to_owned(),
        public_authority: Some(host.to_owned()),
        container_identity: identity,
        container_generation: 1,
        entry_id: None,
        components: vec!["tmp".to_owned()],
    };
    run_fixture(&provider, parent)
}

fn run_cross(serial: &str, host: &str, username: &str, fingerprint: &str) -> Result<()> {
    let password = rpassword::read_password()?;
    let adb_identity = remote_container_identity(RemoteProviderKind::Adb, serial);
    let adb = Arc::new(AdbProvider::new(AdbClient::discover()?));
    adb.register_device(adb_identity, serial.to_owned())?;
    let adb_parent = VirtualLocationDescriptor {
        provider_id: "adb".to_owned(),
        public_authority: Some(serial.to_owned()),
        container_identity: adb_identity,
        container_generation: 1,
        entry_id: None,
        components: vec![
            "data".to_owned(),
            "local".to_owned(),
            "tmp".to_owned(),
            unique_name(),
        ],
    };
    let sftp_identity = remote_container_identity(RemoteProviderKind::Sftp, host);
    let mut profile = SftpProfile::new(
        host.to_owned(),
        host.to_owned(),
        22,
        username.to_owned(),
        sftp_identity,
    )?;
    profile.host_key_fingerprint = Some(fingerprint.to_owned());
    let sftp = Arc::new(SftpProvider::new()?);
    sftp.register_profile(profile, password)?;
    let sftp_parent = VirtualLocationDescriptor {
        provider_id: "sftp".to_owned(),
        public_authority: Some(host.to_owned()),
        container_identity: sftp_identity,
        container_generation: 1,
        entry_id: None,
        components: vec!["tmp".to_owned(), unique_name()],
    };
    let cancellation = CancellationToken::new();
    adb.create_directory(&adb_parent, &cancellation)?;
    sftp.create_directory(&sftp_parent, &cancellation)?;
    let local = tempfile::tempdir()?;
    let tree = local.path().join("matrix-tree");
    std::fs::create_dir_all(tree.join("nested"))?;
    std::fs::write(
        tree.join(MARKER),
        b"SuperExplorer owned destructive fixture v1",
    )?;
    let payload = vec![0x5a; 4 * 1024 * 1024 + 17];
    std::fs::write(tree.join("nested").join("payload.bin"), &payload)?;
    let root_marker = local.path().join(MARKER);
    std::fs::write(&root_marker, b"SuperExplorer owned destructive fixture v1")?;
    adb.upload(&root_marker, &adb_parent, &cancellation)?;
    sftp.upload(&root_marker, &sftp_parent, &cancellation)?;
    adb.upload(&tree, &adb_parent, &cancellation)?;
    sftp.upload(&tree, &sftp_parent, &cancellation)?;

    let mut registry = RemoteProviderRegistry::default();
    registry.register(adb.clone())?;
    registry.register(sftp.clone())?;
    let engine = TransferEngine::new(&registry);
    let adb_tree = child(&adb_parent, "matrix-tree");
    let sftp_tree = child(&sftp_parent, "matrix-tree");
    let adb_to_sftp_bytes = AtomicU64::new(0);
    let adb_to_sftp_callbacks = AtomicUsize::new(0);
    let adb_to_sftp = engine.transfer_with_conflict_and_progress(
        explorer_model::LocationDescriptor::Virtual(adb_tree.clone()),
        explorer_model::LocationDescriptor::Virtual(sftp_parent.clone()),
        TransferMode::Copy,
        ConflictDecision::Replace,
        &cancellation,
        &|delta| {
            adb_to_sftp_bytes.fetch_add(delta, Ordering::AcqRel);
            adb_to_sftp_callbacks.fetch_add(1, Ordering::AcqRel);
        },
    );
    if adb_to_sftp.result != TransferResult::Succeeded {
        bail!("ADB to SFTP matrix transfer failed");
    }
    let sftp_to_adb_bytes = AtomicU64::new(0);
    let sftp_to_adb_callbacks = AtomicUsize::new(0);
    let sftp_to_adb = engine.transfer_with_conflict_and_progress(
        explorer_model::LocationDescriptor::Virtual(sftp_tree.clone()),
        explorer_model::LocationDescriptor::Virtual(adb_parent.clone()),
        TransferMode::Copy,
        ConflictDecision::Replace,
        &cancellation,
        &|delta| {
            sftp_to_adb_bytes.fetch_add(delta, Ordering::AcqRel);
            sftp_to_adb_callbacks.fetch_add(1, Ordering::AcqRel);
        },
    );
    if sftp_to_adb.result != TransferResult::Succeeded {
        bail!("SFTP to ADB matrix transfer failed");
    }
    for (direction, bytes, callbacks) in [
        (
            "adb-to-sftp",
            adb_to_sftp_bytes.load(Ordering::Acquire),
            adb_to_sftp_callbacks.load(Ordering::Acquire),
        ),
        (
            "sftp-to-adb",
            sftp_to_adb_bytes.load(Ordering::Acquire),
            sftp_to_adb_callbacks.load(Ordering::Acquire),
        ),
    ] {
        if bytes == 0 || callbacks < 2 {
            bail!("{direction} did not publish intermediate byte progress");
        }
    }
    let local_downloads = tempfile::tempdir()?;
    adb.download(
        &adb_tree,
        &local_downloads.path().join("adb-tree"),
        &cancellation,
    )?;
    sftp.download(
        &sftp_tree,
        &local_downloads.path().join("sftp-tree"),
        &cancellation,
    )?;
    for path in [
        local_downloads.path().join("adb-tree/nested/payload.bin"),
        local_downloads.path().join("sftp-tree/nested/payload.bin"),
    ] {
        if std::fs::read(path)? != payload {
            bail!("cross-provider matrix content mismatch");
        }
    }
    verify_and_delete(adb.as_ref(), &adb_parent, &cancellation)?;
    verify_and_delete(sftp.as_ref(), &sftp_parent, &cancellation)?;
    println!("cross_provider_matrix_verified=true");
    Ok(())
}

fn run_fixture(provider: &dyn RemoteProvider, parent: VirtualLocationDescriptor) -> Result<()> {
    let cancellation = CancellationToken::new();
    let fixture = child(&parent, &unique_name());
    provider.create_directory(&fixture, &cancellation)?;
    let local = tempfile::tempdir()?;
    let marker = local.path().join(MARKER);
    std::fs::write(&marker, b"SuperExplorer owned destructive fixture v1")?;
    provider.upload(&marker, &fixture, &cancellation)?;
    verify_and_delete(provider, &fixture, &cancellation)?;
    let parent_entries = provider.list(&parent, &cancellation)?;
    let fixture_name = fixture.components.last().context("fixture name")?;
    if parent_entries
        .iter()
        .any(|entry| &entry.name == fixture_name)
    {
        bail!("owned remote fixture still exists after cleanup");
    }
    println!(
        "owned_fixture_verified=true provider={}",
        provider.provider_id()
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        Some("adb") => run_adb(&arguments.next().context("missing ADB serial")?),
        Some("sftp") => run_sftp(
            &arguments.next().context("missing SFTP host")?,
            &arguments.next().context("missing SFTP username")?,
            &arguments.next().context("missing SFTP fingerprint")?,
        ),
        Some("cross") => run_cross(
            &arguments.next().context("missing ADB serial")?,
            &arguments.next().context("missing SFTP host")?,
            &arguments.next().context("missing SFTP username")?,
            &arguments.next().context("missing SFTP fingerprint")?,
        ),
        _ => bail!(
            "usage: remote_owned_fixture_probe adb <serial> | sftp <host> <user> <fingerprint>"
        ),
    }
}
