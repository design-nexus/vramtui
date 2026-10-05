use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stack {
    Ollama,
    LmStudio,
    Other,
    Orphan,
}

impl Stack {
    pub fn label(self) -> &'static str {
        match self {
            Stack::Ollama => "ollama",
            Stack::LmStudio => "lmstudio",
            Stack::Other => "other",
            Stack::Orphan => "orphan",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub ppid: u32,
    pub comm: String,
    pub cmdline: String,
}

impl ProcInfo {
    pub fn is_llama_server(&self) -> bool {
        self.comm == "llama-server" || self.cmdline.split_whitespace().any(|p| p.ends_with("llama-server"))
    }

    pub fn is_run_as_service(&self) -> bool {
        self.cmdline.split_whitespace().any(|p| p == "--run-as-service")
            && (self.comm.contains("lm-studio") || self.cmdline.contains("lm-studio"))
    }

    pub fn looks_ollama(&self) -> bool {
        self.comm == "ollama"
            || self.cmdline.split_whitespace().any(|p| p.ends_with("ollama"))
    }

    pub fn looks_lmstudio(&self) -> bool {
        self.comm.contains("lm-studio")
            || self.cmdline.contains("lm-studio")
            || self.cmdline.contains(".lmstudio")
    }

    pub fn looks_systemd(&self) -> bool {
        self.comm == "systemd"
    }
}

pub fn scan() -> Vec<ProcInfo> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let pid: u32 = match name.to_str().and_then(|s| s.parse().ok()) {
            Some(pid) => pid,
            None => continue,
        };
        if let Some(info) = read_proc(pid) {
            out.push(info);
        }
    }
    out
}

pub fn read_proc(pid: u32) -> Option<ProcInfo> {
    read_proc_at(Path::new("/proc"), pid)
}

fn read_proc_at(root: &Path, pid: u32) -> Option<ProcInfo> {
    let dir = root.join(pid.to_string());
    let stat = fs::read_to_string(dir.join("stat")).ok()?;
    let (ppid, comm) = parse_stat(&stat)?;
    let cmdline = fs::read(dir.join("cmdline"))
        .ok()
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .replace('\0', " ")
                .trim()
                .to_string()
        })
        .unwrap_or_default();
    Some(ProcInfo {
        pid,
        ppid,
        comm,
        cmdline,
    })
}

pub fn parse_stat(stat: &str) -> Option<(u32, String)> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    if close <= open {
        return None;
    }
    let comm = stat[open + 1..close].to_string();
    let after = stat.get(close + 2..)?;
    let mut parts = after.split_whitespace();
    let _state = parts.next()?;
    let ppid = parts.next()?.parse().ok()?;
    Some((ppid, comm))
}

pub fn classify(proc: &ProcInfo, by_pid: &HashMap<u32, ProcInfo>, daemon_pid: Option<u32>) -> Stack {
    let ancestors = ancestor_chain(proc, by_pid);
    let has_ollama = proc.looks_ollama() || ancestors.iter().any(|p| p.looks_ollama());
    let has_lms = proc.looks_lmstudio() || ancestors.iter().any(|p| p.looks_lmstudio());

    if proc.is_llama_server() {
        if has_ollama {
            return Stack::Ollama;
        }
        if has_lms {
            return Stack::LmStudio;
        }
        let parent = by_pid.get(&proc.ppid);
        let parent_init = proc.ppid <= 1 || parent.map(ProcInfo::looks_systemd).unwrap_or(false);
        if parent_init || !has_ollama && !has_lms {
            return Stack::Orphan;
        }
    }

    if proc.is_run_as_service() {
        if let Some(daemon) = daemon_pid {
            if proc.pid != daemon && !same_tree(proc.pid, daemon, by_pid) {
                return Stack::Orphan;
            }
        }
        return Stack::LmStudio;
    }

    if has_ollama {
        return Stack::Ollama;
    }
    if has_lms {
        return Stack::LmStudio;
    }
    Stack::Other
}

pub fn ancestor_chain<'a>(proc: &ProcInfo, by_pid: &'a HashMap<u32, ProcInfo>) -> Vec<&'a ProcInfo> {
    let mut out = Vec::new();
    let mut pid = proc.ppid;
    let mut guard = 0;
    while pid > 1 && guard < 32 {
        guard += 1;
        match by_pid.get(&pid) {
            Some(next) => {
                out.push(next);
                pid = next.ppid;
            }
            None => break,
        }
    }
    out
}

pub fn same_tree(a: u32, b: u32, by_pid: &HashMap<u32, ProcInfo>) -> bool {
    if a == b {
        return true;
    }
    is_ancestor(a, b, by_pid) || is_ancestor(b, a, by_pid)
}

fn is_ancestor(ancestor: u32, mut pid: u32, by_pid: &HashMap<u32, ProcInfo>) -> bool {
    let mut guard = 0;
    while pid > 1 && guard < 32 {
        guard += 1;
        match by_pid.get(&pid) {
            Some(info) => {
                if info.ppid == ancestor {
                    return true;
                }
                pid = info.ppid;
            }
            None => return false,
        }
    }
    false
}

pub fn index(procs: &[ProcInfo]) -> HashMap<u32, ProcInfo> {
    procs.iter().cloned().map(|p| (p.pid, p)).collect()
}

pub fn kill_pid(pid: u32, signal: i32) -> Result<(), String> {
    let rc = unsafe { libc::kill(pid as i32, signal) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, ppid: u32, comm: &str, cmdline: &str) -> ProcInfo {
        ProcInfo {
            pid,
            ppid,
            comm: comm.into(),
            cmdline: cmdline.into(),
        }
    }

    fn map(list: &[ProcInfo]) -> HashMap<u32, ProcInfo> {
        index(list)
    }

    #[test]
    fn parses_stat_with_spaces_in_comm() {
        let (ppid, comm) =
            parse_stat("370180 (lm studio) S 1 370180 370180 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 0 0 0")
                .unwrap();
        assert_eq!(ppid, 1);
        assert_eq!(comm, "lm studio");
    }

    #[test]
    fn llama_server_under_ollama_is_ollama() {
        let procs = vec![
            proc(1, 0, "systemd", "/usr/lib/systemd/systemd"),
            proc(80, 1, "ollama", "/usr/bin/ollama serve"),
            proc(81, 80, "llama-server", "/usr/bin/ollama runner llama-server"),
        ];
        let by = map(&procs);
        assert_eq!(classify(&procs[2], &by, None), Stack::Ollama);
    }

    #[test]
    fn llama_server_reparented_to_init_is_orphan() {
        let procs = vec![
            proc(1, 0, "systemd", "/usr/lib/systemd/systemd"),
            proc(99, 1, "llama-server", "/tmp/llama-server --port 8080"),
        ];
        let by = map(&procs);
        assert_eq!(classify(&procs[1], &by, None), Stack::Orphan);
    }

    #[test]
    fn llama_server_under_user_systemd_is_orphan() {
        let procs = vec![
            proc(1, 0, "systemd", "/usr/lib/systemd/systemd"),
            proc(500, 1, "systemd", "/usr/lib/systemd/systemd --user"),
            proc(99, 500, "llama-server", "llama-server"),
        ];
        let by = map(&procs);
        assert_eq!(classify(&procs[2], &by, None), Stack::Orphan);
    }

    #[test]
    fn extra_run_as_service_outside_tree_is_orphan() {
        let procs = vec![
            proc(1, 0, "systemd", "/usr/lib/systemd/systemd"),
            proc(370180, 1, "lm-studio", "lm-studio --run-as-service"),
            proc(999, 1, "lm-studio", "lm-studio --run-as-service"),
        ];
        let by = map(&procs);
        assert_eq!(classify(&procs[1], &by, Some(370180)), Stack::LmStudio);
        assert_eq!(classify(&procs[2], &by, Some(370180)), Stack::Orphan);
    }

    #[test]
    fn appimage_parent_of_daemon_is_lmstudio() {
        let procs = vec![
            proc(1, 0, "systemd", "/usr/lib/systemd/systemd"),
            proc(370184, 1, "lm-studio", "/opt/lm-studio/lm-studio.AppImage --run-as-service"),
            proc(370180, 370184, "lm-studio", "/tmp/.mount_lm-stu/lm-studio --run-as-service"),
        ];
        let by = map(&procs);
        assert_eq!(classify(&procs[1], &by, Some(370180)), Stack::LmStudio);
        assert_eq!(classify(&procs[2], &by, Some(370180)), Stack::LmStudio);
    }
}
