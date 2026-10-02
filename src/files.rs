//! Group-file upload extension point. No hosting service or public URL is assumed.
use crate::server::App;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{io::Write, path::PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupFileRequest {
    pub account: String,
    pub group: String,
    pub path: PathBuf,
    pub name: String,
    pub content_type: String,
}
struct TemporaryFile(PathBuf);
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub async fn send_log(
    app: &App,
    account: &str,
    group: &str,
    scope: &str,
    session: &str,
) -> Result<bool> {
    let _gate = app.maintenance.read().await;
    let Some(plugin) = app.plugins.uploader().await else {
        return Ok(false);
    };
    let directory = app.paths.runtime.join("transfers");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{}.txt", uuid::Uuid::new_v4()));
    let _cleanup = TemporaryFile(path.clone());
    let mut file = std::fs::File::create(&path)?;
    let mut after = 0;
    let mut bytes = 0usize;
    loop {
        let rows = app.store.logs(scope, session, after, 500)?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            after = row["id"].as_i64().context("日志编号无效")?;
            let line = format!(
                "[{}] {}: {}\n",
                row["time"].as_str().unwrap_or(""),
                row["actor"].as_str().unwrap_or(""),
                row["text"].as_str().unwrap_or("")
            );
            bytes += line.len();
            ensure!(bytes <= 64 * 1024 * 1024, "团录过大，请在管理端导出");
            file.write_all(line.as_bytes())?;
        }
    }
    ensure!(bytes > 0, "当前群没有这份日志");
    file.sync_all()?;
    drop(file);
    let name = session
        .chars()
        .filter(|c| !c.is_control() && !"/\\:*?\"<>|".contains(*c))
        .take(100)
        .collect::<String>();
    let request = GroupFileRequest {
        account: account.into(),
        group: group.into(),
        path: path.clone(),
        name: format!("{name}.txt"),
        content_type: "text/plain; charset=utf-8".into(),
    };
    let result = app
        .plugins
        .call_named(
            &plugin,
            "file.upload",
            json!({"event":"file.upload","file":request}),
            &app.store,
        )
        .await;
    let response = result?;
    ensure!(
        response["confirmed"] == true,
        "群文件发送未确认，不自动重发"
    );
    Ok(true)
}
