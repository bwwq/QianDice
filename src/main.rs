use anyhow::{Result,Context,ensure};
use clap::{Parser,Subcommand};
use qianbian::{portable::{Paths,load_config},store::{Store,validate_backup,recover_restore},plugin::{Manager,worker_main},server::{App,router},onebot};
use std::{path::PathBuf,sync::Arc};

#[derive(Parser)]#[command(name="千变",version,about="便携、可热加载插件的跑团骰系")]
struct Cli{#[arg(long,global=true)]data_dir:Option<PathBuf>,#[arg(long,global=true)]root:Option<PathBuf>,#[arg(long)]listen:Option<String>,#[command(subcommand)]command:Option<Action>}
#[derive(Subcommand)]enum Action{Serve,Endpoint,Worker{#[arg(long)]kind:String},Backup,Restore{path:PathBuf,#[arg(long)]confirm:bool},CheckBackup{path:PathBuf}}
#[tokio::main]async fn main()->Result<()>{
    let cli=Cli::parse();if let Some(Action::Worker{kind})=&cli.command{return worker_main(kind).await}
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_|"qianbian=info".into())).with_writer(std::io::stderr).init();
    let paths=Paths::discover(cli.data_dir,cli.root)?;
    if matches!(cli.command,Some(Action::Endpoint)){let config=load_config(&paths)?;let mut address:std::net::SocketAddr=config.listen.parse()?;if address.ip().is_unspecified(){address.set_ip("127.0.0.1".parse()?);}println!("http://{address}");return Ok(())}
    let _lock=paths.lock()?;recover_restore(&paths.data)?;
    if let Some(Action::CheckBackup{path})=&cli.command{validate_backup(path)?;println!("备份校验通过");return Ok(())}
    if let Some(Action::Restore{path,confirm})=&cli.command{
        ensure!(*confirm,"恢复会回到备份时间点。请停止后台，核对目录后添加 --confirm");let manifest=validate_backup(path)?;
        let store=Store::open(&paths.data.join("qianbian.sqlite"))?;let before=store.backup(&paths.data)?;drop(store);
        let stage=paths.data.join("restore-staging");ensure!(!stage.exists(),"发现上次未完成的恢复目录，请先核对并恢复备份");std::fs::create_dir_all(&stage)?;
        for (name,_) in manifest["files"].as_object().context("备份清单无效")?{let target=stage.join(name);std::fs::create_dir_all(target.parent().unwrap())?;std::fs::copy(path.join(name),target)?;}
        for name in ["config","plugins","rules","decks","logs"]{std::fs::create_dir_all(stage.join(name))?;}
        qianbian::portable::atomic_write(&paths.data.join("restore-journal.json"),&serde_json::to_vec(&serde_json::json!({"id":uuid::Uuid::new_v4().to_string(),"backup":before}))?)?;
        recover_restore(&paths.data)?;println!("恢复完成，恢复前备份：{before}");return Ok(())
    }
    let store=Arc::new(Store::open(&paths.data.join("qianbian.sqlite"))?);
    if matches!(cli.command,Some(Action::Backup)){println!("{}",store.backup(&paths.data)?);return Ok(())}
    let mut config=load_config(&paths)?;if let Some(listen)=cli.listen{config.listen=listen;}
    qianbian::server::ensure_config(&config)?;let address=config.listen.parse::<std::net::SocketAddr>()?;let listener=tokio::net::TcpListener::bind(address).await.context("监听地址不可用")?;
    let executable=paths.retain_release()?;let plugins=Arc::new(Manager::new(paths.clone(),executable));plugins.start_defaults().await?;
    let app=App::new(paths.clone(),store,plugins.clone(),config.clone())?;
    tokio::spawn(app.clone().run_timers());
    for account in config.accounts{if account.enabled&&account.mode=="forward"{tokio::spawn(onebot::forward(app.clone(),account));}}
    qianbian::portable::atomic_write(&paths.data.join("instance.json"),&serde_json::to_vec(&serde_json::json!({"pid":std::process::id(),"api":1,"address":address.to_string(),"version":env!("CARGO_PKG_VERSION")}))?)?;
    println!("千变已启动：http://{address}\n管理令牌文件：{}\n数据目录：{}",paths.data.join("config/admin-token.txt").display(),paths.data.display());
    let mut stop=app.shutdown.subscribe();let shutdown_app=app.clone();
    axum::serve(listener,router(app.clone())).with_graceful_shutdown(async move{tokio::select!{_=tokio::signal::ctrl_c()=>{},_=stop.changed()=>{}}let _=shutdown_app.shutdown.send(true);}).await?;
    plugins.shutdown().await;Ok(())
}
