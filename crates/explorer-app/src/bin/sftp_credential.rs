//! Small credential-vault helper for SFTP profiles; secrets never enter argv or profile JSON.

fn main() -> anyhow::Result<()> {
    let alias = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: sftp_credential <profile-alias>"))?;
    let secret = rpassword::prompt_password("SFTP password: ")?;
    explorer_automation_win::store_windows_credential(
        &format!("SuperExplorer/SFTP/{alias}"),
        secret,
    )?;
    println!("SFTP credential stored for profile {alias}");
    Ok(())
}
