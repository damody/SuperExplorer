use explorer_model::{CancellationToken, SftpProfile, new_remote_container_identity};
use explorer_remote::{RemoteProvider, sftp::SftpProvider};

fn main() -> anyhow::Result<()> {
    let mut arguments = std::env::args().skip(1);
    let host = arguments.next().ok_or_else(|| {
        anyhow::anyhow!("usage: sftp_auth_probe <host> <user> <fingerprint> [port]")
    })?;
    let username = arguments
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing user"))?;
    let fingerprint = arguments
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing fingerprint"))?;
    let port = arguments
        .next()
        .map(|value| value.parse::<u16>())
        .transpose()?
        .unwrap_or(22);
    let password = rpassword::read_password()?;
    let identity = new_remote_container_identity();
    let mut profile = SftpProfile::new("integration-probe".into(), host, port, username, identity)?;
    profile.host_key_fingerprint = Some(fingerprint);
    let provider = SftpProvider::new()?;
    provider.register_profile(profile, password)?;
    let location =
        explorer_model::LocationDescriptor::try_virtual("sftp", identity, 1, None, Vec::new())?;
    let explorer_model::LocationDescriptor::Virtual(location) = location else {
        unreachable!();
    };
    let entries = provider.list(&location, &CancellationToken::new())?;
    println!("authenticated=true entries={}", entries.len());
    Ok(())
}
