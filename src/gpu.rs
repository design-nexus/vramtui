use std::process::Command;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gpu {
    pub name: String,
    pub total_mib: u64,
    pub used_mib: u64,
    pub free_mib: u64,
    pub util_pct: u8,
    pub mem_util_pct: u8,
    pub temp_c: u8,
    pub power_w: f32,
    pub power_limit_w: Option<f32>,
    pub clock_sm: Option<u32>,
    pub clock_mem: Option<u32>,
    pub fan_pct: Option<u8>,
    pub pstate: String,
    pub driver: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeApp {
    pub pid: u32,
    pub name: String,
    pub used_mib: u64,
}

pub fn sample() -> (Gpu, Vec<ComputeApp>) {
    let gpu = match Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,memory.used,memory.free,utilization.gpu,utilization.memory,temperature.gpu,power.draw,power.limit,clocks.sm,clocks.mem,fan.speed,pstate,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        Ok(output) if output.status.success() => {
            parse_gpu(&String::from_utf8_lossy(&output.stdout)).unwrap_or_else(|error| Gpu {
                error: Some(error),
                ..Gpu::default()
            })
        }
        Ok(output) => Gpu {
            error: Some(stderr_or(&output, "nvidia-smi failed")),
            ..Gpu::default()
        },
        Err(err) => Gpu {
            error: Some(format!("nvidia-smi was not found: {err}")),
            ..Gpu::default()
        },
    };
    let apps = match Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,process_name,used_gpu_memory",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        Ok(output) if output.status.success() => parse_apps(&String::from_utf8_lossy(&output.stdout)),
        _ => Vec::new(),
    };
    (gpu, apps)
}

pub fn parse_gpu(csv: &str) -> Result<Gpu, String> {
    let line = csv
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "nvidia-smi returned no GPU rows".to_string())?;
    let parts = split_csv(line);
    if parts.len() < 7 {
        return Err(format!("unexpected nvidia-smi row: {line}"));
    }
    if parts.len() >= 14 {
        Ok(Gpu {
            name: parts[0].clone(),
            total_mib: parse_num(&parts[1]),
            used_mib: parse_num(&parts[2]),
            free_mib: parse_num(&parts[3]),
            util_pct: parse_num(&parts[4]) as u8,
            mem_util_pct: parse_num(&parts[5]) as u8,
            temp_c: parse_num(&parts[6]) as u8,
            power_w: parse_f32(&parts[7]),
            power_limit_w: parse_opt_f32(&parts[8]),
            clock_sm: parse_opt_u32(&parts[9]),
            clock_mem: parse_opt_u32(&parts[10]),
            fan_pct: parse_opt_u32(&parts[11]).map(|n| n as u8),
            pstate: parse_text(&parts[12]),
            driver: parse_text(&parts[13]),
            error: None,
        })
    } else {
        Ok(Gpu {
            name: parts[0].clone(),
            total_mib: parse_num(&parts[1]),
            used_mib: parse_num(&parts[2]),
            free_mib: parse_num(&parts[3]),
            util_pct: parse_num(&parts[4]) as u8,
            temp_c: parse_num(&parts[5]) as u8,
            power_w: parse_f32(&parts[6]),
            error: None,
            ..Gpu::default()
        })
    }
}

pub fn parse_apps(csv: &str) -> Vec<ComputeApp> {
    csv.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('[') {
                return None;
            }
            let parts = split_csv(line);
            if parts.len() < 3 {
                return None;
            }
            let pid = parts[0].parse().ok()?;
            Some(ComputeApp {
                pid,
                name: parts[1].clone(),
                used_mib: parse_num(&parts[2]),
            })
        })
        .collect()
}

fn split_csv(line: &str) -> Vec<String> {
    line.split(',')
        .map(|part| part.trim().to_string())
        .collect()
}

fn parse_num(text: &str) -> u64 {
    let cleaned: String = text
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect();
    cleaned.parse().unwrap_or(0)
}

fn is_na(text: &str) -> bool {
    let t = text.trim();
    t.is_empty() || t.eq_ignore_ascii_case("[n/a]") || t.eq_ignore_ascii_case("n/a")
}

fn parse_text(text: &str) -> String {
    if is_na(text) {
        String::new()
    } else {
        text.trim().to_string()
    }
}

fn parse_f32(text: &str) -> f32 {
    parse_opt_f32(text).unwrap_or(0.0)
}

fn parse_opt_f32(text: &str) -> Option<f32> {
    if is_na(text) {
        return None;
    }
    let cleaned: String = text
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect();
    cleaned.parse().ok()
}

fn parse_opt_u32(text: &str) -> Option<u32> {
    if is_na(text) {
        return None;
    }
    Some(parse_num(text) as u32)
}

fn stderr_or(output: &std::process::Output, fallback: &str) -> String {
    let text = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if text.is_empty() {
        fallback.into()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nounits_gpu_row() {
        let gpu = parse_gpu(
            "NVIDIA GeForce RTX 5080 Laptop GPU, 16303, 2, 15932, 13, 46, 18.79\n",
        )
        .unwrap();
        assert_eq!(gpu.name, "NVIDIA GeForce RTX 5080 Laptop GPU");
        assert_eq!(gpu.total_mib, 16303);
        assert_eq!(gpu.used_mib, 2);
        assert_eq!(gpu.util_pct, 13);
        assert_eq!(gpu.temp_c, 46);
        assert!((gpu.power_w - 18.79).abs() < 0.01);
    }

    #[test]
    fn strips_units_if_present() {
        let gpu = parse_gpu("GPU, 16303 MiB, 1024 MiB, 15000 MiB, 40 %, 70, 80.00 W").unwrap();
        assert_eq!(gpu.total_mib, 16303);
        assert_eq!(gpu.used_mib, 1024);
        assert_eq!(gpu.util_pct, 40);
    }

    #[test]
    fn parses_compute_apps() {
        let apps = parse_apps(
            "370180, lm-studio, 4096\n\
             22, /usr/bin/ollama, 2048\n",
        );
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].pid, 370180);
        assert_eq!(apps[0].used_mib, 4096);
        assert_eq!(apps[1].name, "/usr/bin/ollama");
    }

    #[test]
    fn empty_compute_apps() {
        assert!(parse_apps("\n").is_empty());
    }

    #[test]
    fn parses_extended_row_and_na_fields() {
        let gpu = parse_gpu(
            "NVIDIA GeForce RTX 5080 Laptop GPU, 16303, 2, 15932, 0, 0, 45, 7.81, [N/A], 180, 405, [N/A], P8, 615.71.09\n",
        )
        .unwrap();
        assert_eq!(gpu.mem_util_pct, 0);
        assert!((gpu.power_w - 7.81).abs() < 0.01);
        assert!(gpu.power_limit_w.is_none());
        assert_eq!(gpu.clock_sm, Some(180));
        assert_eq!(gpu.clock_mem, Some(405));
        assert!(gpu.fan_pct.is_none());
        assert_eq!(gpu.pstate, "P8");
        assert_eq!(gpu.driver, "615.71.09");
    }
}
