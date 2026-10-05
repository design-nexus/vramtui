use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tokio::sync::mpsc::UnboundedSender;

use crate::brand::Logo;
use crate::config::Config;
use crate::gpu::{self, ComputeApp, Gpu};
use crate::lms::{self, Daemon, Installed as LmsInstalled, LoadedModel};
use crate::ollama::{self, Installed as OllamaInstalled, RunningModel};
use crate::proc::{self, ProcInfo, Stack};
use crate::theme::Theme;
use crate::util::{bytes_to_mib, fuzzy_score, is_unreachable};

const HIST_CAP: usize = 48;

#[derive(Clone, Debug)]
pub enum Bus {
    Snapshot(Snapshot),
    Status(String),
    Error(String),
    Theme,
    Installed {
        stack: Stack,
        ollama: Vec<OllamaInstalled>,
        lms: Vec<LmsInstalled>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub gpu: Gpu,
    pub apps: Vec<ComputeApp>,
    pub procs: Vec<ProcInfo>,
    pub ollama_online: bool,
    pub ollama_models: Vec<RunningModel>,
    pub ollama_error: Option<String>,
    pub lms_daemon: Option<Daemon>,
    pub lms_models: Vec<LoadedModel>,
    pub lms_error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TenantKind {
    OllamaModel,
    LmsModel,
    Process,
    Orphan,
}

#[derive(Clone, Debug)]
pub struct Tenant {
    pub key: String,
    pub stack: Stack,
    pub name: String,
    pub vram_mib: u64,
    pub share_pct: u8,
    pub context: Option<u64>,
    pub status: String,
    pub pid: Option<u32>,
    pub kind: TenantKind,
    pub unload_id: Option<String>,
    pub quant: String,
    pub size_mib: u64,
    pub ram_mib: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Occupancy {
    pub ollama: u64,
    pub lmstudio: u64,
    pub other: u64,
    pub free: u64,
    pub used: u64,
    pub total: u64,
}

#[derive(Clone, Debug)]
pub enum Mode {
    Normal,
    Help,
    Confirm(Confirm),
    Load(LoadUi),
}

#[derive(Clone, Debug)]
pub struct Confirm {
    pub prompt: String,
    pub action: ConfirmAction,
}

#[derive(Clone, Debug)]
pub enum ConfirmAction {
    Kill(u32),
    UnloadAll(Stack),
    Park(Stack),
    Load { stack: Stack, key: String },
    StopOllama,
    StopLms,
}

#[derive(Clone, Debug)]
pub struct LoadUi {
    pub stack: Stack,
    pub query: String,
    pub items: Vec<LoadItem>,
    pub selected: usize,
}

#[derive(Clone, Debug)]
pub struct LoadItem {
    pub key: String,
    pub label: String,
    pub size_bytes: u64,
    pub meta: String,
}

pub struct App {
    pub config: Config,
    pub theme: Theme,
    pub snapshot: Snapshot,
    pub tenants: Vec<Tenant>,
    pub occupancy: Occupancy,
    pub selected: usize,
    pub mode: Mode,
    pub status: String,
    pub service_action: Option<&'static str>,
    pub lms_path: PathBuf,
    pub json_path: PathBuf,
    pub logo: Logo,
    pub tick: u32,
    pub table_area: Rect,
    pub vram_hist: Vec<u64>,
    pub util_hist: Vec<u64>,
    pub last_poll: Option<Instant>,
    tx: UnboundedSender<Bus>,
}

impl App {
    pub fn new(config: Config, theme: Theme, tx: UnboundedSender<Bus>) -> Self {
        let lms_path = config.lms_bin();
        Self {
            config,
            theme,
            snapshot: Snapshot::default(),
            tenants: Vec::new(),
            occupancy: Occupancy::default(),
            selected: 0,
            mode: Mode::Normal,
            status: String::new(),
            service_action: None,
            lms_path,
            json_path: lms::default_json_path(),
            logo: Logo::new(),
            tick: 0,
            table_area: Rect::default(),
            vram_hist: Vec::new(),
            util_hist: Vec::new(),
            last_poll: None,
            tx,
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        self.logo.step();
    }

    pub fn blink_on(&self) -> bool {
        self.tick % 20 < 12
    }

    pub fn budget_mib(&self) -> u64 {
        self.config.budget_mib(self.snapshot.gpu.total_mib)
    }

    pub fn tenant_vram(&self) -> u64 {
        self.occupancy.ollama + self.occupancy.lmstudio
    }

    pub fn over_budget(&self, extra_mib: u64) -> bool {
        let budget = self.budget_mib();
        budget > 0 && self.tenant_vram() + extra_mib > budget
    }

    pub fn selected_tenant(&self) -> Option<&Tenant> {
        self.tenants.get(self.selected)
    }

    pub fn take_service_action(&mut self) -> Option<&'static str> {
        self.service_action.take()
    }

    pub fn finish_service(&mut self, result: Result<String, String>) {
        match result {
            Ok(msg) => self.status = msg,
            Err(err) => self.status = err,
        }
    }

    pub fn on_bus(&mut self, bus: Bus) {
        match bus {
            Bus::Snapshot(snap) => {
                let key = self.selected_tenant().map(|t| t.key.clone());
                let total = snap.gpu.total_mib.max(1);
                push_hist(
                    &mut self.vram_hist,
                    snap.gpu.used_mib.saturating_mul(100) / total,
                );
                push_hist(&mut self.util_hist, snap.gpu.util_pct as u64);
                self.last_poll = Some(Instant::now());
                self.snapshot = snap;
                rebuild(self);
                if let Some(key) = key {
                    if let Some(idx) = self.tenants.iter().position(|t| t.key == key) {
                        self.selected = idx;
                    }
                }
                if self.selected >= self.tenants.len() {
                    self.selected = self.tenants.len().saturating_sub(1);
                }
            }
            Bus::Status(msg) => self.status = msg,
            Bus::Error(err) => self.status = err,
            Bus::Theme => {
                if let Some((_, theme, _)) = crate::theme::find_omarchy() {
                    self.theme = crate::theme::resolve(Some(&theme));
                }
            }
            Bus::Installed { stack, ollama, lms } => {
                let items = match stack {
                    Stack::Ollama => ollama
                        .into_iter()
                        .map(|m| LoadItem {
                            key: m.name.clone(),
                            label: m.name,
                            size_bytes: m.size_bytes,
                            meta: crate::util::human_bytes(m.size_bytes),
                        })
                        .collect(),
                    _ => lms
                        .into_iter()
                        .map(|m| LoadItem {
                            key: m.key.clone(),
                            label: if m.display_name.is_empty() {
                                m.key
                            } else {
                                m.display_name
                            },
                            size_bytes: m.size_bytes,
                            meta: format!(
                                "{} {}",
                                crate::util::human_bytes(m.size_bytes),
                                m.quantization
                            )
                            .trim()
                            .to_string(),
                        })
                        .collect(),
                };
                self.mode = Mode::Load(LoadUi {
                    stack,
                    query: String::new(),
                    items,
                    selected: 0,
                });
            }
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return true;
        }
        match &self.mode {
            Mode::Help => {
                self.mode = Mode::Normal;
                false
            }
            Mode::Confirm(_) => {
                self.on_confirm_key(key);
                false
            }
            Mode::Load(_) => {
                self.on_load_key(key);
                false
            }
            Mode::Normal => self.on_normal_key(key),
        }
    }

    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(self.mode, Mode::Normal) {
            return;
        }
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let area = self.table_area;
                if mouse.column >= area.x
                    && mouse.column < area.x.saturating_add(area.width)
                    && mouse.row >= area.y
                    && mouse.row < area.y.saturating_add(area.height)
                {
                    let idx = (mouse.row.saturating_sub(area.y)) as usize;
                    if idx < self.tenants.len() {
                        self.selected = idx;
                    }
                }
            }
            MouseEventKind::ScrollDown => self.move_sel(1),
            MouseEventKind::ScrollUp => self.move_sel(-1),
            _ => {}
        }
    }

    fn on_normal_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char('j') | KeyCode::Down => self.move_sel(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_sel(-1),
            KeyCode::Char('r') => self.status = "Refreshing…".into(),
            KeyCode::Char('R') => {
                self.logo.replay();
                self.status = "Replaying wordmark".into();
            }
            KeyCode::Char('u') => self.unload_selected(),
            KeyCode::Char('U') => self.unload_all_selected_stack(),
            KeyCode::Char('p') => self.park_other(),
            KeyCode::Char('l') => self.open_load(),
            KeyCode::Char('o') => self.toggle_ollama(),
            KeyCode::Char('m') => self.toggle_lms(),
            KeyCode::Char('x') => self.kill_selected(),
            _ => {}
        }
        false
    }

    fn on_confirm_key(&mut self, key: KeyEvent) {
        let Mode::Confirm(confirm) = &self.mode else {
            return;
        };
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                let action = confirm.action.clone();
                self.mode = Mode::Normal;
                self.run_confirm(action);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status = "Cancelled".into();
            }
            _ => {}
        }
    }

    fn on_load_key(&mut self, key: KeyEvent) {
        if !matches!(self.mode, Mode::Load(_)) {
            return;
        }
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Up => {
                if let Mode::Load(ui) = &mut self.mode {
                    ui.selected = ui.selected.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Mode::Load(ui) = &mut self.mode {
                    let len = filtered(ui).len();
                    if len > 0 {
                        ui.selected = (ui.selected + 1).min(len - 1);
                    }
                }
            }
            KeyCode::Backspace => {
                if let Mode::Load(ui) = &mut self.mode {
                    ui.query.pop();
                    ui.selected = 0;
                }
            }
            KeyCode::Enter => {
                let (stack, item) = {
                    let Mode::Load(ui) = &self.mode else { return };
                    let items = filtered(ui);
                    let item = items.get(ui.selected).cloned();
                    (ui.stack, item)
                };
                if let Some(item) = item {
                    let extra = bytes_to_mib(item.size_bytes);
                    if self.over_budget(extra) {
                        self.mode = Mode::Confirm(Confirm {
                            prompt: format!(
                                "Load {} ({} > budget {}M)? y/n",
                                item.key,
                                crate::util::human_mib(extra),
                                self.budget_mib()
                            ),
                            action: ConfirmAction::Load {
                                stack,
                                key: item.key,
                            },
                        });
                    } else {
                        self.mode = Mode::Normal;
                        self.load_model(stack, item.key);
                    }
                }
            }
            KeyCode::Char(ch) => {
                if let Mode::Load(ui) = &mut self.mode {
                    if !key.modifiers.contains(KeyModifiers::CONTROL) {
                        ui.query.push(ch);
                        ui.selected = 0;
                    }
                }
            }
            _ => {}
        }
    }

    fn move_sel(&mut self, delta: i32) {
        if self.tenants.is_empty() {
            return;
        }
        let next = self.selected as i32 + delta;
        self.selected = next.clamp(0, self.tenants.len() as i32 - 1) as usize;
    }

    fn unload_selected(&mut self) {
        let Some(tenant) = self.selected_tenant().cloned() else {
            self.status = "Nothing to unload".into();
            return;
        };
        match tenant.kind {
            TenantKind::OllamaModel => {
                if let Some(id) = tenant.unload_id {
                    let host = self.config.host();
                    self.spawn_status(async move { ollama::stop_model(&host, &id).await });
                }
            }
            TenantKind::LmsModel => {
                let bin = self.lms_path.clone();
                let id = tenant.unload_id.clone();
                self.spawn_status(async move { lms::unload(&bin, id.as_deref()).await });
            }
            TenantKind::Orphan => self.kill_selected(),
            TenantKind::Process => {
                self.status = "That process is not a loaded model".into();
            }
        }
    }

    fn unload_all_selected_stack(&mut self) {
        let stack = self
            .selected_tenant()
            .map(|t| t.stack)
            .filter(|s| matches!(s, Stack::Ollama | Stack::LmStudio));
        let Some(stack) = stack else {
            self.status = "Select an Ollama or LM Studio row".into();
            return;
        };
        self.mode = Mode::Confirm(Confirm {
            prompt: format!("Unload all {} models? y/n", stack.label()),
            action: ConfirmAction::UnloadAll(stack),
        });
    }

    fn park_other(&mut self) {
        let focused = self.selected_tenant().map(|t| t.stack);
        let other = match focused {
            Some(Stack::Ollama) => Stack::LmStudio,
            Some(Stack::LmStudio) => Stack::Ollama,
            _ => {
                self.mode = Mode::Confirm(Confirm {
                    prompt: "Park both stacks (unload all models)? y/n".into(),
                    action: ConfirmAction::Park(Stack::Other),
                });
                return;
            }
        };
        self.mode = Mode::Confirm(Confirm {
            prompt: format!("Park {} (unload its models)? y/n", other.label()),
            action: ConfirmAction::Park(other),
        });
    }

    fn kill_selected(&mut self) {
        let Some(tenant) = self.selected_tenant() else {
            return;
        };
        if tenant.kind != TenantKind::Orphan {
            if tenant.kind == TenantKind::Process && tenant.stack == Stack::Orphan {
                // ok
            } else if tenant.stack != Stack::Orphan {
                self.status = "x kills an orphan process".into();
                return;
            }
        }
        let Some(pid) = tenant.pid else {
            self.status = "No pid to kill".into();
            return;
        };
        self.mode = Mode::Confirm(Confirm {
            prompt: format!("Kill pid {pid} ({})? y/n", tenant.name),
            action: ConfirmAction::Kill(pid),
        });
    }

    fn open_load(&mut self) {
        let stack = match self.selected_tenant().map(|t| t.stack) {
            Some(Stack::Ollama) => Stack::Ollama,
            Some(Stack::LmStudio) => Stack::LmStudio,
            _ if self.snapshot.lms_daemon.as_ref().is_some()
                && proc_alive(self.snapshot.lms_daemon.as_ref()) =>
            {
                Stack::LmStudio
            }
            _ if self.snapshot.ollama_online => Stack::Ollama,
            _ => {
                self.status = "No inference stack is online".into();
                return;
            }
        };
        let host = self.config.host();
        let bin = self.lms_path.clone();
        let json = self.json_path.clone();
        let tx = self.tx.clone();
        self.status = "Listing models…".into();
        tokio::spawn(async move {
            let result = match stack {
                Stack::Ollama => ollama::tags(&host).await.map(|ollama| Bus::Installed {
                    stack,
                    ollama,
                    lms: Vec::new(),
                }),
                _ => lms::installed(&bin, &json).await.map(|lms| Bus::Installed {
                    stack,
                    ollama: Vec::new(),
                    lms,
                }),
            };
            match result {
                Ok(bus) => {
                    let _ = tx.send(bus);
                }
                Err(err) => {
                    let _ = tx.send(Bus::Error(err));
                }
            }
        });
    }

    fn toggle_ollama(&mut self) {
        if self.snapshot.ollama_online {
            self.mode = Mode::Confirm(Confirm {
                prompt: "Stop Ollama? y/n".into(),
                action: ConfirmAction::StopOllama,
            });
        } else {
            self.service_action = Some("start");
        }
    }

    fn toggle_lms(&mut self) {
        if self.snapshot.lms_daemon.is_some() && proc_alive(self.snapshot.lms_daemon.as_ref()) {
            self.mode = Mode::Confirm(Confirm {
                prompt: "Stop LM Studio server? y/n".into(),
                action: ConfirmAction::StopLms,
            });
        } else {
            let bin = self.lms_path.clone();
            self.spawn_status(async move { lms::server(&bin, "start").await });
        }
    }

    fn run_confirm(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::Kill(pid) => {
                self.status = match kill_orphan(pid) {
                    Ok(()) => format!("Killed {pid}"),
                    Err(err) => err,
                };
            }
            ConfirmAction::UnloadAll(Stack::Ollama) => {
                let host = self.config.host();
                let names: Vec<String> = self
                    .snapshot
                    .ollama_models
                    .iter()
                    .map(|m| m.name.clone())
                    .collect();
                self.spawn_status(async move {
                    for name in names {
                        ollama::stop_model(&host, &name).await?;
                    }
                    Ok("Unloaded Ollama models".into())
                });
            }
            ConfirmAction::UnloadAll(Stack::LmStudio) => {
                let bin = self.lms_path.clone();
                self.spawn_status(async move { lms::unload(&bin, None).await });
            }
            ConfirmAction::UnloadAll(_) => {}
            ConfirmAction::Park(Stack::Ollama) => {
                let host = self.config.host();
                let names: Vec<String> = self
                    .snapshot
                    .ollama_models
                    .iter()
                    .map(|m| m.name.clone())
                    .collect();
                self.spawn_status(async move {
                    for name in names {
                        ollama::stop_model(&host, &name).await?;
                    }
                    Ok("Parked Ollama".into())
                });
            }
            ConfirmAction::Park(Stack::LmStudio) => {
                let bin = self.lms_path.clone();
                self.spawn_status(async move { lms::unload(&bin, None).await });
            }
            ConfirmAction::Park(Stack::Other) => {
                let host = self.config.host();
                let names: Vec<String> = self
                    .snapshot
                    .ollama_models
                    .iter()
                    .map(|m| m.name.clone())
                    .collect();
                let bin = self.lms_path.clone();
                self.spawn_status(async move {
                    for name in names {
                        ollama::stop_model(&host, &name).await?;
                    }
                    lms::unload(&bin, None).await?;
                    Ok("Parked both stacks".into())
                });
            }
            ConfirmAction::Park(_) => {}
            ConfirmAction::Load { stack, key } => self.load_model(stack, key),
            ConfirmAction::StopOllama => self.service_action = Some("stop"),
            ConfirmAction::StopLms => {
                let bin = self.lms_path.clone();
                self.spawn_status(async move { lms::server(&bin, "stop").await });
            }
        }
    }

    fn load_model(&mut self, stack: Stack, key: String) {
        match stack {
            Stack::Ollama => {
                self.status = format!("Load {key} from ollatui or `ollama run` — Ollama has no load-without-chat in v1");
                // Ollama loads on first request. Keep a generate keep_alive ping.
                let host = self.config.host();
                let model = key.clone();
                self.spawn_status(async move {
                    warmup_ollama(&host, &model).await
                });
            }
            _ => {
                let bin = self.lms_path.clone();
                self.spawn_status(async move { lms::load(&bin, &key).await });
            }
        }
    }

    fn spawn_status<F>(&mut self, fut: F)
    where
        F: std::future::Future<Output = Result<String, String>> + Send + 'static,
    {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let bus = match fut.await {
                Ok(msg) => Bus::Status(msg),
                Err(err) => Bus::Error(err),
            };
            let _ = tx.send(bus);
        });
        self.status = "Working…".into();
    }
}

fn proc_alive(daemon: Option<&Daemon>) -> bool {
    daemon.map(|d| lms::pid_alive(d.pid)).unwrap_or(false)
}

fn filtered(ui: &LoadUi) -> Vec<LoadItem> {
    let mut scored: Vec<(u32, LoadItem)> = ui
        .items
        .iter()
        .filter_map(|item| {
            let score = fuzzy_score(&ui.query, &item.key)
                .or_else(|| fuzzy_score(&ui.query, &item.label))?;
            Some((score, item.clone()))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.label.cmp(&b.1.label)));
    scored.into_iter().map(|(_, item)| item).collect()
}

pub fn visible_load_items(ui: &LoadUi) -> Vec<LoadItem> {
    filtered(ui)
}

fn kill_orphan(pid: u32) -> Result<(), String> {
    proc::kill_pid(pid, libc::SIGTERM).map_err(|err| format!("SIGTERM {pid}: {err}"))?;
    std::thread::sleep(Duration::from_millis(400));
    if PathBuf::from(format!("/proc/{pid}/stat")).is_file() {
        proc::kill_pid(pid, libc::SIGKILL).map_err(|err| format!("SIGKILL {pid}: {err}"))?;
    }
    Ok(())
}

async fn warmup_ollama(host: &str, model: &str) -> Result<String, String> {
    let url = format!("{}/api/generate", ollama::normalize_host(host));
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .build()
        .map_err(|err| err.to_string())?;
    let body = serde_json::json!({
        "model": model,
        "prompt": "",
        "keep_alive": "10m",
        "stream": false,
    });
    client
        .post(url)
        .json(&body)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?;
    Ok(format!("Loaded {model}"))
}

pub fn rebuild(app: &mut App) {
    let snap = &app.snapshot;
    let by_pid = proc::index(&snap.procs);
    let daemon_pid = snap.lms_daemon.as_ref().map(|d| d.pid);
    let mut vram_by_pid: HashMap<u32, u64> = HashMap::new();
    let mut stack_vram = Occupancy {
        total: snap.gpu.total_mib,
        used: snap.gpu.used_mib,
        free: snap.gpu.total_mib.saturating_sub(snap.gpu.used_mib),
        ..Occupancy::default()
    };
    for app_gpu in &snap.apps {
        vram_by_pid.insert(app_gpu.pid, app_gpu.used_mib);
        if let Some(info) = by_pid.get(&app_gpu.pid) {
            match proc::classify(info, &by_pid, daemon_pid) {
                Stack::Ollama => stack_vram.ollama += app_gpu.used_mib,
                Stack::LmStudio => stack_vram.lmstudio += app_gpu.used_mib,
                Stack::Orphan => stack_vram.other += app_gpu.used_mib,
                Stack::Other => stack_vram.other += app_gpu.used_mib,
            }
        } else {
            stack_vram.other += app_gpu.used_mib;
        }
    }
    if stack_vram.ollama == 0 {
        stack_vram.ollama = snap.ollama_models.iter().map(|m| m.vram_mib).sum();
    }
    if stack_vram.lmstudio == 0 {
        stack_vram.lmstudio = snap.lms_models.iter().map(|m| m.vram_mib).sum();
    }
    let attributed = stack_vram.ollama + stack_vram.lmstudio + stack_vram.other;
    if attributed < stack_vram.used {
        stack_vram.other += stack_vram.used - attributed;
    }

    let total = stack_vram.total.max(1);
    let share = |mib: u64| ((mib.saturating_mul(100)) / total) as u8;

    let mut tenants = Vec::new();
    for model in &snap.ollama_models {
        tenants.push(Tenant {
            key: format!("ollama:{}", model.name),
            stack: Stack::Ollama,
            name: model.name.clone(),
            vram_mib: model.vram_mib,
            share_pct: share(model.vram_mib),
            context: model.context,
            status: keep_alive(&model.expires),
            pid: None,
            kind: TenantKind::OllamaModel,
            unload_id: Some(model.name.clone()),
            quant: model.quantization.clone(),
            size_mib: model.size_mib,
            ram_mib: 0,
        });
    }
    for model in &snap.lms_models {
        tenants.push(Tenant {
            key: format!("lms:{}", model.identifier),
            stack: Stack::LmStudio,
            name: if model.display_name.is_empty() {
                model.identifier.clone()
            } else {
                model.display_name.clone()
            },
            vram_mib: model.vram_mib,
            share_pct: share(model.vram_mib),
            context: model.context,
            status: model.status.clone(),
            pid: daemon_pid,
            kind: TenantKind::LmsModel,
            unload_id: Some(model.identifier.clone()),
            quant: model.quantization.clone(),
            size_mib: model.size_mib,
            ram_mib: model.ram_mib,
        });
    }
    let mut seen_pids: Vec<u32> = tenants.iter().filter_map(|t| t.pid).collect();
    for info in &snap.procs {
        let stack = proc::classify(info, &by_pid, daemon_pid);
        if stack != Stack::Orphan {
            continue;
        }
        if seen_pids.contains(&info.pid) {
            continue;
        }
        seen_pids.push(info.pid);
        let vram = vram_by_pid.get(&info.pid).copied().unwrap_or(0);
        tenants.push(Tenant {
            key: format!("orphan:{}", info.pid),
            stack: Stack::Orphan,
            name: display_proc(info),
            vram_mib: vram,
            share_pct: share(vram),
            context: None,
            status: "orphan".into(),
            pid: Some(info.pid),
            kind: TenantKind::Orphan,
            unload_id: None,
            quant: String::new(),
            size_mib: 0,
            ram_mib: 0,
        });
    }
    for app_gpu in &snap.apps {
        if seen_pids.contains(&app_gpu.pid) {
            continue;
        }
        let stack = by_pid
            .get(&app_gpu.pid)
            .map(|info| proc::classify(info, &by_pid, daemon_pid))
            .unwrap_or(Stack::Other);
        if stack != Stack::Other {
            continue;
        }
        seen_pids.push(app_gpu.pid);
        tenants.push(Tenant {
            key: format!("proc:{}", app_gpu.pid),
            stack: Stack::Other,
            name: app_gpu.name.rsplit('/').next().unwrap_or(&app_gpu.name).to_string(),
            vram_mib: app_gpu.used_mib,
            share_pct: share(app_gpu.used_mib),
            context: None,
            status: "gpu".into(),
            pid: Some(app_gpu.pid),
            kind: TenantKind::Process,
            unload_id: None,
            quant: String::new(),
            size_mib: 0,
            ram_mib: 0,
        });
    }

    app.occupancy = stack_vram;
    app.tenants = tenants;
}

fn push_hist(hist: &mut Vec<u64>, value: u64) {
    hist.push(value);
    if hist.len() > HIST_CAP {
        hist.remove(0);
    }
}

fn display_proc(info: &ProcInfo) -> String {
    if info.comm.is_empty() {
        info.cmdline
            .split_whitespace()
            .next()
            .unwrap_or("process")
            .rsplit('/')
            .next()
            .unwrap_or("process")
            .to_string()
    } else {
        info.comm.clone()
    }
}

fn keep_alive(expires: &str) -> String {
    if expires.is_empty() {
        return "loaded".into();
    }
    let Ok(end) = chrono::DateTime::parse_from_rfc3339(expires) else {
        return "loaded".into();
    };
    let now = chrono::Utc::now();
    let secs = end.with_timezone(&chrono::Utc).signed_duration_since(now).num_seconds();
    if secs <= 0 {
        "expiring".into()
    } else if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m", secs / 60)
    }
}

pub async fn poll_once(config: &Config, json_path: &PathBuf, lms_path: &PathBuf) -> Snapshot {
    let (gpu, apps) = gpu::sample();
    let procs = proc::scan();
    let ollama = ollama::running(&config.host()).await;
    let (ollama_online, ollama_models, ollama_error) = match ollama {
        Ok(models) => (true, models, None),
        Err(err) => {
            let lower = err.to_ascii_lowercase();
            let online = false;
            let err = if is_unreachable(&err) || lower.contains("connection") || lower.contains("connect")
            {
                None
            } else {
                Some(err)
            };
            (online, Vec::new(), err)
        }
    };
    let lms = lms::loaded(lms_path, json_path).await;
    let (lms_daemon, lms_models, lms_error) = match lms {
        Ok((daemon, models)) => (daemon, models, None),
        Err(err) => {
            let daemon = lms::read_daemon_file(json_path);
            let err = if is_unreachable(&err) { None } else { Some(err) };
            (daemon, Vec::new(), err)
        }
    };
    Snapshot {
        gpu,
        apps,
        procs,
        ollama_online,
        ollama_models,
        ollama_error,
        lms_daemon,
        lms_models,
        lms_error,
    }
}

pub fn spawn_poller(config: Config, tx: UnboundedSender<Bus>) {
    tokio::spawn(async move {
        let json = lms::default_json_path();
        let lms_path = config.lms_bin();
        let mut interval = tokio::time::interval(Duration::from_millis(config.gpu_poll_ms.max(250)));
        loop {
            interval.tick().await;
            let snap = poll_once(&config, &json, &lms_path).await;
            if tx.send(Bus::Snapshot(snap)).is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    fn app() -> App {
        let (tx, _rx) = unbounded_channel();
        App::new(Config::default(), crate::theme::mocha(), tx)
    }

    #[test]
    fn budget_flags_a_model_that_does_not_fit() {
        let mut app = app();
        app.snapshot.gpu.total_mib = 16303;
        app.occupancy.ollama = 12000;
        app.occupancy.lmstudio = 0;
        assert!(app.over_budget(5000));
        assert!(!app.over_budget(100));
    }

    #[test]
    fn rebuild_lists_orphan_llama_server() {
        let mut app = app();
        app.snapshot.procs = vec![
            ProcInfo {
                pid: 1,
                ppid: 0,
                comm: "systemd".into(),
                cmdline: "/usr/lib/systemd/systemd".into(),
            },
            ProcInfo {
                pid: 99,
                ppid: 1,
                comm: "llama-server".into(),
                cmdline: "llama-server --port 8080".into(),
            },
        ];
        rebuild(&mut app);
        assert_eq!(app.tenants.len(), 1);
        assert_eq!(app.tenants[0].stack, Stack::Orphan);
        assert_eq!(app.tenants[0].pid, Some(99));
    }
}
