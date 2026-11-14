//! Proyectos: escaneo de repos con git, React, Prisma, Rust…

use crate::action::Action;
use crate::app::{tab, Ctx};
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

#[derive(Default)]
pub struct Projects {
    pub state: ListState,
    pub filter: String,
    pub filtering: bool,
    /// Menú local (tecla m).
    pub menu: crate::menus::Menu,
}

/// Filas del menú local de Proyectos.
const PRJ_MENU: &[&str] = &[
    "abrir en el editor",
    "ver su git",
    "explorar en el gestor",
    "copiar la ruta",
    "reescanear",
];

impl Projects {
    fn filtered<'a>(&self, ctx: &'a Ctx<'a>) -> Vec<&'a crate::data::ProjectInfo> {
        let f = self.filter.to_lowercase();
        ctx.projects
            .iter()
            .filter(|p| {
                f.is_empty()
                    || p.name.to_lowercase().contains(&f)
                    || p.path.to_lowercase().contains(&f)
                    || p.kinds.iter().any(|k| k.contains(&f))
            })
            .collect()
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let list = self.filtered(ctx);

        let rows = Layout::vertical([Constraint::Min(4), Constraint::Length(1)]).split(area);

        // ── Pie: los atajos, siempre a la vista ──
        ui::hint_bar(
            f,
            t,
            rows[1],
            &[
                ("intro", "abrir en editor"),
                ("g", "ver git"),
                ("o", "explorar"),
                ("/", "filtrar"),
                ("r", "reescanear"),
                ("m", "menú"),
            ],
        );

        let title = if self.filtering {
            format!(" ❖  Proyectos · buscar: {}▏", self.filter)
        } else {
            format!(" ❖  Proyectos · {} · / para filtrar ", list.len())
        };
        let block = ui::panel_on(t, ctx.wp, rows[0], title, 0.8);
        let inner = block.inner(rows[0]);
        f.render_widget(block, rows[0]);
        if inner.height < 3 {
            return;
        }

        let items: Vec<ListItem> = list
            .iter()
            .map(|p| {
                let mut spans = vec![
                    Span::styled(" ▸ ", Style::default().fg(t.accent)),
                    Span::styled(
                        ui::truncate(&p.name, 28),
                        Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                    ),
                ];
                for k in p.kinds.iter().take(4) {
                    spans.push(Span::styled(
                        format!(" {k}"),
                        Style::default().fg(t.accent2),
                    ));
                }
                if p.dirty > 0 {
                    spans.push(Span::styled(
                        format!(" ●{}", p.dirty),
                        Style::default().fg(t.warn),
                    ));
                }
                if !p.branch.is_empty() {
                    spans.push(Span::styled(
                        format!(" ⎇{}", ui::truncate(&p.branch, 16)),
                        Style::default().fg(t.ok),
                    ));
                }
                let mut lines = vec![Line::from(spans)];
                lines.push(Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(
                        ui::truncate(&p.path, (inner.width as usize).saturating_sub(6)),
                        Style::default().fg(t.fg_faint),
                    ),
                ]));
                if !p.last_commit.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("      ", Style::default()),
                        Span::styled(
                            ui::truncate(&p.last_commit, (inner.width as usize).saturating_sub(8)),
                            Style::default().fg(t.fg_dim),
                        ),
                        Span::styled(
                            format!("  {}", p.last_time),
                            Style::default().fg(t.fg_faint),
                        ),
                    ]));
                }
                ListItem::new(lines)
            })
            .collect();

        let listw = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(t.sel_bg)
                    .fg(t.sel_fg)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▎");

        if self.state.selected().unwrap_or(0) >= list.len() {
            self.state
                .select(if list.is_empty() { None } else { Some(0) });
        }
        if self.state.selected().is_none() && !list.is_empty() {
            self.state.select(Some(0));
        }
        f.render_stateful_widget(listw, inner, &mut self.state);

        // ── Menú local (tecla m) ──
        self.menu
            .render(f, area, " ⌗  Proyectos ", PRJ_MENU, t, ctx.wp);
    }

    pub fn on_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Option<Action> {
        let list = self.filtered(ctx);
        let len = list.len();

        if self.filtering {
            return match key.code {
                KeyCode::Esc => {
                    self.filtering = false;
                    self.filter.clear();
                    Some(Action::Keep)
                }
                KeyCode::Enter => {
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

        // ── Menú local (m) ──
        if self.menu.open || matches!(key.code, KeyCode::Char('m')) {
            match self.menu.on_key(key, PRJ_MENU.len()) {
                crate::menus::Nav::Pass => {}
                crate::menus::Nav::Handled => return Some(Action::Keep),
                crate::menus::Nav::Run(n) => return self.menu_run(n, ctx),
            }
        }

        match key.code {
            KeyCode::Up => {
                let i = self.state.selected().unwrap_or(0);
                self.state.select(Some(i.saturating_sub(1)));
                Some(Action::Keep)
            }
            KeyCode::Down => {
                let i = self.state.selected().unwrap_or(0);
                self.state.select(Some((i + 1).min(len.saturating_sub(1))));
                Some(Action::Keep)
            }
            KeyCode::PageUp => {
                let i = self.state.selected().unwrap_or(0);
                self.state.select(Some(i.saturating_sub(8)));
                Some(Action::Keep)
            }
            KeyCode::PageDown => {
                let i = self.state.selected().unwrap_or(0);
                self.state.select(Some((i + 8).min(len.saturating_sub(1))));
                Some(Action::Keep)
            }
            KeyCode::Char('/') => {
                self.filtering = true;
                Some(Action::Keep)
            }
            KeyCode::Char('r') => Some(Action::Refresh),
            KeyCode::Char('g') => {
                if let Some(p) = list.get(self.state.selected().unwrap_or(0)) {
                    if p.has_git {
                        return Some(Action::Tab(tab::GIT));
                    }
                }
                Some(Action::Toast("ese proyecto no tiene git".into()))
            }
            KeyCode::Enter => {
                if let Some(p) = list.get(self.state.selected().unwrap_or(0)) {
                    let editor = ctx.cfg.projects.editor.clone();
                    let path = p.path.clone();
                    return Some(Action::Run(crate::action::Job::Plain(vec![editor, path])));
                }
                Some(Action::Keep)
            }
            KeyCode::Char('o') => {
                if let Some(p) = list.get(self.state.selected().unwrap_or(0)) {
                    let path = p.path.clone();
                    return Some(Action::Run(crate::action::Job::Plain(vec![
                        "xdg-open".into(),
                        path,
                    ])));
                }
                Some(Action::Keep)
            }
            KeyCode::Char('c') => {
                if let Some(p) = list.get(self.state.selected().unwrap_or(0)) {
                    return Some(Action::Toast(format!("ruta: {}", p.path)));
                }
                Some(Action::Keep)
            }
            _ => None,
        }
    }

    /// Acciones del menú local.
    fn menu_run(&mut self, n: usize, ctx: &Ctx) -> Option<Action> {
        let list = self.filtered(ctx);
        let Some(p) = list.get(self.state.selected().unwrap_or(0)) else {
            return Some(Action::Toast("ningún proyecto seleccionado".into()));
        };
        match n {
            0 => {
                let editor = ctx.cfg.projects.editor.clone();
                let path = p.path.clone();
                Some(Action::Run(crate::action::Job::Plain(vec![editor, path])))
            }
            1 => {
                if p.has_git {
                    Some(Action::Tab(tab::GIT))
                } else {
                    Some(Action::Toast("ese proyecto no tiene git".into()))
                }
            }
            2 => Some(Action::Run(crate::action::Job::Plain(vec![
                "xdg-open".into(),
                p.path.clone(),
            ]))),
            3 => Some(Action::Toast(format!("ruta: {}", p.path))),
            _ => Some(Action::Refresh),
        }
    }
}
