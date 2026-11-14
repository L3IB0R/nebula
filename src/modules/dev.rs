//! Desarrollo: React, Prisma y bases de datos PostgreSQL.

use crate::action::{Action, Job};
use crate::app::Ctx;
use crate::data::DbResult;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

#[derive(Default)]
pub struct Dev {
    pub focus: usize, // 0 proyectos · 1 scripts · 2 bases
    pub projects: ListState,
    pub scripts: ListState,
    pub dbs: ListState,
    pub results: Vec<DbResult>,
    pub schema: SchemaInfo,
    pub scripts_cache: Vec<(String, String)>,
    pub schema_path: String,
    /// Menú local (tecla m).
    pub menu: crate::menus::Menu,
}

/// Filas del menú local de Desarrollo.
const DEV_MENU: &[&str] = &[
    "abrir el proyecto en el editor",
    "probar la base de datos",
    "cambiar de columna",
    "refrescar",
];

#[derive(Default, Clone)]
pub struct SchemaInfo {
    pub models: usize,
    pub provider: String,
    pub url_env: String,
    pub error: String,
}

impl Dev {
    fn candidates<'a>(&self, ctx: &'a Ctx<'a>) -> Vec<&'a crate::data::ProjectInfo> {
        ctx.projects
            .iter()
            .filter(|p| {
                p.kinds
                    .iter()
                    .any(|k| matches!(k.as_str(), "react" | "prisma" | "next" | "vite" | "ts"))
            })
            .collect()
    }

    pub fn selected_project<'a>(&self, ctx: &'a Ctx<'a>) -> Option<&'a crate::data::ProjectInfo> {
        let list = self.candidates(ctx);
        list.get(self.projects.selected().unwrap_or(0)).copied()
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let list = self.candidates(ctx);
        let rows = Layout::vertical([
            Constraint::Percentage(56),
            Constraint::Percentage(42),
            Constraint::Length(1),
        ])
        .split(area);
        // ── Pie: los atajos, siempre a la vista ──
        ui::hint_bar(
            f,
            t,
            rows[2],
            &[
                ("←/→", "proyectos/scripts/bd"),
                ("↑/↓", "mover"),
                ("enter", "ejecutar script"),
                ("t", "probar bd"),
                ("r", "refrescar"),
                ("m", "menú"),
            ],
        );
        let cols = Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(rows[0]);

        // ── Proyectos ──
        let block = ui::panel_on(
            t,
            ctx.wp,
            cols[0],
            if self.focus == 0 {
                " ▣  Proyectos web ◀ ".to_string()
            } else {
                format!(" ▣  Proyectos web · {} ", list.len())
            },
            0.8,
        );
        let inner = block.inner(cols[0]);
        f.render_widget(block, cols[0]);
        let items: Vec<ListItem> = list
            .iter()
            .map(|p| {
                ListItem::new(Line::from(vec![
                    Span::styled(" ▸ ", Style::default().fg(t.accent)),
                    Span::styled(
                        ui::truncate(&p.name, 22),
                        Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" {}", p.kinds.join("/")),
                        Style::default().fg(t.accent2),
                    ),
                ]))
            })
            .collect();
        let lw = List::new(items).highlight_style(
            Style::default()
                .bg(t.sel_bg)
                .fg(t.sel_fg)
                .add_modifier(Modifier::BOLD),
        );
        if list.is_empty() {
            f.render_widget(
                ratatui::widgets::Paragraph::new(
                    "aún no hay proyectos con\nReact/Prisma en tus carpetas.\n\nAjusta `projects.dirs` en\n~/.config/nebula/config.toml",
                )
                .style(Style::default().fg(t.fg_faint)),
                inner,
            );
        } else {
            if self.projects.selected().unwrap_or(0) >= list.len() {
                self.projects.select(Some(0));
            }
            f.render_stateful_widget(lw, inner, &mut self.projects);
        }

        // ── Scripts ──
        if let Some(p) = self.selected_project(ctx) {
            self.scripts_cache = p.scripts.clone();
            self.load_schema(&p.path);
        }
        let block = ui::panel_on(
            t,
            ctx.wp,
            cols[1],
            if self.focus == 1 {
                " ▶  Scripts npm ◀ ".to_string()
            } else {
                " ▶  Scripts npm ".to_string()
            },
            0.8,
        );
        let inner = block.inner(cols[1]);
        f.render_widget(block, cols[1]);
        if self.scripts_cache.is_empty() {
            f.render_widget(
                ratatui::widgets::Paragraph::new("sin package.json scripts")
                    .style(Style::default().fg(t.fg_faint)),
                inner,
            );
        } else {
            let items: Vec<ListItem> = self
                .scripts_cache
                .iter()
                .take(inner.height as usize)
                .map(|(k, v)| {
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!(" {k:<14} "),
                            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            ui::truncate(v, (inner.width as usize).saturating_sub(18)),
                            Style::default().fg(t.fg_dim),
                        ),
                    ]))
                })
                .collect();
            let lw = List::new(items).highlight_style(
                Style::default()
                    .bg(t.sel_bg)
                    .fg(t.sel_fg)
                    .add_modifier(Modifier::BOLD),
            );
            if self.scripts_cache.is_empty() {
                self.scripts.select(None);
            } else if self.scripts.selected().unwrap_or(0) >= self.scripts_cache.len() {
                self.scripts.select(Some(0));
            }
            f.render_stateful_widget(lw, inner, &mut self.scripts);
        }

        // ── Prisma + Bases de datos ──
        let block = ui::panel_on(
            t,
            ctx.wp,
            rows[1],
            if self.focus == 2 {
                " ⛁  Prisma & PostgreSQL ◀ ".to_string()
            } else {
                " ⛁  Prisma & PostgreSQL ".to_string()
            },
            0.8,
        );
        let inner = block.inner(rows[1]);
        f.render_widget(block, rows[1]);
        if inner.height < 3 {
            return;
        }
        let mut lines = Vec::new();
        if self.schema.error.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("schema   ", Style::default().fg(t.fg_faint)),
                Span::styled(
                    format!(
                        "{} · {} modelos · {}",
                        if self.schema.provider.is_empty() {
                            "—"
                        } else {
                            &self.schema.provider
                        },
                        self.schema.models,
                        if self.schema.url_env.is_empty() {
                            "sin url"
                        } else {
                            &self.schema.url_env
                        }
                    ),
                    Style::default().fg(t.fg),
                ),
            ]));
        } else {
            lines.push(Line::from(Span::styled(
                self.schema.error.clone(),
                Style::default().fg(t.fg_faint),
            )));
        }

        let mut db_lines = Vec::new();
        for (i, r) in self.results.iter().enumerate() {
            let mark = if r.ok { "●" } else { "○" };
            let col = if r.ok { t.ok } else { t.err };
            let mut spans = vec![
                Span::styled(format!(" {mark} "), Style::default().fg(col)),
                Span::styled(
                    ui::truncate(&r.name, 24),
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                ),
            ];
            if r.ok {
                spans.push(Span::styled(
                    format!(" {} tablas · {} ms", r.tables.len(), r.elapsed_ms),
                    Style::default().fg(t.fg_dim),
                ));
                if !r.tables.is_empty() {
                    spans.push(Span::styled(
                        format!(
                            " · {}",
                            r.tables
                                .iter()
                                .take(6)
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        Style::default().fg(t.accent2),
                    ));
                }
            } else {
                spans.push(Span::styled(
                    format!(
                        " {}",
                        ui::truncate(&r.error, (inner.width as usize).saturating_sub(28))
                    ),
                    Style::default().fg(t.err),
                ));
            }
            db_lines.push(Line::from(spans));
            let _ = i;
        }
        if db_lines.is_empty() {
            db_lines.push(Line::from(Span::styled(
                "pulsa t para probar la conexión a la base de datos",
                Style::default().fg(t.fg_faint),
            )));
        }
        lines.extend(db_lines);
        f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);

        // ── Menú local (tecla m) ──
        self.menu
            .render(f, area, " ⌗  Desarrollo ", DEV_MENU, t, ctx.wp);
    }

    fn load_schema(&mut self, path: &str) {
        let p = std::path::Path::new(path).join("schema.prisma");
        if !p.exists() {
            if self.schema.error.is_empty() {
                self.schema.error = "sin schema.prisma en este proyecto".into();
            }
            return;
        }
        if self.schema_path == path && (self.schema.models > 0 || !self.schema.error.is_empty()) {
            return;
        }
        self.schema_path = path.to_string();
        let Ok(s) = std::fs::read_to_string(&p) else {
            self.schema.error = "no se pudo leer schema.prisma".into();
            return;
        };
        self.schema.models = s
            .lines()
            .filter(|l| l.trim_start().starts_with("model "))
            .count();
        self.schema.provider = s
            .lines()
            .find(|l| l.contains("provider"))
            .and_then(|l| l.split('"').nth(1))
            .unwrap_or("")
            .to_string();
        self.schema.url_env = s
            .lines()
            .find(|l| l.contains("env("))
            .and_then(|l| l.split("env(\"").nth(1))
            .and_then(|l| l.split('"').next())
            .unwrap_or("")
            .to_string();
        self.schema.error.clear();
    }

    pub fn on_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Option<Action> {
        let n_projects = self.candidates(ctx).len();

        // ── Menú local (m) ──
        if self.menu.open || matches!(key.code, KeyCode::Char('m')) {
            match self.menu.on_key(key, DEV_MENU.len()) {
                crate::menus::Nav::Pass => {}
                crate::menus::Nav::Handled => return Some(Action::Keep),
                crate::menus::Nav::Run(n) => return self.menu_run(n, ctx),
            }
        }

        match key.code {
            KeyCode::Left => {
                self.focus = self.focus.saturating_sub(1);
                Some(Action::Keep)
            }
            KeyCode::Right => {
                self.focus = (self.focus + 1).min(2);
                Some(Action::Keep)
            }
            KeyCode::Up => {
                match self.focus {
                    0 => {
                        let i = self.projects.selected().unwrap_or(0);
                        self.projects.select(Some(i.saturating_sub(1)));
                        self.schema = SchemaInfo::default();
                    }
                    1 => {
                        let i = self.scripts.selected().unwrap_or(0);
                        self.scripts.select(Some(i.saturating_sub(1)));
                    }
                    _ => {
                        let i = self.dbs.selected().unwrap_or(0);
                        self.dbs.select(Some(i.saturating_sub(1)));
                    }
                }
                Some(Action::Keep)
            }
            KeyCode::Down => {
                match self.focus {
                    0 => {
                        let i = self.projects.selected().unwrap_or(0);
                        self.projects
                            .select(Some((i + 1).min(n_projects.saturating_sub(1))));
                        self.schema = SchemaInfo::default();
                    }
                    1 => {
                        let i = self.scripts.selected().unwrap_or(0);
                        self.scripts.select(Some(
                            (i + 1).min(self.scripts_cache.len().saturating_sub(1)),
                        ));
                    }
                    _ => {
                        let i = self.dbs.selected().unwrap_or(0);
                        self.dbs
                            .select(Some((i + 1).min(ctx.cfg.databases.len().saturating_sub(1))));
                    }
                }
                Some(Action::Keep)
            }
            KeyCode::Char('t') => {
                if ctx.cfg.databases.is_empty() {
                    return Some(Action::Toast(
                        "no hay bases configuradas en ~/.config/nebula/config.toml".into(),
                    ));
                }
                let idx = self
                    .dbs
                    .selected()
                    .unwrap_or(0)
                    .min(ctx.cfg.databases.len() - 1);
                Some(Action::TestDb(idx))
            }
            KeyCode::Enter => match self.focus {
                0 => {
                    if let Some(p) = self.selected_project(ctx) {
                        let (editor, path) = (ctx.cfg.projects.editor.clone(), p.path.clone());
                        return Some(Action::Run(Job::Plain(vec![editor, path])));
                    }
                    Some(Action::Keep)
                }
                1 => {
                    let sel = self.scripts.selected().unwrap_or(0);
                    if let Some((name, cmd)) = self.scripts_cache.get(sel) {
                        if let Some(p) = self.selected_project(ctx) {
                            let (cwd, name) = (p.path.clone(), name.clone());
                            let _ = cmd;
                            // La salida del script, en el visor interno.
                            return Some(Action::Run(Job::View {
                                title: format!("npm run {name}"),
                                cwd,
                                cmd: format!("npm run {name}"),
                            }));
                        }
                    }
                    Some(Action::Keep)
                }
                _ => Some(Action::Keep),
            },
            KeyCode::Char('r') => {
                self.schema = SchemaInfo::default();
                Some(Action::Refresh)
            }
            _ => None,
        }
    }

    /// Acciones del menú local.
    fn menu_run(&mut self, n: usize, ctx: &Ctx) -> Option<Action> {
        match n {
            0 => {
                if let Some(p) = self.selected_project(ctx) {
                    let (editor, path) = (ctx.cfg.projects.editor.clone(), p.path.clone());
                    return Some(Action::Run(Job::Plain(vec![editor, path])));
                }
                Some(Action::Toast("ningún proyecto seleccionado".into()))
            }
            1 => {
                if ctx.cfg.databases.is_empty() {
                    return Some(Action::Toast(
                        "no hay bases configuradas en ~/.config/nebula/config.toml".into(),
                    ));
                }
                let idx = self
                    .dbs
                    .selected()
                    .unwrap_or(0)
                    .min(ctx.cfg.databases.len() - 1);
                Some(Action::TestDb(idx))
            }
            2 => {
                self.focus = (self.focus + 1) % 3;
                Some(Action::Keep)
            }
            _ => {
                self.schema = SchemaInfo::default();
                Some(Action::Refresh)
            }
        }
    }
}
