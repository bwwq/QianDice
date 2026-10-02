use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const API_VERSION: u32 = 1;
pub const SCHEMA_VERSION: i64 = 1;
pub const PLUGIN_API: u32 = 1;

#[derive(Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub data: PathBuf,
    pub runtime: PathBuf,
}
impl Paths {
    pub fn discover(data: Option<PathBuf>, root: Option<PathBuf>) -> Result<Self> {
        let root = root.unwrap_or(
            std::env::current_exe()?
                .parent()
                .context("程序目录不可用")?
                .to_path_buf(),
        );
        let root = fs::canonicalize(root)?;
        let data = data.unwrap_or_else(|| root.join("data"));
        fs::create_dir_all(&data).context("数据目录不可写，请移动程序或使用 --data-dir")?;
        let data = fs::canonicalize(data)?;
        for dir in ["config", "plugins", "rules", "decks", "logs", "backups"] {
            fs::create_dir_all(data.join(dir))?;
        }
        let runtime = root.join("runtime");
        fs::create_dir_all(&runtime)?;
        Ok(Self {
            root,
            data,
            runtime,
        })
    }
    pub fn lock(&self) -> Result<File> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.data.join("instance.lock"))?;
        file.try_lock_exclusive()
            .context("这份数据已有后台使用；请连接已有实例，或停止旧版本后重试")?;
        Ok(file)
    }
    pub fn retain_release(&self) -> Result<PathBuf> {
        let exe = std::env::current_exe()?;
        let hash = file_hash(&exe)?;
        let version = self
            .runtime
            .join(format!("{}-{}", env!("CARGO_PKG_VERSION"), &hash[..12]));
        fs::create_dir_all(&version)?;
        let dest = version.join(if cfg!(windows) {
            "qianbian.exe"
        } else {
            "qianbian"
        });
        if !dest.exists() {
            atomic_copy(&exe, &dest)?;
        }
        if file_hash(&dest)? != hash {
            bail!("保留的发布文件校验失败");
        }
        Ok(dest)
    }
}
pub fn file_hash(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    // Windows rename does not replace an existing file; retain a recoverable previous file.
    let old = path.with_extension("previous");
    if path.exists() {
        if old.exists() {
            fs::remove_file(&old)?;
        }
        fs::rename(path, &old)?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        if old.exists() {
            let _ = fs::rename(&old, path);
        }
        return Err(e.into());
    }
    Ok(())
}
pub fn atomic_copy(from: &Path, to: &Path) -> Result<()> {
    let tmp = to.with_extension("partial");
    fs::copy(from, &tmp)?;
    // Windows FlushFileBuffers requires a writable handle.
    OpenOptions::new()
        .write(true)
        .open(&tmp)?
        .sync_all()
        .context("同步保留的发布文件失败")?;
    fs::rename(tmp, to)?;
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub listen: String,
    pub masters: Vec<String>,
    pub prefixes: Vec<String>,
    pub accounts: Vec<Account>,
    pub blocked_users: Vec<String>,
    pub allowed_groups: Vec<String>,
    pub cooldown_ms: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:9610".into(),
            masters: vec![],
            prefixes: vec![".".into(), "。".into()],
            accounts: vec![],
            blocked_users: vec![],
            allowed_groups: vec![],
            cooldown_ms: 500,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub self_id: String,
    pub mode: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub token: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}
fn yes() -> bool {
    true
}
pub fn load_config(paths: &Paths) -> Result<Config> {
    let path = paths.data.join("config/server.json");
    if !path.exists() {
        atomic_write(&path, &serde_json::to_vec_pretty(&Config::default())?)?;
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
