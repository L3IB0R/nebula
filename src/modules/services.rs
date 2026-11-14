//! Servicios systemd.

use crate::action::{Action, Job};
use crate::app::Ctx;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Cell, Row, Table, TableState};
use ratatui::Frame;

#[derive(Default)]
pub struct Services {
    pub state: TableState,
    pub sel: usize,
    pub filter: String,
    pub filtering: bool,
    /// Menú local (tecla m): iniciar · detener · reiniciar · refrescar.
    pub menu: crate::menus::Menu,
}

/// Filas del menú local de Servicios.
const SVC_MENU: &[&str] = &[
    "iniciar el seleccionado",
    "detener el seleccionado",
    "reiniciar el seleccionado",
    "refrescar la lista",
];

impl Services {
    fn filtered<'a>(&self, ctx: &'a Ctx<'a>) -> Vec<&'a crate::data::ServiceInfo> {
        let f = self.filter.to_lowercase();
        ctx.services
            .iter()
            .filter(|s| {
                f.is_empty()
                    || s.name.to_lowercase().contains(&f)
                    || s.desc.to_lowercase().contains(&f)
            })
            .collect()
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let list = self.filtered(ctx);
        let running = ctx.services.iter().filter(|s| s.sub == "running").count();

        let rows = Layout::vertical([Constraint::Min(4), Constraint::Length(1)]).split(area);
        // ── Pie: los atajos, siempre a la vista ──
        ui::hint_bar(
            f,
            t,
            rows[1],
            &[
                ("↑/↓", "mover"),
                ("s", "iniciar"),
                ("x", "detener"),
                ("/", "filtrar"),
                ("r", "refrescar"),
                ("m", "menú"),
            ],
        );

        let title = if self.filtering {
            format!(" ⚙  Servicios · buscar: {}▏", self.filter)
        } else {
            format!(
                " ⚙  Servicios · {} en marcha de {} · / filtrar ",
                running,
                ctx.services.len()
            )
        };
        let block = ui::panel_on(t, ctx.wp, rows[0], title, 0.8);
        let inner = block.inner(rows[0]);
        f.render_widget(block, rows[0]);
        if inner.height < 3 {
            return;
        }

        if list.is_empty() {
            let msg = if ctx.services.is_empty() {
                "leyendo systemctl… pulsa r para refrescar"
            } else {
                "ningún servicio coincide con el filtro"
            };
            f.render_widget(
                ratatui::widgets::Paragraph::new(msg).style(Style::default().fg(t.fg_dim)),
                inner,
            );
            self.menu
                .render(f, area, " ⚙  Servicios ", SVC_MENU, t, ctx.wp);
            return;
        }

        let widths = [
            Constraint::Percentage(26),
            Constraint::Percentage(9),
            Constraint::Percentage(11),
            Constraint::Percentage(11),
            Constraint::Percentage(43),
        ];
        let rows_v: Vec<Row> = list
            .iter()
            .take(inner.height.saturating_sub(1) as usize)
            .map(|s| {
                let color = match s.sub.as_str() {
                    "running" => t.ok,
                    "exited" | "dead" => t.fg_faint,
                    "waiting" => t.warn,
                    _ => t.accent2,
                };
                Row::new(vec![
                    Cell::from(ui::truncate(&s.name, 34)).style(Style::default().fg(t.fg)),
                    Cell::from(ui::truncate(&s.load, 8)).style(Style::default().fg(t.fg_faint)),
                    Cell::from(ui::truncate(&s.active, 9)).style(Style::default().fg(color)),
                    Cell::from(ui::truncate(&s.sub, 10)).style(Style::default().fg(color)),
                    Cell::from(ui::truncate(&s.desc, 60)).style(Style::default().fg(t.fg_dim)),
                ])
            })
            .collect();
        let header = Row::new(vec!["SERVICIO", "CARGA", "ACTIVO", "SUB", "DESCRIPCIÓN"])
            .style(Style::default().fg(t.accent).add_modifier(Modifier::BOLD));
        let table = Table::new(rows_v, widths)
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
        f.render_stateful_widget(table, inner, &mut self.state);

        // ── Menú local (tecla m) ──
        self.menu
            .render(f, area, " ⚙  Servicios ", SVC_MENU, t, ctx.wp);
    }

    pub fn on_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Option<Action> {
        let list = self.filtered(ctx);
        let len = list.len();

        if self.filtering {
            return match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.filtering = false;
                    Some(Action::Keep)
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                    Some(Action::Keep)
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    Some(Action::Keep)
                }
                _ => Some(Action::Keep),
            };
        }

        let selected = || {
            list.get(self.sel)
                .map(|s| s.name.clone())
                .unwrap_or_default()
        };

        // ── Menú local (m) ──
        if self.menu.open || matches!(key.code, KeyCode::Char('m')) {
            match self.menu.on_key(key, SVC_MENU.len()) {
                crate::menus::Nav::Pass => {}
                crate::menus::Nav::Handled => return Some(Action::Keep),
                crate::menus::Nav::Run(n) => {
                    let name = selected();
                    if name.is_empty() {
                        return Some(Action::Toast("ningún servicio seleccionado".into()));
                    }
                    return Some(match n {
                        0 => Action::Confirm(
                            format!("¿iniciar {name}?"),
                            Job::Service(name, "start".into()),
                        ),
                        1 => Action::Confirm(
                            format!("¿detener {name}?"),
                            Job::Service(name, "stop".into()),
                        ),
                        2 => Action::Confirm(
                            format!("¿reiniciar {name}?"),
                            Job::Service(name, "restart".into()),
                        ),
                        _ => Action::Refresh,
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
            KeyCode::Char('/') => {
                self.filtering = true;
                Some(Action::Keep)
            }
            KeyCode::Char('s') => {
                let n = selected();
                if n.is_empty() {
                    return Some(Action::Keep);
                }
                Some(Action::Confirm(
                    format!("¿iniciar {n}?"),
                    Job::Service(n, "start".into()),
                ))
            }
            KeyCode::Char('x') => {
                let n = selected();
                if n.is_empty() {
                    return Some(Action::Keep);
                }
                Some(Action::Confirm(
                    format!("¿detener {n}?"),
                    Job::Service(n, "stop".into()),
                ))
            }
            KeyCode::Char('r') => {
                let n = selected();
                if n.is_empty() {
                    return Some(Action::Keep);
                }
                Some(Action::Confirm(
                    format!("¿reiniciar {n}?"),
                    Job::Service(n, "restart".into()),
                ))
            }
            KeyCode::Char('R') => Some(Action::Refresh),
            KeyCode::Enter => {
                let n = selected();
                if n.is_empty() {
                    return Some(Action::Keep);
                }
                // El estado, dentro de Nébula (sin `read -rp` ni terminal).
                Some(Action::Run(Job::View {
                    title: format!("systemctl status {n}"),
                    cwd: String::new(),
                    cmd: format!("systemctl status {n} --no-pager -l"),
                }))
            }
            _ => None,
        }
    }
}
