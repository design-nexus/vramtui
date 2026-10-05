use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, Gauge, Paragraph, Row, Table, Wrap,
};
use ratatui::Frame;

use crate::app::{visible_load_items, App, Confirm, LoadUi, Mode, Tenant};
use crate::brand::{gradient_at, paint_pixel_rows, paint_word};
use crate::proc::Stack;
use crate::theme::Theme;
use crate::util::{human_mib, is_unreachable, short_error, truncate};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let theme = &app.theme;
    frame.render_widget(Block::default().style(theme.base()), frame.area());

    let area = frame.area();
    let logo_h = logo_height(area);
    let footer_h = 2u16;
    let rest = area.height.saturating_sub(logo_h + footer_h);
    let mid_h = if rest >= 28 {
        20
    } else if rest >= 24 {
        18
    } else if rest >= 20 {
        16
    } else if rest >= 16 {
        13
    } else if rest >= 12 {
        10
    } else if rest >= 10 {
        9
    } else if rest >= 7 {
        6
    } else {
        0
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(logo_h),
            Constraint::Length(mid_h),
            Constraint::Min(6),
            Constraint::Length(footer_h),
        ])
        .split(area);

    draw_logo(frame, chunks[0], app);
    if mid_h > 0 {
        draw_mid(frame, chunks[1], app);
    }
    draw_table(frame, chunks[2], app);
    draw_footer(frame, chunks[3], app);

    match &app.mode {
        Mode::Help => draw_help(frame, app),
        Mode::Load(ui) => draw_load(frame, app, ui),
        Mode::Confirm(c) => draw_confirm(frame, app, c),
        Mode::Normal => {}
    }
}

fn logo_height(area: Rect) -> u16 {
    if area.height >= 28 && area.width >= 40 {
        12
    } else if area.height >= 22 && area.width >= 40 {
        9
    } else if area.height >= 16 && area.width >= 24 {
        3
    } else {
        1
    }
}

fn logo_scale(width: u16) -> u16 {
    if width >= 78 {
        2
    } else {
        1
    }
}

fn draw_logo(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 {
        return;
    }
    let theme = &app.theme;
    let live = app.last_poll.is_some();
    let dot = if live {
        if app.blink_on() {
            Span::styled("●", Style::default().fg(theme.green))
        } else {
            Span::styled("●", Style::default().fg(theme.cyan))
        }
    } else {
        Span::styled("○", Style::default().fg(theme.muted))
    };
    let live_label = if live { " online" } else { " waiting" };

    if area.height == 1 {
        let mut spans = vec![
            Span::raw(" "),
            Span::styled("DNX / 01", Style::default().fg(theme.blue)),
            Span::raw("  "),
        ];
        spans.extend(paint_word().spans);
        spans.push(Span::raw("  "));
        spans.push(dot);
        spans.push(Span::styled(live_label, Style::default().fg(theme.muted)));
        frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.base()), area);
        return;
    }

    if area.height <= 3 {
        let top = Line::from(vec![
            Span::raw(" "),
            Span::styled("DNX / 01", Style::default().fg(theme.blue)),
            Span::raw("  "),
            Span::styled(
                "GPU LANDLORD",
                Style::default().fg(theme.cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            dot.clone(),
            Span::styled(live_label, Style::default().fg(theme.muted)),
        ]);
        let word = pad_line(paint_word(), 1);
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);
        frame.render_widget(Paragraph::new(top).style(theme.base()), split[0]);
        frame.render_widget(Paragraph::new(word).style(theme.base()), split[1]);
        return;
    }

    let scale = logo_scale(area.width);
    let pixel_h = 7;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(pixel_h),
            Constraint::Min(1),
        ])
        .split(area);

    let gpu_name = if app.snapshot.gpu.name.is_empty() {
        "no GPU yet".into()
    } else {
        short_gpu(&app.snapshot.gpu.name)
    };
    let header = Line::from(vec![
        Span::raw("  "),
        Span::styled("DNX / 01", Style::default().fg(theme.blue)),
        Span::raw("   "),
        Span::styled(
            "GPU LANDLORD",
            Style::default().fg(theme.cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled(gpu_name, Style::default().fg(theme.fg_dim)),
        Span::raw("   "),
        dot,
        Span::styled(live_label, Style::default().fg(theme.muted)),
        Span::raw("   "),
        Span::styled("R replay", Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(header).style(theme.base()), rows[1]);

    let pixel_lines = paint_pixel_rows(&app.logo, scale);
    let cols = app.logo.columns.saturating_mul(scale);
    let left = area.width.saturating_sub(cols) / 2;
    let padded: Vec<Line> = pixel_lines
        .into_iter()
        .map(|line| pad_line(line, left as usize))
        .collect();
    frame.render_widget(Paragraph::new(padded).style(theme.base()), rows[3]);

    let tag = Line::from(vec![
        Span::raw(" "),
        Span::styled("─".repeat(8), Style::default().fg(theme.border)),
        Span::raw(" "),
        Span::styled(
            "who holds the GPU",
            Style::default().fg(theme.muted),
        ),
        Span::raw(" "),
        Span::styled("─".repeat(8), Style::default().fg(theme.border)),
    ]);
    frame.render_widget(
        Paragraph::new(tag)
            .style(theme.base())
            .alignment(Alignment::Center),
        rows[4],
    );
}

fn draw_mid(frame: &mut Frame, area: Rect, app: &App) {
    let stacked = area.height >= 15;
    if stacked {
        let card_h = if area.height >= 18 {
            9
        } else if area.height >= 16 {
            8
        } else {
            7
        };
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(card_h)])
            .split(area);
        draw_top_row(frame, split[0], app, false);
        draw_stack_cards(frame, split[1], app);
    } else {
        draw_top_row(frame, area, app, true);
    }
}

fn draw_top_row(frame: &mut Frame, area: Rect, app: &App, cards_in_vram: bool) {
    let wide = area.width >= 120;
    let medium = area.width >= 82;
    if wide {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(44),
                Constraint::Min(40),
                Constraint::Length(34),
            ])
            .split(area);
        draw_gpu_card(frame, cols[0], app);
        draw_vram(frame, cols[1], app, cards_in_vram);
        draw_inspector(frame, cols[2], app);
    } else if medium {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(44), Constraint::Min(36)])
            .split(area);
        draw_gpu_card(frame, cols[0], app);
        draw_vram(frame, cols[1], app, cards_in_vram);
    } else {
        draw_vram(frame, area, app, cards_in_vram);
    }
}

fn draw_gpu_card(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let gpu = &app.snapshot.gpu;
    let block = Block::default()
        .title(Span::styled(" GPU ", Style::default().fg(theme.cyan)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    if let Some(err) = &gpu.error {
        frame.render_widget(
            Paragraph::new(short_error(err)).style(Style::default().fg(theme.red)),
            inner,
        );
        return;
    }

    let name = if gpu.name.is_empty() {
        "waiting for nvidia-smi".into()
    } else {
        truncate(&short_gpu(&gpu.name), inner.width as usize)
    };
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            name,
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        )),
        gpu_stat(
            inner.width,
            "Compute",
            &format!("{}% busy", gpu.util_pct),
            theme,
            Style::default().fg(theme.cyan),
        ),
        gpu_stat(
            inner.width,
            "Memory I/O",
            &format!("{}% busy", gpu.mem_util_pct),
            theme,
            Style::default().fg(theme.magenta),
        ),
        gpu_stat(
            inner.width,
            "Temperature",
            &format!("{} °C", gpu.temp_c),
            theme,
            temp_style(theme, gpu.temp_c),
        ),
        gpu_stat(
            inner.width,
            "Power draw",
            &power_label(gpu.power_w, gpu.power_limit_w),
            theme,
            Style::default().fg(theme.yellow),
        ),
        gpu_stat(
            inner.width,
            "Power state",
            &power_state(&gpu.pstate),
            theme,
            Style::default().fg(theme.blue),
        ),
        gpu_stat(
            inner.width,
            "Core clock",
            &mhz(gpu.clock_sm),
            theme,
            Style::default().fg(theme.fg),
        ),
        gpu_stat(
            inner.width,
            "Memory clock",
            &mhz(gpu.clock_mem),
            theme,
            Style::default().fg(theme.fg),
        ),
    ];
    if let Some(fan) = gpu.fan_pct {
        lines.push(gpu_stat(
            inner.width,
            "Fan",
            &format!("{fan}%"),
            theme,
            Style::default().fg(theme.fg_dim),
        ));
    }
    if !gpu.driver.is_empty() {
        lines.push(gpu_stat(
            inner.width,
            "Driver",
            &gpu.driver,
            theme,
            Style::default().fg(theme.muted),
        ));
    }

    let gauge_n = if inner.height >= lines.len() as u16 + 2 {
        2
    } else {
        0
    };
    let stats_h = inner.height.saturating_sub(gauge_n);
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(stats_h), Constraint::Min(0)])
        .split(inner);
    frame.render_widget(Paragraph::new(lines), split[0]);
    if gauge_n == 2 && split[1].height >= 2 {
        let gauges = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(split[1]);
        labeled_gauge(
            frame,
            gauges[0],
            theme,
            "Compute",
            gpu.util_pct,
            theme.cyan,
        );
        labeled_gauge(
            frame,
            gauges[1],
            theme,
            "Mem I/O",
            gpu.mem_util_pct,
            theme.magenta,
        );
    }
}

fn labeled_gauge(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    label: &str,
    pct: u8,
    color: Color,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(14), Constraint::Min(6)])
        .split(area);
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("{label:<12}"),
            Style::default().fg(theme.muted),
        )),
        cols[0],
    );
    frame.render_widget(
        Gauge::default()
            .percent(pct.min(100) as u16)
            .gauge_style(Style::default().fg(color).bg(theme.bg_raised))
            .label(format!("{pct}%")),
        cols[1],
    );
}

fn gpu_stat<'a>(width: u16, label: &str, value: &str, theme: &Theme, value_style: Style) -> Line<'a> {
    const LABEL_W: usize = 14;
    let value = value.to_string();
    let gap = (width as usize).saturating_sub(LABEL_W + value.len()).max(1);
    Line::from(vec![
        Span::styled(
            format!("{label:<LABEL_W$}"),
            Style::default().fg(theme.muted),
        ),
        Span::raw(" ".repeat(gap)),
        Span::styled(value, value_style),
    ])
}

fn power_label(draw: f32, limit: Option<f32>) -> String {
    match limit {
        Some(limit) => format!("{} / {}", watts(draw), watts(limit)),
        None => watts(draw),
    }
}

fn power_state(pstate: &str) -> String {
    match pstate {
        "" => "—".into(),
        "P0" | "P1" => format!("{pstate}  full"),
        "P2" | "P3" | "P4" | "P5" => format!("{pstate}  active"),
        "P8" | "P10" | "P12" => format!("{pstate}  idle"),
        other => other.to_string(),
    }
}

fn mhz(clock: Option<u32>) -> String {
    match clock {
        Some(n) => format!("{n} MHz"),
        None => "—".into(),
    }
}

fn draw_vram(frame: &mut Frame, area: Rect, app: &App, include_cards: bool) {
    let theme = &app.theme;
    let occ = &app.occupancy;
    let bar_h = 4u16.min(area.height);
    let split = if include_cards && area.height >= 7 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(bar_h), Constraint::Min(3)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1)])
            .split(area)
    };

    let bar_block = Block::default()
        .title(Span::styled(" VRAM ", Style::default().fg(theme.magenta)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(theme.base());
    let bar_inner = bar_block.inner(split[0]);
    frame.render_widget(bar_block, split[0]);
    if bar_inner.height >= 2 {
        let lines = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(bar_inner);
        frame.render_widget(Paragraph::new(stacked_bar(app, lines[0].width as usize)), lines[0]);
        let budget = app.budget_mib();
        let used_pct = if occ.total == 0 {
            0
        } else {
            occ.used.saturating_mul(100) / occ.total
        };
        let budget_pct = if budget == 0 {
            0
        } else {
            app.tenant_vram().saturating_mul(100) / budget
        };
        let over = app.over_budget(0);
        let used_style = if over {
            Style::default().fg(theme.red).add_modifier(Modifier::BOLD)
        } else if used_pct >= 80 {
            Style::default().fg(theme.yellow)
        } else {
            Style::default().fg(theme.fg)
        };
        let mut stats = vec![
            gpu_stat(
                bar_inner.width,
                "In use",
                &format!("{} / {}  {}%", human_mib(occ.used), human_mib(occ.total), used_pct),
                theme,
                used_style,
            ),
            gpu_stat(
                bar_inner.width,
                "Budget",
                &format!("{}  {}%", human_mib(budget), budget_pct),
                theme,
                if over {
                    Style::default().fg(theme.red)
                } else {
                    Style::default().fg(theme.green)
                },
            ),
            gpu_stat(
                bar_inner.width,
                "Ollama",
                &human_mib(occ.ollama),
                theme,
                Style::default().fg(theme.cyan),
            ),
            gpu_stat(
                bar_inner.width,
                "LM Studio",
                &human_mib(occ.lmstudio),
                theme,
                Style::default().fg(theme.magenta),
            ),
            gpu_stat(
                bar_inner.width,
                "Other",
                &human_mib(occ.other),
                theme,
                Style::default().fg(theme.yellow),
            ),
            gpu_stat(
                bar_inner.width,
                "Free",
                &human_mib(occ.free),
                theme,
                Style::default().fg(theme.muted),
            ),
        ];
        let show = lines[1].height as usize;
        stats.truncate(show.max(1));
        frame.render_widget(Paragraph::new(stats), lines[1]);
    }

    if include_cards && split.len() > 1 && split[1].height >= 3 {
        draw_stack_cards(frame, split[1], app);
    }
}

fn stacked_bar(app: &App, width: usize) -> Line<'static> {
    let occ = &app.occupancy;
    let theme = &app.theme;
    let total = occ.total.max(1);
    let bar_w = width.max(8);
    let mut counts = [
        (occ.ollama, theme.cyan, "ollama"),
        (occ.lmstudio, theme.magenta, "lms"),
        (occ.other, theme.yellow, "other"),
        (occ.free, theme.muted, "free"),
    ];
    let sum: u64 = counts.iter().map(|c| c.0).sum();
    if sum < total {
        counts[3].0 += total - sum;
    }
    let mut cells = Vec::new();
    let mut used_cells = 0usize;
    for (i, (mib, color, _)) in counts.iter().enumerate() {
        let n = if i == counts.len() - 1 {
            bar_w.saturating_sub(used_cells)
        } else {
            let n = ((*mib as usize) * bar_w) / total as usize;
            used_cells += n;
            n
        };
        if n > 0 {
            cells.push(Span::styled("█".repeat(n), Style::default().fg(*color)));
        }
    }
    Line::from(cells)
}

fn draw_stack_cards(frame: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(34),
            Constraint::Percentage(32),
        ])
        .split(area);
    let theme = &app.theme;
    let total = app.occupancy.total.max(1);
    let share = |mib: u64| format!("{}  {}%", human_mib(mib), mib.saturating_mul(100) / total);
    let selected = app.selected_tenant().map(|t| t.stack);

    let ollama_n = app
        .tenants
        .iter()
        .filter(|t| t.stack == Stack::Ollama)
        .count();
    let ollama_online = app.snapshot.ollama_online;
    let ollama_host = app
        .config
        .host()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .to_string();
    let mut ollama_stats = vec![
        (
            "Status",
            if ollama_online {
                "online".into()
            } else {
                "offline".into()
            },
            Style::default().fg(if ollama_online {
                theme.green
            } else {
                theme.red
            }),
        ),
        (
            "Models",
            ollama_n.to_string(),
            Style::default().fg(theme.fg),
        ),
        (
            "VRAM",
            share(app.occupancy.ollama),
            Style::default().fg(theme.cyan),
        ),
        (
            "Host",
            ollama_host,
            Style::default().fg(theme.fg_dim),
        ),
    ];
    if let Some(model) = app.tenants.iter().find(|t| t.stack == Stack::Ollama) {
        ollama_stats.push((
            "Loaded",
            model.name.clone(),
            Style::default().fg(theme.fg),
        ));
    }

    let lms_n = app
        .tenants
        .iter()
        .filter(|t| t.stack == Stack::LmStudio)
        .count();
    let (lms_status, lms_style, lms_host, lms_port, lms_pid, lms_alive) =
        match &app.snapshot.lms_daemon {
            Some(d) if crate::lms::pid_alive(d.pid) => (
                "online".into(),
                Style::default().fg(theme.green),
                d.host.clone(),
                d.port.to_string(),
                d.pid.to_string(),
                true,
            ),
            Some(d) => (
                "stale".into(),
                Style::default().fg(theme.yellow),
                d.host.clone(),
                d.port.to_string(),
                d.pid.to_string(),
                false,
            ),
            None => (
                "offline".into(),
                Style::default().fg(theme.red),
                "—".into(),
                "—".into(),
                "—".into(),
                false,
            ),
        };
    let mut lms_stats = vec![
        ("Status", lms_status, lms_style),
        ("Models", lms_n.to_string(), Style::default().fg(theme.fg)),
        (
            "VRAM",
            share(app.occupancy.lmstudio),
            Style::default().fg(theme.magenta),
        ),
        ("Host", lms_host, Style::default().fg(theme.fg_dim)),
        ("Port", lms_port, Style::default().fg(theme.fg_dim)),
        ("PID", lms_pid, Style::default().fg(theme.fg_dim)),
    ];
    if let Some(model) = app.tenants.iter().find(|t| t.stack == Stack::LmStudio) {
        lms_stats.push((
            "Loaded",
            model.name.clone(),
            Style::default().fg(theme.fg),
        ));
    }

    let orphans: Vec<&Tenant> = app
        .tenants
        .iter()
        .filter(|t| t.stack == Stack::Orphan)
        .collect();
    let orphan_n = orphans.len();
    let orphan_style = if orphan_n > 0 {
        if app.blink_on() {
            Style::default().fg(theme.red)
        } else {
            Style::default().fg(theme.yellow)
        }
    } else {
        Style::default().fg(theme.green)
    };
    let mut orphan_stats = vec![
        (
            "Status",
            if orphan_n == 0 {
                "none".into()
            } else {
                "loose".into()
            },
            orphan_style,
        ),
        (
            "Processes",
            orphan_n.to_string(),
            Style::default().fg(theme.fg),
        ),
        (
            "VRAM",
            share(app.occupancy.other),
            Style::default().fg(theme.yellow),
        ),
    ];
    if let Some(orphan) = orphans.first() {
        orphan_stats.push((
            "Name",
            orphan.name.clone(),
            Style::default().fg(theme.fg),
        ));
        orphan_stats.push((
            "PID",
            orphan
                .pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "—".into()),
            Style::default().fg(theme.fg_dim),
        ));
    }

    stack_card(
        frame,
        cols[0],
        app,
        " OLLAMA ",
        app.theme.cyan,
        selected == Some(Stack::Ollama),
        ollama_stats,
        "o",
        if ollama_online { "stop" } else { "start" },
    );
    stack_card(
        frame,
        cols[1],
        app,
        " LM STUDIO ",
        app.theme.magenta,
        selected == Some(Stack::LmStudio),
        lms_stats,
        "m",
        if lms_alive { "stop" } else { "start" },
    );
    stack_card(
        frame,
        cols[2],
        app,
        " ORPHANS ",
        app.theme.red,
        selected == Some(Stack::Orphan),
        orphan_stats,
        "x",
        "kill selected",
    );
}

fn stack_card(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    title: &str,
    accent: Color,
    focused: bool,
    stats: Vec<(&str, String, Style)>,
    hotkey: &str,
    hotkey_label: &str,
) {
    let theme = &app.theme;
    let border = if focused { accent } else { theme.border };
    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(accent)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let body_h = inner.height.saturating_sub(1);
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(body_h.max(1)), Constraint::Length(1)])
        .split(inner);

    let show = split[0].height as usize;
    let lines: Vec<Line> = stats
        .into_iter()
        .take(show)
        .map(|(label, value, style)| gpu_stat(split[0].width, label, &value, theme, style))
        .collect();
    frame.render_widget(Paragraph::new(lines), split[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("({hotkey})"),
                Style::default()
                    .fg(gradient_at(1.0))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {hotkey_label}"),
                Style::default().fg(theme.muted),
            ),
        ])),
        split[1],
    );
}

fn draw_inspector(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = Block::default()
        .title(Span::styled(" INSPECT ", Style::default().fg(theme.blue)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(tenant) = app.selected_tenant() else {
        let idle = vec![
            Line::from(Span::styled(
                "GPU idle",
                Style::default().fg(theme.cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "o  start Ollama",
                Style::default().fg(theme.fg_dim),
            )),
            Line::from(Span::styled(
                "m  start LM Studio",
                Style::default().fg(theme.fg_dim),
            )),
            Line::from(Span::styled(
                "l  load a model",
                Style::default().fg(theme.fg_dim),
            )),
        ];
        frame.render_widget(Paragraph::new(idle), inner);
        return;
    };

    let mut lines = vec![
        kv(theme, "model", &tenant.name),
        kv(theme, "stack", tenant.stack.label()),
        kv(
            theme,
            "vram",
            &format!("{}  {}%", human_mib(tenant.vram_mib), tenant.share_pct),
        ),
    ];
    if !tenant.quant.is_empty() {
        lines.push(kv(theme, "quant", &tenant.quant));
    }
    if tenant.size_mib > 0 {
        lines.push(kv(theme, "size", &human_mib(tenant.size_mib)));
    }
    if tenant.ram_mib > 0 {
        lines.push(kv(theme, "ram", &human_mib(tenant.ram_mib)));
    }
    lines.push(kv(
        theme,
        "ctx",
        &tenant
            .context
            .map(|c| c.to_string())
            .unwrap_or_else(|| "—".into()),
    ));
    lines.push(kv(theme, "ttl", &tenant.status));
    lines.push(kv(
        theme,
        "pid",
        &tenant
            .pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "—".into()),
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        hint_for(tenant),
        Style::default().fg(theme.yellow),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn kv(theme: &Theme, key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{key:<6}",),
            Style::default().fg(theme.muted),
        ),
        Span::styled(value.to_string(), Style::default().fg(theme.fg)),
    ])
}

fn hint_for(tenant: &Tenant) -> &'static str {
    match tenant.kind {
        crate::app::TenantKind::OllamaModel | crate::app::TenantKind::LmsModel => {
            "u unload   U unload stack   p park other"
        }
        crate::app::TenantKind::Orphan => "x kill this orphan (confirm)",
        crate::app::TenantKind::Process => "other GPU process — not a loaded model",
    }
}

fn draw_table(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = &app.theme;
    let block = Block::default()
        .title(Span::styled(" TENANTS ", Style::default().fg(theme.accent)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    app.table_area = Rect {
        x: inner.x,
        y: inner.y.saturating_add(1),
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };

    if app.tenants.is_empty() {
        let empty = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  nothing on the GPU",
                Style::default().fg(theme.cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "  start a stack, then load a model",
                Style::default().fg(theme.muted),
            )),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                keycap("o", theme),
                Span::styled(" ollama   ", Style::default().fg(theme.fg_dim)),
                keycap("m", theme),
                Span::styled(" lm studio   ", Style::default().fg(theme.fg_dim)),
                keycap("l", theme),
                Span::styled(" load", Style::default().fg(theme.fg_dim)),
            ]),
        ];
        frame.render_widget(Paragraph::new(empty), inner);
        return;
    }

    let wide = inner.width >= 92;
    let medium = inner.width >= 72;
    let header = if wide {
        Row::new(["STACK", "MODEL", "QUANT", "VRAM", "SHARE", "CTX", "TTL", "PID"]).style(
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        )
    } else if medium {
        Row::new(["STACK", "MODEL", "VRAM", "SHARE", "TTL", "PID"]).style(
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Row::new(["STACK", "MODEL", "VRAM", "TTL"]).style(
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        )
    };

    let blink = app.blink_on();
    let rows = app.tenants.iter().map(|t| {
        let stack_style = stack_style(theme, t.stack, blink);
        let share = format!(
            "{} {:>3}%",
            mini_bar(t.share_pct, 6),
            t.share_pct
        );
        let ctx = t
            .context
            .map(|c| c.to_string())
            .unwrap_or_else(|| "—".into());
        let pid = t.pid.map(|p| p.to_string()).unwrap_or_else(|| "—".into());
        let quant = if t.quant.is_empty() {
            "—".into()
        } else {
            t.quant.clone()
        };
        if wide {
            Row::new(vec![
                Cell::from(Span::styled(t.stack.label().to_string(), stack_style)),
                Cell::from(t.name.clone()),
                Cell::from(Span::styled(quant, Style::default().fg(theme.fg_dim))),
                Cell::from(Span::styled(
                    human_mib(t.vram_mib),
                    Style::default().fg(theme.accent),
                )),
                Cell::from(Span::styled(share, Style::default().fg(stack_color(theme, t.stack)))),
                Cell::from(ctx),
                Cell::from(t.status.clone()),
                Cell::from(pid),
            ])
        } else if medium {
            Row::new(vec![
                Cell::from(Span::styled(t.stack.label().to_string(), stack_style)),
                Cell::from(t.name.clone()),
                Cell::from(Span::styled(
                    human_mib(t.vram_mib),
                    Style::default().fg(theme.accent),
                )),
                Cell::from(Span::styled(share, Style::default().fg(stack_color(theme, t.stack)))),
                Cell::from(t.status.clone()),
                Cell::from(pid),
            ])
        } else {
            Row::new(vec![
                Cell::from(Span::styled(t.stack.label().to_string(), stack_style)),
                Cell::from(t.name.clone()),
                Cell::from(human_mib(t.vram_mib)),
                Cell::from(t.status.clone()),
            ])
        }
    });

    let widths: Vec<Constraint> = if wide {
        vec![
            Constraint::Length(9),
            Constraint::Min(16),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(12),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(8),
        ]
    } else if medium {
        vec![
            Constraint::Length(9),
            Constraint::Min(14),
            Constraint::Length(7),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(8),
        ]
    } else {
        vec![
            Constraint::Length(9),
            Constraint::Min(12),
            Constraint::Length(7),
            Constraint::Length(8),
        ]
    };

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(theme.selected())
        .style(theme.base());
    let mut state = ratatui::widgets::TableState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(table, inner, &mut state);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let keys = Line::from({
        let mut spans = vec![Span::raw(" ")];
        for (key, label) in [
            ("j/k", "move"),
            ("u", "unload"),
            ("U", "stack"),
            ("p", "park"),
            ("l", "load"),
            ("?", "help"),
            ("q", "quit"),
        ] {
            spans.push(keycap(key, theme));
            spans.push(Span::styled(
                format!(" {label}  "),
                Style::default().fg(theme.fg_dim),
            ));
        }
        spans
    });
    frame.render_widget(Paragraph::new(keys).style(theme.status()), split[0]);

    let status = match &app.mode {
        Mode::Confirm(c) => c.prompt.clone(),
        Mode::Load(_) => "type to filter    enter load    esc close".into(),
        Mode::Help => "any key closes help".into(),
        Mode::Normal => {
            let age = poll_age(app);
            if app.status.is_empty() {
                format!("{age}")
            } else {
                format!("{}   {age}", app.status)
            }
        }
    };
    let warn = if app.over_budget(0) {
        Span::styled("  OVER BUDGET", Style::default().fg(theme.red).add_modifier(Modifier::BOLD))
    } else {
        Span::raw("")
    };
    let err = app
        .snapshot
        .ollama_error
        .as_ref()
        .or(app.snapshot.lms_error.as_ref())
        .or(app.snapshot.gpu.error.as_ref())
        .filter(|e| !is_unreachable(e));
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(status, Style::default().fg(theme.fg)),
        warn,
    ];
    if let Some(err) = err {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            short_error(err),
            Style::default().fg(theme.yellow),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.status()), split[1]);
}

fn draw_help(frame: &mut Frame, app: &App) {
    let area = centered(frame.area(), 72, 18);
    frame.render_widget(Clear, area);
    let theme = &app.theme;
    let body = vec![
        Line::from(Span::styled(
            "GPU landlord",
            Style::default().fg(theme.cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Ollama and LM Studio on one VRAM bar",
            Style::default().fg(theme.muted),
        )),
        Line::from(""),
        Line::from("j/k  move     u  unload selected     U  unload that stack"),
        Line::from("p    park the other stack's models"),
        Line::from("l    load a model onto the focused stack"),
        Line::from("o    start/stop Ollama     m  start/stop LM Studio"),
        Line::from("x    kill selected orphan  r  refresh   R  replay logo"),
        Line::from("click a tenant row to select it"),
        Line::from(""),
        Line::from(Span::styled(
            "Polling never starts LM Studio. lms runs only after its daemon pid is alive.",
            Style::default().fg(theme.muted),
        )),
        Line::from(Span::styled(
            "Budget defaults to total VRAM minus 2 GiB.",
            Style::default().fg(theme.muted),
        )),
    ];
    frame.render_widget(
        Paragraph::new(body).style(theme.base()).block(
            Block::default()
                .title(Span::styled(" help ", Style::default().fg(theme.accent)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        ),
        area,
    );
}

fn draw_load(frame: &mut Frame, app: &App, ui: &LoadUi) {
    let area = centered(frame.area(), 74, 18);
    frame.render_widget(Clear, area);
    let items = visible_load_items(ui);
    let selected = if items.is_empty() {
        0
    } else {
        ui.selected.min(items.len() - 1)
    };
    let accent = match ui.stack {
        Stack::Ollama => app.theme.cyan,
        Stack::LmStudio => app.theme.magenta,
        _ => app.theme.accent,
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("Load on {}", ui.stack.label()),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw("    "),
            Span::styled(
                format!("{} shown", items.len()),
                Style::default().fg(app.theme.muted),
            ),
        ]),
        Line::from(vec![
            Span::styled("filter ", Style::default().fg(app.theme.muted)),
            Span::styled(
                if ui.query.is_empty() {
                    "type to search".into()
                } else {
                    ui.query.clone()
                },
                Style::default().fg(app.theme.fg),
            ),
        ]),
        Line::from(""),
    ];
    if items.is_empty() {
        lines.push(Line::from(Span::styled(
            "No models",
            Style::default().fg(app.theme.yellow),
        )));
    } else {
        for (i, item) in items.iter().enumerate() {
            let marker = if i == selected { "▸ " } else { "  " };
            let style = if i == selected {
                app.theme.selected()
            } else {
                app.theme.base()
            };
            lines.push(Line::from(Span::styled(
                format!("{marker}{}  {}", item.label, item.meta),
                style,
            )));
        }
    }
    frame.render_widget(
        Paragraph::new(lines).style(app.theme.base()).block(
            Block::default()
                .title(Span::styled(" load ", Style::default().fg(accent)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(accent)),
        ),
        area,
    );
}

fn draw_confirm(frame: &mut Frame, app: &App, confirm: &Confirm) {
    let area = centered(frame.area(), 56, 8);
    frame.render_widget(Clear, area);
    let theme = &app.theme;
    let body = vec![
        Line::from(""),
        Line::from(Span::styled(
            confirm.prompt.trim_end_matches(" y/n").to_string(),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            keycap("y", theme),
            Span::styled(" yes    ", Style::default().fg(theme.green)),
            keycap("n", theme),
            Span::styled(" no", Style::default().fg(theme.red)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(body)
            .alignment(Alignment::Center)
            .style(theme.base())
            .block(
                Block::default()
                    .title(Span::styled(
                        " confirm ",
                        Style::default().fg(theme.accent),
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.accent)),
            ),
        area,
    );
}

fn keycap(key: &str, _theme: &Theme) -> Span<'static> {
    Span::styled(
        format!(" {key} "),
        Style::default()
            .bg(gradient_at(1.0))
            .fg(Color::Rgb(0xf8, 0xf8, 0xf2))
            .add_modifier(Modifier::BOLD),
    )
}

fn stack_color(theme: &Theme, stack: Stack) -> ratatui::style::Color {
    match stack {
        Stack::Ollama => theme.cyan,
        Stack::LmStudio => theme.magenta,
        Stack::Orphan => theme.red,
        Stack::Other => theme.yellow,
    }
}

fn stack_style(theme: &Theme, stack: Stack, blink: bool) -> Style {
    let color = stack_color(theme, stack);
    if stack == Stack::Orphan && !blink {
        Style::default().fg(theme.yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }
}

fn mini_bar(pct: u8, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let filled = ((pct as usize) * width) / 100;
    let mut out = String::new();
    for i in 0..width {
        out.push(if i < filled { '█' } else { '░' });
    }
    out
}

fn short_gpu(name: &str) -> String {
    name.replace("NVIDIA GeForce ", "")
        .replace("NVIDIA ", "")
        .replace(" Laptop GPU", "")
}

fn watts(w: f32) -> String {
    if w >= 10.0 {
        format!("{w:.0}W")
    } else {
        format!("{w:.1}W")
    }
}

fn temp_style(theme: &Theme, temp: u8) -> Style {
    if temp >= 80 {
        Style::default().fg(theme.red)
    } else if temp >= 70 {
        Style::default().fg(theme.yellow)
    } else {
        Style::default().fg(theme.green)
    }
}

fn poll_age(app: &App) -> String {
    match app.last_poll {
        Some(t) => {
            let s = t.elapsed().as_secs();
            if s < 2 {
                "live".into()
            } else {
                format!("{s}s ago")
            }
        }
        None => "waiting for nvidia-smi".into(),
    }
}

fn pad_line(line: Line<'static>, left: usize) -> Line<'static> {
    let mut spans = vec![Span::raw(" ".repeat(left))];
    spans.extend(line.spans);
    Line::from(spans)
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}
