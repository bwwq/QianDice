use crate::portable::{SCHEMA_VERSION, file_hash};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::{fs, path::Path, sync::Mutex};

pub struct Store {
    conn: Mutex<Connection>,
}
struct BackupStage(std::path::PathBuf);
impl Drop for BackupStage {
    fn drop(&mut self) { if self.0.exists() { let _ = fs::remove_dir_all(&self.0); } }
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            bail!(
                "数据版本 {version} 高于本程序支持版本 {SCHEMA_VERSION}，已拒绝写入；请使用新版或恢复备份"
            )
        }
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        if version == 0 {
            let tx = conn.transaction()?;
            tx.execute_batch("CREATE TABLE IF NOT EXISTS kv(namespace TEXT NOT NULL,key TEXT NOT NULL,value TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(namespace,key)); CREATE TABLE IF NOT EXISTS logs(id INTEGER PRIMARY KEY,scope TEXT NOT NULL,session TEXT NOT NULL,time TEXT NOT NULL,actor TEXT NOT NULL,text TEXT NOT NULL); PRAGMA user_version=1;")?;
            tx.commit()?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
    pub fn get(&self, ns: &str, key: &str) -> Result<(Value, i64)> {
        let c = self.conn.lock().unwrap();
        let row: Option<(String, i64)> = c
            .query_row(
                "SELECT value,revision FROM kv WHERE namespace=?1 AND key=?2",
                params![ns, key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            Some((v, n)) => Ok((serde_json::from_str(&v)?, n)),
            None => Ok((Value::Null, 0)),
        }
    }
    pub fn put(&self, ns: &str, key: &str, value: &Value, expected: i64) -> Result<i64> {
        let mut c = self.conn.lock().unwrap();
        let tx = c.transaction()?;
        let current: i64 = tx
            .query_row(
                "SELECT revision FROM kv WHERE namespace=?1 AND key=?2",
                params![ns, key],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        if current != expected {
            bail!("数据已被其他操作修改，请刷新后重试")
        }
        tx.execute("INSERT INTO kv(namespace,key,value,revision) VALUES(?1,?2,?3,?4) ON CONFLICT(namespace,key) DO UPDATE SET value=excluded.value,revision=excluded.revision",params![ns,key,serde_json::to_string(value)?,current+1])?;
        tx.commit()?;
        Ok(current + 1)
    }
    pub fn list(&self, ns: &str) -> Result<Vec<Value>> {
        let c = self.conn.lock().unwrap();
        let mut q = c.prepare(
            "SELECT key,value,revision FROM kv WHERE namespace=?1 ORDER BY key LIMIT 500",
        )?;
        let rows = q.query_map([ns], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        rows.map(|r| {
            let (k, v, n) = r?;
            Ok(json!({"key":k,"value":serde_json::from_str::<Value>(&v)?,"revision":n}))
        })
        .collect()
    }
    pub fn log(&self, scope: &str, session: &str, actor: &str, text: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO logs(scope,session,time,actor,text) VALUES(?1,?2,?3,?4,?5)",
            params![scope, session, chrono::Utc::now().to_rfc3339(), actor, text],
        )?;
        Ok(())
    }
    pub fn logs(&self, scope: &str, session: &str, after: i64, limit: i64) -> Result<Vec<Value>> {
        let c = self.conn.lock().unwrap();
        let mut q=c.prepare("SELECT id,time,actor,text FROM logs WHERE scope=?1 AND session=?2 AND id>?3 ORDER BY id LIMIT ?4")?;
        let rows=q.query_map(params![scope,session,after,limit.clamp(1,10000)],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"time":r.get::<_,String>(1)?,"actor":r.get::<_,String>(2)?,"text":r.get::<_,String>(3)?})))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn sessions(&self) -> Result<Vec<Value>> {
        let c = self.conn.lock().unwrap();
        let mut q=c.prepare("SELECT scope,session,COUNT(*),MIN(time),MAX(time) FROM logs GROUP BY scope,session ORDER BY MAX(id) DESC LIMIT 500")?;
        let rows=q.query_map([],|r|Ok(json!({"scope":r.get::<_,String>(0)?,"session":r.get::<_,String>(1)?,"count":r.get::<_,i64>(2)?,"started":r.get::<_,String>(3)?,"updated":r.get::<_,String>(4)?})))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    // Caller holds the global maintenance write gate throughout this operation.
    pub fn backup(&self, data: &Path) -> Result<String> {
        self.backup_selected(data, &crate::backup::Contents::default(), false)
    }
    pub fn backup_selected(&self, data: &Path, contents: &crate::backup::Contents, automatic: bool) -> Result<String> {
        contents.validate()?;
        let id = format!(
            "{}-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        let stage = data.join("backups").join(format!(".{id}.partial"));
        fs::create_dir(&stage)?;
        let _cleanup = BackupStage(stage.clone());
        {
            let c = self.conn.lock().unwrap();
            c.backup(
                rusqlite::DatabaseName::Main,
                stage.join("qianbian.sqlite"),
                None,
            )?;
        }
        {
            let mut snapshot = Connection::open(stage.join("qianbian.sqlite"))?;
            let tx = snapshot.transaction()?;
            if !contents.game {
                tx.execute("DELETE FROM kv WHERE namespace='world'", [])?;
                tx.execute("DELETE FROM logs", [])?;
            }
            if !contents.plugins { tx.execute("DELETE FROM kv WHERE namespace<>'world'", [])?; }
            tx.commit()?;
        }
        for dir in contents.directories() {
            let source = data.join(dir);
            for entry in walkdir::WalkDir::new(&source).follow_links(false) {
                let e = entry?;
                if e.file_type().is_symlink() {
                    bail!("备份拒绝符号链接：{}", e.path().display())
                }
                let rel = e.path().strip_prefix(data)?;
                let dest = stage.join(rel);
                if e.file_type().is_dir() {
                    fs::create_dir_all(dest)?;
                } else {
                    fs::copy(e.path(), dest)?;
                }
            }
        }
        let mut files = serde_json::Map::new();
        for e in walkdir::WalkDir::new(&stage) {
            let e = e?;
            if e.file_type().is_file() {
                files.insert(
                    e.path()
                        .strip_prefix(&stage)?
                        .to_string_lossy()
                        .replace('\\', "/"),
                    json!(file_hash(e.path())?),
                );
            }
        }
        fs::write(
            stage.join("manifest.json"),
            serde_json::to_vec_pretty(
                &json!({"application":env!("CARGO_PKG_VERSION"),"schema":if *contents == crate::backup::Contents::default() { 1 } else { 2 },"database_schema":SCHEMA_VERSION,"files":files,"contents":contents,"automatic":automatic,"created_at":chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos,true)}),
            )?,
        )?;
        fs::rename(stage, data.join("backups").join(&id))?;
        Ok(id)
    }
}
pub fn validate_backup(path: &Path) -> Result<Value> {
    let m: Value = serde_json::from_slice(&fs::read(path.join("manifest.json"))?)?;
    if !matches!(m["schema"].as_i64(), Some(1 | 2)) || m["database_schema"].as_i64().unwrap_or(1) != SCHEMA_VERSION {
        bail!("备份数据版本不兼容")
    }
    let contents: crate::backup::Contents = if m["schema"] == 2 {
        serde_json::from_value(m["contents"].clone()).context("自定义备份缺少内容清单")?
    } else { crate::backup::Contents::default() };
    contents.validate()?;
    let files = m["files"].as_object().context("备份缺少校验清单")?;
    if !files.contains_key("qianbian.sqlite") {
        bail!("备份缺少数据库")
    }
    for (name, hash) in files {
        let rel = Path::new(name);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            bail!("非法备份路径")
        }
        let top = rel
            .components()
            .next()
            .context("备份路径为空")?
            .as_os_str()
            .to_string_lossy();
        if ![
            "qianbian.sqlite",
            "config",
            "plugins",
            "rules",
            "decks",
            "logs",
        ]
        .contains(&top.as_ref())
        {
            bail!("备份包含不支持的根目录：{top}")
        }
        if top != "qianbian.sqlite" && !contents.directories().contains(&top.as_ref()) {
            bail!("备份文件与内容清单不一致：{top}")
        }
        let p = path.join(rel);
        let base = std::fs::canonicalize(path)?;
        if !std::fs::canonicalize(&p)?.starts_with(&base)
            || std::fs::symlink_metadata(&p)?.file_type().is_symlink()
        {
            bail!("备份包含目录外文件")
        };
        if file_hash(&p)? != hash.as_str().unwrap_or("") {
            bail!("备份校验失败：{name}")
        }
    }
    Ok(m)
}
/// Build a complete staged database, replacing only namespaces selected by the snapshot.
/// This runs offline, after the instance lock and a full recovery-point backup.
pub fn prepare_restore(data: &Path, source: &Path, manifest: &Value) -> Result<()> {
    let stage = data.join("restore-staging");
    if stage.exists() { bail!("发现上次未完成的恢复目录，请先核对目录") }
    fs::create_dir(&stage)?;
    let result = (|| -> Result<()> {
        let contents: crate::backup::Contents = if manifest["schema"] == 2 {
            serde_json::from_value(manifest["contents"].clone())?
        } else { Default::default() };
        for (name, _) in manifest["files"].as_object().context("备份清单无效")? {
            if name == "qianbian.sqlite" { continue; }
            let target = stage.join(name);
            fs::create_dir_all(target.parent().unwrap())?;
            fs::copy(source.join(name), target)?;
        }
        for name in contents.directories() { fs::create_dir_all(stage.join(name))?; }
        if contents.game && contents.plugins {
            fs::copy(source.join("qianbian.sqlite"), stage.join("qianbian.sqlite"))?;
        } else {
            let current = Connection::open(data.join("qianbian.sqlite"))?;
            current.backup(rusqlite::DatabaseName::Main, stage.join("qianbian.sqlite"), None)?;
            drop(current);
            let mut next = Connection::open(stage.join("qianbian.sqlite"))?;
            next.execute("ATTACH DATABASE ?1 AS incoming", [source.join("qianbian.sqlite").to_string_lossy().as_ref()])?;
            let tx = next.transaction()?;
            if contents.game {
                tx.execute("DELETE FROM kv WHERE namespace='world'", [])?;
                tx.execute("INSERT INTO kv SELECT * FROM incoming.kv WHERE namespace='world'", [])?;
                tx.execute("DELETE FROM logs", [])?;
                tx.execute("INSERT INTO logs SELECT * FROM incoming.logs", [])?;
            }
            if contents.plugins {
                tx.execute("DELETE FROM kv WHERE namespace<>'world'", [])?;
                tx.execute("INSERT INTO kv SELECT * FROM incoming.kv WHERE namespace<>'world'", [])?;
            }
            tx.commit()?;
            next.execute("DETACH DATABASE incoming", [])?;
        }
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_dir_all(&stage); }
    result
}
pub fn recover_restore(data: &Path) -> Result<()> {
    let journal = data.join("restore-journal.json");
    if !journal.exists() {
        return Ok(());
    }
    let value: Value = serde_json::from_slice(&fs::read(&journal)?)?;
    let id = value["id"].as_str().context("恢复日志无效")?;
    if !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        bail!("恢复日志标识无效")
    }
    let stage = data.join("restore-staging");
    let old = data.join("backups").join(format!("restore-previous-{id}"));
    fs::create_dir_all(&old)?;
    for name in ["qianbian.sqlite-wal", "qianbian.sqlite-shm"] {
        let current = data.join(name);
        if current.exists() {
            if old.join(name).exists() {
                fs::remove_file(&current)?;
            } else {
                fs::rename(&current, old.join(name))?;
            }
        }
    }
    for name in [
        "qianbian.sqlite",
        "config",
        "plugins",
        "rules",
        "decks",
        "logs",
    ] {
        let next = stage.join(name);
        let current = data.join(name);
        if next.exists() {
            if current.exists() {
                if old.join(name).exists() {
                    bail!("恢复现场存在冲突：{name}，请保留目录并检查")
                };
                fs::rename(&current, old.join(name))?;
            }
            fs::rename(next, current)?;
        } else if !current.exists() {
            bail!("恢复资源不完整：{name}")
        }
    }
    fs::remove_file(journal)?;
    if stage.exists() {
        fs::remove_dir(stage)?;
    }
    Ok(())
}
