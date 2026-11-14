//! Contenedores Docker.

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
pub struct Docker {
    pub state: TableState,
    pub sel: usize,
    pub tab: usize, // 0 contenedores · 1 imágenes
    /// Menú local (tecla m).
    pub menu: crate::menus::Menu,
}

/// Filas del menú local de Docker (contenedores).
const DKR_MENU: &[&str] = &[
    "iniciar",
    "detener",
    "reiniciar",
    "eliminar (no borra la imagen)",
    "cambiar a imágenes",
    "refrescar",
    "instalar docker.io con apt",
];

impl Docker {
    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let d = &ctx.docker;
        if !d.available {
            let rows = Layout::vertical([Constraint::Min(4), Constraint::Length(1)]).split(area);
            ui::hint_bar(
                f,
                t,
                rows[1],
                &[("i", "instalar docker"), ("r", "reintentar"), ("m", "menú")],
            );
            let block = ui::panel_on(t, ctx.wp, rows[0], " ▣  Contenedores ", 0.9);
            let inner = block.inner(rows[0]);
            f.render_widget(block, rows[0]);
            f.render_widget(
                ratatui::widgets::Paragraph::new(vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "  Docker no está instalado en este sistema.",
                        Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "  Nébula puede instalarlo por ti:",
                        Style::default().fg(t.fg_dim),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            "    [i] ",
                            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "instalar docker.io con apt (pide privilegios)",
                            Style::default().fg(t.fg),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            "    [r] ",
                            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("reintentar la detección", Style::default().fg(t.fg)),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        "  (también vale podman, si lo prefieres)",
                        Style::default().fg(t.fg_faint),
                    )),
                ]),
                inner,
            );
            self.menu
                .render(f, area, " ▣  Docker ", DKR_MENU, t, ctx.wp);
            return;
        }

        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(area);
        // ── Pie: los atajos, siempre a la vista ──
        ui::hint_bar(
            f,
            t,
            rows[2],
            &[
                ("espacio", "contenedores/imágenes"),
                ("↑/↓", "mover"),
                ("s", "iniciar"),
                ("x", "detener"),
                ("r", "refrescar"),
                ("m", "menú"),
            ],
        );
        let tabs = Line::from(vec![
            Span::styled(
                " ▣ contenedores ",
                Style::default()
                    .fg(if self.tab == 0 { t.accent } else { t.fg_faint })
                    .add_modifier(if self.tab == 0 {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled("│", Style::default().fg(t.border)),
            Span::styled(
                " ▤ imágenes ",
                Style::default()
                    .fg(if self.tab == 1 { t.accent } else { t.fg_faint })
                    .add_modifier(if self.tab == 1 {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled(
                format!(
                    "      servidor {} {}",
                    if d.engine.is_empty() {
                        "docker"
                    } else {
                        &d.engine
                    },
                    d.server
                ),
                Style::default().fg(t.fg_faint),
            ),
        ]);
        f.render_widget(ratatui::widgets::Paragraph::new(tabs), rows[0]);

        let widths = [
            Constraint::Percentage(24),
            Constraint::Percentage(34),
            Constraint::Percentage(26),
            Constraint::Percentage(16),
        ];

        if self.tab == 0 {
            let block = ui::panel_on(
                t,
                ctx.wp,
                rows[1],
                format!(" contenedores · {} ", d.containers.len()),
                0.8,
            );
            let inner = block.inner(rows[1]);
            f.render_widget(block, rows[1]);
            if d.containers.is_empty() {
                f.render_widget(
                    ratatui::widgets::Paragraph::new("sin contenedores · docker ps está vacío")
                        .style(Style::default().fg(t.fg_faint)),
                    inner,
                );
                return;
            }
            let rows_v: Vec<Row> = d
                .containers
                .iter()
                .map(|c| {
                    let running = c.c.to_lowercase().contains("up");
                    Row::new(vec![
                        Cell::from(format!(
                            "{} {}",
                            if running { "●" } else { "○" },
                            ui::truncate(&c.a, 22)
                        ))
                        .style(Style::default().fg(if running {
                            t.ok
                        } else {
                            t.fg_dim
                        })),
                        Cell::from(ui::truncate(&c.b, 34)).style(Style::default().fg(t.accent2)),
                        Cell::from(ui::truncate(&c.c, 24)).style(Style::default().fg(t.fg_dim)),
                        Cell::from(ui::truncate(&c.d, 16)).style(Style::default().fg(t.fg_faint)),
                    ])
                })
                .collect();
            let header = Row::new(vec!["NOMBRE", "IMAGEN", "ESTADO", "PUERTOS"])
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
            if self.sel >= d.containers.len() {
                self.sel = d.containers.len().saturating_sub(1);
            }
            self.state.select(Some(self.sel));
            f.render_stateful_widget(table, inner, &mut self.state);
        } else {
            let block = ui::panel_on(
                t,
                ctx.wp,
                rows[1],
                format!(" imágenes · {} ", d.images.len()),
                0.8,
            );
            let inner = block.inner(rows[1]);
            f.render_widget(block, rows[1]);
            let rows_v: Vec<Row> = d
                .images
                .iter()
                .map(|c| {
                    Row::new(vec![
                        Cell::from(ui::truncate(&c.a, 40)).style(Style::default().fg(t.fg)),
                        Cell::from(ui::truncate(&c.b, 16)).style(Style::default().fg(t.accent2)),
                        Cell::from(ui::truncate(&c.c, 20)).style(Style::default().fg(t.fg_faint)),
                        Cell::from(""),
                    ])
                })
                .collect();
            let header = Row::new(vec!["REPOSITORIO", "TAMAÑO", "CREADA", ""])
                .style(Style::default().fg(t.accent).add_modifier(Modifier::BOLD));
            let widths2 = [
                Constraint::Percentage(46),
                Constraint::Percentage(16),
                Constraint::Percentage(24),
                Constraint::Percentage(14),
            ];
            let table = Table::new(rows_v, widths2)
                .header(header)
                .row_highlight_style(
                    Style::default()
                        .bg(t.sel_bg)
                        .fg(t.sel_fg)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▎ ");
            if self.sel >= d.images.len() {
                self.sel = d.images.len().saturating_sub(1);
            }
            self.state.select(Some(self.sel));
            f.render_stateful_widget(table, inner, &mut self.state);
        }

        // ── Menú local (tecla m) ──
        self.menu
            .render(f, area, " ▣  Docker ", DKR_MENU, t, ctx.wp);
    }

    pub fn on_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Option<Action> {
        let len = if self.tab == 0 {
            ctx.docker.containers.len()
        } else {
            ctx.docker.images.len()
        };
        let name = || {
            if self.tab == 0 {
                ctx.docker
                    .containers
                    .get(self.sel)
                    .map(|c| c.a.clone())
                    .unwrap_or_default()
            } else {
                ctx.docker
                    .images
                    .get(self.sel)
                    .map(|c| c.a.clone())
                    .unwrap_or_default()
            }
        };

        // ── Menú local (m) ──
        if self.menu.open || matches!(key.code, KeyCode::Char('m')) {
            match self.menu.on_key(key, DKR_MENU.len()) {
                crate::menus::Nav::Pass => {}
                crate::menus::Nav::Handled => return Some(Action::Keep),
                crate::menus::Nav::Run(n) => {
                    let name = name();
                    return Some(match n {
                        0 => Action::Run(Job::Docker("start".into(), name)),
                        1 => Action::Confirm(
                            format!("¿detener el contenedor {name}?"),
                            Job::Docker("stop".into(), name),
                        ),
                        2 => Action::Run(Job::Docker("restart".into(), name)),
                        3 => Action::Confirm(
                            format!("¿eliminar el contenedor {name}? (no borra la imagen)"),
                            Job::Docker("rm".into(), name),
                        ),
                        4 => {
                            self.tab = 1;
                            self.sel = 0;
                            self.state.select(Some(0));
                            Action::Keep
                        }
                        5 => Action::Refresh,
                        _ => Action::Confirm(
                            "¿instalar docker.io con apt?".into(),
                            Job::Apt(vec!["install".into(), "-y".into(), "docker.io".into()]),
                        ),
                    });
                }
            }
        }

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
            KeyCode::Char(' ') => {
                self.tab = 1 - self.tab;
                self.sel = 0;
                self.state.select(Some(0));
                Some(Action::Keep)
            }
            KeyCode::Char('s') if self.tab == 0 => {
                Some(Action::Run(Job::Docker("start".into(), name())))
            }
            KeyCode::Char('x') if self.tab == 0 => Some(Action::Confirm(
                format!("¿detener el contenedor {}?", name()),
                Job::Docker("stop".into(), name()),
            )),
            KeyCode::Char('r') if self.tab == 0 => {
                Some(Action::Run(Job::Docker("restart".into(), name())))
            }
            KeyCode::Char('d') if self.tab == 0 => Some(Action::Confirm(
                format!("¿eliminar el contenedor {}? (no borra la imagen)", name()),
                Job::Docker("rm".into(), name()),
            )),
            KeyCode::Char('R') => Some(Action::Refresh),
            KeyCode::Char('r') => Some(Action::Refresh),
            KeyCode::Char('i') => Some(Action::Confirm(
                "¿instalar docker.io con apt?".into(),
                Job::Apt(vec!["install".into(), "-y".into(), "docker.io".into()]),
            )),
            _ => None,
        }
    }
}
