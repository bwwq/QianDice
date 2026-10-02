use serde_json::json;
fn main()->anyhow::Result<()>{
    qianbian::sdk::serve(|request,host|{
        let rolled=host.call("dice.roll",json!({"expression":"1d20"}))?;
        Ok(json!({"public":format!("原生 Rust 插件向 {} 问好，投出了 {}",request["context"]["user"].as_str().unwrap_or("调查员"),rolled["total"])}))
    })
}
