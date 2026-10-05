use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::util::bytes_to_mib;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Daemon {
    pub host: String,
    pub pid: u32,
    pub port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedModel {
    pub identifier: String,
    pub display_name: String,
    pub vram_mib: u64,
    pub ram_mib: u64,
    pub size_mib: u64,
    pub context: Option<u64>,
    pub status: String,
    pub quantization: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub key: String,
    pub display_name: String,
    pub size_bytes: u64,
    pub quantization: String,
}

#[derive(Debug, Deserialize)]
struct RawDaemon {
    host: Option<String>,
    pid: u32,
    port: u16,
}

pub fn default_json_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".lmstudio/.internal/http-server.json")
}

pub fn read_daemon_file(path: &Path) -> Option<Daemon> {
    let text = fs::read_to_string(path).ok()?;
    parse_daemon(&text)
}

pub fn parse_daemon(text: &str) -> Option<Daemon> {
    let raw: RawDaemon = serde_json::from_str(text).ok()?;
    Some(Daemon {
        host: raw.host.unwrap_or_else(|| "127.0.0.1".into()),
        pid: raw.pid,
        port: raw.port,
    })
}

pub fn pid_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).join("stat").is_file()
}

pub fn port_open(host: &str, port: u16) -> bool {
    let addr = format!("{host}:{port}");
    let Ok(mut addrs) = addr.to_socket_addrs() else {
        return false;
    };
    let Some(addr) = addrs.next() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(150)).is_ok()
}

/// Call `lms` only when the daemon pid is alive and its control port listens.
pub fn should_call_lms(
    daemon: Option<&Daemon>,
    pid_alive: impl Fn(u32) -> bool,
    port_open: impl Fn(&str, u16) -> bool,
) -> bool {
    match daemon {
        Some(d) => pid_alive(d.pid) && port_open(&d.host, d.port),
        None => false,
    }
}

/// True when the json pid is alive and the control port answers.
/// That is the app, not the inference server on :1234.
#[allow(dead_code)]
pub fn is_ready(daemon: Option<&Daemon>) -> bool {
    should_call_lms(daemon, pid_alive, port_open)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerStatus {
    pub running: bool,
    pub port: u16,
}

pub fn parse_server_status(raw: &str) -> ServerStatus {
    let text = raw.trim();
    if let Ok(data) = serde_json::from_str::<Value>(text) {
        return ServerStatus {
            running: data.get("running").and_then(Value::as_bool).unwrap_or(false),
            port: data.get("port").and_then(Value::as_u64).unwrap_or(0) as u16,
        };
    }
    ServerStatus::default()
}

#[allow(dead_code)]
pub fn query_loaded(
    daemon: Option<&Daemon>,
    run: impl FnOnce() -> Result<String, String>,
) -> Result<Vec<LoadedModel>, String> {
    if !should_call_lms(daemon, pid_alive, port_open) {
        return Ok(Vec::new());
    }
    parse_ps(&run()?)
}

pub fn parse_ps(raw: &str) -> Result<Vec<LoadedModel>, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let data: Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
    let list = data.as_array().ok_or_else(|| "lms ps JSON was not an array".to_string())?;
    Ok(list
        .iter()
        .map(|m| {
            let identifier = m["identifier"]
                .as_str()
                .or_else(|| m["modelKey"].as_str())
                .unwrap_or("")
                .to_string();
            let display = m["displayName"]
                .as_str()
                .unwrap_or(&identifier)
                .to_string();
            let vram = m["vramBytes"]
                .as_u64()
                .or_else(|| m["gpuMemoryBytes"].as_u64())
                .unwrap_or(0);
            let ram = m["ramBytes"]
                .as_u64()
                .or_else(|| m["cpuMemoryBytes"].as_u64())
                .unwrap_or(0);
            let size = m["sizeBytes"].as_u64().unwrap_or(vram + ram);
            let quant = m["quantization"]["name"].as_str().unwrap_or("").to_string();
            LoadedModel {
                identifier,
                display_name: display,
                vram_mib: bytes_to_mib(vram),
                ram_mib: bytes_to_mib(ram),
                size_mib: bytes_to_mib(size),
                context: m["contextLength"]
                    .as_u64()
                    .or_else(|| m["maxContextLength"].as_u64()),
                status: m["status"].as_str().unwrap_or("idle").to_string(),
                quantization: quant,
            }
        })
        .filter(|m| !m.identifier.is_empty())
        .collect())
}

pub fn parse_ls(raw: &str) -> Result<Vec<Installed>, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let data: Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
    let list = data.as_array().ok_or_else(|| "lms ls JSON was not an array".to_string())?;
    Ok(list
        .iter()
        .filter_map(|m| {
            let key = m["modelKey"]
                .as_str()
                .or_else(|| m["identifier"].as_str())?
                .to_string();
            if key.is_empty() {
                return None;
            }
            Some(Installed {
                display_name: m["displayName"].as_str().unwrap_or(&key).to_string(),
                size_bytes: m["sizeBytes"].as_u64().unwrap_or(0),
                quantization: m["quantization"]["name"].as_str().unwrap_or("").to_string(),
                key,
            })
        })
        .collect())
}

pub async fn run(bin: &Path, args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new(bin)
        .args(args)
        .output()
        .await
        .map_err(|err| format!("lms: {err}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("lms {} failed", args.join(" "))
        } else {
            err
        })
    }
}

pub async fn loaded(
    bin: &Path,
    json_path: &Path,
) -> Result<(Option<Daemon>, bool, ServerStatus, Vec<LoadedModel>), String> {
    let daemon = read_daemon_file(json_path);
    let app_up = should_call_lms(daemon.as_ref(), pid_alive, port_open);
    if !app_up {
        return Ok((daemon, false, ServerStatus::default(), Vec::new()));
    }
    let status = match run(bin, &["server", "status", "--json"]).await {
        Ok(raw) => parse_server_status(&raw),
        Err(err) if err.to_ascii_lowercase().contains("not running") => ServerStatus::default(),
        Err(err) => return Err(err),
    };
    if !status.running {
        return Ok((daemon, true, status, Vec::new()));
    }
    let models = parse_ps(&run(bin, &["ps", "--json"]).await?)?;
    Ok((daemon, true, status, models))
}

pub async fn installed(bin: &Path, json_path: &Path) -> Result<Vec<Installed>, String> {
    let daemon = read_daemon_file(json_path);
    if !should_call_lms(daemon.as_ref(), pid_alive, port_open) {
        return Ok(Vec::new());
    }
    let raw = run(bin, &["ls", "--json"]).await?;
    parse_ls(&raw)
}

pub async fn unload(bin: &Path, identifier: Option<&str>) -> Result<String, String> {
    let mut args = vec!["unload"];
    if let Some(id) = identifier {
        args.push(id);
    } else {
        args.push("--all");
    }
    run(bin, &args).await?;
    Ok(match identifier {
        Some(id) => format!("Unloaded {id}"),
        None => "Unloaded all LM Studio models".into(),
    })
}

pub async fn load(bin: &Path, key: &str) -> Result<String, String> {
    run(bin, &["load", key, "-y"]).await?;
    Ok(format!("Loading {key}"))
}

pub async fn server(bin: &Path, action: &str) -> Result<String, String> {
    run(bin, &["server", action]).await?;
    Ok(match action {
        "start" => "Started LM Studio server".into(),
        "stop" => "Stopped LM Studio server".into(),
        _ => format!("lms server {action}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn parses_daemon_json() {
        let d = parse_daemon(r#"{"host":"127.0.0.1","pid":370180,"port":41343}"#).unwrap();
        assert_eq!(d.pid, 370180);
        assert_eq!(d.port, 41343);
    }

    #[test]
    fn stale_json_never_invokes_lms() {
        let called = AtomicBool::new(false);
        let daemon = parse_daemon(r#"{"host":"127.0.0.1","pid":1,"port":9}"#);
        let result = query_loaded_with(
            daemon.as_ref(),
            |_| false,
            |_, _| true,
            || {
                called.store(true, Ordering::SeqCst);
                Ok("[]".into())
            },
        )
        .unwrap();
        assert!(result.is_empty());
        assert!(!called.load(Ordering::SeqCst));
    }

    #[test]
    fn missing_json_never_invokes_lms() {
        let called = AtomicBool::new(false);
        let result = query_loaded_with(
            None,
            |_| true,
            |_, _| true,
            || {
                called.store(true, Ordering::SeqCst);
                Ok("[]".into())
            },
        )
        .unwrap();
        assert!(result.is_empty());
        assert!(!called.load(Ordering::SeqCst));
    }

    #[test]
    fn closed_port_is_not_ready() {
        let daemon = parse_daemon(r#"{"host":"127.0.0.1","pid":370180,"port":41343}"#);
        assert!(!should_call_lms(daemon.as_ref(), |_| true, |_, _| false));
        assert!(!is_ready(None));
    }

    #[test]
    fn server_status_json_can_be_down() {
        let down = parse_server_status(r#"{"running":false,"port":1234}"#);
        assert!(!down.running);
        assert_eq!(down.port, 1234);
        let up = parse_server_status(r#"{"running":true,"port":1234}"#);
        assert!(up.running);
        assert!(!parse_server_status("the server is not running").running);
    }

    #[test]
    fn live_daemon_calls_lms() {
        let called = AtomicBool::new(false);
        let daemon = parse_daemon(r#"{"host":"127.0.0.1","pid":370180,"port":41343}"#);
        let result = query_loaded_with(
            daemon.as_ref(),
            |_| true,
            |_, _| true,
            || {
                called.store(true, Ordering::SeqCst);
                Ok(r#"[{"identifier":"gemma","displayName":"Gemma","vramBytes":4294967296,"status":"idle","contextLength":4096,"quantization":{"name":"Q4_0"}}]"#.into())
            },
        )
        .unwrap();
        assert!(called.load(Ordering::SeqCst));
        assert_eq!(result[0].identifier, "gemma");
        assert_eq!(result[0].vram_mib, 4096);
    }

    fn query_loaded_with(
        daemon: Option<&Daemon>,
        pid_alive: impl Fn(u32) -> bool,
        port_open: impl Fn(&str, u16) -> bool,
        run: impl FnOnce() -> Result<String, String>,
    ) -> Result<Vec<LoadedModel>, String> {
        if !should_call_lms(daemon, pid_alive, port_open) {
            return Ok(Vec::new());
        }
        parse_ps(&run()?)
    }

    #[test]
    fn parses_ls_sample() {
        let list = parse_ls(
            r#"[{"type":"llm","modelKey":"google/gemma-4-12b-qat","displayName":"Gemma 4 12B QAT","sizeBytes":7151067268,"quantization":{"name":"Q4_0"}}]"#,
        )
        .unwrap();
        assert_eq!(list[0].key, "google/gemma-4-12b-qat");
        assert_eq!(list[0].quantization, "Q4_0");
    }
}
