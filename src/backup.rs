use crate::{portable::{atomic_write, file_hash}, server::App, store::validate_backup};
use anyhow::{Context, Result, ensure};
use hmac::{Hmac, Mac};
use reqwest::{Client, Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, fmt::Write as _, path::{Path, PathBuf}, sync::Arc, time::Duration};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Contents {
    pub game: bool,
    pub config: bool,
    pub plugins: bool,
    pub rules: bool,
    pub decks: bool,
    pub logs: bool,
}
impl Default for Contents {
    fn default() -> Self {
        Self { game: true, config: true, plugins: true, rules: true, decks: true, logs: true }
    }
}
impl Contents {
    pub fn directories(&self) -> Vec<&'static str> {
        [ ("config", self.config), ("plugins", self.plugins), ("rules", self.rules),
          ("decks", self.decks), ("logs", self.logs) ]
            .into_iter().filter_map(|(name, selected)| selected.then_some(name)).collect()
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.game || self.config || self.plugins || self.rules || self.decks || self.logs, "请至少选择一项备份内容");
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WebDav {
    pub enabled: bool,
    pub url: String,
    pub username: String,
    pub password: String,
    #[serde(skip_serializing)]
    pub clear_credentials: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct S3 {
    pub enabled: bool,
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub prefix: String,
    pub path_style: bool,
    pub access_key: String,
    pub secret_key: String,
    pub session_token: String,
    #[serde(skip_serializing)]
    pub clear_credentials: bool,
}
impl Default for S3 {
    fn default() -> Self {
        Self { enabled: false, endpoint: String::new(), region: "us-east-1".into(), bucket: String::new(),
            prefix: "qianbian".into(), path_style: true, access_key: String::new(), secret_key: String::new(),
            session_token: String::new(), clear_credentials: false }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupConfig {
    pub enabled: bool,
    pub interval_minutes: u64,
    pub keep: usize,
    pub contents: Contents,
    pub webdav: WebDav,
    pub s3: S3,
}
impl Default for BackupConfig {
    fn default() -> Self {
        Self { enabled: false, interval_minutes: 1440, keep: 7, contents: Default::default(),
            webdav: Default::default(), s3: Default::default() }
    }
}
fn endpoint(value: &str) -> Result<Url> {
    let url = Url::parse(value).context("远端地址无效")?;
    ensure!(["https", "http"].contains(&url.scheme()) && url.host_str().is_some()
        && url.username().is_empty() && url.password().is_none() && url.query().is_none()
        && url.fragment().is_none(), "地址须为 HTTP(S)，不能含凭据、查询参数或片段");
    Ok(url)
}
impl BackupConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!((1..=525600).contains(&self.interval_minutes), "备份间隔应为 1–525600 分钟");
        ensure!((1..=1000).contains(&self.keep), "保留数量应为 1–1000 次");
        self.contents.validate()?;
        if self.webdav.enabled {
            endpoint(&self.webdav.url)?;
            ensure!(!self.webdav.username.contains(':'), "WebDAV 用户名不能含冒号");
        }
        if self.s3.enabled {
            endpoint(&self.s3.endpoint)?;
            ensure!(!self.s3.region.is_empty() && self.s3.region.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'), "S3 区域无效");
            ensure!(!self.s3.bucket.is_empty() && self.s3.bucket.len() <= 63
                && self.s3.bucket.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.'), "S3 存储桶无效");
            ensure!(!self.s3.access_key.is_empty() && !self.s3.secret_key.is_empty(), "请填写 S3 访问密钥");
            ensure!(self.s3.prefix.len() <= 512 && !self.s3.prefix.split('/').any(|s| s == "." || s == ".."), "S3 前缀无效");
        }
        Ok(())
    }
    pub fn preserve_credentials(&mut self, old: &Self) {
        if !self.webdav.clear_credentials && self.webdav.password.is_empty() {
            self.webdav.password = old.webdav.password.clone();
        }
        if self.s3.clear_credentials {
            self.s3.secret_key.clear(); self.s3.session_token.clear();
        } else {
            if self.s3.secret_key.is_empty() { self.s3.secret_key = old.s3.secret_key.clone(); }
            if self.s3.session_token.is_empty() { self.s3.session_token = old.s3.session_token.clone(); }
        }
        if self.webdav.clear_credentials { self.webdav.password.clear(); }
        self.webdav.clear_credentials = false; self.s3.clear_credentials = false;
    }
    pub fn public(&self) -> Value {
        let mut value = serde_json::to_value(self).unwrap();
        value["webdav"]["has_password"] = json!(!self.webdav.password.is_empty());
        value["webdav"]["password"] = json!("");
        value["s3"]["has_secret_key"] = json!(!self.s3.secret_key.is_empty());
        value["s3"]["has_session_token"] = json!(!self.s3.session_token.is_empty());
        value["s3"]["secret_key"] = json!("");
        value["s3"]["session_token"] = json!("");
        value
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct State {
    next_at: Option<i64>,
    interval_minutes: u64,
    last_attempt: Option<String>,
    last_result: Value,
    remote: Vec<RemoteFile>,
}
#[derive(Clone, Serialize, Deserialize)]
struct RemoteFile { target: String, destination: String, key: String, id: String, automatic: bool,
    #[serde(default)] created_at: String }
pub struct Backups {
    state: Mutex<State>,
    operation: Mutex<()>,
    client: Client,
}
struct TemporaryArchive(PathBuf);
impl Drop for TemporaryArchive {
    fn drop(&mut self) { let _ = fs::remove_file(&self.0); }
}
impl Backups {
    pub fn new(data: &Path) -> Result<Self> {
        let state = match fs::read(data.join("backup-state.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("备份调度状态损坏，请检查 backup-state.json")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(e) => return Err(e.into()),
        };
        let client = Client::builder().connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(600)).redirect(reqwest::redirect::Policy::none()).build()?;
        Ok(Self { state: Mutex::new(state), operation: Mutex::new(()), client })
    }
    fn persist(data: &Path, state: &State) -> Result<()> {
        atomic_write(&data.join("backup-state.json"), &serde_json::to_vec_pretty(state)?)
    }
    pub async fn status(&self) -> Value {
        let state = self.state.lock().await;
        json!({"running": self.operation.try_lock().is_err(), "next_at": state.next_at,
            "last_attempt": state.last_attempt, "last_result": state.last_result})
    }
    pub async fn changed(&self, data: &Path, config: &BackupConfig) -> Result<()> {
        let mut state = self.state.lock().await;
        if !config.enabled { state.next_at = None; }
        else if state.next_at.is_none() || state.interval_minutes != config.interval_minutes {
            state.next_at = Some(chrono::Utc::now().timestamp() + config.interval_minutes as i64 * 60);
        }
        state.interval_minutes = config.interval_minutes;
        Self::persist(data, &state)
    }
    pub async fn run(self_app: Arc<App>) {
        let mut stop = self_app.shutdown.subscribe();
        let initial = self_app.config.read().await.backup.clone();
        if let Err(e) = self_app.backups.changed(&self_app.paths.data, &initial).await {
            self_app.emit("error", format!("备份调度启动失败：{e}")); return;
        }
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! { _ = stop.changed() => break, _ = tick.tick() => {} }
            if *stop.borrow() { break; }
            let cfg = self_app.config.read().await.backup.clone();
            let due = cfg.enabled && self_app.backups.state.lock().await.next_at
                .is_some_and(|at| at <= chrono::Utc::now().timestamp());
            let idle = self_app.backups.operation.try_lock().is_ok();
            if due && idle {
                let result = self_app.backups.create(&self_app, true, None).await;
                if let Err(e) = result { self_app.emit("error", format!("自动备份失败：{e}")); }
            }
        }
    }
    pub async fn create(&self, app: &Arc<App>, automatic: bool, contents: Option<Contents>) -> Result<Value> {
        let _operation = self.operation.try_lock().context("备份正在进行，请稍后重试")?;
        let cfg = app.config.read().await.backup.clone();
        let selected = contents.unwrap_or_else(|| cfg.contents.clone());
        selected.validate()?;
        {
            let mut state = self.state.lock().await;
            state.last_attempt = Some(chrono::Utc::now().to_rfc3339());
            if automatic { state.next_at = Some(chrono::Utc::now().timestamp() + cfg.interval_minutes as i64 * 60); }
            Self::persist(&app.paths.data, &state)?;
        }
        let result = self.create_inner(app, &cfg, &selected, automatic).await;
        {
            let mut state = self.state.lock().await;
            state.last_result = match &result { Ok(value) => value.clone(), Err(e) => json!({"status":"failed", "message": e.to_string()}) };
            // Configuration may have changed during a long upload; do not override its schedule.
            Self::persist(&app.paths.data, &state)?;
        }
        result
    }
    async fn create_inner(&self, app: &Arc<App>, cfg: &BackupConfig, selected: &Contents, automatic: bool) -> Result<Value> {
        let id = {
            let gate = tokio::time::timeout(Duration::from_secs(35), app.maintenance.clone().write_owned()).await
                .context("无法暂停写入，已取消备份")?;
            let store = app.store.clone(); let data = app.paths.data.clone(); let selected = selected.clone();
            // The blocking operation owns the gate even if the caller is cancelled.
            let id = tokio::task::spawn_blocking(move || { let _gate = gate; store.backup_selected(&data, &selected, automatic) }).await??;
            id
        };
        app.emit("backup", format!("本地备份完成：{id}"));
        let directory = app.paths.data.join("backups").join(&id);
        let created_at = chrono::Utc::now().to_rfc3339();
        let mut uploads = vec![];
        if cfg.webdav.enabled || cfg.s3.enabled {
            let archive = directory.with_extension("zip");
            let _archive_cleanup = TemporaryArchive(archive.clone());
            let src = directory.clone(); let dest = archive.clone();
            let packed = tokio::task::spawn_blocking(move || pack(&src, &dest)).await?;
            if let Err(e) = packed {
                uploads.push(json!({"target":"archive","status":"failed","message":e.to_string()}));
            } else {
                for target in ["webdav", "s3"] {
                    if (target == "webdav" && !cfg.webdav.enabled) || (target == "s3" && !cfg.s3.enabled) { continue; }
                    let mut stop = app.shutdown.subscribe();
                    let uploaded = if *stop.borrow() { Err(anyhow::anyhow!("后台正在停止，上传已取消")) }
                        else { tokio::select! {
                            value = self.upload(target, cfg, &id, &archive) => value,
                            _ = stop.changed() => Err(anyhow::anyhow!("后台正在停止，上传已取消")),
                        }};
                    match uploaded {
                        Ok(key) => {
                            let mut state = self.state.lock().await;
                            state.remote.push(RemoteFile { target: target.into(), destination: destination(target, cfg), key, id: id.clone(), automatic, created_at: created_at.clone() });
                            Self::persist(&app.paths.data, &state)?;
                            uploads.push(json!({"target":target,"status":"success"}));
                        }
                        Err(e) => {
                            app.emit("error", format!("{target} 上传失败，本地备份已保留：{e}"));
                            uploads.push(json!({"target":target,"status":"failed","message":e.to_string()}));
                        }
                    }
                }
            }
        }
        let mut warnings = vec![];
        if !*app.shutdown.borrow() {
            let mut stop = app.shutdown.subscribe();
            let cleanup = tokio::select! {
                value = self.prune_remote(&app.paths.data, cfg) => value,
                _ = stop.changed() => Err(anyhow::anyhow!("后台正在停止，远端清理已取消")),
            };
            if let Err(e) = cleanup { warnings.push(e.to_string()); }
        }
        let data = app.paths.data.clone(); let keep = cfg.keep;
        if let Err(e) = tokio::task::spawn_blocking(move || prune_local(&data, keep)).await? { warnings.push(e.to_string()); }
        let result = json!({"id":id,"status":if uploads.iter().any(|v|v["status"]=="failed") {"partial"} else {"success"},"uploads":uploads,"warnings":warnings});
        Ok(result)
    }
    async fn upload(&self, target: &str, cfg: &BackupConfig, id: &str, path: &Path) -> Result<String> {
        let length = fs::metadata(path)?.len();
        ensure!(length <= 5 * 1024 * 1024 * 1024, "备份包超过单次上传的 5 GiB 上限");
        let file = tokio::fs::File::open(path).await?;
        let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file));
        if target == "webdav" {
            let mut base = endpoint(&cfg.webdav.url)?;
            if !base.path().ends_with('/') { base.set_path(&format!("{}/", base.path())); }
            // RFC 4918 does not let MKCOL create missing intermediate collections.
            let path = base.path().to_owned();
            for (index, _) in path.match_indices('/').filter(|(i, _)| *i > 0) {
                let mut collection = base.clone(); collection.set_path(&path[..=index]);
                let response = self.client.request(Method::from_bytes(b"MKCOL")?, collection)
                    .basic_auth(&cfg.webdav.username, Some(&cfg.webdav.password)).send().await
                    .map_err(|_| anyhow::anyhow!("WebDAV 连接失败"))?;
                ensure!([201, 405].contains(&response.status().as_u16()), "WebDAV 创建目录失败（HTTP {}）", response.status().as_u16());
            }
            let url = base.join(&format!("{id}.zip"))?;
            let response = self.client.put(url).basic_auth(&cfg.webdav.username, Some(&cfg.webdav.password))
                .header("content-type", "application/zip").header("content-length", length).body(body).send().await
                .map_err(|_| anyhow::anyhow!("WebDAV 上传连接失败或超时"))?;
            ensure!([200, 201, 204].contains(&response.status().as_u16()), "WebDAV 上传失败（HTTP {}）", response.status().as_u16());
            Ok(format!("{id}.zip"))
        } else {
            let key = format!("{}{id}.zip", if cfg.s3.prefix.is_empty() { String::new() } else { format!("{}/", cfg.s3.prefix.trim_matches('/')) });
            let hash_path = path.to_owned();
            let hash = tokio::task::spawn_blocking(move || file_hash(&hash_path)).await??;
            let request = s3_request(&self.client, Method::PUT, &cfg.s3, &key, &hash, chrono::Utc::now())?;
            let response = request.header("content-type", "application/zip").header("content-length", length).body(body).send().await
                .map_err(|_| anyhow::anyhow!("S3 上传连接失败或超时"))?;
            ensure!(response.status().as_u16() == 200, "S3 上传失败（HTTP {}）", response.status().as_u16());
            Ok(key)
        }
    }
    async fn prune_remote(&self, data: &Path, cfg: &BackupConfig) -> Result<()> {
        for target in ["webdav", "s3"] {
            if (target == "webdav" && !cfg.webdav.enabled) || (target == "s3" && !cfg.s3.enabled) { continue; }
            let fingerprint = destination(target, cfg);
            let mut candidates: Vec<_> = self.state.lock().await.remote.iter()
                .filter(|e| e.automatic && e.target == target && e.destination == fingerprint).cloned().collect();
            candidates.sort_by(|a,b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
            for old in candidates.into_iter().skip(cfg.keep) {
                let request = if target == "webdav" {
                    let mut base = endpoint(&cfg.webdav.url)?;
                    if !base.path().ends_with('/') { base.set_path(&format!("{}/", base.path())); }
                    self.client.delete(base.join(&old.key)?).basic_auth(&cfg.webdav.username, Some(&cfg.webdav.password))
                } else {
                    s3_request(&self.client, Method::DELETE, &cfg.s3, &old.key, &hex::encode(Sha256::digest(b"")), chrono::Utc::now())?
                };
                let response = request.send().await.map_err(|_| anyhow::anyhow!("{target} 清理连接失败"))?;
                ensure!([200,204,404].contains(&response.status().as_u16()), "{target} 清理失败（HTTP {}）", response.status().as_u16());
                let mut state = self.state.lock().await;
                state.remote.retain(|e| !(e.destination == old.destination && e.target == old.target && e.key == old.key));
                Self::persist(data, &state)?;
            }
        }
        Ok(())
    }
}
fn destination(target: &str, cfg: &BackupConfig) -> String {
    let source = if target == "webdav" { format!("{}\n{}", cfg.webdav.url.trim_end_matches('/'), cfg.webdav.username) }
        else { format!("{}\n{}\n{}\n{}", cfg.s3.endpoint, cfg.s3.bucket, cfg.s3.prefix, cfg.s3.path_style) };
    hex::encode(Sha256::digest(source.as_bytes()))
}
pub fn pack(directory: &Path, path: &Path) -> Result<()> {
    validate_backup(directory)?;
    let result = (|| -> Result<()> {
        let file = fs::File::create(path)?;
        let mut zip = zip::ZipWriter::new(file);
        for entry in walkdir::WalkDir::new(directory).follow_links(false) {
            let e = entry?;
            ensure!(!e.file_type().is_symlink(), "备份包拒绝符号链接");
            if e.file_type().is_file() {
                let name = e.path().strip_prefix(directory)?.to_string_lossy().replace('\\', "/");
                zip.start_file(name, zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated))?;
                std::io::copy(&mut fs::File::open(e.path())?, &mut zip)?;
            }
        }
        zip.finish()?.sync_all()?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(path); }
    result
}
fn prune_local(data: &Path, keep: usize) -> Result<()> {
    let base = fs::canonicalize(data.join("backups"))?;
    let mut candidates: Vec<(PathBuf, String)> = vec![];
    for entry in fs::read_dir(&base)? {
        let entry = entry?; let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') || entry.file_type()?.is_symlink() || !entry.file_type()?.is_dir() { continue; }
        if let Ok(manifest) = validate_backup(&path) {
            if manifest["automatic"] == true { candidates.push((path, manifest["created_at"].as_str().unwrap_or("").into())); }
        }
    }
    candidates.sort_by(|a,b| b.1.cmp(&a.1).then_with(|| b.0.file_name().cmp(&a.0.file_name())));
    for (path, _) in candidates.into_iter().skip(keep) {
        ensure!(fs::canonicalize(&path)?.parent() == Some(base.as_path()), "拒绝清理备份目录之外的路径");
        fs::remove_dir_all(path)?;
    }
    Ok(())
}
fn hmac(key: &[u8], text: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts arbitrary key lengths");
    mac.update(text); mac.finalize().into_bytes().to_vec()
}
fn uri_encode(text: &str) -> String {
    let mut output = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) { output.push(b as char); }
        else { write!(&mut output, "%{b:02X}").unwrap(); }
    }
    output
}
fn s3_request(client: &Client, method: Method, cfg: &S3, key: &str, payload: &str, now: chrono::DateTime<chrono::Utc>) -> Result<reqwest::RequestBuilder> {
    let mut base = endpoint(&cfg.endpoint)?;
    let original = base.path().trim_end_matches('/').to_owned();
    if !cfg.path_style {
        let hostname = format!("{}.{}", cfg.bucket, base.host_str().context("S3 主机无效")?);
        base.set_host(Some(&hostname))?;
    }
    let path = if cfg.path_style { format!("{original}/{}/{}", cfg.bucket, uri_encode(key)) }
        else { format!("{original}/{}", uri_encode(key)) };
    let url = Url::parse(&format!("{}{}", &base[..url::Position::BeforePath], path))?;
    let host = url[url::Position::BeforeHost..url::Position::AfterPort].to_owned();
    let date = now.format("%Y%m%d").to_string();
    let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
    let mut canonical = format!("host:{host}\nx-amz-content-sha256:{payload}\nx-amz-date:{timestamp}\n");
    let mut signed = "host;x-amz-content-sha256;x-amz-date".to_string();
    if !cfg.session_token.is_empty() {
        canonical.push_str(&format!("x-amz-security-token:{}\n", cfg.session_token.trim()));
        signed.push_str(";x-amz-security-token");
    }
    let canonical_request = format!("{}\n{}\n\n{canonical}\n{signed}\n{payload}", method.as_str(), url.path());
    let scope = format!("{date}/{}/s3/aws4_request", cfg.region);
    let to_sign = format!("AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{}", hex::encode(Sha256::digest(canonical_request.as_bytes())));
    let k_date = hmac(format!("AWS4{}", cfg.secret_key).as_bytes(), date.as_bytes());
    let k_region = hmac(&k_date, cfg.region.as_bytes());
    let k_service = hmac(&k_region, b"s3");
    let k_signing = hmac(&k_service, b"aws4_request");
    let signature = hex::encode(hmac(&k_signing, to_sign.as_bytes()));
    let mut request = client.request(method, url).header("host", host).header("x-amz-date", timestamp)
        .header("x-amz-content-sha256", payload).header("authorization", format!("AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}", cfg.access_key));
    if !cfg.session_token.is_empty() { request = request.header("x-amz-security-token", &cfg.session_token); }
    Ok(request)
}
