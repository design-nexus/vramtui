use std::time::Duration;

use serde_json::Value;

use crate::util::bytes_to_mib;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunningModel {
    pub name: String,
    pub size_mib: u64,
    pub vram_mib: u64,
    pub context: Option<u64>,
    pub expires: String,
    pub quantization: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    pub size_bytes: u64,
}

pub fn normalize_host(host: &str) -> String {
    let host = host.trim().trim_end_matches('/');
    if host.starts_with("http://") || host.starts_with("https://") {
        host.to_string()
    } else {
        format!("http://{host}")
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(400))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub async fn running(host: &str) -> Result<Vec<RunningModel>, String> {
    let url = format!("{}/api/ps", normalize_host(host));
    let response = http()
        .get(url)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("ollama /api/ps {}", response.status()));
    }
    let body: Value = response.json().await.map_err(|err| err.to_string())?;
    Ok(parse_ps(&body))
}

pub async fn tags(host: &str) -> Result<Vec<Installed>, String> {
    let url = format!("{}/api/tags", normalize_host(host));
    let response = http()
        .get(url)
        .timeout(Duration::from_secs(4))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("ollama /api/tags {}", response.status()));
    }
    let body: Value = response.json().await.map_err(|err| err.to_string())?;
    Ok(parse_tags(&body))
}

pub fn parse_ps(body: &Value) -> Vec<RunningModel> {
    body["models"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| {
            let size = item["size"].as_u64().unwrap_or(0);
            let vram = item["size_vram"].as_u64().unwrap_or(size);
            RunningModel {
                name: item["name"]
                    .as_str()
                    .or_else(|| item["model"].as_str())
                    .unwrap_or("")
                    .to_string(),
                size_mib: bytes_to_mib(size),
                vram_mib: bytes_to_mib(vram),
                context: item["context_length"].as_u64(),
                expires: item["expires_at"].as_str().unwrap_or("").to_string(),
                quantization: item["details"]["quantization_level"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            }
        })
        .filter(|m| !m.name.is_empty())
        .collect()
}

pub fn parse_tags(body: &Value) -> Vec<Installed> {
    body["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let name = item["name"].as_str()?.to_string();
            Some(Installed {
                name,
                size_bytes: item["size"].as_u64().unwrap_or(0),
            })
        })
        .collect()
}

pub async fn stop_model(host: &str, name: &str) -> Result<String, String> {
    let output = tokio::process::Command::new("ollama")
        .args(["stop", name])
        .env("OLLAMA_HOST", ollama_host_env(host))
        .output()
        .await
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(format!("Unloaded {name}"))
    } else {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("ollama stop {name} failed")
        } else {
            err
        })
    }
}

fn ollama_host_env(host: &str) -> String {
    normalize_host(host)
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .to_string()
}

#[allow(dead_code)]
pub fn service_state() -> String {
    if systemd_unit_loaded() {
        if let Ok(output) = std::process::Command::new("systemctl")
            .args(["is-active", "ollama"])
            .output()
        {
            let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !state.is_empty() {
                return state;
            }
        }
    }
    if serve_pids().is_empty() {
        "inactive".into()
    } else {
        "running".into()
    }
}

/// Start or stop Ollama. Uses the systemd unit when it is installed.
/// `sudo` is used only after a permission failure, and it inherits the terminal
/// so the password prompt is visible. Without a unit, this manages `ollama serve`.
pub fn control(action: &str) -> Result<String, String> {
    if !matches!(action, "start" | "stop") {
        return Err(format!("unknown service action {action}"));
    }
    let result = if systemd_unit_loaded() {
        control_systemd(action)
    } else {
        control_user_process(action)
    };
    if result.is_ok() && action == "start" {
        std::thread::sleep(Duration::from_millis(600));
    }
    result.map(|_| {
        if action == "start" {
            "Started Ollama".into()
        } else {
            "Stopped Ollama".into()
        }
    })
}

fn control_systemd(action: &str) -> Result<(), String> {
    match systemctl(action, false) {
        Ok(()) => Ok(()),
        Err(message) if needs_privilege(&message) => {
            println!("Administrator permission is required to {action} the Ollama service.");
            systemctl(action, true)
        }
        Err(message) => Err(message),
    }
}

fn systemctl(action: &str, sudo: bool) -> Result<(), String> {
    let output = if sudo {
        std::process::Command::new("sudo")
            .args(["systemctl", action, "ollama"])
            .status()
            .map_err(|err| err.to_string())?
    } else {
        let output = std::process::Command::new("systemctl")
            .args([action, "ollama"])
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|err| err.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        let mut detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if detail.is_empty() {
            detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
        if detail.is_empty() {
            detail = format!("systemctl {action} ollama failed");
        }
        return Err(detail);
    };
    if output.success() {
        Ok(())
    } else {
        Err(format!("sudo systemctl {action} ollama failed"))
    }
}

fn needs_privilege(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("authentication")
        || lower.contains("access denied")
        || lower.contains("permission denied")
        || lower.contains("not authorized")
        || lower.contains("polkit")
        || lower.contains("interactive")
}

fn systemd_unit_loaded() -> bool {
    std::process::Command::new("systemctl")
        .args(["show", "-p", "LoadState", "--value", "ollama"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "loaded")
        .unwrap_or(false)
}

fn control_user_process(action: &str) -> Result<(), String> {
    if action == "stop" {
        stop_user_server()
    } else {
        start_user_server()
    }
}

fn start_user_server() -> Result<(), String> {
    if !serve_pids().is_empty() {
        return Ok(());
    }
    let log_path = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("vramtui")
        .join("ollama.log");
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| err.to_string())?;
    let err_log = log.try_clone().map_err(|err| err.to_string())?;
    use std::os::unix::process::CommandExt;
    std::process::Command::new("ollama")
        .arg("serve")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log))
        .stderr(std::process::Stdio::from(err_log))
        .process_group(0)
        .spawn()
        .map_err(|err| format!("could not start ollama serve: {err}"))?;
    Ok(())
}

fn stop_user_server() -> Result<(), String> {
    let pids = serve_pids();
    if pids.is_empty() {
        return Ok(());
    }
    for pid in pids {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
    }
    Ok(())
}

fn serve_pids() -> Vec<u32> {
    let Ok(output) = std::process::Command::new("ps")
        .args(["-eo", "pid,args"])
        .output()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (pid, args) = line.split_once(' ')?;
            let pid = pid.trim().parse().ok()?;
            if args.contains("ollama") && args.split_whitespace().any(|part| part == "serve") {
                Some(pid)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_ps_vram_and_name() {
        let body = json!({
            "models": [{
                "name": "qwen3:8b",
                "size": 5368709120u64,
                "size_vram": 4294967296u64,
                "expires_at": "2026-04-01T00:00:00Z",
                "context_length": 4096,
                "details": { "quantization_level": "Q4_K_M" }
            }]
        });
        let models = parse_ps(&body);
        assert_eq!(models[0].name, "qwen3:8b");
        assert_eq!(models[0].vram_mib, 4096);
        assert_eq!(models[0].context, Some(4096));
        assert_eq!(models[0].quantization, "Q4_K_M");
    }

    #[test]
    fn privilege_errors_ask_for_sudo() {
        assert!(needs_privilege("Interactive authentication required."));
        assert!(!needs_privilege("Unit ollama.service not found."));
    }
}
