//! Linux/cloud contract tests using real OneBot 11 WebSocket frames.
//! No QQ account, external service or third-party Python package is required.
use anyhow::{Context, Result, ensure};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream as StdStream},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tempfile::TempDir;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, accept_hdr_async, connect_async,
    tungstenite::{Message, client::IntoClientRequest, http::HeaderValue},
};

const ACCESS: &str = "onebot-functional-contract-access";
const USER: i64 = 11001;
const GROUP: i64 = 22001;

struct Http {
    status: u16,
    body: Vec<u8>,
}
impl Http {
    fn json(self) -> Result<Value> {
        ensure!(
            self.status == 200,
            "HTTP {}: {}",
            self.status,
            String::from_utf8_lossy(&self.body)
        );
        Ok(serde_json::from_slice(&self.body)?)
    }
}
struct Backend {
    root: TempDir,
    child: Child,
    address: SocketAddr,
    token: String,
}
impl Backend {
    async fn start(accounts: Value) -> Result<Self> {
        let root = tempfile::Builder::new().prefix("千变 ob功能 ").tempdir()?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        drop(listener);
        fs::create_dir_all(root.path().join("data/config"))?;
        fs::write(
            root.path().join("data/config/server.json"),
            serde_json::to_vec(
                &json!({"listen":address.to_string(),"masters":[USER.to_string()],"prefixes":[".","。"],"accounts":accounts,"blocked_users":[],"allowed_groups":[],"cooldown_ms":0}),
            )?,
        )?;
        let error = fs::File::create(root.path().join("backend.log"))?;
        let child = Command::new(env!("CARGO_BIN_EXE_qianbian"))
            .arg("--root")
            .arg(root.path())
            .current_dir(std::env::temp_dir())
            .stdout(Stdio::null())
            .stderr(error)
            .spawn()?;
        let mut backend = Self {
            root,
            child,
            address,
            token: String::new(),
        };
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(15) {
            if backend.child.try_wait()?.is_some() {
                anyhow::bail!(
                    "backend exited: {}",
                    fs::read_to_string(backend.root.path().join("backend.log"))?
                );
            }
            if let Ok(token) =
                fs::read_to_string(backend.root.path().join("data/config/admin-token.txt"))
            {
                backend.token = token.trim().into();
                if backend.api("/status", "GET", None).is_ok() {
                    return Ok(backend);
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        anyhow::bail!("backend startup timeout")
    }
    fn request(&self, path: &str, method: &str, body: Option<Value>) -> Result<Http> {
        let data = body
            .map(|v| serde_json::to_vec(&v))
            .transpose()?
            .unwrap_or_default();
        let mut stream = StdStream::connect_timeout(&self.address, Duration::from_secs(2))?;
        stream.set_read_timeout(Some(Duration::from_secs(35)))?;
        write!(
            stream,
            "{method} /api/v1{path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nX-Qianbian: 1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.address,
            self.token,
            data.len()
        )?;
        stream.write_all(&data)?;
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes)?;
        let split = bytes
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .context("missing HTTP headers")?;
        let header = std::str::from_utf8(&bytes[..split])?;
        let status = header
            .split_whitespace()
            .nth(1)
            .context("missing status")?
            .parse()?;
        let mut body = bytes[split + 4..].to_vec();
        if header
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            let mut decoded = Vec::new();
            let mut offset = 0;
            loop {
                let end = body[offset..]
                    .windows(2)
                    .position(|w| w == b"\r\n")
                    .context("invalid chunk")?
                    + offset;
                let length = usize::from_str_radix(
                    std::str::from_utf8(&body[offset..end])?
                        .split(';')
                        .next()
                        .unwrap(),
                    16,
                )?;
                if length == 0 {
                    break;
                }
                let start = end + 2;
                decoded.extend_from_slice(&body[start..start + length]);
                offset = start + length + 2;
            }
            body = decoded;
        }
        Ok(Http { status, body })
    }
    fn api(&self, path: &str, method: &str, body: Option<Value>) -> Result<Value> {
        self.request(path, method, body)?.json()
    }
    fn world(&self, account: &str) -> Result<Value> {
        self.api(
            &format!("/world?scope=%5B%22qq%22%2C%22{account}%22%5D"),
            "GET",
            None,
        )
    }
    async fn connected(&self, account: &str) -> Result<()> {
        for _ in 0..80 {
            if self.api("/status", "GET", None)?["connections"]
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x == account)
            {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        anyhow::bail!("account not connected: {account}")
    }
    async fn stop(&mut self) -> Result<()> {
        self.api("/shutdown", "POST", Some(json!({})))?;
        for _ in 0..150 {
            if self.child.try_wait()?.is_some() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        anyhow::bail!("backend did not shut down")
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.request("/shutdown", "POST", Some(json!({})));
            for _ in 0..100 {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
fn account(id: &str, self_id: i64) -> Value {
    json!({"id":id,"self_id":self_id.to_string(),"mode":"reverse","url":"","token":ACCESS,"enabled":true})
}
static NEXT_MESSAGE: AtomicU64 = AtomicU64::new(1);
fn event(
    self_id: i64,
    user: i64,
    group: Option<i64>,
    text: &str,
    array: bool,
    admin: bool,
) -> Value {
    let mut packet = json!({"time":1700000000,"self_id":self_id,"post_type":"message","message_type":if group.is_some(){"group"}else{"private"},"sub_type":if group.is_some(){"normal"}else{"friend"},"message_id":NEXT_MESSAGE.fetch_add(1,Ordering::Relaxed),"user_id":user,"message":if array{json!([{"type":"text","data":{"text":text}}])}else{json!(text)},"raw_message":text,"font":0,"sender":{"user_id":user,"nickname":"调查员","card":"","role":if admin{"admin"}else{"member"}}});
    if let Some(group) = group {
        packet["group_id"] = json!(group);
    }
    packet
}
fn message(action: &Value) -> String {
    action["params"]["message"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["data"]["text"].as_str().unwrap_or(""))
        .collect()
}
struct Peer {
    out: mpsc::Sender<Value>,
    actions: mpsc::Receiver<Value>,
    auto: Arc<AtomicBool>,
    fail_private: Arc<AtomicBool>,
    trace: Arc<std::sync::Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Peer {
    fn from_socket<S>(socket: WebSocketStream<S>) -> Self
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let (mut sink, mut read) = socket.split();
        let (out, mut packets) = mpsc::channel::<Value>(512);
        let (actions_tx, actions) = mpsc::channel(512);
        let auto = Arc::new(AtomicBool::new(true));
        let fail_private = Arc::new(AtomicBool::new(false));
        let ack = auto.clone();
        let fail = fail_private.clone();
        let trace = Arc::new(std::sync::Mutex::new(Vec::new()));
        let frames = trace.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    packet=packets.recv()=>{match packet{Some(p)=>{frames.lock().unwrap().push(json!({"direction":"to_bot","payload":p}));if sink.send(Message::Text(p.to_string())).await.is_err(){break}},None=>{let _=sink.close().await;break}}},
                    packet=read.next()=>{match packet{Some(Ok(Message::Text(text)))=>{if let Ok(p)=serde_json::from_str::<Value>(&text){frames.lock().unwrap().push(json!({"direction":"from_bot","payload":p}));if !p["action"].is_null(){if actions_tx.send(p.clone()).await.is_err(){break}if ack.load(Ordering::SeqCst){let failed=fail.load(Ordering::SeqCst)&&p["action"]=="send_private_msg";let response=json!({"status":if failed{"failed"}else{"ok"},"retcode":if failed{100}else{0},"data":if failed{json!(null)}else{json!({"message_id":42})},"echo":p["echo"]});frames.lock().unwrap().push(json!({"direction":"to_bot","payload":response}));if sink.send(Message::Text(response.to_string())).await.is_err(){break}}}}},Some(Ok(Message::Ping(data)))=>{let _=sink.send(Message::Pong(data)).await;},Some(Ok(Message::Close(_)))|Some(Err(_))|None=>break,_=>{}}}
                }
            }
        });
        Self {
            out,
            actions,
            auto,
            fail_private,
            trace,
            task,
        }
    }
    async fn reverse(backend: &Backend, id: &str, self_id: i64) -> Result<Self> {
        let req = reverse_request(backend, id, self_id, ACCESS, "Universal")?;
        let (socket, _) = connect_async(req).await?;
        Ok(Self::from_socket(socket))
    }
    fn save_trace(&self, name: &str) -> Result<()> {
        if let Ok(directory) = std::env::var("QIANBIAN_TEST_REPORT_DIR") {
            let folder = Path::new(&directory).join("transcripts");
            fs::create_dir_all(&folder)?;
            fs::write(
                folder.join(format!("{name}.json")),
                serde_json::to_vec_pretty(&*self.trace.lock().unwrap())?,
            )?;
        }
        Ok(())
    }
    async fn send(&self, packet: Value) -> Result<()> {
        self.out.send(packet).await?;
        Ok(())
    }
    async fn action(&mut self) -> Result<Value> {
        let p = tokio::time::timeout(Duration::from_secs(20), self.actions.recv())
            .await?
            .context("peer disconnected")?;
        ensure!(p["echo"].is_string(), "missing echo");
        let key = if p["action"] == "send_group_msg" {
            "group_id"
        } else {
            "user_id"
        };
        ensure!(
            p["params"][key].is_i64(),
            "OneBot target ID must be a JSON number: {p}"
        );
        ensure!(
            p["params"]["message"].is_array(),
            "message must be segments"
        );
        Ok(p)
    }
    async fn command(
        &mut self,
        self_id: i64,
        user: i64,
        group: Option<i64>,
        text: &str,
    ) -> Result<Value> {
        self.send(event(self_id, user, group, text, true, user == USER))
            .await?;
        self.action().await
    }
    async fn quiet(&mut self) -> Result<()> {
        ensure!(
            tokio::time::timeout(Duration::from_millis(220), self.actions.recv())
                .await
                .is_err(),
            "unexpected reply"
        );
        Ok(())
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
fn reverse_request(
    backend: &Backend,
    id: &str,
    self_id: i64,
    token: &str,
    role: &str,
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>> {
    let mut req = format!("ws://{}/onebot/{id}", backend.address).into_client_request()?;
    req.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {token}"))?,
    );
    req.headers_mut()
        .insert("X-Self-ID", HeaderValue::from_str(&self_id.to_string())?);
    req.headers_mut()
        .insert("X-Client-Role", HeaderValue::from_str(role)?);
    Ok(req)
}
fn record(id: &str, features: &[&str]) -> Result<()> {
    if let Ok(directory) = std::env::var("QIANBIAN_TEST_REPORT_DIR") {
        fs::create_dir_all(&directory)?;
        fs::write(
            Path::new(&directory).join(format!("{id}.json")),
            serde_json::to_vec_pretty(
                &json!({"id":id,"status":"passed","features":features,"transport":"real loopback WebSocket; OneBot11 specification peer; no QQ network"}),
            )?,
        )?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn onebot_reverse_cards_coc_dnd() -> Result<()> {
    let mut backend = Backend::start(json!([account("a", 33001)])).await?;
    for (token, self_id, role, status) in [
        ("wrong", 33001, "Universal", 401),
        (ACCESS, 33002, "Universal", 403),
        (ACCESS, 33001, "API", 400),
    ] {
        let result = connect_async(reverse_request(&backend, "a", self_id, token, role)?).await;
        match result {
            Err(tokio_tungstenite::tungstenite::Error::Http(response)) => ensure!(
                response.status().as_u16() == status,
                "handshake rejection mismatch"
            ),
            _ => anyhow::bail!("invalid handshake accepted"),
        }
    }
    let mut peer = Peer::reverse(&backend, "a", 33001).await?;
    backend.connected("a").await?;
    peer.send(json!({"time":1700000000,"self_id":33001,"post_type":"meta_event","meta_event_type":"lifecycle","sub_type":"connect"})).await?;
    peer.send(json!({"time":1700000000,"self_id":33001,"post_type":"meta_event","meta_event_type":"heartbeat","status":{"online":true,"good":true},"interval":5000})).await?;
    peer.quiet().await?;
    let r = peer
        .command(33001, USER, Some(GROUP), "。r 3#(1d1+2)*2 原因")
        .await?;
    ensure!(
        r["params"]["group_id"] == GROUP
            && message(&r).matches("= 6").count() == 3
            && message(&r).contains("原因"),
        "dice/reason/repetition"
    );
    peer.send(event(
        33001,
        USER,
        None,
        ".r 1d1 &#91;原样&#93; &amp;",
        false,
        true,
    ))
    .await?;
    let r = peer.action().await?;
    ensure!(
        r["params"]["user_id"] == USER && message(&r).contains("[原样] &"),
        "CQ text decoding/private target"
    );
    for text in [
        ".st new 调查员",
        ".st 力量60 理智60 侦查50",
        ".st lock",
        ".st new 法师",
        ".st 力量80 理智80 侦查80",
    ] {
        peer.command(33001, USER, Some(GROUP), text).await?;
    }
    ensure!(
        message(&peer.command(33001, USER, Some(GROUP), ".st 力量").await?).ends_with("60"),
        "group binding lost"
    );
    ensure!(
        message(
            &peer
                .command(33001, USER, Some(GROUP + 1), ".st 力量")
                .await?
        )
        .ends_with("80"),
        "other group default card"
    );
    for text in [
        ".st alias 观察=侦查",
        ".ra 观察",
        ".rab1 侦查",
        ".rap1 侦查",
        ".coc",
        ".ti",
        ".li",
    ] {
        ensure!(
            !message(&peer.command(33001, USER, Some(GROUP), text).await?).starts_with("未完成"),
            "CoC operation: {text}"
        );
    }
    peer.command(33001, USER, Some(GROUP), ".sc 1/1 20").await?;
    ensure!(
        backend.world("a")?["value"]["players"][USER.to_string()]["cards"]["调查员"]["attrs"]["理智"]
            == 60,
        "explicit SC mutated card"
    );
    peer.command(33001, USER, Some(GROUP), ".sc 1/1").await?;
    ensure!(
        backend.world("a")?["value"]["players"][USER.to_string()]["cards"]["调查员"]["attrs"]["理智"]
            == 59,
        "SC did not persist"
    );
    peer.command(33001, USER, Some(GROUP), ".en 侦查 1").await?;
    ensure!(
        backend.world("a")?["value"]["players"][USER.to_string()]["cards"]["调查员"]["attrs"]["侦查"]
            == 50,
        "explicit growth mutated card"
    );
    let growth = message(&peer.command(33001, USER, Some(GROUP), ".en 侦查").await?);
    let after: i64 = growth
        .rsplit(" = ")
        .next()
        .context("growth output")?
        .parse()?;
    ensure!(
        backend.world("a")?["value"]["players"][USER.to_string()]["cards"]["调查员"]["attrs"]["侦查"]
            == after,
        "growth did not persist"
    );
    peer.command(33001, USER + 1, Some(GROUP), ".st new 对手")
        .await?;
    peer.command(33001, USER + 1, Some(GROUP), ".st 侦查60")
        .await?;
    peer.command(33001, USER + 1, Some(GROUP), ".st lock")
        .await?;
    let mut oppose = event(33001, USER, Some(GROUP), "", true, true);
    oppose["message"] = json!([{"type":"text","data":{"text":".rav 侦查 "}},{"type":"at","data":{"qq":(USER+1).to_string()}}]);
    peer.send(oppose).await?;
    ensure!(
        !message(&peer.action().await?).starts_with("未完成"),
        "array @ opposition"
    );
    let exported = message(&peer.command(33001, USER, Some(GROUP), ".st export").await?);
    let mut card: Value = serde_json::from_str(&exported)?;
    card["name"] = json!("导入卡");
    peer.command(33001, USER, Some(GROUP), &format!(".st import {card}"))
        .await?;
    ensure!(
        message(&peer.command(33001, USER, Some(GROUP), ".st list").await?).contains("导入卡"),
        "card import/list"
    );
    peer.command(33001, USER, Some(GROUP), ".st unlock").await?;
    peer.command(33001, USER, Some(GROUP), ".st set 导入卡")
        .await?;
    peer.command(33001, USER, Some(GROUP), ".st clear").await?;
    peer.command(33001, USER, Some(GROUP), ".st del 导入卡")
        .await?;
    let dnd = message(&peer.command(33001, USER, Some(GROUP), ".dnd").await?);
    let start = dnd.find('[').context("DND output")?;
    let attrs: Vec<i64> = serde_json::from_str(&dnd[start..])?;
    ensure!(
        attrs.len() == 6 && attrs.iter().all(|v| (3..=18).contains(v)),
        "DND stat generation"
    );
    for cmd in [".adv 5", ".dis -2", ".ri 勇者 5", ".ri 法师 -1"] {
        ensure!(
            !message(&peer.command(33001, USER, Some(GROUP), cmd).await?).starts_with("未完成"),
            "DND operation: {cmd}"
        );
    }
    ensure!(
        message(&peer.command(33001, USER, Some(GROUP), ".init").await?).contains("勇者"),
        "initiative listing"
    );
    peer.command(33001, USER, Some(GROUP), ".init clear")
        .await?;
    peer.command(33001, USER, Some(GROUP), ".setrule dnd5e")
        .await?;
    let default = message(&peer.command(33001, USER, Some(GROUP), ".r").await?);
    ensure!(default.starts_with("1d20"), "DND default die");
    peer.command(33001, USER, Some(GROUP), ".setcoc 1").await?;
    let r = peer.command(33001, USER, Some(GROUP), ".r 1/0").await?;
    ensure!(
        message(&r).starts_with("未完成"),
        "invalid expression not rejected"
    );
    ensure!(
        message(&peer.command(33001, USER, Some(GROUP), ".r 1d1").await?).contains("= 1"),
        "expression error killed worker"
    );
    let duplicate = event(33001, USER, Some(GROUP), ".r 1d1", true, true);
    peer.send(duplicate.clone()).await?;
    peer.action().await?;
    peer.send(duplicate).await?;
    peer.quiet().await?;
    peer.save_trace("rules_cards")?;
    drop(peer);
    backend.stop().await?;
    record(
        "rules_cards",
        &[
            "reverse authentication",
            "numeric targets",
            "CQ and array messages",
            "deduplication",
            "dice arithmetic/repetition/reason/defaults/errors",
            "CoC generation/checks/bonus/penalty/opposition/SAN/growth/madness",
            "cards CRUD/import/export/aliases/binding",
            "DND generation/advantage/disadvantage/initiative",
        ],
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn onebot_decks_logs_permissions_isolation() -> Result<()> {
    let mut backend = Backend::start(json!([account("a", 33001), account("b", 33002)])).await?;
    let mut a = Peer::reverse(&backend, "a", 33001).await?;
    let mut b = Peer::reverse(&backend, "b", 33002).await?;
    backend.api(
        "/content/decks/once.json",
        "PUT",
        Some(json!({"without_replacement":true,"entries":[{"text":"唯一卡片","weight":1}]})),
    )?;
    ensure!(
        message(&a.command(33001, USER, Some(GROUP), ".draw once").await?) == "唯一卡片",
        "draw"
    );
    ensure!(
        message(&a.command(33001, USER, Some(GROUP), ".draw once").await?).contains("抽完"),
        "without replacement exhaustion"
    );
    ensure!(
        message(
            &a.command(33001, USER, Some(GROUP + 1), ".draw once")
                .await?
        ) == "唯一卡片",
        "deck state leaked across groups"
    );
    ensure!(
        message(
            &a.command(33001, USER + 1, Some(GROUP), ".drawreset once")
                .await?
        )
        .contains("管理员"),
        "unprivileged reset"
    );
    a.command(33001, USER, Some(GROUP), ".drawreset once")
        .await?;
    ensure!(
        message(&a.command(33001, USER, Some(GROUP), ".draw once").await?) == "唯一卡片",
        "reset"
    );
    backend.api(
        "/content/decks/once.json",
        "PUT",
        Some(json!({"without_replacement":true,"entries":[{"text":"新版卡片","weight":1}]})),
    )?;
    ensure!(
        message(&a.command(33001, USER, Some(GROUP), ".draw once").await?) == "新版卡片",
        "content reload retained stale indexes"
    );
    backend.api("/content/decks/nested.json","PUT",Some(json!({"without_replacement":false,"entries":[{"text":"不应抽到","weight":0},{"text":"来自{临时疯狂}","weight":1}]})))?;
    ensure!(
        message(&a.command(33001, USER, Some(GROUP), ".draw nested").await?).starts_with("来自"),
        "weight/nested reference"
    );
    backend.api("/content/rules/target.json","PUT",Some(json!({"id":"target","label":"二面目标","faces":2,"comparison":"gte","critical":null,"fumble":null})))?;
    a.command(33001, USER, Some(GROUP), ".st new 规则卡")
        .await?;
    a.command(33001, USER, Some(GROUP), ".st 力量60").await?;
    a.command(33001, USER, Some(GROUP), ".setrule target")
        .await?;
    ensure!(
        backend.world("a")?["value"]["players"][USER.to_string()]["cards"]["规则卡"]["attrs"]["力量"]
            == 60,
        "rule switch overwrote card"
    );
    a.command(33001, USER, Some(GROUP), ".st temp target")
        .await?;
    ensure!(
        message(&a.command(33001, USER, Some(GROUP), ".ra 测试 1").await?).contains("二面目标"),
        "custom rule hot load"
    );
    a.command(33001, USER, Some(GROUP), ".reply 问好=你好调查员")
        .await?;
    ensure!(
        message(&a.command(33001, USER + 1, Some(GROUP), ".问好").await?) == "你好调查员",
        "custom reply"
    );
    a.command(33001, USER, Some(GROUP), ".nn 测试昵称").await?;
    a.command(33001, USER, Some(GROUP), ".log on 同名团录")
        .await?;
    a.command(33001, USER, Some(GROUP + 1), ".log on 同名团录")
        .await?;
    a.send(event(
        33001,
        USER,
        Some(GROUP + 1),
        "另一个群独有文本",
        true,
        true,
    ))
    .await?;
    b.command(33002, USER, Some(GROUP), ".log on 同名团录")
        .await?;
    b.send(event(
        33002,
        USER,
        Some(GROUP),
        "另一个账号独有文本",
        true,
        true,
    ))
    .await?;
    a.send(event(
        33001,
        USER,
        Some(GROUP),
        "<script>alert(1)</script> & 原文",
        true,
        true,
    ))
    .await?;
    for i in 0..205 {
        a.send(event(
            33001,
            USER,
            Some(GROUP),
            &format!("逐条记录-{i}"),
            true,
            true,
        ))
        .await?;
    }
    a.command(33001, USER, Some(GROUP), ".log off").await?;
    a.send(event(
        33001,
        USER,
        Some(GROUP),
        "暂停期不得记录",
        true,
        true,
    ))
    .await?;
    a.command(33001, USER, Some(GROUP), ".log on").await?;
    a.command(33001, USER, Some(GROUP), ".rh 1d1+876542")
        .await?;
    let public = a.action().await?;
    ensure!(
        !message(&public).contains("876543"),
        "dark result leaked to group"
    );
    a.fail_private.store(true, Ordering::SeqCst);
    let hidden = a
        .command(33001, USER, Some(GROUP), ".rh 1d1+765431")
        .await?;
    ensure!(
        hidden["action"] == "send_private_msg" && message(&hidden).contains("765432"),
        "private result target"
    );
    let warning = a.action().await?;
    ensure!(
        warning["action"] == "send_group_msg"
            && message(&warning).contains("不会公开")
            && !message(&warning).contains("765432"),
        "failed private fallback leaked result"
    );
    a.fail_private.store(false, Ordering::SeqCst);
    ensure!(
        message(&a.command(33001, USER, Some(GROUP), ".log end").await?).contains("尚未配置"),
        "upload interface fabricated delivery"
    );
    let sessions = backend.api("/sessions", "GET", None)?;
    let matches: Vec<_> = sessions
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["session"] == "同名团录")
        .collect();
    ensure!(matches.len() == 3, "cross-group/account log isolation");
    let selected = matches
        .iter()
        .find(|s| s["scope"] == json!(["qq", "a", GROUP.to_string()]).to_string())
        .context("missing session")?;
    let query = format!(
        "scope={}&session={}",
        url_encode(selected["scope"].as_str().unwrap()),
        url_encode("同名团录")
    );
    let export = backend.api(&format!("/export?{query}&format=json"), "GET", None)?;
    let rows = export.as_array().context("log export is not an array")?;
    ensure!(rows.len() > 200, "long log missing records");
    let text = rows
        .iter()
        .map(|row| row["text"].as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    for banned in [
        "另一个群独有文本",
        "另一个账号独有文本",
        "暂停期不得记录",
        "876543",
        "765432",
    ] {
        ensure!(
            !text.contains(banned),
            "log leaked forbidden text: {banned}"
        );
    }
    ensure!(
        text.contains("逐条记录-204") && rows.iter().any(|row| row["actor"] == "测试昵称"),
        "complete logs/nickname"
    );
    let first = backend.api(&format!("/logs?{query}"), "GET", None)?;
    ensure!(first.as_array().unwrap().len() == 200, "log page limit");
    let after = first.as_array().unwrap().last().unwrap()["id"]
        .as_i64()
        .unwrap();
    ensure!(
        !backend
            .api(&format!("/logs?{query}&after={after}"), "GET", None)?
            .as_array()
            .unwrap()
            .is_empty(),
        "log cursor"
    );
    let html = String::from_utf8(
        backend
            .request(&format!("/export?{query}&format=html"), "GET", None)?
            .body,
    )?;
    ensure!(
        html.contains("&lt;script&gt;") && !html.contains("<script>alert(1)"),
        "HTML export not escaped"
    );
    let txt = String::from_utf8(
        backend
            .request(&format!("/export?{query}&format=txt"), "GET", None)?
            .body,
    )?;
    ensure!(txt.contains("逐条记录-204"), "TXT export incomplete");
    a.command(33001, USER, Some(GROUP), ".st new 账号A").await?;
    b.command(33002, USER, Some(GROUP), ".st new 账号B").await?;
    ensure!(
        backend.world("a")?["value"]["players"][USER.to_string()]["cards"]
            .get("账号B")
            .is_none(),
        "accounts share cards"
    );
    ensure!(
        message(&a.command(33001, USER + 1, Some(GROUP), ".bot off").await?).contains("仅骰主"),
        "member enabled admin action"
    );
    a.command(33001, USER, Some(GROUP), ".bot off").await?;
    a.send(event(33001, USER, Some(GROUP), ".r 1d1", true, true))
        .await?;
    a.quiet().await?;
    a.command(33001, USER, Some(GROUP), ".bot on").await?;
    backend.api(
        "/config",
        "PUT",
        Some(json!({"blocked_users":[(USER+1).to_string()],"allowed_groups":[GROUP.to_string()]})),
    )?;
    a.send(event(33001, USER + 1, Some(GROUP), ".r 1d1", true, false))
        .await?;
    a.quiet().await?;
    a.send(event(33001, USER, Some(GROUP + 1), ".r 1d1", true, true))
        .await?;
    a.quiet().await?;
    ensure!(
        message(&a.command(33001, USER, None, ".r 1d1").await?).contains("= 1"),
        "group allowlist blocked private messages"
    );
    backend.api("/config", "PUT", Some(json!({"cooldown_ms":5000})))?;
    ensure!(
        message(&a.command(33001, USER, None, ".r 1d1").await?).contains("太快"),
        "cooldown not enforced"
    );
    a.save_trace("decks_logs_permissions_a")?;
    b.save_trace("decks_logs_permissions_b")?;
    drop(a);
    drop(b);
    backend.stop().await?;
    record(
        "decks_logs_permissions",
        &[
            "weighted/nested/nonreplacement decks",
            "deck exhaustion/reset/content update/group scope",
            "custom rules",
            "nicknames/custom replies",
            "record pause/resume/end",
            "three scoped same-name logs",
            "TXT HTML JSON exports and pagination",
            "dark send failure confidentiality",
            "multi-account card isolation",
            "master/member/group switches",
            "blacklist/allowlist/cooldown",
            "deferred upload interface",
        ],
    )
}
fn url_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn onebot_forward_reconnect_and_echo() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let remote = listener.local_addr()?;
    let mut forward = account("f", 33003);
    forward["mode"] = json!("forward");
    forward["url"] = json!(format!("ws://{remote}"));
    let mut backend = Backend::start(json!([forward, account("r", 33004)])).await?;
    async fn accept(listener: &TcpListener) -> Result<Peer> {
        let (stream, _) =
            tokio::time::timeout(Duration::from_secs(10), listener.accept()).await??;
        let socket = accept_hdr_async(
            stream,
            |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                assert_eq!(
                    request
                        .headers()
                        .get("authorization")
                        .unwrap()
                        .to_str()
                        .unwrap(),
                    format!("Bearer {ACCESS}")
                );
                Ok(response)
            },
        )
        .await?;
        Ok(Peer::from_socket(socket))
    }
    let mut f = accept(&listener).await?;
    let mut r = Peer::reverse(&backend, "r", 33004).await?;
    backend.connected("f").await?;
    ensure!(
        message(&f.command(33003, USER, None, ".r 1d1").await?).contains("= 1"),
        "forward private message"
    );
    f.auto.store(false, Ordering::SeqCst);
    let private = f
        .command(33003, USER, Some(GROUP), ".rh 1d1+987653")
        .await?;
    ensure!(private["action"] == "send_private_msg", "dark sequence");
    r.send(json!({"status":"ok","retcode":0,"data":{"message_id":1},"echo":private["echo"]}))
        .await?;
    f.quiet().await?;
    f.send(json!({"status":"ok","retcode":0,"data":{"message_id":1},"echo":"wrong-echo"}))
        .await?;
    f.quiet().await?;
    f.send(json!({"status":"ok","retcode":0,"data":{"message_id":1},"echo":private["echo"]}))
        .await?;
    let public = f.action().await?;
    ensure!(
        public["action"] == "send_group_msg" && !message(&public).contains("987654"),
        "echo correlation/confidentiality"
    );
    f.send(json!({"status":"ok","retcode":0,"data":{"message_id":2},"echo":public["echo"]}))
        .await?;
    f.save_trace("forward_initial")?;
    drop(f);
    let mut reconnect = accept(&listener).await?;
    backend.connected("f").await?;
    ensure!(
        message(&reconnect.command(33003, USER, None, ".r 1d1").await?).contains("= 1"),
        "forward did not reconnect"
    );
    reconnect.save_trace("forward_reconnected")?;
    r.save_trace("reverse_echo_peer")?;
    drop(reconnect);
    drop(r);
    backend.stop().await?;
    record(
        "forward_echo",
        &[
            "forward bearer authorization",
            "forward private/group messages",
            "echo request correlation",
            "cross-account echo rejection",
            "wrong echo rejection",
            "dark replies ordered after confirmation",
            "disconnect/reconnect",
        ],
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn onebot_plugins_failed_hot_update() -> Result<()> {
    let mut backend = Backend::start(json!([account("a", 33001)])).await?;
    let mut peer = Peer::reverse(&backend, "a", 33001).await?;
    let version = backend.root.path().join("data/plugins/contract/1.0.0");
    fs::create_dir_all(&version)?;
    fs::write(
        version.join("main.py"),
        r#"import sys,json
for line in sys.stdin:
 p=json.loads(line)
 if p['method']=='initialize': result={'api':1}
 else: result={'public':'旧版仍可用'}
 print(json.dumps({'jsonrpc':'2.0','id':p['id'],'result':result}),flush=True)
"#,
    )?;
    let manifest = json!({"id":"contract","version":"1.0.0","api":1,"entry":"python3","args":["main.py"],"commands":["oldok"],"rules":[],"dependencies":[],"capabilities":[],"config_schema":{"type":"object","properties":{"enabled":{"type":"boolean"}},"required":["enabled"]}});
    fs::write(version.join("plugin.json"), serde_json::to_vec(&manifest)?)?;
    backend.api(
        "/plugins/load",
        "POST",
        Some(json!({"path":"contract/1.0.0/plugin.json"})),
    )?;
    ensure!(
        message(&peer.command(33001, USER, Some(GROUP), ".oldok").await?) == "旧版仍可用",
        "python plugin command"
    );
    let revision = backend.api("/plugin-config/contract", "GET", None)?["revision"].clone();
    ensure!(
        backend
            .request(
                "/plugin-config/contract",
                "PUT",
                Some(json!({"value":{"enabled":"wrong"},"revision":revision}))
            )?
            .status
            == 400,
        "plugin schema not checked"
    );
    backend.api(
        "/plugin-config/contract",
        "PUT",
        Some(json!({"value":{"enabled":true},"revision":revision})),
    )?;
    for (v, api, entry) in [("2.0.0", 1, "does-not-exist"), ("3.0.0", 999, "python3")] {
        let dir = backend
            .root
            .path()
            .join(format!("data/plugins/contract/{v}"));
        fs::create_dir_all(&dir)?;
        let mut bad = manifest.clone();
        bad["version"] = json!(v);
        bad["api"] = json!(api);
        bad["entry"] = json!(entry);
        fs::write(dir.join("plugin.json"), serde_json::to_vec(&bad)?)?;
        ensure!(
            backend
                .request(
                    "/plugins/load",
                    "POST",
                    Some(json!({"path":format!("contract/{v}/plugin.json")}))
                )?
                .status
                == 400,
            "bad upgrade accepted"
        );
        ensure!(
            message(&peer.command(33001, USER, Some(GROUP), ".oldok").await?) == "旧版仍可用",
            "failed upgrade stopped old plugin"
        );
        ensure!(
            serde_json::from_slice::<String>(&fs::read(
                backend
                    .root
                    .path()
                    .join("data/plugins/contract/active.json")
            )?)? == "1.0.0",
            "failed update changed active version"
        );
    }
    let world = backend.world("a")?;
    backend.api(
        "/world",
        "PUT",
        Some(json!({"scope":"[\"qq\",\"a\"]","value":world["value"],"revision":world["revision"]})),
    )?;
    ensure!(backend.request("/world","PUT",Some(json!({"scope":"[\"qq\",\"a\"]","value":world["value"],"revision":world["revision"]})))?.status==400,"optimistic version conflict accepted");
    peer.save_trace("plugin_failure")?;
    drop(peer);
    backend.stop().await?;
    record(
        "plugin_failure",
        &[
            "Python process plugin over real QQ transport",
            "declarative config validation",
            "failed executable update retains old command",
            "incompatible API update retains old command",
            "active version remains unchanged",
            "optimistic transaction conflict",
        ],
    )
}
