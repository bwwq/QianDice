use crate::{
    game,
    portable::{PLUGIN_API, Paths},
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{Mutex, RwLock, Semaphore},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    #[serde(default = "api")]
    pub api: u32,
    pub entry: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub config_schema: Value,
    #[serde(default)]
    pub events: Vec<String>,
}
fn api() -> u32 {
    PLUGIN_API
}
struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    seq: u64,
    #[cfg(windows)]
    job: Job,
}
impl Worker {
    async fn spawn(manifest: &Manifest, dir: &Path) -> Result<Self> {
        ensure!(manifest.api == PLUGIN_API, "插件接口版本不兼容");
        let mut command = Command::new(if Path::new(&manifest.entry).is_absolute() {
            PathBuf::from(&manifest.entry)
        } else {
            dir.join(&manifest.entry)
        });
        command
            .args(&manifest.args)
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command
            .spawn()
            .with_context(|| format!("无法启动插件 {}，请检查程序或解释器", manifest.id))?;
        #[cfg(windows)]
        let job = Job::attach(child.id().context("插件进程已退出")?)?;
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut stderr = BufReader::new(child.stderr.take().unwrap());
        let name = manifest.id.clone();
        tokio::spawn(async move {
            let mut line = String::new();
            while let Ok(n) = stderr.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                tracing::warn!(plugin=%name,message=%line.trim(),"插件输出");
                line.clear();
            }
        });
        let mut worker = Self {
            child,
            input,
            output,
            seq: 0,
            #[cfg(windows)]
            job,
        };
        let info = worker
            .call(
                "initialize",
                json!({"api":PLUGIN_API,"preflight":true}),
                None,
                None,
            )
            .await?;
        ensure!(
            info["api"].as_u64() == Some(PLUGIN_API as u64),
            "插件握手失败"
        );
        Ok(worker)
    }
    async fn call(
        &mut self,
        method: &str,
        params: Value,
        store: Option<&crate::store::Store>,
        manifest: Option<&Manifest>,
    ) -> Result<Value> {
        let context = params.get("context").cloned().unwrap_or(Value::Null);
        let mut replies = Vec::new();
        self.seq += 1;
        let id = self.seq;
        self.write(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        loop {
            let mut bytes = Vec::new();
            let n = (&mut self.output)
                .take(4 * 1024 * 1024)
                .read_until(b'\n', &mut bytes)
                .await?;
            ensure!(n > 0, "插件进程已退出");
            ensure!(
                n < 4 * 1024 * 1024 && bytes.last() == Some(&b'\n'),
                "插件消息过长"
            );
            let packet: Value =
                serde_json::from_slice(&bytes).context("插件标准输出混入非JSON内容")?;
            if let Some(host_method) = packet["method"].as_str() {
                let value = host_call(
                    host_method,
                    &packet["params"],
                    store,
                    manifest,
                    &context,
                    &mut replies,
                );
                if !packet["id"].is_null() {
                    self.write(&match value{Ok(v)=>json!({"jsonrpc":"2.0","id":packet["id"],"result":v}),Err(e)=>json!({"jsonrpc":"2.0","id":packet["id"],"error":{"code":-32000,"message":e.to_string()}})}).await?;
                }
            } else if packet["id"] == json!(id) {
                if !packet["error"].is_null() {
                    bail!(
                        "PLUGIN_ERROR:{}",
                        packet["error"]["message"]
                            .as_str()
                            .unwrap_or("插件请求失败")
                    )
                }
                let mut result = packet["result"].clone();
                for (private, text) in replies {
                    let key = if private { "private" } else { "public" };
                    let old = result[key].as_str().unwrap_or("");
                    result[key] = json!(if old.is_empty() {
                        text
                    } else {
                        format!("{old}\n{text}")
                    });
                }
                return Ok(result);
            }
        }
    }
    async fn write(&mut self, value: &Value) -> Result<()> {
        let mut b = serde_json::to_vec(value)?;
        b.push(b'\n');
        self.input.write_all(&b).await?;
        self.input.flush().await?;
        Ok(())
    }
    async fn stop(&mut self) {
        #[cfg(unix)]
        if let Some(id) = self.child.id() {
            unsafe {
                libc::kill(-(id as i32), libc::SIGTERM);
            }
        }
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}
fn host_call(
    method: &str,
    params: &Value,
    store: Option<&crate::store::Store>,
    m: Option<&Manifest>,
    context: &Value,
    replies: &mut Vec<(bool, String)>,
) -> Result<Value> {
    let m = m.context("预检阶段不能调用宿主能力")?;
    match method {
        "dice.roll" => {
            ensure!(m.capabilities.iter().any(|v| v == "dice"), "缺少dice能力");
            Ok(serde_json::to_value(crate::dice::roll(
                params["expression"].as_str().unwrap_or("1d100"),
                100,
            )?)?)
        }
        "storage.get" | "storage.put" => {
            ensure!(
                m.capabilities.iter().any(|v| v == "storage"),
                "缺少storage能力"
            );
            let s = store.context("存储不可用")?;
            let key = params["key"].as_str().context("缺少key")?;
            ensure!(key.len() < 512, "key过长");
            let ns = format!("plugin:{}", m.id);
            if method == "storage.get" {
                let (v, r) = s.get(&ns, key)?;
                Ok(json!({"value":v,"revision":r}))
            } else {
                Ok(
                    json!({"revision":s.put(&ns,key,&params["value"],params["revision"].as_i64().context("缺少revision")?)?}),
                )
            }
        }
        "config.get" => {
            let s = store.context("存储不可用")?;
            let (v, r) = s.get("plugin-config", &m.id)?;
            Ok(json!({"value":v,"revision":r}))
        }
        "message.reply" => {
            ensure!(m.capabilities.iter().any(|v| v == "reply"), "缺少reply能力");
            ensure!(!context.is_null(), "此请求没有会话上下文");
            let text = params["text"].as_str().context("缺少回复文本")?;
            ensure!(text.len() <= 12000 && replies.len() < 8, "回复超出限制");
            let private = params["private"].as_bool().unwrap_or(false);
            replies.push((private, text.into()));
            Ok(json!({"queued":true}))
        }
        "schedule.put" | "schedule.cancel" => {
            ensure!(
                m.capabilities.iter().any(|v| v == "schedule"),
                "缺少schedule能力"
            );
            ensure!(!context.is_null(), "此请求没有上下文");
            let id = params["id"].as_str().context("缺少定时任务标识")?;
            ensure!(id.len() <= 128 && !id.is_empty(), "任务标识无效");
            let store = store.context("存储不可用")?;
            let key = format!("{}:{id}", m.id);
            let (_, rev) = store.get("schedule", &key)?;
            let value = if method == "schedule.cancel" {
                json!({"enabled":false,"plugin":m.id})
            } else {
                let delay = params["delay_seconds"]
                    .as_i64()
                    .context("需要delay_seconds")?;
                ensure!((1..=31_536_000).contains(&delay), "延时超出范围");
                let every = params["every_seconds"].as_i64().unwrap_or(0);
                ensure!(
                    every == 0 || (1..=31_536_000).contains(&every),
                    "重复周期超出范围"
                );
                json!({"enabled":true,"state":"pending","plugin":m.id,"id":id,"context":context,"payload":params["payload"],"at":chrono::Utc::now().timestamp()+delay,"every":every})
            };
            store.put("schedule", &key, &value, rev)?;
            Ok(json!({"ok":true}))
        }
        _ => bail!("未知或未授权宿主方法：{method}"),
    }
}
pub struct Slot {
    pub manifest: Manifest,
    pub directory: PathBuf,
    worker: Mutex<Option<Worker>>,
    gate: RwLock<()>,
    queue: Semaphore,
    pub error: std::sync::Mutex<Option<String>>,
}
impl Slot {
    async fn new(manifest: Manifest, directory: PathBuf) -> Result<Self> {
        let worker = tokio::time::timeout(
            Duration::from_secs(10),
            Worker::spawn(&manifest, &directory),
        )
        .await??;
        Ok(Self {
            manifest,
            directory,
            worker: Mutex::new(Some(worker)),
            gate: RwLock::new(()),
            queue: Semaphore::new(256),
            error: std::sync::Mutex::new(None),
        })
    }
    pub async fn call(
        &self,
        method: &str,
        params: Value,
        store: &crate::store::Store,
    ) -> Result<Value> {
        let _admission = self.queue.try_acquire().context("插件忙，请稍后重试")?;
        let _gate = self.gate.read().await;
        let mut worker = self.worker.lock().await;
        let w = worker.as_mut().context("插件已停用")?;
        let result = tokio::time::timeout(
            Duration::from_secs(25),
            w.call(method, params, Some(store), Some(&self.manifest)),
        )
        .await;
        match result {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) if e.to_string().starts_with("PLUGIN_ERROR:") => Err(anyhow::anyhow!(
                "{}",
                e.to_string().trim_start_matches("PLUGIN_ERROR:")
            )),
            Ok(Err(e)) => {
                *self.error.lock().unwrap() = Some(e.to_string());
                w.stop().await;
                *worker = None;
                Err(e)
            }
            Err(_) => {
                w.stop().await;
                *worker = None;
                *self.error.lock().unwrap() = Some("执行超时，已停用；不自动重试".into());
                bail!("插件超时，任务不会自动重放")
            }
        }
    }
    pub async fn stop(&self) -> Result<()> {
        let _gate = tokio::time::timeout(Duration::from_secs(30), self.gate.write())
            .await
            .context("等待任务结束超时")?;
        if let Some(mut w) = self.worker.lock().await.take() {
            w.stop().await;
        }
        Ok(())
    }
    pub async fn enabled(&self) -> bool {
        self.worker.lock().await.is_some()
    }
}
pub struct Manager {
    slots: RwLock<BTreeMap<String, Arc<Slot>>>,
    pub executable: PathBuf,
    pub paths: Paths,
    operations: Mutex<()>,
}
impl Manager {
    pub fn new(paths: Paths, executable: PathBuf) -> Self {
        Self {
            slots: RwLock::new(BTreeMap::new()),
            paths,
            executable,
            operations: Mutex::new(()),
        }
    }
    pub async fn start_defaults(&self) -> Result<()> {
        for (id, commands) in [
            ("onebot", vec![]),
            (
                "trpg",
                vec![
                    "help", "帮助", "r", "rh", "ra", "rah", "rab", "rap", "rav", "st", "coc",
                    "dnd", "sc", "en", "ri", "init", "adv", "dis", "setcoc", "setrule", "settemp",
                    "bot", "nn",
                ],
            ),
            ("decks", vec!["draw", "drawh", "ti", "li"]),
            ("logger", vec!["log"]),
            ("replies", vec!["reply"]),
        ] {
            let disabled = self
                .paths
                .data
                .join("config")
                .join(format!("disabled-{id}"));
            let m = Manifest {
                id: id.into(),
                version: env!("CARGO_PKG_VERSION").into(),
                api: PLUGIN_API,
                entry: self.executable.to_string_lossy().into(),
                args: vec!["worker".into(), "--kind".into(), id.into()],
                commands: commands.into_iter().map(str::to_string).collect(),
                rules: if id == "trpg" {
                    vec!["coc7".into(), "dnd5e".into()]
                } else {
                    vec![]
                },
                dependencies: vec![],
                capabilities: vec!["dice".into()],
                config_schema: json!({}),
                events: vec![],
            };
            let slot = if disabled.exists() {
                Slot {
                    manifest: m,
                    directory: self.paths.root.clone(),
                    worker: Mutex::new(None),
                    gate: RwLock::new(()),
                    queue: Semaphore::new(256),
                    error: std::sync::Mutex::new(None),
                }
            } else {
                Slot::new(m, self.paths.root.clone()).await?
            };
            self.slots.write().await.insert(id.into(), Arc::new(slot));
        }
        let plugins = self.paths.data.join("plugins");
        for item in std::fs::read_dir(plugins)? {
            let item = item?;
            let active = item.path().join("active.json");
            if active.exists() {
                match std::fs::read(&active)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<String>(&b).ok())
                {
                    Some(version) => {
                        let manifest = item.path().join(version).join("plugin.json");
                        if let Err(e) = self.load(&manifest).await {
                            tracing::error!("插件加载失败 {}: {e}", manifest.display());
                        }
                    }
                    None => tracing::warn!("无效的插件版本记录"),
                }
            }
        }
        Ok(())
    }
    pub async fn load(&self, path: &Path) -> Result<()> {
        let _op = self.operations.lock().await;
        let manifest: Manifest = serde_json::from_slice(&std::fs::read(path)?)?;
        ensure!(
            !manifest.id.is_empty()
                && manifest
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "插件标识无效"
        );
        ensure!(
            manifest
                .version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-'),
            "插件版本无效"
        );
        let expected = self
            .paths
            .data
            .join("plugins")
            .join(&manifest.id)
            .join(&manifest.version)
            .join("plugin.json");
        ensure!(
            std::fs::canonicalize(path)? == std::fs::canonicalize(expected)?,
            "插件目录须为 标识/版本/plugin.json"
        );
        {
            let slots = self.slots.read().await;
            for dep in &manifest.dependencies {
                ensure!(slots.contains_key(dep), "缺少依赖 {dep}");
            }
            for (id, s) in slots.iter() {
                if id != &manifest.id {
                    for command in &manifest.commands {
                        ensure!(
                            !s.manifest.commands.contains(command),
                            "指令 {command} 已由 {id} 注册"
                        );
                    }
                }
            }
        }
        let slot = Arc::new(
            Slot::new(
                manifest.clone(),
                path.parent().context("插件目录无效")?.into(),
            )
            .await?,
        );
        let mut slots =
            match tokio::time::timeout(Duration::from_secs(30), self.slots.write()).await {
                Ok(v) => v,
                Err(_) => {
                    let _ = slot.stop().await;
                    bail!("等待旧请求结束超时，保留旧版本")
                }
            };
        if let Some(old) = slots.get(&manifest.id) {
            if let Err(e) = old.stop().await {
                let _ = slot.stop().await;
                return Err(e);
            }
        }
        if self
            .paths
            .data
            .join("config")
            .join(format!("disabled-{}", manifest.id))
            .exists()
        {
            slot.stop().await?;
        }
        slots.insert(manifest.id.clone(), slot);
        let active = self.paths.data.join("plugins").join(&manifest.id);
        std::fs::create_dir_all(&active)?;
        crate::portable::atomic_write(
            &active.join("active.json"),
            &serde_json::to_vec(&manifest.version)?,
        )?;
        Ok(())
    }
    pub async fn call_command(
        &self,
        request: game::CommandRequest,
        store: &crate::store::Store,
    ) -> Result<game::CommandResult> {
        let command = request.command.split_whitespace().next().unwrap_or("");
        let slots = self.slots.read().await;
        let room = request.context.room();
        let player = request.world.players.get(&request.context.user);
        let selected = player
            .and_then(|p| p.cards.get(p.bindings.get(&room).unwrap_or(&p.current)))
            .map(|c| c.rule.as_str())
            .or_else(|| request.world.rooms.get(&room).map(|r| r.rule.as_str()))
            .unwrap_or("coc7");
        let rule_plugin = if command.starts_with("ra") && command != "rav" {
            slots
                .values()
                .find(|s| s.manifest.id != "trpg" && s.manifest.rules.iter().any(|r| r == selected))
        } else {
            None
        };
        let slot = rule_plugin
            .or_else(|| {
                slots.values().find(|s| {
                    s.manifest.commands.iter().any(|v| {
                        command == v || ((v == "rab" || v == "rap") && command.starts_with(v))
                    })
                })
            })
            .or_else(|| slots.get("trpg"))
            .cloned()
            .context("没有可处理此指令的插件")?;
        let original = request.world.clone();
        let result = slot
            .call("command", serde_json::to_value(request)?, store)
            .await?;
        if result.get("world").is_some() {
            Ok(serde_json::from_value(result)?)
        } else {
            Ok(game::CommandResult {
                public: result["public"].as_str().unwrap_or("").into(),
                private: result["private"].as_str().map(str::to_string),
                world: original,
                export_log: None,
            })
        }
    }
    pub async fn list(&self) -> Vec<Value> {
        let slots = self.slots.read().await;
        let mut list = vec![];
        for slot in slots.values() {
            list.push(json!({"manifest":slot.manifest,"enabled":slot.enabled().await,"error":*slot.error.lock().unwrap()}));
        }
        list
    }
    pub async fn call_named(
        &self,
        id: &str,
        method: &str,
        params: Value,
        store: &crate::store::Store,
    ) -> Result<Value> {
        let slots = self.slots.read().await;
        slots
            .get(id)
            .context("插件未安装")?
            .call(method, params, store)
            .await
    }
    pub async fn active(&self, id: &str) -> bool {
        match self.slots.read().await.get(id) {
            Some(s) => s.enabled().await,
            None => false,
        }
    }
    pub async fn event(
        &self,
        event: &str,
        params: Value,
        store: &crate::store::Store,
    ) -> Vec<Value> {
        let slots = self.slots.read().await;
        let mut result = vec![];
        for s in slots.values() {
            if s.manifest.events.iter().any(|v| v == event) && s.enabled().await {
                match s.call("event", params.clone(), store).await {
                    Ok(v) => result.push(v),
                    Err(e) => tracing::warn!(plugin=%s.manifest.id,"事件处理失败：{e}"),
                }
            }
        }
        result
    }
    pub async fn disable(&self, id: &str) -> Result<()> {
        let _op = self.operations.lock().await;
        let slots = self.slots.read().await;
        for (other, s) in slots.iter() {
            ensure!(
                !s.manifest.dependencies.iter().any(|v| v == id) || !s.enabled().await,
                "{other} 仍依赖此插件"
            );
        }
        let s = slots.get(id).context("插件不存在")?.clone();
        drop(slots);
        s.stop().await?;
        crate::portable::atomic_write(
            &self
                .paths
                .data
                .join("config")
                .join(format!("disabled-{id}")),
            b"disabled",
        )?;
        Ok(())
    }
    pub async fn enable(&self, id: &str) -> Result<()> {
        let _op = self.operations.lock().await;
        let s = self
            .slots
            .read()
            .await
            .get(id)
            .cloned()
            .context("插件不存在，重新加载插件清单")?;
        let fresh = Arc::new(Slot::new(s.manifest.clone(), s.directory.clone()).await?);
        s.stop().await?;
        self.slots.write().await.insert(id.into(), fresh);
        let marker = self
            .paths
            .data
            .join("config")
            .join(format!("disabled-{id}"));
        if marker.exists() {
            std::fs::remove_file(marker)?;
        }
        Ok(())
    }
    pub async fn shutdown(&self) {
        for s in self.slots.read().await.values() {
            let _ = s.stop().await;
        }
    }
}
pub async fn worker_main(_kind: &str) -> Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.len() > 4 * 1024 * 1024 {
            bail!("请求过长")
        };
        let packet: Value = serde_json::from_str(&line)?;
        let result: Result<Value> = match packet["method"].as_str() {
            Some("initialize") => Ok(json!({"api":PLUGIN_API})),
            Some("command") => {
                serde_json::from_value::<game::CommandRequest>(packet["params"].clone())
                    .map_err(Into::into)
                    .and_then(game::execute)
                    .and_then(|v| Ok(serde_json::to_value(v)?))
            }
            Some("adapter.decode") => {
                Ok(json!({"text":crate::onebot::message_text(&packet["params"]["message"])}))
            }
            Some("adapter.encode") => {
                let p = &packet["params"];
                let message = json!([{"type":"text","data":{"text":p["text"]}}]);
                Ok(if p["group"].is_null() {
                    json!({"action":"send_private_msg","params":{"user_id":p["user"],"message":message}})
                } else {
                    json!({"action":"send_group_msg","params":{"group_id":p["group"],"message":message}})
                })
            }
            Some("health") => Ok(json!({"ok":true})),
            _ => Err(anyhow::anyhow!("未知插件方法")),
        };
        let response = match result {
            Ok(value) => json!({"jsonrpc":"2.0","id":packet["id"],"result":value}),
            Err(e) => {
                json!({"jsonrpc":"2.0","id":packet["id"],"error":{"code":-32000,"message":e.to_string()}})
            }
        };
        let mut bytes = serde_json::to_vec(&response)?;
        bytes.push(b'\n');
        out.write_all(&bytes).await?;
        out.flush().await?;
    }
    Ok(())
}
#[cfg(windows)]
struct Job(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for Job {}
#[cfg(windows)]
impl Job {
    fn attach(pid: u32) -> Result<Self> {
        use windows_sys::Win32::{
            Foundation::*,
            System::{JobObjects::*, Threading::*},
        };
        unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            ensure!(!h.is_null(), "创建Job失败");
            let job = Self(h);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            ensure!(
                SetInformationJobObject(
                    h,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of_val(&info) as u32
                ) != 0,
                "设置Job失败"
            );
            let p = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false as i32, pid);
            ensure!(!p.is_null(), "无法打开插件进程");
            let ok = AssignProcessToJobObject(h, p);
            CloseHandle(p);
            ensure!(ok != 0, "无法管理插件进程树");
            Ok(job)
        }
    }
}
#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
