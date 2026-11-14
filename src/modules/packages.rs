//! Paquetes: apt y flatpak.

use crate::action::{Action, Job};
use crate::app::Ctx;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Row, Table, TableState};
use ratatui::Frame;

#[derive(Default)]
pub struct Packages {
    pub state: TableState,
    pub sel: usize,
    pub scroll: usize,
    /// Menú de operaciones apt (tecla m).
    pub menu: Option<usize>,
    /// Panel de texto para escribir paquetes a instalar (tecla i).
    pub install: Option<String>,
}

/// Operaciones apt del menú: (etiqueta, subcomando apt).
const APT_MENU: &[(&str, &str)] = &[
    ("actualizar lista", "update"),
    ("mejorar instalados (upgrade)", "upgrade -y"),
    ("todo lo nuevo (full-upgrade)", "full-upgrade -y"),
    ("quitar el seleccionado", "REMOVE"),
    ("limpiar huérfanos (autoremove)", "autoremove -y"),
    ("vaciar caché (clean)", "clean"),
];

impl Packages {
    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let p = &ctx.pkgs;
        let rows = Layout::vertical([
            Constraint::Length(3),
            Constraint::Percentage(45),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(area);
        // Fila del medio: a la izquierda las operaciones apt, a la derecha
        // los flatpaks.
        let mid =
            Layout::horizontal([Constraint::Percentage(46), Constraint::Min(30)]).split(rows[2]);

        // ── Resumen ──
        let block = ui::panel_on(t, ctx.wp, rows[0], " ⬡  Paquetes ", 0.8);
        let inner = block.inner(rows[0]);
        f.render_widget(block, rows[0]);
        let summary = Line::from(vec![
            Span::styled("apt ", Style::default().fg(t.accent)),
            Span::styled(
                format!(
                    "{} instalados · {} por actualizar · refresco {}",
                    p.installed,
                    p.upgradable.len(),
                    if p.last_run.is_empty() {
                        "pendiente"
                    } else {
                        &p.last_run
                    }
                ),
                Style::default().fg(t.fg),
            ),
            Span::styled("     flatpak ", Style::default().fg(t.accent2)),
            Span::styled(
                format!("{} apps", p.flatpaks.len()),
                Style::default().fg(t.fg),
            ),
        ]);
        f.render_widget(ratatui::widgets::Paragraph::new(summary), inner);

        // ── Actualizaciones ──
        let block = ui::panel_on(
            t,
            ctx.wp,
            rows[1],
            format!(" ↑  Actualizaciones · {} ", p.upgradable.len()),
            0.8,
        );
        let inner = block.inner(rows[1]);
        f.render_widget(block, rows[1]);
        if p.upgradable.is_empty() {
            let msg = if p.apt_ok {
                "todo actualizado ✦"
            } else {
                "apt no disponible en este sistema"
            };
            f.render_widget(
                ratatui::widgets::Paragraph::new(msg).style(Style::default().fg(t.ok)),
                inner,
            );
        } else {
            let widths = [
                Constraint::Percentage(40),
                Constraint::Percentage(30),
                Constraint::Percentage(30),
            ];
            let items: Vec<&(String, String)> = p.upgradable.iter().skip(self.scroll).collect();
            let rows_v: Vec<Row> = items
                .iter()
                .take(inner.height as usize)
                .map(|(name, ver)| {
                    Row::new(vec![
                        Cell::from(ui::truncate(name, 36)).style(Style::default().fg(t.fg)),
                        Cell::from(ui::truncate(ver, 30)).style(Style::default().fg(t.accent2)),
                        Cell::from("").style(Style::default()),
                    ])
                })
                .collect();
            let header = Row::new(vec!["PAQUETE", "NUEVA VERSIÓN", ""])
                .style(Style::default().fg(t.accent).add_modifier(Modifier::BOLD));
            let table = Table::new(rows_v, widths)
                .header(header)
                .row_highlight_style(
                    Style::default()
                        .bg(t.sel_bg)
                        .fg(t.sel_fg)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▎ ");
            if self.sel >= p.upgradable.len() {
                self.sel = p.upgradable.len().saturating_sub(1);
            }
            self.state.select(Some(self.sel));
            f.render_stateful_widget(table, inner, &mut self.state);
        }

        // ── Operaciones apt: siempre a la vista; la tecla m las enfoca ──
        let n_ops = APT_MENU.len() + 2;
        let title = if self.menu.is_some() {
            " ⬡  Operaciones apt · eligiendo ".to_string()
        } else {
            " ⬡  Operaciones apt ".to_string()
        };
        let block = ui::panel_on(t, ctx.wp, mid[0], title, 0.84);
        let inner0 = block.inner(mid[0]);
        f.render_widget(block, mid[0]);
        let mut ops: Vec<Line> = Vec::new();
        // Las 6 operaciones apt + las dos filas extra (i y f).
        let mut rows_ops: Vec<(&str, &str)> =
            APT_MENU.iter().map(|(label, _)| ("·", *label)).collect();
        rows_ops.push(("＋", "instalar paquetes (i)"));
        rows_ops.push(("＋", "actualizar flatpaks (f)"));
        for (i, (icon, label)) in rows_ops.iter().take(n_ops).enumerate() {
            let s = self.menu == Some(i);
            ops.push(Line::from(vec![
                Span::styled(
                    format!(" {} ", if s { "▸" } else { icon }),
                    Style::default()
                        .fg(if s { t.accent } else { t.fg_faint })
                        .add_modifier(if s { Modifier::BOLD } else { Modifier::empty() }),
                ),
                Span::styled(
                    format!("{:<28}", label),
                    Style::default()
                        .fg(if s { t.fg } else { t.fg_dim })
                        .add_modifier(if s { Modifier::BOLD } else { Modifier::empty() }),
                ),
            ]));
        }
        f.render_widget(ratatui::widgets::Paragraph::new(ops), inner0);

        // ── Flatpak ──
        let block = ui::panel_on(
            t,
            ctx.wp,
            mid[1],
            format!(" ⬢  Flatpak · {} ", p.flatpaks.len()),
            0.8,
        );
        let inner = block.inner(mid[1]);
        f.render_widget(block, mid[1]);
        let mut lines = Vec::new();
        for (name, ver) in p
            .flatpaks
            .iter()
            .skip(self.scroll)
            .take(inner.height as usize)
        {
            lines.push(Line::from(vec![
                Span::styled(" ▸ ", Style::default().fg(t.accent2)),
                Span::styled(
                    ui::truncate(name, (inner.width as usize).saturating_sub(20)),
                    Style::default().fg(t.fg),
                ),
                Span::styled(format!(" {ver}"), Style::default().fg(t.fg_faint)),
            ]));
        }
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "sin flatpaks de usuario",
                Style::default().fg(t.fg_faint),
            )));
        }
        f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);

        // ── Pie: los atajos, siempre a la vista ──
        ui::hint_bar(
            f,
            t,
            rows[3],
            &[
                ("↑/↓", "paquetes"),
                ("m", "operaciones apt"),
                ("i", "instalar"),
                ("f", "flatpak"),
                ("r", "refrescar"),
            ],
        );

        // ── Panel de instalación (tecla i) ──
        if let Some(text) = &self.install {
            self.render_install(f, area, text, ctx);
        }
    }

    /// Ventana para escribir los paquetes que faltan instalar.
    fn render_install(&self, f: &mut Frame, area: Rect, text: &str, ctx: &Ctx) {
        let t = ctx.theme;
        let w = (area.width * 66 / 100).clamp(46, 84).min(area.width);
        let h = 8.min(area.height);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(w)) / 2,
            y: area.y + (area.height.saturating_sub(h)) / 2,
            width: w,
            height: h,
        };
        let buf = f.buffer_mut();
        ui::dim_area(buf, area, 0.55);
        let block = ui::panel_on(t, ctx.wp, rect, " ⬡  Instalar con apt ", 0.97);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        let lines = vec![
            Line::from(Span::styled(
                "  nombres separados por espacios (apt install -y)",
                Style::default().fg(t.fg_faint),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  ▸ ", Style::default().fg(t.accent)),
                Span::styled(
                    format!("{text}▏"),
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  intro instalar · esc cancelar",
                Style::default().fg(t.fg_faint),
            )),
        ];
        f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
    }

    pub fn on_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Option<Action> {
        // ── Panel de instalación abierto ──
        if let Some(text) = self.install.take() {
            return match key.code {
                KeyCode::Esc => Some(Action::Keep),
                KeyCode::Backspace => {
                    let mut t = text;
                    t.pop();
                    self.install = Some(t);
                    Some(Action::Keep)
                }
                KeyCode::Enter => {
                    let pkgs: Vec<String> =
                        text.split_whitespace().map(|s| s.to_string()).collect();
                    if pkgs.is_empty() {
                        return Some(Action::Toast("escribe al menos un paquete".into()));
                    }
                    Some(Action::Run(Job::Apt({
                        let mut args = vec!["install".to_string(), "-y".to_string()];
                        args.extend(pkgs);
                        args
                    })))
                }
                KeyCode::Char(c) if !c.is_control() => {
                    let mut t = text;
                    t.push(c);
                    self.install = Some(t);
                    Some(Action::Keep)
                }
                _ => {
                    self.install = Some(text);
                    Some(Action::Keep)
                }
            };
        }

        // ── Panel de operaciones apt enfocado (m) ──
        if let Some(sel) = self.menu {
            let n_ops = APT_MENU.len() + 2;
            return match key.code {
                KeyCode::Esc => {
                    self.menu = None;
                    Some(Action::Keep)
                }
                KeyCode::Up => {
                    self.menu = Some(sel.saturating_sub(1));
                    Some(Action::Keep)
                }
                KeyCode::Down => {
                    self.menu = Some((sel + 1).min(n_ops - 1));
                    Some(Action::Keep)
                }
                KeyCode::Enter => {
                    self.menu = None;
                    if sel < APT_MENU.len() {
                        self.apt_action(sel, ctx)
                    } else if sel == APT_MENU.len() {
                        // instalar paquetes escritos a mano
                        self.install = Some(String::new());
                        Some(Action::Keep)
                    } else {
                        // actualizar todos los flatpaks
                        Some(Action::Confirm(
                            "¿actualizar todos los flatpaks?".into(),
                            Job::View {
                                title: "flatpak update".into(),
                                cwd: String::new(),
                                cmd: "flatpak update -y".into(),
                            },
                        ))
                    }
                }
                _ => None,
            };
        }

        let len = ctx.pkgs.upgradable.len();
        match key.code {
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
                self.scroll = self.scroll.saturating_sub(5);
                Some(Action::Keep)
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(5);
                Some(Action::Keep)
            }
            // Menú de operaciones (update · upgrade · quitar · limpiar…).
            KeyCode::Char('m') | KeyCode::Char('a') => {
                self.menu = Some(0);
                Some(Action::Keep)
            }
            KeyCode::Char('u') => Some(Action::Confirm(
                "¿apt update && apt full-upgrade?".into(),
                Job::Privileged(vec![
                    "sh".into(),
                    "-c".into(),
                    "apt update && apt full-upgrade -y".into(),
                ]),
            )),
            KeyCode::Char('U') => {
                if let Some((name, _)) = ctx.pkgs.upgradable.get(self.sel) {
                    return Some(Action::Confirm(
                        format!("¿actualizar {name}?"),
                        Job::Apt(vec!["install".into(), "-y".into(), name.clone()]),
                    ));
                }
                Some(Action::Keep)
            }
            KeyCode::Char('f') => Some(Action::Confirm(
                "¿actualizar todos los flatpaks?".into(),
                Job::View {
                    title: "flatpak update".into(),
                    cwd: String::new(),
                    cmd: "flatpak update -y".into(),
                },
            )),
            KeyCode::Char('r') => Some(Action::Refresh),
            // Escribir paquetes aquí dentro: la salida va al visor.
            KeyCode::Char('i') => {
                self.install = Some(String::new());
                Some(Action::Keep)
            }
            _ => None,
        }
    }

    /// Traduce la fila del menú apt a una acción con confirmación.
    fn apt_action(&self, sel: usize, ctx: &Ctx) -> Option<Action> {
        let (label, sub) = APT_MENU.get(sel)?;
        // «quitar el seleccionado» necesita un paquete de la lista.
        if *sub == "REMOVE" {
            return match ctx.pkgs.upgradable.get(self.sel) {
                Some((name, _)) => Some(Action::Confirm(
                    format!("¿quitar {name}? (pide privilegios)"),
                    Job::Apt(vec!["remove".into(), "-y".into(), name.clone()]),
                )),
                None => Some(Action::Toast("no hay ningún paquete seleccionado".into())),
            };
        }
        let args: Vec<String> = sub.split_whitespace().map(|s| s.to_string()).collect();
        Some(Action::Confirm(
            format!("¿{label}? (pide privilegios)"),
            Job::Apt(args),
        ))
    }
}
