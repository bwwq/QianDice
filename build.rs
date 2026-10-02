fn main() {
    println!("cargo:rerun-if-env-changed=QIANBIAN_UI_PAYLOAD");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("payload.rs");
    let code = match std::env::var("QIANBIAN_UI_PAYLOAD") {
        Ok(path) => {
            let path = std::fs::canonicalize(path).expect("UI payload is missing");
            println!("cargo:rerun-if-changed={}", path.display());
            format!(
                "pub static PAYLOAD: &[u8] = include_bytes!({:?});",
                path.to_string_lossy()
            )
        }
        Err(_) => "pub static PAYLOAD: &[u8] = &[];".into(),
    };
    std::fs::write(out, code).unwrap();
}
