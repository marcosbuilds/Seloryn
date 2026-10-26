use std::{env, fs, path::PathBuf};

fn embedded_bytes(var: &str, symbol: &str) -> String {
    let Some(path) = env::var_os(var).map(PathBuf::from) else {
        return format!("pub static {symbol}: &[u8] = &[];\n");
    };
    if !path.is_file() {
        return format!("pub static {symbol}: &[u8] = &[];\n");
    }
    println!("cargo:rerun-if-changed={}", path.display());
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("pub static {symbol}: &[u8] = include_bytes!(\"{escaped}\");\n")
}

fn main() {
    println!("cargo:rerun-if-env-changed=SELORYN_RUNTIME_PAYLOAD");
    println!("cargo:rerun-if-env-changed=SELORYN_LAUNCHER_PAYLOAD");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let payload = format!(
        "{}{}",
        embedded_bytes("SELORYN_RUNTIME_PAYLOAD", "RUNTIME_PAYLOAD"),
        embedded_bytes("SELORYN_LAUNCHER_PAYLOAD", "LAUNCHER_PAYLOAD")
    );
    fs::write(out_dir.join("installer_payload.rs"), payload)
        .expect("write generated installer payload module");
}
