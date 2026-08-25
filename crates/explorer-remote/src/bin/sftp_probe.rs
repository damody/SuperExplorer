fn main() -> anyhow::Result<()> {
    let mut arguments = std::env::args().skip(1);
    let host = arguments
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: sftp_probe <host> [port]"))?;
    let port = arguments
        .next()
        .map(|value| value.parse::<u16>())
        .transpose()?
        .unwrap_or(22);
    let provider = explorer_remote::sftp::SftpProvider::new()?;
    println!("{}", provider.probe_host_key(&host, port)?);
    Ok(())
}
