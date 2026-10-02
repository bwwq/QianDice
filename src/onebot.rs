use crate::{game::ContextInfo, portable::Account, server::App};
use anyhow::{Context, Result, ensure};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{Mutex, RwLock, mpsc, oneshot};

#[derive(Default)]
pub struct Hub {
    pub connections: RwLock<HashMap<String, (String, mpsc::Sender<String>)>>,
    pending: Mutex<HashMap<String, (String, oneshot::Sender<Value>)>>,
    seen: Mutex<VecDeque<String>>,
}
impl Hub {
    pub async fn send(&self, account: &str, action: &str, params: Value) -> Result<Value> {
        let tx = self
            .connections
            .read()
            .await
            .get(account)
            .map(|(_, s)| s.clone())
            .context("QQ 连接已断开")?;
        let id = uuid::Uuid::new_v4().to_string();
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .await
            .insert(id.clone(), (account.into(), sender));
        if tx
            .send(json!({"action":action,"params":params,"echo":id}).to_string())
            .await
            .is_err()
        {
            self.pending.lock().await.remove(&id);
            anyhow::bail!("发送队列已关闭")
        }
        let result = tokio::time::timeout(Duration::from_secs(15), receiver).await;
        self.pending.lock().await.remove(&id);
        let packet = result
            .context("消息发送超时，无法确定是否已发送，不自动重发")?
            .context("连接已关闭")?;
        ensure!(
            packet["status"] == "ok" && packet["retcode"] == 0,
            "QQ 协议端未确认成功：{}",
            packet["retcode"]
        );
        Ok(packet["data"].clone())
    }
    pub async fn reply(
        &self,
        account: &str,
        user: &str,
        group: Option<&str>,
        text: &str,
    ) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let packet = reply_packet(user, group, text)?;
        self.send(
            account,
            packet["action"].as_str().unwrap(),
            packet["params"].clone(),
        )
        .await?;
        Ok(())
    }
    pub async fn ingest(self: &Arc<Self>, app: Arc<App>, account: String, packet: Value) {
        if let Some(id) = packet["echo"].as_str() {
            let mut p = self.pending.lock().await;
            if p.get(id).is_some_and(|(a, _)| a == &account) {
                if let Some((_, sender)) = p.remove(id) {
                    let _ = sender.send(packet);
                }
            }
            return;
        }
        if packet["post_type"] != "message" {
            return;
        }
        let user = id_string(&packet["user_id"]);
        let own = id_string(&packet["self_id"]);
        if user == own || user.is_empty() {
            return;
        }
        let identity = format!(
            "{account}:{}:{}",
            packet["message_type"], packet["message_id"]
        );
        if !packet["message_id"].is_null() {
            let mut seen = self.seen.lock().await;
            if seen.contains(&identity) {
                return;
            }
            seen.push_back(identity);
            if seen.len() > 4096 {
                seen.pop_front();
            }
        }
        let group = if packet["message_type"] == "group" {
            Some(id_string(&packet["group_id"]))
        } else {
            None
        };
        let role = packet["sender"]["role"].as_str().unwrap_or("");
        let name = packet["sender"]["card"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| packet["sender"]["nickname"].as_str())
            .unwrap_or(&user)
            .to_string();
        let context = ContextInfo {
            platform: "qq".into(),
            account: account.clone(),
            user: user.clone(),
            group: group.clone(),
            name,
            admin: role == "owner" || role == "admin",
        };
        let text = match app
            .plugins
            .call_named("onebot", "adapter.decode", packet.clone(), &app.store)
            .await
        {
            Ok(v) => v["text"].as_str().unwrap_or("").to_owned(),
            Err(e) => {
                app.emit("warning", format!("QQ适配失败：{e}"));
                return;
            }
        };
        let log_scope = context.log_scope();
        tokio::spawn(async move {
            match app.process(context, text, false).await {
                Ok(mut result) => {
                    if let Some(secret) = result.private {
                        if app.reply(&account, &user, None, &secret).await.is_err() {
                            let _=app.reply(&account,&user,group.as_deref(),"暗骰结果未能确认送达，请检查好友或私聊权限；结果不会公开，也不会自动重新投掷。").await;
                            return;
                        }
                    }
                    if let (Some(group), Some(session)) =
                        (group.as_deref(), result.export_log.as_deref())
                    {
                        match crate::files::send_log(&app, &account, group, &log_scope, session)
                            .await
                        {
                            Ok(true) => result.public.push_str("\n团录已作为群文件发送。"),
                            Ok(false) => result.public.push_str(
                                "\n团录已保存在千变；群文件上传接口尚未配置，可先从管理端导出。",
                            ),
                            Err(e) => {
                                app.emit("warning", format!("群文件未确认：{e}"));
                                result.public.push_str(
                                    "\n群文件发送未确认，日志仍保存在千变，不会自动重发。",
                                );
                            }
                        }
                    }
                    if let Err(e) = app
                        .reply(&account, &user, group.as_deref(), &result.public)
                        .await
                    {
                        app.emit("warning", format!("回复发送失败：{e}"));
                    }
                }
                Err(e) => {
                    let _ = app
                        .reply(&account, &user, group.as_deref(), &format!("未完成：{e}"))
                        .await;
                }
            }
        });
    }
}
/// OneBot 11 API target IDs are JSON numbers, even though storage keys are strings.
pub fn reply_packet(user: &str, group: Option<&str>, text: &str) -> Result<Value> {
    let (action, key, target) = match group {
        Some(group) => ("send_group_msg", "group_id", group),
        None => ("send_private_msg", "user_id", user),
    };
    let id: i64 = target.parse().context("QQ目标ID必须是数字")?;
    ensure!(id > 0, "QQ目标ID必须是正整数");
    let mut params = json!({"message":[{"type":"text","data":{"text":text}}]});
    params[key] = json!(id);
    Ok(json!({"action":action,"params":params}))
}
pub fn id_string(v: &Value) -> String {
    v.as_str().map(str::to_string).unwrap_or_else(|| {
        if v.is_number() {
            v.to_string()
        } else {
            String::new()
        }
    })
}
pub fn message_text(v: &Value) -> String {
    if let Some(a) = v.as_array() {
        a.iter()
            .map(|s| match s["type"].as_str() {
                Some("text") => s["data"]["text"].as_str().unwrap_or("").to_string(),
                Some("at") => format!(" @{} ", id_string(&s["data"]["qq"])),
                _ => " ".into(),
            })
            .collect()
    } else if let Some(raw) = v.as_str() {
        let mut result = String::new();
        let mut rest = raw;
        while let Some(i) = rest.find("[CQ:") {
            result.push_str(&decode(&rest[..i]));
            let Some(end) = rest[i..].find(']') else {
                break;
            };
            let code = &rest[i + 4..i + end];
            if let Some(id) = code.strip_prefix("at,qq=") {
                result.push_str(&format!(" @{} ", id.split(',').next().unwrap_or("")));
            } else {
                result.push(' ');
            }
            rest = &rest[i + end + 1..];
        }
        result.push_str(&decode(rest));
        result
    } else {
        String::new()
    }
}
fn decode(s: &str) -> String {
    s.replace("&#91;", "[")
        .replace("&#93;", "]")
        .replace("&amp;", "&")
}
pub async fn forward(app: Arc<App>, account: Account) {
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{Message, client::IntoClientRequest, http::HeaderValue},
    };
    let mut delay = 1;
    while !*app.shutdown.borrow() {
        let attempt=async{
            let mut req=account.url.clone().into_client_request()?;
            if !account.token.is_empty(){req.headers_mut().insert("Authorization",HeaderValue::from_str(&format!("Bearer {}",account.token))?);}
            let(socket,_)=connect_async(req).await?;delay=1;
            let(mut write,mut read)=socket.split();let(tx,mut rx)=mpsc::channel::<String>(256);let generation=uuid::Uuid::new_v4().to_string();
            app.hub.connections.write().await.insert(account.id.clone(),(generation.clone(),tx));app.emit("connection",format!("{} 已连接",account.id));
            let mut shutdown=app.shutdown.subscribe();
            loop{tokio::select!{
                _=shutdown.changed()=>{break},
                out=rx.recv()=>{match out{Some(text)=>write.send(Message::Text(text)).await?,None=>break}},
                input=read.next()=>{match input{Some(Ok(Message::Text(text)))=>{if text.len()<=1024*1024{if let Ok(v)=serde_json::from_str(&text){app.hub.ingest(app.clone(),account.id.clone(),v).await;}}},Some(Ok(Message::Ping(b)))=>write.send(Message::Pong(b)).await?,Some(Ok(Message::Close(_)))|None=>break,Some(Err(e))=>return Err(anyhow::Error::from(e)),_=>{}}}
            }}
            let mut connections=app.hub.connections.write().await;if connections.get(&account.id).is_some_and(|(g,_)|g==&generation){connections.remove(&account.id);}Ok::<_,anyhow::Error>(())
        }.await;
        if let Err(e) = attempt {
            app.hub.connections.write().await.remove(&account.id);
            app.emit("warning", format!("{} 连接中断：{e}", account.id));
        }
        if *app.shutdown.borrow() {
            break;
        }
        tokio::time::sleep(Duration::from_secs(delay)).await;
        delay = (delay * 2).min(30);
    }
}
