//! Sistema: CPU, memoria, discos, temperaturas y procesos.

use crate::action::{Action, Job};
use crate::app::Ctx;
use crate::modules::home::{level_color, uptime};
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Gauge, Row, Table, TableState};
use ratatui::Frame;

/// Filas del menú local de Sistema (tecla m).
const SYS_MENU: &[&str] = &[
    "cambiar orden (cpu · mem · pid · nombre)",
    "matar el proceso seleccionado",
    "refrescar",
];

#[derive(Clone, Copy, PartialEq)]
pub enum Sort {
    Cpu,
    Mem,
    Pid,
    Name,
}

impl Sort {
    fn next(self) -> Self {
        match self {
            Sort::Cpu => Sort::Mem,
            Sort::Mem => Sort::Pid,
            Sort::Pid => Sort::Name,
            Sort::Name => Sort::Cpu,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Sort::Cpu => "cpu",
            Sort::Mem => "mem",
            Sort::Pid => "pid",
            Sort::Name => "nombre",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Filter {
    #[default]
    Todos,
    /// Con más de 1% de CPU.
    Activos,
    /// Sólo los míos.
    Mios,
    /// Nombre o comando que contiene el texto escrito.
    Buscar,
}

impl Filter {
    fn next(self) -> Self {
        match self {
            Filter::Todos => Filter::Activos,
            Filter::Activos => Filter::Mios,
            Filter::Mios => Filter::Buscar,
            Filter::Buscar => Filter::Todos,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Filter::Todos => "todos",
            Filter::Activos => "cpu >1%",
            Filter::Mios => "míos",
            Filter::Buscar => "buscar",
        }
    }
}

pub struct SystemMod {
    pub state: TableState,
    pub sort: Sort,
    pub cpu_hist: Vec<u64>,
    pub sel: usize,
    /// Menú local (tecla m): orden · matar · refrescar.
    pub menu: crate::menus::Menu,
    /// Filtro de la lista de procesos (tecla f).
    filter: Filter,
    /// Texto del filtro «buscar».
    query: String,
    /// ¿Se está escribiendo el filtro?
    typing: bool,
}

impl Default for SystemMod {
    fn default() -> Self {
        SystemMod {
            state: TableState::default(),
            sort: Sort::Cpu,
            cpu_hist: Vec::new(),
            sel: 0,
            menu: crate::menus::Menu::default(),
            filter: Filter::Todos,
            query: String::new(),
            typing: false,
        }
    }
}

impl SystemMod {
    pub fn on_sys(&mut self, cpu: f32) {
        self.cpu_hist.push(cpu as u64);
        if self.cpu_hist.len() > 120 {
            self.cpu_hist.remove(0);
        }
    }

    fn sorted<'a>(&self, ctx: &'a Ctx<'a>) -> Vec<&'a crate::data::ProcInfo> {
        let mut v: Vec<&crate::data::ProcInfo> = ctx.sys.procs.iter().collect();
        match self.sort {
            Sort::Cpu => v.sort_by(|a, b| {
                b.cpu
                    .partial_cmp(&a.cpu)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            Sort::Mem => v.sort_by_key(|p| std::cmp::Reverse(p.mem)),
            Sort::Pid => v.sort_by_key(|a| a.pid),
            Sort::Name => v.sort_by_key(|a| a.name.to_lowercase()),
        }
        v
    }

    /// Procesos que pasan el filtro, ya ordenados.
    fn visible<'a>(&self, ctx: &'a Ctx<'a>) -> Vec<&'a crate::data::ProcInfo> {
        let q = self.query.to_lowercase();
        let me = std::env::var("USER").unwrap_or_default().to_lowercase();
        self.sorted(ctx)
            .into_iter()
            .filter(|p| match self.filter {
                Filter::Todos => true,
                Filter::Activos => p.cpu > 1.0,
                Filter::Mios => !me.is_empty() && p.user.to_lowercase() == me,
                Filter::Buscar => {
                    q.is_empty()
                        || p.name.to_lowercase().contains(&q)
                        || p.cmd.to_lowercase().contains(&q)
                }
            })
            .collect()
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let s = ctx.sys;
        if area.width < 60 {
            f.render_widget(
                ratatui::widgets::Paragraph::new("terminal demasiado estrecho"),
                area,
            );
            return;
        }
        let lay = Layout::vertical([Constraint::Min(4), Constraint::Length(1)]).split(area);
        let cols = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(lay[0]);
        let left = Layout::vertical([
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Min(5),
        ])
        .split(cols[0]);

        // ── CPU ──
        let block = ui::panel_on(t, ctx.wp, left[0], " ◉  CPU ", 0.8);
        let inner = block.inner(left[0]);
        f.render_widget(block, left[0]);
        if inner.height >= 2 {
            let ratio = (s.cpu / 100.0).clamp(0.0, 1.0);
            let col = level_color(t, ratio);
            f.render_widget(
                Gauge::default()
                    .gauge_style(Style::default().fg(col).bg(t.bg_elevated))
                    .label(format!("{:.1}%", s.cpu))
                    .ratio(ratio as f64),
                Rect {
                    x: inner.x,
                    y: inner.y,
                    width: inner.width,
                    height: 1,
                },
            );
            // historial
            if self.cpu_hist.len() > 2 && inner.height > 4 {
                let hist: Vec<u64> = ui::spark(&self.cpu_hist, inner.width as usize);
                f.render_widget(
                    ratatui::widgets::Sparkline::default()
                        .data(&hist)
                        .style(Style::default().fg(t.accent)),
                    Rect {
                        x: inner.x,
                        y: inner.y + 1,
                        width: inner.width,
                        height: 2,
                    },
                );
            }
            // núcleos
            let cores = &s.per_cpu;
            let y0 = inner.y + if self.cpu_hist.len() > 2 { 3 } else { 1 };
            // la última fila queda reservada para el modelo de CPU
            let y_end = if s.cpu_model.is_empty() {
                inner.bottom()
            } else {
                inner.bottom().saturating_sub(1)
            };
            let per_col = if cores.len() > 8 { 2 } else { 1 };
            let col_w = inner.width / per_col as u16;
            for (i, c) in cores.iter().take(16).enumerate() {
                let cx = inner.x + ((i % per_col) as u16) * col_w;
                let cy = y0 + (i / per_col) as u16;
                if cy >= y_end {
                    break;
                }
                let label = format!("{:>2}", i + 1);
                let barw = col_w.saturating_sub(5) as usize;
                let line = format!("{} {}", label, ui::thin_bar(*c / 100.0, barw));
                f.render_widget(
                    ratatui::widgets::Paragraph::new(line)
                        .style(Style::default().fg(if *c > 85.0 { t.warn } else { t.fg_dim })),
                    Rect {
                        x: cx,
                        y: cy,
                        width: col_w,
                        height: 1,
                    },
                );
            }
            if !s.cpu_model.is_empty() {
                let y = inner.bottom().saturating_sub(1);
                if y > inner.y {
                    f.render_widget(
                        ratatui::widgets::Paragraph::new(format!(
                            "{} · {:.0} MHz",
                            ui::truncate(&s.cpu_model, (inner.width as usize).saturating_sub(12)),
                            s.cpu_freq
                        ))
                        .style(Style::default().fg(t.fg_faint)),
                        Rect {
                            x: inner.x,
                            y,
                            width: inner.width,
                            height: 1,
                        },
                    );
                }
            }
        }

        // ── Memoria ──
        let block = ui::panel_on(t, ctx.wp, left[1], " ▤  Memoria ", 0.8);
        let inner = block.inner(left[1]);
        f.render_widget(block, left[1]);
        // histórico de memoria (últimas 90 muestras)
        if inner.height >= 6 && s.mem_hist.len() > 2 {
            let hist: Vec<u64> = ui::spark(&s.mem_hist, inner.width as usize);
            f.render_widget(
                ratatui::widgets::Sparkline::default().data(&hist).style(
                    Style::default().fg(level_color(t, *hist.last().unwrap_or(&0) as f32 / 100.0)),
                ),
                Rect {
                    x: inner.x,
                    y: inner.bottom().saturating_sub(3),
                    width: inner.width,
                    height: 2,
                },
            );
        }
        if inner.height >= 3 {
            let mp = if s.mem_total > 0 {
                s.mem_used as f64 / s.mem_total as f64
            } else {
                0.0
            };
            let sp = if s.swap_total > 0 {
                s.swap_used as f64 / s.swap_total as f64
            } else {
                0.0
            };
            f.render_widget(
                Gauge::default()
                    .gauge_style(
                        Style::default()
                            .fg(level_color(t, mp as f32))
                            .bg(t.bg_elevated),
                    )
                    .label(format!(
                        "{} / {}",
                        ui::human_bytes(s.mem_used),
                        ui::human_bytes(s.mem_total)
                    ))
                    .ratio(mp),
                Rect {
                    x: inner.x,
                    y: inner.y,
                    width: inner.width,
                    height: 1,
                },
            );
            f.render_widget(
                ratatui::widgets::Paragraph::new(format!(
                    "swap {} / {}",
                    ui::human_bytes(s.swap_used),
                    ui::human_bytes(s.swap_total)
                ))
                .style(Style::default().fg(t.fg_dim)),
                Rect {
                    x: inner.x,
                    y: inner.y + 2,
                    width: inner.width,
                    height: 1,
                },
            );
            if s.swap_total > 0 {
                f.render_widget(
                    ratatui::widgets::Paragraph::new(ui::thin_bar(sp as f32, inner.width as usize))
                        .style(Style::default().fg(level_color(t, sp as f32))),
                    Rect {
                        x: inner.x,
                        y: inner.y + 3,
                        width: inner.width,
                        height: 1,
                    },
                );
            }
        }

        // ── Info + temperaturas ──
        let block = ui::panel_on(t, ctx.wp, left[2], " ✦  Estado ", 0.8);
        let inner = block.inner(left[2]);
        f.render_widget(block, left[2]);
        let mut info = vec![
            line_kv(t, "uptime", uptime(s.uptime)),
            line_kv(
                t,
                "carga",
                format!("{:.2} {:.2} {:.2}", s.load[0], s.load[1], s.load[2]),
            ),
        ];
        if !s.kernel.is_empty() {
            info.push(line_kv(t, "kernel", ui::truncate(&s.kernel, 24)));
        }
        if !s.gpu.is_empty() {
            info.push(line_kv(t, "gpu", ui::truncate(&s.gpu, 24)));
        }
        for (label, temp) in s.temps.iter().take(2) {
            info.push(line_kv(
                t,
                &ui::truncate(label, 10),
                format!("{temp:.0} °C"),
            ));
        }
        f.render_widget(ratatui::widgets::Paragraph::new(info), inner);

        // ── Discos ──
        let block = ui::panel_on(t, ctx.wp, left[3], " ▣  Discos ", 0.8);
        let inner = block.inner(left[3]);
        f.render_widget(block, left[3]);
        let mut dlines = Vec::new();
        for d in s.disks.iter().take(inner.height as usize) {
            let used = d.total.saturating_sub(d.avail);
            let p = if d.total > 0 {
                used as f32 / d.total as f32
            } else {
                0.0
            };
            dlines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled(
                    format!("{:>7} ", ui::human_bytes(d.total)),
                    Style::default().fg(t.fg_faint),
                ),
                ratatui::text::Span::styled(
                    ui::bar(p, (inner.width as usize).saturating_sub(16)),
                    Style::default().fg(level_color(t, p)),
                ),
            ]));
            let dev = if d.name.is_empty() {
                String::new()
            } else {
                format!(" · {}", ui::truncate(&d.name, 12))
            };
            dlines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                format!("  {} · {}{}", ui::truncate(&d.mount, 22), d.fs, dev),
                Style::default().fg(t.fg_dim),
            )));
        }
        if dlines.is_empty() {
            dlines.push(ratatui::text::Line::from("—"));
        }
        f.render_widget(ratatui::widgets::Paragraph::new(dlines), inner);

        // ── Procesos ──
        self.processes(f, cols[1], ctx);

        // ── Menú local (tecla m) ──
        self.menu
            .render(f, area, " ▣  Sistema ", SYS_MENU, t, ctx.wp);

        // ── Pie: los atajos, siempre a la vista ──
        ui::hint_bar(
            f,
            t,
            lay[1],
            &[
                ("s", "orden"),
                ("f", "filtro"),
                ("/", "buscar nombre"),
                ("k", "matar proceso"),
                ("r", "refrescar"),
                ("m", "menú"),
            ],
        );
    }

    fn processes(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let list = self.visible(ctx);
        let filtro = if self.typing {
            format!("buscar «{}»▏", self.query)
        } else if self.filter == Filter::Buscar && !self.query.is_empty() {
            format!("«{}»", self.query)
        } else {
            self.filter.label().to_string()
        };
        let title = format!(
            " ⚙  Procesos · orden: {} · filtro: {} ",
            self.sort.label(),
            filtro
        );
        let block = ui::panel_on(t, ctx.wp, area, title, 0.8);
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.height < 3 {
            return;
        }
        if list.is_empty() {
            f.render_widget(
                ratatui::widgets::Paragraph::new("ningún proceso pasa el filtro")
                    .style(Style::default().fg(t.fg_faint)),
                inner,
            );
            return;
        }

        let widths = [
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Min(10),
        ];
        // El que más CPU se está comiendo, realzado en su fila.
        let top = list
            .iter()
            .max_by(|a, b| {
                a.cpu
                    .partial_cmp(&b.cpu)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.pid);
        let mem_total = ctx.sys.mem_total;
        let rows: Vec<Row> = list
            .iter()
            .map(|p| {
                let is_top = Some(p.pid) == top && p.cpu > 0.0;
                let cpu_col = level_color(t, p.cpu / 100.0);
                let mem_p = if mem_total > 0 {
                    (p.mem as f32 / mem_total as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let style = if is_top {
                    Style::default()
                        .bg(t.bg_elevated)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                Row::new(vec![
                    Cell::from(format!("{}", p.pid)).style(Style::default().fg(t.fg_faint)),
                    Cell::from(ui::truncate(&p.user, 7)).style(Style::default().fg(t.fg_dim)),
                    // CPU con su micro-barra.
                    Cell::from(Line::from(vec![
                        Span::styled(format!("{:>5.1}", p.cpu), Style::default().fg(cpu_col)),
                        Span::styled(
                            format!(" {}", ui::thin_bar(p.cpu / 100.0, 4)),
                            Style::default().fg(cpu_col),
                        ),
                    ])),
                    // Memoria con su micro-barra.
                    Cell::from(Line::from(vec![
                        Span::styled(
                            format!("{:>7}", ui::human_bytes(p.mem)),
                            Style::default().fg(t.fg_dim),
                        ),
                        Span::styled(
                            format!(" {}", ui::thin_bar(mem_p, 4)),
                            Style::default().fg(level_color(t, mem_p)),
                        ),
                    ])),
                    Cell::from(ui::truncate(&p.name, 40)).style(Style::default().fg(t.fg)),
                ])
                .style(style)
            })
            .collect();

        let header = Row::new(vec!["PID", "USUARIO", "CPU", "MEMORIA", "PROCESO"])
            .style(Style::default().fg(t.accent).add_modifier(Modifier::BOLD));

        let table = Table::new(rows, widths)
            .header(header)
            .row_highlight_style(
                Style::default()
                    .bg(t.sel_bg)
                    .fg(t.sel_fg)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▎ ")
            .column_spacing(1);

        if self.sel >= list.len() {
            self.sel = list.len().saturating_sub(1);
        }
        self.state.select(if list.is_empty() {
            None
        } else {
            Some(self.sel)
        });

        // pie: comando del proceso seleccionado (usa `cmd` y `status`)
        let footer_h = if list.get(self.sel).is_some() && inner.height > 6 {
            1
        } else {
            0
        };
        let table_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height.saturating_sub(footer_h),
        };
        f.render_stateful_widget(table, table_area, &mut self.state);

        if footer_h == 1 {
            if let Some(p) = list.get(self.sel) {
                let detail = format!(
                    "{} · {} · {}",
                    p.pid,
                    p.status.to_lowercase(),
                    if p.cmd.is_empty() {
                        p.name.as_str()
                    } else {
                        p.cmd.as_str()
                    }
                );
                f.render_widget(
                    ratatui::widgets::Paragraph::new(ratatui::text::Line::from(
                        ratatui::text::Span::styled(
                            ui::truncate(&detail, (inner.width as usize).saturating_sub(2)),
                            Style::default().fg(t.fg_faint),
                        ),
                    )),
                    Rect {
                        x: inner.x,
                        y: inner.bottom().saturating_sub(1),
                        width: inner.width,
                        height: 1,
                    },
                );
            }
        }
    }

    pub fn on_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Option<Action> {
        // ── Escribiendo el filtro ──
        if self.typing {
            return match key.code {
                KeyCode::Esc => {
                    self.typing = false;
                    self.filter = Filter::Todos;
                    self.query.clear();
                    Some(Action::Keep)
                }
                KeyCode::Enter => {
                    self.typing = false;
                    Some(Action::Keep)
                }
                KeyCode::Backspace => {
                    self.query.pop();
                    Some(Action::Keep)
                }
                KeyCode::Char(c) if !c.is_control() => {
                    self.query.push(c);
                    self.sel = 0;
                    Some(Action::Keep)
                }
                _ => Some(Action::Keep),
            };
        }

        // ── Menú local (m) ──
        if self.menu.open || matches!(key.code, KeyCode::Char('m')) {
            match self.menu.on_key(key, SYS_MENU.len()) {
                crate::menus::Nav::Pass => {}
                crate::menus::Nav::Handled => return Some(Action::Keep),
                crate::menus::Nav::Run(n) => return self.menu_run(n, ctx),
            }
        }

        let len = self.visible(ctx).len();
        match key.code {
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => None,
            KeyCode::Up => {
                self.sel = self.sel.saturating_sub(1);
                Some(Action::Keep)
            }
            KeyCode::Down => {
                if len > 0 {
                    self.sel = (self.sel + 1).min(len - 1);
                }
                Some(Action::Keep)
            }
            KeyCode::PageUp => {
                self.sel = self.sel.saturating_sub(10);
                Some(Action::Keep)
            }
            KeyCode::PageDown => {
                if len > 0 {
                    self.sel = (self.sel + 10).min(len - 1);
                }
                Some(Action::Keep)
            }
            KeyCode::Home => {
                self.sel = 0;
                Some(Action::Keep)
            }
            KeyCode::End => {
                self.sel = len.saturating_sub(1);
                Some(Action::Keep)
            }
            KeyCode::Char('s') => {
                self.sort = self.sort.next();
                Some(Action::Toast(format!("orden por {}", self.sort.label())))
            }
            // Ciclo de filtros: todos · cpu>1% · míos · buscar nombre.
            KeyCode::Char('f') => {
                self.filter = self.filter.next();
                self.sel = 0;
                self.query.clear();
                if self.filter == Filter::Buscar {
                    self.typing = true;
                }
                Some(Action::Keep)
            }
            KeyCode::Char('/') => {
                self.filter = Filter::Buscar;
                self.typing = true;
                self.query.clear();
                self.sel = 0;
                Some(Action::Keep)
            }
            KeyCode::Char('k') => {
                let list = self.visible(ctx);
                if let Some(p) = list.get(self.sel) {
                    let (pid, name) = (p.pid, p.name.clone());
                    return Some(Action::Confirm(
                        format!("¿Matar el proceso {name} ({pid})?"),
                        Job::Kill(pid),
                    ));
                }
                Some(Action::Keep)
            }
            KeyCode::Char('r') => Some(Action::Refresh),
            _ => None,
        }
    }

    /// Acciones del menú local.
    fn menu_run(&mut self, n: usize, ctx: &Ctx) -> Option<Action> {
        match n {
            0 => {
                self.sort = self.sort.next();
                Some(Action::Toast(format!("orden por {}", self.sort.label())))
            }
            1 => {
                let list = self.visible(ctx);
                if let Some(p) = list.get(self.sel) {
                    let (pid, name) = (p.pid, p.name.clone());
                    return Some(Action::Confirm(
                        format!("¿Matar el proceso {name} ({pid})?"),
                        Job::Kill(pid),
                    ));
                }
                Some(Action::Toast("ningún proceso seleccionado".into()))
            }
            _ => Some(Action::Refresh),
        }
    }
}

fn line_kv(t: &crate::config::Theme, k: &str, v: String) -> ratatui::text::Line<'static> {
    ratatui::text::Line::from(vec![
        ratatui::text::Span::styled(format!("{k:<9} "), Style::default().fg(t.fg_faint)),
        ratatui::text::Span::styled(v, Style::default().fg(t.fg)),
    ])
}
