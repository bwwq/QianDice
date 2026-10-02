//! Authenticate the actual backend before a desktop window connects to it.
use crate::portable::{API_VERSION, Paths, load_config};
use anyhow::{Context, Result, ensure};
use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream},
    time::Duration,
};

pub fn address(paths: &Paths) -> Result<SocketAddr> {
    let mut address: SocketAddr = load_config(paths)?.listen.parse()?;
    if address.ip().is_unspecified() {
        address.set_ip(if address.is_ipv6() {
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        } else {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        });
    }
    Ok(address)
}

pub fn ready(paths: &Paths) -> Result<bool> {
    let address = address(paths)?;
    let mut stream = match TcpStream::connect_timeout(&address, Duration::from_millis(300)) {
        Ok(stream) => stream,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::TimedOut
            ) =>
        {
            return Ok(false);
        }
        Err(error) => return Err(error.into()),
    };
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let token = std::fs::read_to_string(paths.data.join("config/admin-token.txt"))
        .context("监听端口已被占用，但当前数据目录没有管理令牌")?;
    let token = token.trim();
    ensure!(
        !token.is_empty() && !token.contains(['\r', '\n']),
        "管理令牌格式无效"
    );
    write!(
        stream,
        "GET /api/v1/status HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    )?;
    let mut response = Vec::new();
    stream.take(1024 * 1024).read_to_end(&mut response)?;
    let response = String::from_utf8(response).context("端口上的服务不是兼容的千变后台")?;
    let (header, body) = response
        .split_once("\r\n\r\n")
        .context("后台响应格式不兼容")?;
    ensure!(
        header.starts_with("HTTP/1.1 200 ") || header.starts_with("HTTP/1.0 200 "),
        "端口已被占用，或后台管理令牌与当前数据目录不匹配"
    );
    let status: serde_json::Value = serde_json::from_str(body).context("后台状态响应不兼容")?;
    ensure!(
        status["name"] == "千变" && status["api"] == API_VERSION,
        "端口上的后台版本不兼容"
    );
    let data = status["data_directory"]
        .as_str()
        .context("后台未返回数据目录")?;
    ensure!(
        std::fs::canonicalize(data)? == paths.data,
        "端口上的后台正在使用另一份数据目录"
    );
    Ok(true)
}
