//! Minimal language-neutral JSON-RPC transport for external Rust plugins.
use anyhow::{Result,Context,bail};
use serde_json::{Value,json};
use std::io::{BufRead,Write};
pub struct Host<'a>{reader:&'a mut dyn BufRead,writer:&'a mut dyn Write,sequence:u64}
impl Host<'_>{
    pub fn call(&mut self,method:&str,params:Value)->Result<Value>{self.sequence+=1;let id=format!("host-{}",self.sequence);serde_json::to_writer(&mut self.writer,&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;writeln!(self.writer)?;self.writer.flush()?;let mut line=String::new();self.reader.read_line(&mut line)?;let response:Value=serde_json::from_str(&line)?;if !response["error"].is_null(){bail!("{}",response["error"]["message"])}Ok(response["result"].clone())}
}
pub fn serve(mut command:impl FnMut(Value,&mut Host<'_>)->Result<Value>)->Result<()>{
    let stdin=std::io::stdin();let stdout=std::io::stdout();let mut reader=stdin.lock();let mut writer=stdout.lock();
    loop{let mut line=String::new();if reader.read_line(&mut line)?==0{break}let packet:Value=serde_json::from_str(&line)?;
        let result=match packet["method"].as_str().context("missing method")?{"initialize"=>Ok(json!({"api":1})),"health"=>Ok(json!({"ok":true})),"command"|"event"=>command(packet["params"].clone(),&mut Host{reader:&mut reader,writer:&mut writer,sequence:0}),other=>Err(anyhow::anyhow!("unsupported method {other}"))};
        let reply=match result{Ok(v)=>json!({"jsonrpc":"2.0","id":packet["id"],"result":v}),Err(e)=>json!({"jsonrpc":"2.0","id":packet["id"],"error":{"code":-32000,"message":e.to_string()}})};
        serde_json::to_writer(&mut writer,&reply)?;writeln!(writer)?;writer.flush()?;
    }Ok(())
}
