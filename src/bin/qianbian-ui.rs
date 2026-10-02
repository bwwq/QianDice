#![cfg_attr(windows, windows_subsystem = "windows")]
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};
include!(concat!(env!("OUT_DIR"), "/payload.rs"));
fn main() {
    if let Err(e) = run() {
        eprintln!("千变启动失败：{e:#}");
        if let Ok(exe) = std::env::current_exe() {
            if let Some(root) = exe.parent() {
                let _ = fs::write(
                    root.join("qianbian-launcher-error.txt"),
                    format!("千变启动失败：{e:#}"),
                );
            }
        }
    }
}
fn run() -> Result<()> {
    ensure!(
        !PAYLOAD.is_empty(),
        "此启动器未嵌入桌面资源，请使用Windows UI正式发布包"
    );
    let paths = qianbian::portable::Paths::discover(None, None)?;
    let hash = hex::encode(Sha256::digest(PAYLOAD));
    let dir = paths.runtime.join(format!("ui-{}", &hash[..16]));
    let launcher_lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(paths.runtime.join("launcher.lock"))?;
    fs2::FileExt::lock_exclusive(&launcher_lock)?;
    let valid = if dir.join("complete.json").exists() {
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("complete.json"))?)?;
        manifest.as_object().is_some_and(|m| {
            m.iter().all(|(p, h)| {
                qianbian::portable::file_hash(&dir.join(p)).ok().as_deref() == h.as_str()
            })
        })
    } else {
        false
    };
    if !valid {
        ensure!(
            !dir.exists(),
            "已有资源目录损坏，请停止后台后移走该目录再重试：{}",
            dir.display()
        );
        let stage = paths
            .runtime
            .join(format!(".ui-{}.partial", uuid::Uuid::new_v4()));
        fs::create_dir_all(&stage)?;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(PAYLOAD))?;
        let mut manifest = serde_json::Map::new();
        let mut size = 0u64;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let relative = file.enclosed_name().context("发布包路径越界")?;
            ensure!(
                file.unix_mode().is_none_or(|m| m & 0o170000 != 0o120000),
                "发布包不能包含符号链接"
            );
            size = size.checked_add(file.size()).context("发布包过大")?;
            ensure!(size <= 2 * 1024 * 1024 * 1024, "发布包解压大小超限");
            let target = stage.join(&relative);
            if file.is_dir() {
                fs::create_dir_all(target)?;
            } else {
                fs::create_dir_all(target.parent().unwrap())?;
                let mut out = fs::File::create(&target)?;
                std::io::copy(&mut file, &mut out)?;
                out.sync_all()?;
                manifest.insert(
                    relative.to_string_lossy().into(),
                    serde_json::json!(qianbian::portable::file_hash(&target)?),
                );
            }
        }
        ensure!(
            stage.join("qianbian-core.exe").exists() && stage.join("qianbian_ui.exe").exists(),
            "发布包缺少启动程序"
        );
        fs::write(stage.join("complete.json"), serde_json::to_vec(&manifest)?)?;
        fs::copy(
            std::env::current_exe()?,
            stage.join("qianbian-ui-release.exe"),
        )?;
        fs::rename(stage, &dir)?;
    }
    fs2::FileExt::unlock(&launcher_lock)?;
    let config = qianbian::portable::load_config(&paths)?;
    let address: std::net::SocketAddr = config.listen.parse()?;
    let connect = if address.ip().is_unspecified() {
        std::net::SocketAddr::new("127.0.0.1".parse()?, address.port())
    } else {
        address
    };
    let endpoint = format!("http://{connect}");
    let running =
        std::net::TcpStream::connect_timeout(&connect, Duration::from_millis(500)).is_ok();
    if !running {
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths.data.join("logs/launcher.log"))?;
        let mut command = Command::new(dir.join("qianbian-core.exe"));
        command
            .arg("--root")
            .arg(&paths.root)
            .arg("--data-dir")
            .arg(&paths.data)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000 | 0x00000008);
        }
        command.spawn()?;
    }
    let mut ready = false;
    for _ in 0..100 {
        if std::net::TcpStream::connect_timeout(&connect, Duration::from_millis(200)).is_ok()
            && paths.data.join("config/admin-token.txt").exists()
        {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    ensure!(ready, "后台未能启动，请查看 data/logs/launcher.log");
    let mut ui = Command::new(dir.join("qianbian_ui.exe"));
    ui.arg("--data-dir")
        .arg(&paths.data)
        .arg("--endpoint")
        .arg(endpoint)
        .current_dir(&dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        ui.creation_flags(0x08000000);
    }
    ui.spawn()?;
    Ok(())
}
