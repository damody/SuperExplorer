//! Native Google Drive provider using Drive API v3 and desktop PKCE OAuth.

use std::{
    collections::HashMap,
    io::{Read as _, Write as _},
    net::TcpListener,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use explorer_model::{
    CancellationToken, GdriveProfile, LocationDescriptor, RemoteProviderCapabilities,
    VirtualLocationDescriptor,
};
use sha2::{Digest, Sha256};

use crate::{
    RemoteEntry, RemoteEntryKind, RemoteMetadata, RemoteProvider,
    gdrive_protocol::{
        DRIVE_ABOUT_URL, DRIVE_FILES_URL, DRIVE_SCOPE, DRIVE_UPLOAD_URL, FOLDER_MIME,
        OAUTH_AUTH_URL, OAUTH_TOKEN_URL, ROOT_ID, decode_drive_component, encode_drive_component,
        export_target, is_folder_mime, is_google_native, is_shortcut_mime, list_fields, list_query,
        parse_drive_error_body, parse_rfc3339_unix_seconds, percent_encode, retryable_drive_reason,
        sanitize_gdrive_display, user_facing_drive_failure,
    },
    provider::validate_remote_location,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
const OAUTH_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_RETRIES: u8 = 5;

struct ProfileSecret(String);

impl std::fmt::Debug for ProfileSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ProfileSecret(*)")
    }
}

#[derive(Clone, Debug)]
pub struct GdriveOAuthClient {
    pub client_id: String,
    pub client_secret: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GdriveHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct GdriveHttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub trait GdriveTransport: Send + Sync {
    fn execute(&self, request: GdriveHttpRequest) -> Result<GdriveHttpResponse>;
}

struct ReqwestTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestTransport {
    fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .build()
                .context("Google Drive HTTP client")?,
        })
    }
}

impl GdriveTransport for ReqwestTransport {
    fn execute(&self, request: GdriveHttpRequest) -> Result<GdriveHttpResponse> {
        let mut builder = match request.method.as_str() {
            "GET" => self.client.get(&request.url),
            "POST" => self.client.post(&request.url),
            "PATCH" => self.client.patch(&request.url),
            "PUT" => self.client.put(&request.url),
            other => bail!("unsupported Google Drive HTTP method {other}"),
        };
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let response = builder
            .send()
            .map_err(|error| anyhow::anyhow!("{}", sanitize_gdrive_display(&error.to_string())))?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(key, value)| Some((key.to_string(), value.to_str().ok()?.to_owned())))
            .collect();
        let body = response
            .bytes()
            .map_err(|error| anyhow::anyhow!("{}", sanitize_gdrive_display(&error.to_string())))?;
        Ok(GdriveHttpResponse {
            status,
            headers,
            body: body.to_vec(),
        })
    }
}

struct RegisteredAccount {
    profile: GdriveProfile,
    refresh_token: ProfileSecret,
    access_token: Option<ProfileSecret>,
    access_expires_at: Instant,
}

pub struct GdriveProvider {
    transport: Arc<dyn GdriveTransport>,
    oauth: Mutex<Option<GdriveOAuthClient>>,
    accounts: Mutex<HashMap<[u8; 16], RegisteredAccount>>,
}

impl GdriveProvider {
    pub fn new() -> Result<Self> {
        Ok(Self {
            transport: Arc::new(ReqwestTransport::new()?),
            oauth: Mutex::new(load_oauth_client().ok()),
            accounts: Mutex::new(HashMap::new()),
        })
    }

    pub fn new_with_transport(
        transport: Arc<dyn GdriveTransport>,
        oauth: Option<GdriveOAuthClient>,
    ) -> Self {
        Self {
            transport,
            oauth: Mutex::new(oauth),
            accounts: Mutex::new(HashMap::new()),
        }
    }

    pub fn register_profile(&self, profile: GdriveProfile, refresh_token: String) -> Result<()> {
        profile.validate()?;
        if refresh_token.is_empty() {
            bail!("Google Drive refresh token is missing");
        }
        let mut accounts = self
            .accounts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        accounts.insert(
            profile.container_identity,
            RegisteredAccount {
                profile,
                refresh_token: ProfileSecret(refresh_token),
                access_token: None,
                access_expires_at: Instant::now(),
            },
        );
        Ok(())
    }

    pub fn register_authorized_profile(
        &self,
        profile: GdriveProfile,
        access_token: String,
        refresh_token: String,
    ) -> Result<()> {
        self.register_profile(profile.clone(), refresh_token)?;
        let mut accounts = self
            .accounts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(account) = accounts.get_mut(&profile.container_identity) {
            account.access_token = Some(ProfileSecret(access_token));
            account.access_expires_at = Instant::now() + Duration::from_secs(3000);
        }
        Ok(())
    }

    pub fn remove_profile(&self, identity: [u8; 16]) {
        self.accounts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&identity);
    }

    pub fn registered_profiles(&self) -> Vec<GdriveProfile> {
        self.accounts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .map(|account| account.profile.clone())
            .collect()
    }

    pub fn connect_account(&self, login_hint: Option<&str>) -> Result<(GdriveProfile, String)> {
        let oauth = self.oauth_client()?;
        let (refresh_token, email) = authorize_interactive(&*self.transport, &oauth, login_hint)?;
        let identity = explorer_model::remote_container_identity(
            explorer_model::RemoteProviderKind::Gdrive,
            &email,
        );
        let profile = GdriveProfile::new(email, identity)?;
        self.register_profile(profile.clone(), refresh_token.clone())?;
        Ok((profile, refresh_token))
    }

    fn oauth_client(&self) -> Result<GdriveOAuthClient> {
        if let Some(client) = self
            .oauth
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        {
            return Ok(client);
        }
        let client = load_oauth_client()?;
        *self
            .oauth
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(client.clone());
        Ok(client)
    }

    fn authorized_json(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
        method: &str,
        url: &str,
        content_type: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<serde_json::Value> {
        let response =
            self.authorized_execute(location, cancellation, method, url, content_type, body)?;
        if response.body.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_slice(&response.body)
            .map_err(|_| anyhow::anyhow!("Google Drive returned invalid JSON"))
    }

    fn authorized_execute(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
        method: &str,
        url: &str,
        content_type: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<GdriveHttpResponse> {
        let mut refreshed = false;
        let mut attempt = 0;
        loop {
            ensure_not_cancelled(cancellation)?;
            let token = self.access_token(location, cancellation)?;
            let mut headers = vec![("Authorization".to_owned(), format!("Bearer {token}"))];
            if let Some(content_type) = content_type {
                headers.push(("Content-Type".to_owned(), content_type.to_owned()));
            }
            let response = self.transport.execute(GdriveHttpRequest {
                method: method.to_owned(),
                url: url.to_owned(),
                headers,
                body: body.clone(),
            })?;
            if response.status == 401 && !refreshed {
                self.invalidate_access_token(location);
                refreshed = true;
                continue;
            }
            if retryable_drive_reason(
                parse_drive_error_body(&String::from_utf8_lossy(&response.body))
                    .1
                    .as_deref(),
                response.status,
            ) && attempt < MAX_RETRIES
            {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(200 * u64::from(attempt)));
                continue;
            }
            if response.status >= 400 {
                bail!(
                    "{}",
                    user_facing_drive_failure(
                        &String::from_utf8_lossy(&response.body),
                        response.status
                    )
                );
            }
            return Ok(response);
        }
    }

    fn access_token(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<String> {
        ensure_not_cancelled(cancellation)?;
        let (refresh_token, cached) = {
            let accounts = self
                .accounts
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let account = accounts
                .get(&location.container_identity)
                .context("Google Drive account is not signed in")?;
            if let Some(token) = &account.access_token
                && Instant::now() < account.access_expires_at
            {
                return Ok(token.0.clone());
            }
            (account.refresh_token.0.clone(), ())
        };
        let _ = cached;
        let oauth = self.oauth_client()?;
        let tokens = refresh_access_token(&*self.transport, &oauth, &refresh_token)?;
        let mut accounts = self
            .accounts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let account = accounts
            .get_mut(&location.container_identity)
            .context("Google Drive account is not signed in")?;
        account.access_token = Some(ProfileSecret(tokens.access_token.clone()));
        account.access_expires_at =
            Instant::now() + Duration::from_secs(tokens.expires_in.saturating_sub(60).max(30));
        if let Some(refresh) = tokens.refresh_token {
            account.refresh_token = ProfileSecret(refresh);
        }
        Ok(tokens.access_token)
    }

    fn invalidate_access_token(&self, location: &VirtualLocationDescriptor) {
        let mut accounts = self
            .accounts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(account) = accounts.get_mut(&location.container_identity) {
            account.access_token = None;
            account.access_expires_at = Instant::now();
        }
    }

    fn resolve_file_id(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<String> {
        if let Some(id) = location
            .provider_entry_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(id.to_owned());
        }
        if location.components.is_empty() {
            return Ok(ROOT_ID.to_owned());
        }
        let mut parent_id = ROOT_ID.to_owned();
        let mut prefix = location.clone();
        prefix.components.clear();
        prefix.provider_entry_key = Some(parent_id.clone());
        for (index, component) in location.components.iter().enumerate() {
            let entries = self.list_children(&prefix, &parent_id, cancellation)?;
            let decoded = decode_drive_component(component);
            let found = entries.into_iter().find(|entry| {
                entry.name == decoded || encode_drive_component(&entry.name) == *component
            });
            let Some(found) = found else {
                bail!("Google Drive path was not found");
            };
            let LocationDescriptor::Virtual(found_location) = found.location else {
                bail!("Google Drive path was not found");
            };
            parent_id = found_location
                .provider_entry_key
                .context("Google Drive item is missing a file id")?;
            prefix.components = location.components[..=index].to_vec();
            prefix.provider_entry_key = Some(parent_id.clone());
        }
        Ok(parent_id)
    }

    fn list_children(
        &self,
        location: &VirtualLocationDescriptor,
        parent_id: &str,
        cancellation: &CancellationToken,
    ) -> Result<Vec<RemoteEntry>> {
        let mut entries = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            ensure_not_cancelled(cancellation)?;
            let mut url = format!(
                "{DRIVE_FILES_URL}?q={}&fields={}&pageSize=1000&supportsAllDrives=false",
                percent_encode(&list_query(parent_id)),
                percent_encode(list_fields())
            );
            if let Some(token) = &page_token {
                url.push_str("&pageToken=");
                url.push_str(&percent_encode(token));
            }
            let json = self.authorized_json(location, cancellation, "GET", &url, None, None)?;
            if let Some(files) = json.get("files").and_then(serde_json::Value::as_array) {
                for file in files {
                    if let Some(entry) = remote_entry_from_file(location, file) {
                        entries.push(entry);
                    }
                }
            }
            page_token = json
                .get("nextPageToken")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            if page_token.is_none() {
                break;
            }
        }
        Ok(entries)
    }

    fn child_location(
        parent: &VirtualLocationDescriptor,
        name: &str,
        file_id: &str,
    ) -> VirtualLocationDescriptor {
        let mut child = parent.clone();
        child.components.push(encode_drive_component(name));
        child.entry_id = None;
        child.provider_entry_key = Some(file_id.to_owned());
        child
    }
}

impl RemoteProvider for GdriveProvider {
    fn provider_id(&self) -> &'static str {
        "gdrive"
    }

    fn capabilities(&self) -> RemoteProviderCapabilities {
        RemoteProviderCapabilities::gdrive_defaults()
    }

    fn list(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<Vec<RemoteEntry>> {
        validate_remote_location(location, "gdrive", true)?;
        let parent_id = self.resolve_file_id(location, cancellation)?;
        self.list_children(location, &parent_id, cancellation)
    }

    fn download(
        &self,
        source: &VirtualLocationDescriptor,
        local_destination: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        self.download_with_progress(source, local_destination, cancellation, &|_| {})
    }

    fn download_with_progress(
        &self,
        source: &VirtualLocationDescriptor,
        local_destination: &Path,
        cancellation: &CancellationToken,
        progress: &(dyn Fn(u64) + Send + Sync),
    ) -> Result<()> {
        validate_remote_location(source, "gdrive", false)?;
        let file_id = self.resolve_file_id(source, cancellation)?;
        let info_url = format!(
            "{DRIVE_FILES_URL}/{}?fields=id,name,mimeType,shortcutDetails(targetId,targetMimeType)",
            percent_encode(&file_id)
        );
        let info = self.authorized_json(source, cancellation, "GET", &info_url, None, None)?;
        let (display_name, kind, _, resolved_id) =
            file_presentation(&info).context("Google Drive item metadata is unavailable")?;
        if kind == RemoteEntryKind::Directory {
            bail!("Google Drive folders cannot be downloaded as a single file");
        }
        let mime = info
            .get("shortcutDetails")
            .and_then(|details| details.get("targetMimeType"))
            .and_then(serde_json::Value::as_str)
            .or_else(|| info.get("mimeType").and_then(serde_json::Value::as_str))
            .unwrap_or_default();
        let bytes = if let Some(export) = export_target(mime) {
            let url = format!(
                "{DRIVE_FILES_URL}/{}/export?mimeType={}",
                percent_encode(&resolved_id),
                percent_encode(export.mime_type)
            );
            self.authorized_execute(source, cancellation, "GET", &url, None, None)?
                .body
        } else if is_google_native(mime) {
            bail!("Google Drive item '{display_name}' cannot be downloaded as a regular file");
        } else {
            let url = format!(
                "{DRIVE_FILES_URL}/{}?alt=media",
                percent_encode(&resolved_id)
            );
            self.authorized_execute(source, cancellation, "GET", &url, None, None)?
                .body
        };
        if let Some(parent) = local_destination.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(local_destination, &bytes).context("write Google Drive download")?;
        progress(bytes.len() as u64);
        Ok(())
    }

    fn upload(
        &self,
        local_source: &Path,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        self.upload_with_progress(local_source, destination, cancellation, &|_| {})
    }

    fn upload_with_progress(
        &self,
        local_source: &Path,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
        progress: &(dyn Fn(u64) + Send + Sync),
    ) -> Result<()> {
        validate_remote_location(destination, "gdrive", true)?;
        ensure_not_cancelled(cancellation)?;
        let name = local_source
            .file_name()
            .and_then(|value| value.to_str())
            .context("Google Drive upload requires a file name")?;
        crate::provider::validate_windows_component(name).ok();
        let parent_id = self.resolve_file_id(destination, cancellation)?;
        let bytes = std::fs::read(local_source).context("read Google Drive upload source")?;
        let metadata = serde_json::json!({
            "name": name,
            "parents": [parent_id],
        });
        let boundary = "superexplorer_gdrive_boundary";
        let mut body = Vec::new();
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(metadata.to_string().as_bytes());
        body.extend_from_slice(
            format!("\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(&bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let url = format!("{DRIVE_UPLOAD_URL}?uploadType=multipart&supportsAllDrives=false");
        self.authorized_execute(
            destination,
            cancellation,
            "POST",
            &url,
            Some(&format!("multipart/related; boundary={boundary}")),
            Some(body),
        )?;
        progress(bytes.len() as u64);
        Ok(())
    }

    fn create_directory(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_location(location, "gdrive", false)?;
        let mut parent = location.clone();
        let name = parent
            .components
            .pop()
            .context("Google Drive folder name is missing")?;
        parent.provider_entry_key = None;
        let parent_id = self.resolve_file_id(&parent, cancellation)?;
        let body = serde_json::json!({
            "name": decode_drive_component(&name),
            "mimeType": FOLDER_MIME,
            "parents": [parent_id],
        })
        .to_string()
        .into_bytes();
        self.authorized_execute(
            location,
            cancellation,
            "POST",
            &format!("{DRIVE_FILES_URL}?supportsAllDrives=false"),
            Some("application/json"),
            Some(body),
        )?;
        Ok(())
    }

    fn metadata(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<RemoteMetadata> {
        validate_remote_location(location, "gdrive", true)?;
        if location.components.is_empty() {
            return Ok(RemoteMetadata {
                location: LocationDescriptor::Virtual(location.clone()),
                kind: RemoteEntryKind::Directory,
                size: None,
                unix_mode: None,
                modified_unix_seconds: None,
            });
        }
        let file_id = self.resolve_file_id(location, cancellation)?;
        let url = format!(
            "{DRIVE_FILES_URL}/{}?fields=id,name,mimeType,size,modifiedTime,shortcutDetails(targetId,targetMimeType)",
            percent_encode(&file_id)
        );
        let json = self.authorized_json(location, cancellation, "GET", &url, None, None)?;
        remote_metadata_from_file(location, &json).context("Google Drive metadata is unavailable")
    }

    fn rename(
        &self,
        source: &VirtualLocationDescriptor,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_location(source, "gdrive", false)?;
        validate_remote_location(destination, "gdrive", false)?;
        let file_id = self.resolve_file_id(source, cancellation)?;
        let new_name = destination
            .components
            .last()
            .map(|name| decode_drive_component(name))
            .context("Google Drive rename destination is invalid")?;
        let mut url = format!(
            "{DRIVE_FILES_URL}/{}?supportsAllDrives=false",
            percent_encode(&file_id)
        );
        let source_parent = source.components[..source.components.len().saturating_sub(1)].to_vec();
        let dest_parent =
            destination.components[..destination.components.len().saturating_sub(1)].to_vec();
        if source_parent != dest_parent {
            let mut from_parent = source.clone();
            from_parent.components = source_parent;
            from_parent.provider_entry_key = None;
            let mut to_parent = destination.clone();
            to_parent.components = dest_parent;
            to_parent.provider_entry_key = None;
            let remove = self.resolve_file_id(&from_parent, cancellation)?;
            let add = self.resolve_file_id(&to_parent, cancellation)?;
            url.push_str(&format!(
                "&addParents={}&removeParents={}",
                percent_encode(&add),
                percent_encode(&remove)
            ));
        }
        let body = serde_json::json!({ "name": new_name })
            .to_string()
            .into_bytes();
        self.authorized_execute(
            source,
            cancellation,
            "PATCH",
            &url,
            Some("application/json"),
            Some(body),
        )?;
        Ok(())
    }

    fn delete(
        &self,
        location: &VirtualLocationDescriptor,
        _recursive: bool,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_location(location, "gdrive", false)?;
        let file_id = self.resolve_file_id(location, cancellation)?;
        let url = format!(
            "{DRIVE_FILES_URL}/{}?supportsAllDrives=false",
            percent_encode(&file_id)
        );
        let body = serde_json::json!({ "trashed": true })
            .to_string()
            .into_bytes();
        self.authorized_execute(
            location,
            cancellation,
            "PATCH",
            &url,
            Some("application/json"),
            Some(body),
        )?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct OAuthTokenSet {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

pub fn resolve_oauth_client(
    env_client_id: Option<&str>,
    env_client_secret: Option<&str>,
    file_json: Option<&str>,
    bundled_json: &str,
) -> Result<GdriveOAuthClient> {
    if let Some(client_id) = env_client_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(GdriveOAuthClient {
            client_id: client_id.to_owned(),
            client_secret: env_client_secret
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        });
    }
    if let Some(file_json) = file_json {
        let value: serde_json::Value =
            serde_json::from_str(file_json).context("Google Drive OAuth config JSON is invalid")?;
        if let Some(client) = oauth_client_from_json(&value) {
            return Ok(client);
        }
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(bundled_json)
        && let Some(client) = oauth_client_from_json(&value)
    {
        return Ok(client);
    }
    bail!(
        "Google Drive is not configured. Set SUPEREXPLORER_GOOGLE_OAUTH_CLIENT_ID, create %LOCALAPPDATA%\\RustGpuiExplorer\\remote\\google-oauth.json, or place google-oauth.local.json next to explorer-remote before building"
    );
}

const BUNDLED_OAUTH_JSON: &str = include_str!(env!("SUPEREXPLORER_GDRIVE_OAUTH_JSON"));

pub fn load_oauth_client() -> Result<GdriveOAuthClient> {
    let env_id = std::env::var("SUPEREXPLORER_GOOGLE_OAUTH_CLIENT_ID").ok();
    let env_secret = std::env::var("SUPEREXPLORER_GOOGLE_OAUTH_CLIENT_SECRET").ok();
    let file_json = oauth_config_path().and_then(|path| std::fs::read_to_string(path).ok());
    resolve_oauth_client(
        env_id.as_deref(),
        env_secret.as_deref(),
        file_json.as_deref(),
        BUNDLED_OAUTH_JSON,
    )
}

fn oauth_client_from_json(value: &serde_json::Value) -> Option<GdriveOAuthClient> {
    let source = value
        .get("installed")
        .or_else(|| value.get("web"))
        .unwrap_or(value);
    let client_id = source
        .get("client_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(GdriveOAuthClient {
        client_id: client_id.to_owned(),
        client_secret: source
            .get("client_secret")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
    })
}

fn oauth_config_path() -> Option<std::path::PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(
        std::path::PathBuf::from(local)
            .join("RustGpuiExplorer")
            .join("remote")
            .join("google-oauth.json"),
    )
}

pub fn authorization_url(
    client: &GdriveOAuthClient,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
    login_hint: Option<&str>,
) -> String {
    let mut url = format!(
        "{OAUTH_AUTH_URL}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&code_challenge={}&code_challenge_method=S256&state={}",
        percent_encode(&client.client_id),
        percent_encode(redirect_uri),
        percent_encode(DRIVE_SCOPE),
        percent_encode(challenge),
        percent_encode(state),
    );
    if let Some(hint) = login_hint.filter(|value| !value.is_empty()) {
        url.push_str("&login_hint=");
        url.push_str(&percent_encode(hint));
    }
    url
}

fn generate_pkce() -> (String, String, String) {
    let verifier_bytes = explorer_model::new_remote_container_identity();
    let state_bytes = explorer_model::new_remote_container_identity();
    let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
    let state = URL_SAFE_NO_PAD.encode(state_bytes);
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge, state)
}

fn parse_token_response(body: &str) -> Result<OAuthTokenSet> {
    let value: serde_json::Value =
        serde_json::from_str(body).context("Google OAuth token JSON is invalid")?;
    let access_token = value
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .context("Google OAuth access token is missing")?
        .to_owned();
    let refresh_token = value
        .get("refresh_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let expires_in = value
        .get("expires_in")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(3600);
    Ok(OAuthTokenSet {
        access_token,
        refresh_token,
        expires_in,
    })
}

fn refresh_access_token(
    transport: &dyn GdriveTransport,
    client: &GdriveOAuthClient,
    refresh_token: &str,
) -> Result<OAuthTokenSet> {
    let mut form = format!(
        "client_id={}&refresh_token={}&grant_type=refresh_token",
        percent_encode(&client.client_id),
        percent_encode(refresh_token)
    );
    if let Some(secret) = &client.client_secret {
        form.push_str("&client_secret=");
        form.push_str(&percent_encode(secret));
    }
    let response = transport.execute(GdriveHttpRequest {
        method: "POST".to_owned(),
        url: OAUTH_TOKEN_URL.to_owned(),
        headers: vec![(
            "Content-Type".to_owned(),
            "application/x-www-form-urlencoded".to_owned(),
        )],
        body: Some(form.into_bytes()),
    })?;
    if response.status >= 400 {
        bail!(
            "{}",
            user_facing_drive_failure(&String::from_utf8_lossy(&response.body), response.status)
        );
    }
    parse_token_response(&String::from_utf8_lossy(&response.body))
}

fn exchange_code(
    transport: &dyn GdriveTransport,
    client: &GdriveOAuthClient,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<OAuthTokenSet> {
    let mut form = format!(
        "client_id={}&code={}&redirect_uri={}&grant_type=authorization_code&code_verifier={}",
        percent_encode(&client.client_id),
        percent_encode(code),
        percent_encode(redirect_uri),
        percent_encode(verifier)
    );
    if let Some(secret) = &client.client_secret {
        form.push_str("&client_secret=");
        form.push_str(&percent_encode(secret));
    }
    let response = transport.execute(GdriveHttpRequest {
        method: "POST".to_owned(),
        url: OAUTH_TOKEN_URL.to_owned(),
        headers: vec![(
            "Content-Type".to_owned(),
            "application/x-www-form-urlencoded".to_owned(),
        )],
        body: Some(form.into_bytes()),
    })?;
    if response.status >= 400 {
        bail!(
            "{}",
            user_facing_drive_failure(&String::from_utf8_lossy(&response.body), response.status)
        );
    }
    let tokens = parse_token_response(&String::from_utf8_lossy(&response.body))?;
    if tokens.refresh_token.is_none() {
        bail!("Google OAuth did not return a refresh token");
    }
    Ok(tokens)
}

fn authorize_interactive(
    transport: &dyn GdriveTransport,
    client: &GdriveOAuthClient,
    login_hint: Option<&str>,
) -> Result<(String, String)> {
    let listener = TcpListener::bind("127.0.0.1:0").context("Google Drive OAuth loopback")?;
    listener.set_nonblocking(true).ok();
    let port = listener
        .local_addr()
        .context("OAuth loopback address")?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/");
    let (verifier, challenge, state) = generate_pkce();
    let url = authorization_url(client, &redirect_uri, &state, &challenge, login_hint);
    open_browser(&url)?;
    let code = wait_for_oauth_code(&listener, &state)?;
    let tokens = exchange_code(transport, client, &code, &redirect_uri, &verifier)?;
    let refresh = tokens
        .refresh_token
        .context("Google OAuth did not return a refresh token")?;
    let email = fetch_account_email(transport, &tokens.access_token)?;
    Ok((refresh, email))
}

fn fetch_account_email(transport: &dyn GdriveTransport, access_token: &str) -> Result<String> {
    let response = transport.execute(GdriveHttpRequest {
        method: "GET".to_owned(),
        url: format!("{DRIVE_ABOUT_URL}?fields=user(emailAddress)"),
        headers: vec![("Authorization".to_owned(), format!("Bearer {access_token}"))],
        body: None,
    })?;
    if response.status >= 400 {
        bail!(
            "{}",
            user_facing_drive_failure(&String::from_utf8_lossy(&response.body), response.status)
        );
    }
    let value: serde_json::Value =
        serde_json::from_slice(&response.body).context("Google Drive about JSON is invalid")?;
    value
        .pointer("/user/emailAddress")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .context("Google Drive account email is missing")
}

fn browser_launch_program() -> &'static str {
    "rundll32"
}

fn browser_launch_args(url: &str) -> [&str; 2] {
    ["url.dll,FileProtocolHandler", url]
}

fn open_browser(url: &str) -> Result<()> {
    // Do not use `cmd /C start`: cmd splits on `&`, and `start` treats `https://` as UNC `\\`.
    let mut command = std::process::Command::new(browser_launch_program());
    command.args(browser_launch_args(url));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let status = command.status().context("open Google sign-in browser")?;
    if status.success() {
        Ok(())
    } else {
        bail!("Unable to open the Google sign-in browser")
    }
}

fn wait_for_oauth_code(listener: &TcpListener, expected_state: &str) -> Result<String> {
    let deadline = Instant::now() + OAUTH_TIMEOUT;
    let (mut stream, _) = loop {
        match listener.accept() {
            Ok(accepted) => break accepted,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                if Instant::now() >= deadline {
                    bail!("Google sign-in timed out or was cancelled");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => bail!("Google sign-in timed out or was cancelled"),
        }
    };
    stream.set_read_timeout(Some(Duration::from_secs(15))).ok();
    let mut buffer = [0_u8; 4096];
    let read = stream.read(&mut buffer).unwrap_or(0);
    let request = String::from_utf8_lossy(&buffer[..read]);
    let line = request.lines().next().unwrap_or_default();
    let path = line.split_whitespace().nth(1).unwrap_or_default();
    let query = path.split_once('?').map(|(_, query)| query).unwrap_or("");
    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        match key {
            "code" => code = Some(percent_decode_query(value)),
            "state" => state = Some(percent_decode_query(value)),
            _ => {}
        }
    }
    let body = b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><title>SuperExplorer</title>You can close this window and return to SuperExplorer.";
    let _ = stream.write_all(body);
    if state.as_deref() != Some(expected_state) {
        bail!("Google sign-in state did not match");
    }
    code.filter(|value| !value.is_empty())
        .context("Google sign-in did not return an authorization code")
}

fn percent_decode_query(value: &str) -> String {
    crate::gdrive_protocol::percent_decode(&value.replace('+', " "))
}

fn ensure_not_cancelled(cancellation: &CancellationToken) -> Result<()> {
    if cancellation.is_cancelled() {
        bail!("cancelled");
    }
    Ok(())
}

fn remote_entry_from_file(
    parent: &VirtualLocationDescriptor,
    file: &serde_json::Value,
) -> Option<RemoteEntry> {
    let (name, kind, size, file_id) = file_presentation(file)?;
    let location = GdriveProvider::child_location(parent, &name, &file_id);
    Some(RemoteEntry {
        name,
        location: LocationDescriptor::Virtual(location),
        kind,
        size,
        unix_mode: None,
    })
}

fn remote_metadata_from_file(
    location: &VirtualLocationDescriptor,
    file: &serde_json::Value,
) -> Option<RemoteMetadata> {
    let (name, kind, size, file_id) = file_presentation(file)?;
    let mut resolved = location.clone();
    resolved.provider_entry_key = Some(file_id);
    if resolved.components.last().is_none() {
        resolved.components.push(encode_drive_component(&name));
    }
    Some(RemoteMetadata {
        location: LocationDescriptor::Virtual(resolved),
        kind,
        size,
        unix_mode: None,
        modified_unix_seconds: file
            .get("modifiedTime")
            .and_then(serde_json::Value::as_str)
            .and_then(parse_rfc3339_unix_seconds),
    })
}

fn file_presentation(
    file: &serde_json::Value,
) -> Option<(String, RemoteEntryKind, Option<u64>, String)> {
    let mut mime = file.get("mimeType")?.as_str()?.to_owned();
    let mut file_id = file.get("id")?.as_str()?.to_owned();
    if is_shortcut_mime(&mime) {
        let details = file.get("shortcutDetails")?;
        file_id = details.get("targetId")?.as_str()?.to_owned();
        mime = details
            .get("targetMimeType")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&mime)
            .to_owned();
    }
    let mut name = file.get("name")?.as_str()?.to_owned();
    if let Some(export) = export_target(&mime) {
        name = crate::gdrive_protocol::exported_file_name(&name, export.extension);
    }
    let kind = if is_folder_mime(&mime) {
        RemoteEntryKind::Directory
    } else {
        RemoteEntryKind::File
    };
    let size = file
        .get("size")
        .and_then(|value| match value {
            serde_json::Value::String(text) => text.parse().ok(),
            serde_json::Value::Number(number) => number.as_u64(),
            _ => None,
        })
        .filter(|_| !is_google_native(&mime) && kind == RemoteEntryKind::File);
    Some((name, kind, size, file_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FnTransport<F>(F);

    impl<F> GdriveTransport for FnTransport<F>
    where
        F: Fn(&GdriveHttpRequest) -> Result<GdriveHttpResponse> + Send + Sync,
    {
        fn execute(&self, request: GdriveHttpRequest) -> Result<GdriveHttpResponse> {
            (self.0)(&request)
        }
    }

    fn json_response(status: u16, body: &str) -> GdriveHttpResponse {
        GdriveHttpResponse {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn bytes_response(status: u16, body: &[u8]) -> GdriveHttpResponse {
        GdriveHttpResponse {
            status,
            headers: Vec::new(),
            body: body.to_vec(),
        }
    }

    fn test_location(key: Option<&str>, components: &[&str]) -> VirtualLocationDescriptor {
        VirtualLocationDescriptor {
            provider_id: "gdrive".to_owned(),
            public_authority: Some("you@gmail.com".to_owned()),
            container_identity: [4; 16],
            container_generation: 1,
            entry_id: None,
            provider_entry_key: key.map(str::to_owned),
            components: components.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    fn test_provider(
        handler: impl Fn(&GdriveHttpRequest) -> Result<GdriveHttpResponse> + Send + Sync + 'static,
    ) -> GdriveProvider {
        let provider = GdriveProvider::new_with_transport(
            Arc::new(FnTransport(handler)),
            Some(GdriveOAuthClient {
                client_id: "client.apps.googleusercontent.com".to_owned(),
                client_secret: None,
            }),
        );
        let profile = GdriveProfile::new("you@gmail.com".into(), [4; 16]).unwrap();
        provider
            .register_authorized_profile(profile, "access-token".into(), "refresh-token".into())
            .unwrap();
        provider
    }

    #[test]
    fn list_my_drive_maps_folders_files_shortcuts_and_duplicate_names() {
        let provider = test_provider(|request| {
            assert!(request.url.contains("trashed"));
            Ok(json_response(
                200,
                r#"{"files":[
                    {"id":"folder-1","name":"Work","mimeType":"application/vnd.google-apps.folder"},
                    {"id":"doc-1","name":"Notes","mimeType":"application/vnd.google-apps.document"},
                    {"id":"file-a","name":"Report.docx","mimeType":"text/plain","size":"12"},
                    {"id":"file-b","name":"Report.docx","mimeType":"text/plain","size":"8"},
                    {"id":"shortcut-1","name":"Shared folder","mimeType":"application/vnd.google-apps.shortcut","shortcutDetails":{"targetId":"folder-2","targetMimeType":"application/vnd.google-apps.folder"}}
                ]}"#,
            ))
        });
        let entries = provider
            .list(&test_location(None, &[]), &CancellationToken::new())
            .unwrap();
        assert_eq!(entries.len(), 5);
        assert!(
            entries
                .iter()
                .any(|entry| { entry.name == "Work" && entry.kind == RemoteEntryKind::Directory })
        );
        assert!(entries.iter().any(|entry| entry.name == "Notes.docx"));
        let reports: Vec<_> = entries
            .iter()
            .filter(|entry| entry.name == "Report.docx")
            .collect();
        assert_eq!(reports.len(), 2);
        let LocationDescriptor::Virtual(left) = &reports[0].location else {
            panic!("virtual");
        };
        let LocationDescriptor::Virtual(right) = &reports[1].location else {
            panic!("virtual");
        };
        assert_ne!(left.provider_entry_key, right.provider_entry_key);
        let shortcut = entries
            .iter()
            .find(|entry| entry.name == "Shared folder")
            .unwrap();
        assert_eq!(shortcut.kind, RemoteEntryKind::Directory);
        let LocationDescriptor::Virtual(shortcut_location) = &shortcut.location else {
            panic!("virtual");
        };
        assert_eq!(
            shortcut_location.provider_entry_key.as_deref(),
            Some("folder-2")
        );
    }

    #[test]
    fn download_exports_google_doc_and_writes_office_bytes() {
        let provider = test_provider(|request| {
            if request.url.contains("/export") {
                assert!(request.url.contains("wordprocessingml"));
                return Ok(bytes_response(200, b"PK-docx"));
            }
            if request.url.contains("/files/doc-1") && !request.url.contains("alt=media") {
                return Ok(json_response(
                    200,
                    r#"{"id":"doc-1","name":"Notes","mimeType":"application/vnd.google-apps.document"}"#,
                ));
            }
            Ok(json_response(404, r#"{"error":{"message":"missing"}}"#))
        });
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("Notes.docx");
        provider
            .download_with_progress(
                &test_location(Some("doc-1"), &["Notes"]),
                &destination,
                &CancellationToken::new(),
                &|_| {},
            )
            .unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"PK-docx");
    }

    #[test]
    fn delete_patches_trashed_true_not_permanent_delete() {
        let seen = Arc::new(Mutex::new(String::new()));
        let seen_for_handler = Arc::clone(&seen);
        let provider = test_provider(move |request| {
            *seen_for_handler.lock().unwrap() = format!(
                "{} {} {}",
                request.method,
                request.url,
                String::from_utf8_lossy(request.body.as_deref().unwrap_or(b""))
            );
            Ok(json_response(200, r#"{"id":"file-a","trashed":true}"#))
        });
        provider
            .delete(
                &test_location(Some("file-a"), &["Report.docx"]),
                true,
                &CancellationToken::new(),
            )
            .unwrap();
        let seen = seen.lock().unwrap().clone();
        assert!(seen.starts_with("PATCH "));
        assert!(seen.contains("trashed"));
        assert!(!seen.to_ascii_lowercase().contains("delete "));
    }

    #[test]
    fn upload_and_mkdir_and_rename_use_drive_ids() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_handler = Arc::clone(&calls);
        let provider = test_provider(move |request| {
            calls_for_handler.lock().unwrap().push((
                request.method.clone(),
                request.url.clone(),
                String::from_utf8_lossy(request.body.as_deref().unwrap_or(b"")).into_owned(),
            ));
            Ok(json_response(200, r#"{"id":"created"}"#))
        });
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("hello.txt");
        std::fs::write(&source, b"hello").unwrap();
        provider
            .upload(
                &source,
                &test_location(Some("folder-1"), &["Work"]),
                &CancellationToken::new(),
            )
            .unwrap();
        provider
            .create_directory(
                &test_location(None, &["Projects"]),
                &CancellationToken::new(),
            )
            .unwrap();
        provider
            .rename(
                &test_location(Some("file-a"), &["old.txt"]),
                &test_location(Some("file-a"), &["new.txt"]),
                &CancellationToken::new(),
            )
            .unwrap();
        let calls = calls.lock().unwrap().clone();
        assert!(calls.iter().any(|(method, url, body)| method == "POST"
            && url.contains("/upload/drive/v3/files")
            && body.contains("hello")));
        assert!(calls.iter().any(|(method, _, body)| method == "POST"
            && body.contains(FOLDER_MIME)
            && body.contains("Projects")));
        assert!(
            calls
                .iter()
                .any(|(method, _, body)| method == "PATCH" && body.contains("new.txt"))
        );
    }

    #[test]
    fn unauthorized_list_refreshes_access_token_once() {
        let attempts = AtomicUsize::new(0);
        let provider = test_provider(move |request| {
            if request.url.contains("oauth2.googleapis.com/token") {
                return Ok(json_response(
                    200,
                    r#"{"access_token":"next-token","expires_in":3600}"#,
                ));
            }
            let count = attempts.fetch_add(1, Ordering::SeqCst);
            if count == 0 {
                return Ok(json_response(
                    401,
                    r#"{"error":{"message":"invalid authentication credentials"}}"#,
                ));
            }
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(name, value)| name == "Authorization" && value == "Bearer next-token")
            );
            Ok(json_response(200, r#"{"files":[]}"#))
        });
        let entries = provider
            .list(&test_location(None, &[]), &CancellationToken::new())
            .unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn missing_oauth_client_is_actionable() {
        let error = resolve_oauth_client(None, None, None, "")
            .unwrap_err()
            .to_string();
        assert!(error.contains("SUPEREXPLORER_GOOGLE_OAUTH_CLIENT_ID"));
        assert!(error.contains("google-oauth.json"));
        let client = resolve_oauth_client(
            None,
            None,
            Some(r#"{"client_id":"from-file.apps.googleusercontent.com"}"#),
            "",
        )
        .unwrap();
        assert_eq!(client.client_id, "from-file.apps.googleusercontent.com");
        let installed = resolve_oauth_client(
            None,
            None,
            Some(
                r#"{"installed":{"client_id":"desktop.apps.googleusercontent.com","client_secret":"secret"}}"#,
            ),
            "",
        )
        .unwrap();
        assert_eq!(installed.client_id, "desktop.apps.googleusercontent.com");
        assert_eq!(installed.client_secret.as_deref(), Some("secret"));
        let bundled = resolve_oauth_client(
            None,
            None,
            None,
            r#"{"installed":{"client_id":"bundled.apps.googleusercontent.com","client_secret":"bundled-secret"}}"#,
        )
        .unwrap();
        assert_eq!(bundled.client_id, "bundled.apps.googleusercontent.com");
        assert_eq!(bundled.client_secret.as_deref(), Some("bundled-secret"));
        assert!(resolve_oauth_client(None, None, None, "{}").is_err());
        assert!(serde_json::from_str::<serde_json::Value>(BUNDLED_OAUTH_JSON).is_ok());
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(BUNDLED_OAUTH_JSON)
            && let Some(client) = oauth_client_from_json(&value)
        {
            assert!(
                client.client_id.ends_with(".apps.googleusercontent.com"),
                "bundled Desktop client id must be a Google OAuth client"
            );
            assert!(client.client_secret.is_some());
        }
    }

    #[test]
    fn authorization_url_uses_pkce_and_drive_scope() {
        let client = GdriveOAuthClient {
            client_id: "id.apps.googleusercontent.com".to_owned(),
            client_secret: None,
        };
        let url = authorization_url(
            &client,
            "http://127.0.0.1:1234/",
            "state-1",
            "challenge-1",
            Some("you@gmail.com"),
        );
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge=challenge-1"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains(&percent_encode(DRIVE_SCOPE)));
        assert!(url.contains("login_hint=you%40gmail.com"));
        assert_eq!(browser_launch_program(), "rundll32");
        let args = browser_launch_args(&url);
        assert_eq!(args[0], "url.dll,FileProtocolHandler");
        assert_eq!(args[1], url.as_str());
        assert!(
            args[1].contains("&response_type=code"),
            "OAuth query must be passed as one argument, not through cmd.exe start: {}",
            args[1]
        );
        let tokens = parse_token_response(
            r#"{"access_token":"ya29.a","refresh_token":"1//r","expires_in":3600}"#,
        )
        .unwrap();
        assert_eq!(tokens.refresh_token.as_deref(), Some("1//r"));
        assert_eq!(
            format!("{:?}", ProfileSecret("1//r".into())),
            "ProfileSecret(*)"
        );
        assert!(!format!("{tokens:?}").contains("unused"));
    }

    #[test]
    fn non_exportable_google_form_fails_download() {
        let provider = test_provider(|request| {
            assert!(request.url.contains("/files/form-1"));
            Ok(json_response(
                200,
                r#"{"id":"form-1","name":"Survey","mimeType":"application/vnd.google-apps.form"}"#,
            ))
        });
        let error = provider
            .download(
                &test_location(Some("form-1"), &["Survey"]),
                Path::new("survey.form"),
                &CancellationToken::new(),
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("cannot be downloaded"));
    }

    #[test]
    fn secret_debug_does_not_contain_refresh_token() {
        let secret = ProfileSecret("1//very-secret-refresh".into());
        let debug = format!("{secret:?}");
        assert_eq!(debug, "ProfileSecret(*)");
        assert!(!debug.contains("very-secret"));
    }
}
