//! FTP reply, path, and listing parsers without network I/O.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::{Result, bail};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FtpReply {
    pub code: u16,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FtpListEntry {
    pub name: String,
    pub is_directory: bool,
    pub size: Option<u64>,
    pub modified_unix_seconds: Option<u64>,
    pub unix_mode: Option<u32>,
    pub is_symlink: bool,
}

pub fn sanitize_ftp_display(text: &str) -> String {
    text.chars()
        .filter(|ch| !matches!(ch, '\0' | '\r' | '\n') && !ch.is_control())
        .collect::<String>()
        .replace("://", ":// ")
}

pub fn command_is_safe(command: &str) -> bool {
    !command.contains(['\r', '\n', '\0'])
}

pub fn ftp_path(components: &[String]) -> Result<String> {
    if components.iter().any(|component| {
        component.is_empty()
            || matches!(component.as_str(), "." | "..")
            || component.contains(['/', '\\', '\0', '\r', '\n'])
    }) {
        bail!("FTP path contains an invalid component");
    }
    if components.is_empty() {
        Ok("/".to_owned())
    } else {
        Ok(format!("/{}", components.join("/")))
    }
}

pub fn parse_reply_block(buffer: &str) -> Result<FtpReply> {
    let mut code = None;
    let mut lines = Vec::new();
    for raw in buffer.split(['\r', '\n']).filter(|line| !line.is_empty()) {
        let has_code = raw.len() >= 4
            && raw.as_bytes()[0].is_ascii_digit()
            && raw.as_bytes()[1].is_ascii_digit()
            && raw.as_bytes()[2].is_ascii_digit()
            && matches!(raw.as_bytes()[3], b' ' | b'-');
        if has_code {
            let parsed = raw[..3]
                .parse::<u16>()
                .map_err(|_| anyhow::anyhow!("FTP reply code is invalid"))?;
            code = Some(parsed);
            lines.push(sanitize_ftp_display(raw.get(4..).unwrap_or("")));
            if raw.as_bytes()[3] == b' ' {
                break;
            }
            continue;
        }
        lines.push(sanitize_ftp_display(raw.trim()));
    }
    let Some(code) = code else {
        bail!("FTP reply is empty");
    };
    Ok(FtpReply {
        code,
        text: lines.join(" "),
    })
}

pub fn parse_pasv(text: &str) -> Result<(Ipv4Addr, u16)> {
    let start = text
        .find('(')
        .ok_or_else(|| anyhow::anyhow!("PASV reply is missing '('"))?;
    let end = text[start..]
        .find(')')
        .ok_or_else(|| anyhow::anyhow!("PASV reply is missing ')'"))?;
    let inner = &text[start + 1..start + end];
    let parts = inner.split(',').collect::<Vec<_>>();
    if parts.len() != 6 {
        bail!("PASV reply does not contain six fields");
    }
    let nums = parts
        .iter()
        .map(|part| part.trim().parse::<u8>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| anyhow::anyhow!("PASV field is not a number"))?;
    let ip = Ipv4Addr::new(nums[0], nums[1], nums[2], nums[3]);
    let port = u16::from(nums[4]) * 256 + u16::from(nums[5]);
    Ok((ip, port))
}

pub fn parse_epsv(text: &str) -> Result<u16> {
    let start = text
        .find("|||")
        .ok_or_else(|| anyhow::anyhow!("EPSV reply is missing |||"))?;
    let rest = &text[start + 3..];
    let end = rest
        .find('|')
        .ok_or_else(|| anyhow::anyhow!("EPSV reply is missing closing |"))?;
    rest[..end]
        .parse::<u16>()
        .map_err(|_| anyhow::anyhow!("EPSV port is invalid"))
}

pub fn rewrite_pasv_peer(
    control_peer: SocketAddr,
    pasv_ip: Ipv4Addr,
    pasv_port: u16,
) -> SocketAddr {
    let use_control = match control_peer.ip() {
        IpAddr::V4(control) if !control.is_private() && pasv_ip.is_private() => true,
        IpAddr::V6(_) if pasv_ip.is_private() => true,
        _ => false,
    };
    if use_control {
        SocketAddr::new(control_peer.ip(), pasv_port)
    } else {
        SocketAddr::new(IpAddr::V4(pasv_ip), pasv_port)
    }
}

pub fn parse_mlsd_line(line: &str) -> Result<Option<FtpListEntry>> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    let (facts, name) = line
        .split_once(' ')
        .ok_or_else(|| anyhow::anyhow!("MLSD line is missing a name"))?;
    if matches!(name, "." | "..") {
        return Ok(None);
    }
    if name.contains(['/', '\\', '\0', '\r', '\n']) {
        bail!("MLSD name is not a safe component");
    }
    let mut is_directory = false;
    let mut is_symlink = false;
    let mut size = None;
    let mut modified_unix_seconds = None;
    let mut unix_mode = None;
    for fact in facts.split(';').filter(|fact| !fact.is_empty()) {
        let Some((key, value)) = fact.split_once('=') else {
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "type" => match value {
                "dir" | "cdir" | "pdir" => is_directory = true,
                "os.unix=symlink" | "OS.unix=slink" => is_symlink = true,
                _ if value.to_ascii_lowercase().contains("link") => is_symlink = true,
                _ => {}
            },
            "size" => size = value.parse().ok(),
            "modify" => modified_unix_seconds = parse_ftp_time(value),
            "unix.mode" => unix_mode = u32::from_str_radix(value, 8).ok(),
            _ => {}
        }
    }
    Ok(Some(FtpListEntry {
        name: name.to_owned(),
        is_directory,
        size,
        modified_unix_seconds,
        unix_mode,
        is_symlink,
    }))
}

pub fn parse_list_line(line: &str) -> Option<FtpListEntry> {
    let line = line.trim();
    if line.is_empty() || line.starts_with("total ") {
        return None;
    }
    let mut parts = line.split_whitespace();
    let perms = parts.next()?;
    let is_directory = perms.starts_with('d');
    let is_symlink = perms.starts_with('l');
    let _links = parts.next()?;
    let _owner = parts.next()?;
    let _group = parts.next()?;
    let size = parts.next()?.parse().ok();
    let _month = parts.next()?;
    let _day = parts.next()?;
    let _time_or_year = parts.next()?;
    let name = parts.collect::<Vec<_>>().join(" ");
    let name = name.split(" -> ").next().unwrap_or(&name).to_owned();
    if matches!(name.as_str(), "." | "..") || name.contains(['/', '\\', '\0', '\r', '\n']) {
        return None;
    }
    Some(FtpListEntry {
        name,
        is_directory,
        size,
        modified_unix_seconds: None,
        unix_mode: None,
        is_symlink,
    })
}

fn parse_ftp_time(value: &str) -> Option<u64> {
    if value.len() < 14 {
        return None;
    }
    let year = value.get(0..4)?.parse::<i32>().ok()?;
    let month = value.get(4..6)?.parse::<u32>().ok()?;
    let day = value.get(6..8)?.parse::<u32>().ok()?;
    let hour = value.get(8..10)?.parse::<u32>().ok()?;
    let minute = value.get(10..12)?.parse::<u32>().ok()?;
    let second = value.get(12..14)?.parse::<u32>().ok()?;
    let naive =
        chrono::NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second)?;
    Some(naive.and_utc().timestamp().max(0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddrV4;

    #[test]
    fn parses_multiline_reply_and_strips_controls() {
        let reply = parse_reply_block("220-hello\r\n220 ready\r\n").unwrap();
        assert_eq!(reply.code, 220);
        assert_eq!(reply.text, "hello ready");
    }

    #[test]
    fn parses_vsftpd_feat_continuation_lines_without_reply_codes() {
        let reply = parse_reply_block(
            "211-Features:\r\n EPRT\r\n EPSV\r\n MDTM\r\n PASV\r\n REST STREAM\r\n SIZE\r\n TVFS\r\n211 End\r\n",
        )
        .unwrap();
        assert_eq!(reply.code, 211);
        assert!(reply.text.to_ascii_uppercase().contains("EPSV"));
        assert!(reply.text.to_ascii_uppercase().contains("PASV"));
    }

    #[test]
    fn rejects_crlf_in_commands_and_paths() {
        assert!(!command_is_safe("CWD /tmp\r\nDELE /etc/passwd"));
        assert!(ftp_path(&["..".to_owned()]).is_err());
        assert!(ftp_path(&["ok".to_owned()]).unwrap() == "/ok");
    }

    #[test]
    fn rewrites_private_pasv_to_control_peer() {
        let (ip, port) = parse_pasv("227 Entering Passive Mode (10,0,0,5,4,1)").unwrap();
        assert_eq!(ip, Ipv4Addr::new(10, 0, 0, 5));
        assert_eq!(port, 1025);
        let control = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(45, 32, 49, 125), 21));
        let rewritten = rewrite_pasv_peer(control, ip, port);
        assert_eq!(rewritten.ip(), IpAddr::V4(Ipv4Addr::new(45, 32, 49, 125)));
        assert_eq!(rewritten.port(), 1025);
    }

    #[test]
    fn parses_epsv_and_mlsd() {
        assert_eq!(
            parse_epsv("229 Entering Extended Passive Mode (|||40000|)").unwrap(),
            40000
        );
        let entry = parse_mlsd_line("type=file;size=12;modify=20240101120000; hello.txt")
            .unwrap()
            .unwrap();
        assert_eq!(entry.name, "hello.txt");
        assert_eq!(entry.size, Some(12));
        assert!(!entry.is_directory);
    }

    #[test]
    fn display_sanitizer_removes_userinfo_markers() {
        let cleaned = sanitize_ftp_display("ftp://test:secret@host/\r\n");
        assert!(!cleaned.contains('\r'));
        assert!(!cleaned.contains("ftp://test"));
    }
}
