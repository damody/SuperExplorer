//! Pure Google Drive v3 helpers: path encoding, MIME export, and error sanitizing.

pub const DRIVE_FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";
pub const DRIVE_UPLOAD_URL: &str = "https://www.googleapis.com/upload/drive/v3/files";
pub const DRIVE_ABOUT_URL: &str = "https://www.googleapis.com/drive/v3/about";
pub const OAUTH_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive";
pub const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
pub const SHORTCUT_MIME: &str = "application/vnd.google-apps.shortcut";
pub const ROOT_ID: &str = "root";

const DOC_MIME: &str = "application/vnd.google-apps.document";
const SHEET_MIME: &str = "application/vnd.google-apps.spreadsheet";
const SLIDE_MIME: &str = "application/vnd.google-apps.presentation";
const DRAWING_MIME: &str = "application/vnd.google-apps.drawing";
const DOCX_MIME: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
const XLSX_MIME: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
const PPTX_MIME: &str = "application/vnd.openxmlformats-officedocument.presentationml.presentation";
const PDF_MIME: &str = "application/pdf";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExportTarget {
    pub extension: &'static str,
    pub mime_type: &'static str,
}

pub fn export_target(mime_type: &str) -> Option<ExportTarget> {
    Some(match mime_type {
        DOC_MIME => ExportTarget {
            extension: "docx",
            mime_type: DOCX_MIME,
        },
        SHEET_MIME => ExportTarget {
            extension: "xlsx",
            mime_type: XLSX_MIME,
        },
        SLIDE_MIME => ExportTarget {
            extension: "pptx",
            mime_type: PPTX_MIME,
        },
        DRAWING_MIME => ExportTarget {
            extension: "pdf",
            mime_type: PDF_MIME,
        },
        _ => return None,
    })
}

pub fn is_google_native(mime_type: &str) -> bool {
    mime_type.starts_with("application/vnd.google-apps.")
}

pub fn is_folder_mime(mime_type: &str) -> bool {
    mime_type == FOLDER_MIME
}

pub fn is_shortcut_mime(mime_type: &str) -> bool {
    mime_type == SHORTCUT_MIME
}

pub fn encode_drive_component(name: &str) -> String {
    if name == "." {
        "%2E".to_owned()
    } else if name == ".." {
        "%2E%2E".to_owned()
    } else if name.contains(['/', '\\', '%']) {
        percent_encode(name)
    } else {
        name.to_owned()
    }
}

pub fn decode_drive_component(component: &str) -> String {
    percent_decode(component)
}

pub fn list_query(parent_id: &str) -> String {
    format!("'{}' in parents and trashed = false", escape_q(parent_id))
}

pub fn list_fields() -> &'static str {
    "nextPageToken,files(id,name,mimeType,size,modifiedTime,shortcutDetails(targetId,targetMimeType))"
}

pub fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(decoded) = u8::from_str_radix(
                std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or(""),
                16,
            )
        {
            out.push(decoded);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_rfc3339_unix_seconds(value: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .and_then(|stamp| u64::try_from(stamp.timestamp()).ok())
}

pub fn exported_file_name(name: &str, extension: &str) -> String {
    let suffix = format!(".{extension}");
    if name.to_ascii_lowercase().ends_with(&suffix) {
        name.to_owned()
    } else {
        format!("{name}{suffix}")
    }
}

pub fn parse_drive_error_body(body: &str) -> (String, Option<String>) {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let oauth_code = parsed.get("error").and_then(serde_json::Value::as_str);
    let message = parsed
        .pointer("/error/message")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            parsed
                .get("error_description")
                .and_then(serde_json::Value::as_str)
        })
        .or(oauth_code)
        .unwrap_or("Google Drive request failed");
    let reason = parsed
        .pointer("/error/errors/0/reason")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            parsed
                .pointer("/error/status")
                .and_then(serde_json::Value::as_str)
        })
        .or(oauth_code)
        .map(str::to_owned);
    (sanitize_gdrive_display(message), reason)
}

pub fn user_facing_drive_failure(body: &str, status: u16) -> String {
    let (message, reason) = parse_drive_error_body(body);
    match reason.as_deref() {
        Some("accessNotConfigured" | "SERVICE_DISABLED") => {
            "尚未啟用 Google Drive API。請在 Google Cloud 專案啟用 Google Drive API 後重試。"
                .to_owned()
        }
        Some("invalid_client") => {
            "Google OAuth 用戶端無效。請確認 Desktop client id 已正確內嵌或寫入 google-oauth.json。"
                .to_owned()
        }
        Some("access_denied") => {
            "Google 拒絕授權。請確認這個 Gmail 已加入 OAuth 同意畫面的測試使用者。".to_owned()
        }
        Some("redirect_uri_mismatch") => {
            "Google OAuth 回跳網址不符。Desktop 應用程式應允許本機 loopback。".to_owned()
        }
        _ if status == 403 && message.to_ascii_lowercase().contains("access") => {
            "Google Drive 拒絕存取。請確認測試使用者與 Drive API 已設定。".to_owned()
        }
        _ => message,
    }
}

pub fn sanitize_gdrive_display(value: &str) -> String {
    let mut sanitized = value.to_owned();
    for needle in ["Bearer ", "ya29.", "1//", "refresh_token", "access_token"] {
        if let Some(index) = sanitized.find(needle) {
            sanitized.replace_range(index.., "[redacted]");
        }
    }
    sanitized
}

pub fn retryable_drive_reason(reason: Option<&str>, status: u16) -> bool {
    status == 429
        || matches!(
            reason,
            Some("rateLimitExceeded" | "userRateLimitExceeded" | "backendError")
        )
}

fn escape_q(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_targets_office_formats() {
        assert_eq!(export_target(DOC_MIME).unwrap().extension, "docx");
        assert_eq!(export_target(SHEET_MIME).unwrap().extension, "xlsx");
        assert_eq!(export_target(SLIDE_MIME).unwrap().extension, "pptx");
        assert_eq!(export_target(DRAWING_MIME).unwrap().extension, "pdf");
        assert!(export_target("application/vnd.google-apps.form").is_none());
        assert_eq!(exported_file_name("Notes", "docx"), "Notes.docx");
        assert_eq!(exported_file_name("Notes.docx", "docx"), "Notes.docx");
    }

    #[test]
    fn path_components_encode_hostile_drive_names() {
        assert_eq!(encode_drive_component("Work"), "Work");
        assert_eq!(encode_drive_component("a/b"), "a%2Fb");
        assert_eq!(encode_drive_component("."), "%2E");
        assert_eq!(decode_drive_component("a%2Fb"), "a/b");
        assert_eq!(list_query("root"), "'root' in parents and trashed = false");
        assert_eq!(
            list_query("abc'd"),
            "'abc\\'d' in parents and trashed = false"
        );
    }

    #[test]
    fn drive_error_bodies_are_sanitized_and_classified() {
        let body = r#"{"error":{"message":"Bearer ya29.secret failed","errors":[{"reason":"rateLimitExceeded"}]}}"#;
        let (message, reason) = parse_drive_error_body(body);
        assert!(!message.contains("ya29"));
        assert!(!message.contains("secret"));
        assert_eq!(reason.as_deref(), Some("rateLimitExceeded"));
        assert!(retryable_drive_reason(reason.as_deref(), 403));
        assert!(retryable_drive_reason(None, 429));
        assert!(
            user_facing_drive_failure(
                r#"{"error":{"errors":[{"reason":"accessNotConfigured"}],"message":"Drive API has not been used"}}"#,
                403
            )
            .contains("尚未啟用 Google Drive API")
        );
        assert!(
            user_facing_drive_failure(r#"{"error":"access_denied"}"#, 400).contains("測試使用者")
        );
    }
}
