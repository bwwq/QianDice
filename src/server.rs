use anyhow::{Result,Context,ensure};
use axum::{Router,Json,extract::{State,Path,Query,Request,ws::{WebSocketUpgrade,Message}},http::{StatusCode,HeaderMap,header},response::{IntoResponse,Response,sse::{Sse,Event,KeepAlive}},routing::{get,post},middleware::{self,Next}};
use serde_json::{Value,json};
use serde::{Deserialize,Serialize};
use std::{sync::Arc,collections::{HashMap,BTreeMap},time::{Instant,Duration}};
use tokio::sync::{RwLock,Mutex,broadcast,watch};
use futures_util::{StreamExt,SinkExt};
use crate::{portable::{Paths,Config,API_VERSION,atomic_write},store::Store,plugin::Manager,onebot::Hub,game::{self,ContextInfo,CommandResult,World}};

#[derive(Clone,Serialize)]pub struct Notice{pub time:String,pub level:String,pub message:String}
pub struct App{
    pub paths:Paths,pub store:Arc<Store>,pub plugins:Arc<Manager>,pub config:RwLock<Config>,pub hub:Arc<Hub>,pub shutdown:watch::Sender<bool>,
    pub maintenance:RwLock<()>,pub dispatch:Mutex<()>,events:broadcast::Sender<Notice>,recent:std::sync::Mutex<std::collections::VecDeque<Notice>>,
    token:String,sessions:Mutex<HashMap<String,Instant>>,cooldowns:Mutex<HashMap<String,Instant>>,started:Instant,
}
impl App{
    pub fn new(paths:Paths,store:Arc<Store>,plugins:Arc<Manager>,config:Config)->Result<Arc<Self>>{
        let secret=paths.data.join("config/admin-token.txt");if !secret.exists(){atomic_write(&secret,format!("{}{}",uuid::Uuid::new_v4().simple(),uuid::Uuid::new_v4().simple()).as_bytes())?;
            #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&secret,std::fs::Permissions::from_mode(0o600))?;}}
        let token=std::fs::read_to_string(secret)?.trim().to_owned();ensure!(token.len()>=32,"管理令牌至少需要32个字符");
        let(events,_)=broadcast::channel(256);let(shutdown,_)=watch::channel(false);
        Ok(Arc::new(Self{paths,store,plugins,config:RwLock::new(config),hub:Arc::new(Hub::default()),shutdown,maintenance:RwLock::new(()),dispatch:Mutex::new(()),events,recent:std::sync::Mutex::new(std::collections::VecDeque::new()),token,sessions:Mutex::new(HashMap::new()),cooldowns:Mutex::new(HashMap::new()),started:Instant::now()}))
    }
    pub fn emit(&self,level:&str,message:String){let n=Notice{time:chrono::Utc::now().to_rfc3339(),level:level.into(),message};let mut recent=self.recent.lock().unwrap();recent.push_back(n.clone());if recent.len()>500{recent.pop_front();}let _=self.events.send(n);}
    pub async fn process(&self,mut context:ContextInfo,text:String,simulation:bool)->Result<CommandResult>{
        let _maintenance=self.maintenance.read().await;let _dispatch=self.dispatch.lock().await;
        let config=self.config.read().await.clone();context.admin|=config.masters.contains(&context.user);
        if simulation{context.platform="simulation".into();context.account="sandbox".into();context.admin=true;}
        if !simulation {
            if config.blocked_users.contains(&context.user){return Ok(CommandResult::default())}
            if let Some(group)=&context.group{if !config.allowed_groups.is_empty()&&!config.allowed_groups.contains(group){return Ok(CommandResult::default())}}
        }
        let scope=context.scope();let(value,revision)=self.store.get("world",&scope)?;let world:World=if value.is_null(){World::default()}else{serde_json::from_value(value)?};
        let current=world.rooms.get(&context.room()).cloned().unwrap_or_default();
        if current.recording&&!simulation{self.store.log(&scope,&current.log,&context.name,&text)?;}
        let Some(command)=config.prefixes.iter().find_map(|p|text.trim().strip_prefix(p))else{return Ok(CommandResult{world,..Default::default()})};
        let key=format!("{scope}:{}",context.user);if !simulation{let mut rates=self.cooldowns.lock().await;if rates.get(&key).is_some_and(|t|t.elapsed()<Duration::from_millis(config.cooldown_ms)){anyhow::bail!("指令太快，请稍后重试")}rates.insert(key,Instant::now());if rates.len()>10000{rates.retain(|_,t|t.elapsed()<Duration::from_secs(60));}}
        let decks=self.decks()?;
        let result=self.plugins.call_command(game::CommandRequest{context:context.clone(),command:command.trim().into(),world,decks},&self.store).await?;
        self.store.put("world",&scope,&serde_json::to_value(&result.world)?,revision)?;
        let after=result.world.rooms.get(&context.room()).cloned().unwrap_or_default();
        if !simulation&&!result.public.is_empty(){let session=if after.recording{Some(after.log)}else if current.recording{Some(current.log)}else{None};if let Some(session)=session{self.store.log(&scope,&session,"千变",&result.public)?;}}
        self.emit("command",format!("{} · {} · {}",if simulation{"模拟"}else{"QQ"},context.room(),command.split_whitespace().next().unwrap_or("")));
        Ok(result)
    }
    pub fn decks(&self)->Result<BTreeMap<String,game::Deck>>{let mut decks=game::built_in_decks();for e in std::fs::read_dir(self.paths.data.join("decks"))?{let e=e?;if e.path().extension().is_some_and(|x|x=="json"){let d:game::Deck=serde_json::from_slice(&std::fs::read(e.path())?)?;decks.insert(e.path().file_stem().unwrap().to_string_lossy().into(),d);}}Ok(decks)}
}
struct ApiError(anyhow::Error);
impl<E:Into<anyhow::Error>> From<E> for ApiError{fn from(e:E)->Self{Self(e.into())}}
impl IntoResponse for ApiError{fn into_response(self)->Response{(StatusCode::BAD_REQUEST,Json(json!({"error":self.0.to_string()}))).into_response()}}
type ApiResult<T>=std::result::Result<Json<T>,ApiError>;

pub fn router(app:Arc<App>)->Router {
    let protected=Router::new().route("/status",get(status)).route("/config",get(config_get).put(config_set)).route("/simulate",post(simulate))
        .route("/worlds",get(worlds)).route("/world",get(world_get).put(world_set)).route("/plugins",get(plugins)).route("/plugins/load",post(plugin_load))
        .route("/plugins/:id/:action",post(plugin_action)).route("/content/:kind",get(content_list)).route("/content/:kind/:name",get(content_get).put(content_put).delete(content_delete))
        .route("/backups",get(backups).post(backup)).route("/logs",get(logs)).route("/export",get(export)).route("/events",get(events)).route("/shutdown",post(shutdown))
        .route("/logout",post(logout)).route("/openapi.json",get(openapi)).route_layer(middleware::from_fn_with_state(app.clone(),auth));
    Router::new().nest("/api/v1",protected).route("/api/v1/login",post(login)).route("/onebot/:account",get(reverse)).fallback(get(asset)).with_state(app)
        .layer(axum::extract::DefaultBodyLimit::max(4*1024*1024))
}
fn same_origin(headers:&HeaderMap)->bool{
    if let Some(origin)=headers.get(header::ORIGIN){let o=origin.to_str().unwrap_or("");let host=headers.get(header::HOST).and_then(|h|h.to_str().ok()).unwrap_or("");o==format!("http://{host}")||o==format!("https://{host}")}else{true}
}
fn cookie(headers:&HeaderMap)->Option<String>{headers.get(header::COOKIE)?.to_str().ok()?.split(';').find_map(|v|v.trim().strip_prefix("qianbian_session=").map(str::to_string))}
async fn auth(State(app):State<Arc<App>>,request:Request,next:Next)->Response{
    if !same_origin(request.headers()){return StatusCode::FORBIDDEN.into_response()}
    let bearer=request.headers().get(header::AUTHORIZATION).and_then(|v|v.to_str().ok()).and_then(|s|s.strip_prefix("Bearer "));
    let valid=bearer.is_some_and(|v|constant_eq(v,&app.token))||if let Some(s)=cookie(request.headers()){app.sessions.lock().await.get(&s).is_some_and(|t|t.elapsed()<Duration::from_secs(43200))}else{false};
    if !valid{return (StatusCode::UNAUTHORIZED,Json(json!({"error":"请先登录"}))).into_response()}
    if !matches!(*request.method(),axum::http::Method::GET|axum::http::Method::HEAD)&&request.headers().get("x-qianbian").and_then(|v|v.to_str().ok())!=Some("1"){return StatusCode::FORBIDDEN.into_response()}
    next.run(request).await
}
fn constant_eq(a:&str,b:&str)->bool{if a.len()!=b.len(){return false}a.bytes().zip(b.bytes()).fold(0u8,|v,(a,b)|v|(a^b))==0}
async fn login(State(app):State<Arc<App>>,headers:HeaderMap,Json(v):Json<Value>)->Response{
    if !same_origin(&headers){return StatusCode::FORBIDDEN.into_response()}
    if !constant_eq(v["token"].as_str().unwrap_or(""),&app.token){tokio::time::sleep(Duration::from_millis(500)).await;return StatusCode::UNAUTHORIZED.into_response()}
    let id=uuid::Uuid::new_v4().simple().to_string();let mut sessions=app.sessions.lock().await;sessions.retain(|_,t|t.elapsed()<Duration::from_secs(43200));if sessions.len()>64{sessions.clear();}sessions.insert(id.clone(),Instant::now());
    ([(header::SET_COOKIE,format!("qianbian_session={id}; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200"))],Json(json!({"api":API_VERSION,"ok":true}))).into_response()
}
async fn logout(State(app):State<Arc<App>>,headers:HeaderMap)->Response{if let Some(id)=cookie(&headers){app.sessions.lock().await.remove(&id);}( [(header::SET_COOKIE,"qianbian_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")],Json(json!({"ok":true}))).into_response()}
async fn status(State(app):State<Arc<App>>)->Json<Value>{Json(json!({"name":"千变","version":env!("CARGO_PKG_VERSION"),"api":API_VERSION,"uptime_seconds":app.started.elapsed().as_secs(),"connections":app.hub.connections.read().await.keys().collect::<Vec<_>>(),"notices":app.recent.lock().unwrap().iter().cloned().collect::<Vec<_>>(),"data_directory":app.paths.data,"platform":std::env::consts::OS}))}
async fn config_get(State(app):State<Arc<App>>)->Json<Value>{let mut config=app.config.read().await.clone();for a in &mut config.accounts{a.token=String::new();}Json(serde_json::to_value(config).unwrap())}
async fn config_set(State(app):State<Arc<App>>,Json(mut cfg):Json<Config>)->ApiResult<Value>{
    let _gate=app.maintenance.write().await;ensure_config(&cfg)?;let old=app.config.read().await.clone();for a in &mut cfg.accounts{if a.token.is_empty(){if let Some(o)=old.accounts.iter().find(|o|o.id==a.id){a.token=o.token.clone();}}}
    atomic_write(&app.paths.data.join("config/server.json"),&serde_json::to_vec_pretty(&cfg)?)?;*app.config.write().await=cfg;
    Ok(Json(json!({"ok":true,"message":"配置已保存。账号连接和监听地址在重启后台后应用；权限和前缀立即生效。"})))
}
fn ensure_config(c:&Config)->Result<()>{ensure!(!c.prefixes.is_empty()&&c.prefixes.iter().all(|p|!p.is_empty()&&p.len()<16),"前缀配置无效");c.listen.parse::<std::net::SocketAddr>()?;let mut ids=std::collections::HashSet::new();for a in &c.accounts{ensure!(ids.insert(&a.id),"账号标识重复");ensure!(a.id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_')&&!a.id.is_empty(),"账号标识只能包含字母数字下划线横线");ensure!(a.mode=="forward"||a.mode=="reverse","连接方式无效");if a.mode=="forward"{ensure!(a.url.starts_with("ws://")||a.url.starts_with("wss://"),"需要WebSocket地址")}}Ok(())}
#[derive(Deserialize)]struct Simulation{context:ContextInfo,text:String}
async fn simulate(State(app):State<Arc<App>>,Json(input):Json<Simulation>)->ApiResult<CommandResult>{Ok(Json(app.process(input.context,input.text,true).await?))}
async fn worlds(State(app):State<Arc<App>>)->ApiResult<Value>{Ok(Json(json!(app.store.list("world")?)))}
#[derive(Deserialize)]struct Scope{scope:String}
async fn world_get(State(app):State<Arc<App>>,Query(q):Query<Scope>)->ApiResult<Value>{let(v,r)=app.store.get("world",&q.scope)?;Ok(Json(json!({"value":v,"revision":r})))}
async fn world_set(State(app):State<Arc<App>>,Json(v):Json<Value>)->ApiResult<Value>{let _gate=app.maintenance.read().await;let _lane=app.dispatch.lock().await;let _:World=serde_json::from_value(v["value"].clone())?;let n=app.store.put("world",v["scope"].as_str().context("缺少范围")?,&v["value"],v["revision"].as_i64().context("缺少版本")?)?;Ok(Json(json!({"revision":n})))}
async fn plugins(State(app):State<Arc<App>>)->Json<Value>{Json(json!(app.plugins.list().await))}
async fn plugin_load(State(app):State<Arc<App>>,Json(v):Json<Value>)->ApiResult<Value>{let _gate=app.maintenance.read().await;let p=safe_child(&app.paths.data.join("plugins"),v["path"].as_str().context("缺少插件清单路径")?)?;if !p.file_name().is_some_and(|s|s=="plugin.json"){return Err(anyhow::anyhow!("请选择plugin.json").into())}app.plugins.load(&p).await?;Ok(Json(json!({"ok":true})))}
async fn plugin_action(State(app):State<Arc<App>>,Path((id,action)):Path<(String,String)>)->ApiResult<Value>{let _gate=app.maintenance.read().await;match action.as_str(){"enable"=>app.plugins.enable(&id).await?,"disable"=>app.plugins.disable(&id).await?,_=>return Err(anyhow::anyhow!("操作不存在").into())}Ok(Json(json!({"ok":true})))}
fn safe_child(base:&std::path::Path,name:&str)->Result<std::path::PathBuf>{let rel=std::path::Path::new(name);ensure!(!rel.is_absolute()&&rel.components().all(|c|matches!(c,std::path::Component::Normal(_))),"路径无效");let full=base.join(rel);let parent=if full.exists(){full.clone()}else{full.parent().context("缺少父目录")?.to_path_buf()};ensure!(std::fs::canonicalize(parent)?.starts_with(std::fs::canonicalize(base)?),"路径越界");Ok(full)}
fn content_dir(app:&App,kind:&str)->Result<std::path::PathBuf>{ensure!(["rules","decks"].contains(&kind),"内容类型无效");Ok(app.paths.data.join(kind))}
async fn content_list(State(app):State<Arc<App>>,Path(kind):Path<String>)->ApiResult<Value>{let dir=content_dir(&app,&kind)?;let mut items=vec![];for e in std::fs::read_dir(dir)?{let e=e?;if e.path().extension().is_some_and(|s|s=="json"){items.push(e.file_name().to_string_lossy().to_string());}}Ok(Json(json!(items)))}
async fn content_get(State(app):State<Arc<App>>,Path((kind,name)):Path<(String,String)>)->ApiResult<Value>{Ok(Json(serde_json::from_slice(&std::fs::read(safe_child(&content_dir(&app,&kind)?,&name)?)?)?))}
async fn content_put(State(app):State<Arc<App>>,Path((kind,name)):Path<(String,String)>,Json(v):Json<Value>)->ApiResult<Value>{let _gate=app.maintenance.write().await;if !name.ends_with(".json"){return Err(anyhow::anyhow!("文件名需要.json扩展名").into())}if kind=="decks"{let _:game::Deck=serde_json::from_value(v.clone())?;}atomic_write(&safe_child(&content_dir(&app,&kind)?,&name)?,&serde_json::to_vec_pretty(&v)?)?;Ok(Json(json!({"ok":true})))}
async fn content_delete(State(app):State<Arc<App>>,Path((kind,name)):Path<(String,String)>)->ApiResult<Value>{let _gate=app.maintenance.write().await;std::fs::remove_file(safe_child(&content_dir(&app,&kind)?,&name)?)?;Ok(Json(json!({"ok":true})))}
async fn backups(State(app):State<Arc<App>>)->ApiResult<Value>{let list=std::fs::read_dir(app.paths.data.join("backups"))?.filter_map(|e|e.ok()).filter(|e|e.path().join("manifest.json").exists()).map(|e|e.file_name().to_string_lossy().to_string()).collect::<Vec<_>>();Ok(Json(json!(list)))}
async fn backup(State(app):State<Arc<App>>)->ApiResult<Value>{let _gate=tokio::time::timeout(Duration::from_secs(35),app.maintenance.write()).await?;let id=app.store.backup(&app.paths.data)?;app.emit("backup",format!("备份完成：{id}"));Ok(Json(json!({"id":id})))}
#[derive(Deserialize)]struct LogQuery{scope:String,session:String,#[serde(default)]after:i64,#[serde(default)]format:String}
async fn logs(State(app):State<Arc<App>>,Query(q):Query<LogQuery>)->ApiResult<Value>{Ok(Json(json!(app.store.logs(&q.scope,&q.session,q.after,200)?)))}
async fn export(State(app):State<Arc<App>>,Query(q):Query<LogQuery>)->Result<Response,ApiError>{let rows=app.store.logs(&q.scope,&q.session,0,10000)?;let text=rows.iter().map(|r|format!("[{}] {}: {}",r["time"].as_str().unwrap_or(""),r["actor"].as_str().unwrap_or(""),r["text"].as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n");let (mime,body)=match q.format.as_str(){"html"=>("text/html; charset=utf-8",format!("<!doctype html><meta charset=utf-8><title>千变跑团记录</title><pre>{}</pre>",text.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;"))),"json"=>("application/json",serde_json::to_string_pretty(&rows)?),_=>("text/plain; charset=utf-8",text)};Ok(([(header::CONTENT_TYPE,mime),(header::CONTENT_DISPOSITION,"attachment; filename=log-export")],body).into_response())}
async fn events(State(app):State<Arc<App>>)->impl IntoResponse{let stream=tokio_stream::wrappers::BroadcastStream::new(app.events.subscribe()).filter_map(|v|async move{v.ok().map(|n|Ok::<_,std::convert::Infallible>(Event::default().event("notice").json_data(n).unwrap()))});Sse::new(stream).keep_alive(KeepAlive::default())}
async fn shutdown(State(app):State<Arc<App>>)->Json<Value>{let _=app.shutdown.send(true);Json(json!({"ok":true}))}
async fn openapi()->Json<Value>{Json(json!({"openapi":"3.1.0","info":{"title":"千变管理 API","version":"1"},"servers":[{"url":"/api/v1"}],"paths":{
    "/status":{"get":{"responses":{"200":{"description":"运行状态"}}}},"/simulate":{"post":{"responses":{"200":{"description":"模拟结果，不发送QQ消息"}}}},"/plugins":{"get":{"responses":{"200":{"description":"插件清单与状态"}}}},"/backups":{"post":{"responses":{"200":{"description":"一致性备份标识"}}}},"/events":{"get":{"responses":{"200":{"description":"SSE事件流"}}}}
},"components":{"securitySchemes":{"admin":{"type":"http","scheme":"bearer"}}},"security":[{"admin":[]}]}))}
#[derive(rust_embed::RustEmbed)]#[folder="assets/web/"]struct Web;
async fn asset(uri:axum::http::Uri)->Response{let path=uri.path().trim_start_matches('/');let path=if path.is_empty(){"index.html"}else{path};let file=Web::get(path).or_else(||if !path.contains('.') {Web::get("index.html")}else{None});match file{Some(f)=>([(header::CONTENT_TYPE,mime_guess::from_path(path).first_or_octet_stream().as_ref()),(header::CACHE_CONTROL,if path=="index.html"{"no-store"}else{"private, max-age=3600"})],f.data.to_vec()).into_response(),None=>StatusCode::NOT_FOUND.into_response()}}
async fn reverse(State(app):State<Arc<App>>,Path(id):Path<String>,headers:HeaderMap,ws:WebSocketUpgrade)->Response{
    let config=app.config.read().await;let Some(account)=config.accounts.iter().find(|a|a.id==id&&a.enabled&&a.mode=="reverse").cloned()else{return StatusCode::NOT_FOUND.into_response()};drop(config);
    let auth=headers.get(header::AUTHORIZATION).and_then(|s|s.to_str().ok()).unwrap_or("");if !account.token.is_empty()&&!constant_eq(auth,&format!("Bearer {}",account.token)){return StatusCode::UNAUTHORIZED.into_response()}
    if headers.get("x-self-id").and_then(|v|v.to_str().ok())!=Some(account.self_id.as_str()){return StatusCode::FORBIDDEN.into_response()}
    if headers.get("x-client-role").and_then(|v|v.to_str().ok())!=Some("Universal"){return (StatusCode::BAD_REQUEST,"请选择OneBot Universal反向连接").into_response()}
    ws.max_message_size(1024*1024).on_upgrade(move |socket|async move{let(mut sink,mut stream)=socket.split();let(tx,mut rx)=tokio::sync::mpsc::channel::<String>(256);let generation=uuid::Uuid::new_v4().to_string();app.hub.connections.write().await.insert(id.clone(),(generation.clone(),tx));app.emit("connection",format!("{id} 已连接"));let mut shutdown=app.shutdown.subscribe();loop{tokio::select!{
        _=shutdown.changed()=>break,
        out=rx.recv()=>{match out{Some(s)=>if sink.send(Message::Text(s)).await.is_err(){break},None=>break}},
        input=stream.next()=>{match input{Some(Ok(Message::Text(s)))=>if let Ok(v)=serde_json::from_str(&s){app.hub.ingest(app.clone(),id.clone(),v).await;},Some(Ok(Message::Ping(b)))=>{let _=sink.send(Message::Pong(b)).await;},Some(Ok(Message::Close(_)))|None|Some(Err(_))=>break,_=>{}}}
    }}let mut connections=app.hub.connections.write().await;if connections.get(&id).is_some_and(|(g,_)|g==&generation){connections.remove(&id);}}).into_response()
}
