//! Lanzador deslizante: apps, comandos, carpetas y proyectos.

use crate::action::{Action, Job};
use crate::app::Ctx;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;
use std::path::PathBuf;

#[derive(Clone)]
pub enum Launch {
    Exec(Vec<String>),
    Term(String),
    Tab(usize),
    Nothing,
}

#[derive(Clone)]
pub struct Item {
    pub title: String,
    pub sub: String,
    pub kind: &'static str,
    pub launch: Launch,
}

#[derive(Default)]
pub struct Launcher {
    pub query: String,
    pub items: Vec<Item>,
    pub filtered: Vec<usize>,
    pub state: ListState,
    pub built: bool,
}

impl Launcher {
    pub fn open(&mut self, ctx: &Ctx) {
        if !self.built {
            self.build(ctx);
            self.built = true;
        }
        self.query.clear();
        self.refilter();
        self.state.select(Some(0));
    }

    pub fn close(&mut self) {
        self.query.clear();
    }

    fn build(&mut self, ctx: &Ctx) {
        // 1) Aplicaciones .desktop
        for dir in [
            PathBuf::from("/usr/share/applications"),
            crate::config::home().join(".local/share/applications"),
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
            crate::config::home().join(".local/share/flatpak/exports/share/applications"),
        ] {
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|x| x.to_str()) != Some("desktop") {
                        continue;
                    }
                    if let Some(item) = parse_desktop(&p) {
                        if !self.items.iter().any(|i| i.title == item.title) {
                            self.items.push(item);
                        }
                    }
                }
            }
        }

        // 2) Comandos de terminal presentes
        let cmds: &[(&str, &str)] = &[
            ("htop", "monitor del sistema"),
            ("btop", "monitor del sistema"),
            ("cmus", "música offline"),
            ("cliamp", "música online"),
            ("ncdu", "explorador de disco"),
            ("git", "control de versiones"),
            ("docker", "contenedores"),
            ("podman", "contenedores"),
            ("node", "runtime javascript"),
            ("npm", "paquetes node"),
            ("python3", "intérprete python"),
            ("vim", "editor"),
            ("nvim", "editor"),
            ("nano", "editor"),
            ("htop", "monitor"),
            ("journalctl", "bitácora del sistema"),
            ("nmcli", "red"),
            ("bluetoothctl", "bluetooth"),
            ("flatpak", "apps empaquetadas"),
            ("systemctl", "servicios"),
            ("lazygit", "git visual"),
            ("tmux", "terminal multiplexada"),
            ("ranger", "explorador de archivos"),
            ("lf", "explorador de archivos"),
            ("fzf", "búsqueda difusa"),
        ];
        for (c, desc) in cmds {
            if crate::data::which(c).is_some() {
                self.items.push(Item {
                    title: c.to_string(),
                    sub: desc.to_string(),
                    kind: "cmd",
                    launch: Launch::Term(c.to_string()),
                });
            }
        }

        // 3) Carpetas del home
        if let Ok(rd) = std::fs::read_dir(crate::config::home()) {
            let mut dirs: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            dirs.sort();
            for d in dirs {
                let name = d
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if name.starts_with('.') && !matches!(name.as_str(), ".config" | ".local") {
                    continue;
                }
                self.items.push(Item {
                    title: name,
                    sub: home_short(&d),
                    kind: "carpeta",
                    launch: Launch::Exec(vec!["xdg-open".into(), d.to_string_lossy().to_string()]),
                });
            }
        }

        // 4) Proyectos
        for p in ctx.projects.iter().take(60) {
            self.items.push(Item {
                title: p.name.clone(),
                sub: home_short(std::path::Path::new(&p.path)),
                kind: "proyecto",
                launch: Launch::Exec(vec![ctx.cfg.projects.editor.clone(), p.path.clone()]),
            });
        }

        // 5) Metacciones
        for (title, sub, kind, launch) in [
            (
                "Cerrar Nébula",
                "salir con calma",
                "acción",
                Launch::Nothing,
            ),
            (
                "Ayuda de atajos",
                "qué teclas hay",
                "acción",
                Launch::Tab(99),
            ),
        ] {
            self.items.push(Item {
                title: title.into(),
                sub: sub.into(),
                kind,
                launch,
            });
        }

        self.items.sort_by_key(|a| a.title.to_lowercase());

        // 6) Carpetas frecuentes de zoxide (si está instalado): quedan arriba.
        if crate::data::which("zoxide").is_some() {
            if let Some(out) = crate::data::run("zoxide", &["query", "-l"]) {
                let mut frec: Vec<Item> = Vec::new();
                for line in out.lines() {
                    let path = line.trim();
                    if path.is_empty() {
                        continue;
                    }
                    let p = std::path::Path::new(path);
                    if !p.is_dir() {
                        continue;
                    }
                    let name = p
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    if name.is_empty() || frec.iter().any(|i| i.title == name) {
                        continue;
                    }
                    frec.push(Item {
                        title: name,
                        sub: format!("{} · zoxide", home_short(p)),
                        kind: "frecuente",
                        launch: Launch::Exec(vec!["xdg-open".into(), path.to_string()]),
                    });
                }
                if !frec.is_empty() {
                    let mut all = frec;
                    all.append(&mut self.items);
                    self.items = all;
                }
            }
        }
    }

    pub fn refilter(&mut self) {
        let q = self.query.to_lowercase();
        self.filtered.clear();
        if q.is_empty() {
            for (i, _) in self.items.iter().enumerate().take(40) {
                self.filtered.push(i);
            }
        } else {
            let mut scored: Vec<(u32, usize)> = Vec::new();
            for (i, it) in self.items.iter().enumerate() {
                if let Some(s) = score(&it.title.to_lowercase(), &q) {
                    scored.push((s, i));
                } else if let Some(s) = score(&it.sub.to_lowercase(), &q) {
                    scored.push((s + 5, i));
                }
            }
            scored.sort();
            for (_, i) in scored.into_iter().take(40) {
                self.filtered.push(i);
            }
        }
        if self.state.selected().unwrap_or(0) >= self.filtered.len() {
            self.state.select(if self.filtered.is_empty() {
                None
            } else {
                Some(0)
            });
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let block = ui::panel_on(t, ctx.wp, area, " ❖  Lanzador ", 0.94);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(inner);

        // Consulta
        let query = Line::from(vec![
            Span::styled(
                " ❯ ",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(self.query.clone(), Style::default().fg(t.fg)),
            Span::styled("▏", Style::default().fg(t.accent)),
            Span::styled(
                format!("   {} resultados", self.filtered.len()),
                Style::default().fg(t.fg_faint),
            ),
        ]);
        f.render_widget(ratatui::widgets::Paragraph::new(query), rows[0]);

        // Resultados
        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .take(rows[1].height as usize)
            .map(|&i| {
                let it = &self.items[i];
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" {:<9}", it.kind),
                        Style::default().fg(match it.kind {
                            "app" => t.accent,
                            "cmd" => t.accent2,
                            "proyecto" => t.ok,
                            "carpeta" => t.warn,
                            _ => t.fg_dim,
                        }),
                    ),
                    Span::styled(" ▸ ", Style::default().fg(t.border)),
                    Span::styled(
                        ui::truncate(&it.title, 34),
                        Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("  ·  ", Style::default().fg(t.border)),
                    Span::styled(
                        ui::truncate(&it.sub, (rows[1].width as usize).saturating_sub(54)),
                        Style::default().fg(t.fg_faint),
                    ),
                ]))
            })
            .collect();
        if items.is_empty() {
            f.render_widget(
                ratatui::widgets::Paragraph::new(
                    "nada por aquí · prueba con otro nombre, o escribe ! para un comando",
                )
                .style(Style::default().fg(t.fg_faint)),
                rows[1],
            );
        } else {
            let lw = List::new(items).highlight_style(
                Style::default()
                    .bg(t.sel_bg)
                    .fg(t.sel_fg)
                    .add_modifier(Modifier::BOLD),
            );
            f.render_stateful_widget(lw, rows[1], &mut self.state);
        }

        ui::hint_bar(
            f,
            t,
            rows[2],
            &[
                ("enter", "abrir"),
                ("↑/↓", "mover"),
                ("!", "comando"),
                ("esc", "cerrar"),
            ],
        );
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc => None,
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
                Some(Action::Keep)
            }
            KeyCode::Up => {
                let i = self.state.selected().unwrap_or(0);
                self.state.select(Some(i.saturating_sub(1)));
                Some(Action::Keep)
            }
            KeyCode::Down => {
                let i = self.state.selected().unwrap_or(0);
                self.state
                    .select(Some((i + 1).min(self.filtered.len().saturating_sub(1))));
                Some(Action::Keep)
            }
            KeyCode::PageUp => {
                let i = self.state.selected().unwrap_or(0);
                self.state.select(Some(i.saturating_sub(6)));
                Some(Action::Keep)
            }
            KeyCode::PageDown => {
                let i = self.state.selected().unwrap_or(0);
                self.state
                    .select(Some((i + 6).min(self.filtered.len().saturating_sub(1))));
                Some(Action::Keep)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query.clear();
                self.refilter();
                Some(Action::Keep)
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                self.refilter();
                Some(Action::Keep)
            }
            KeyCode::Enter => {
                let idx = self.state.selected().unwrap_or(0);
                let item = self.filtered.get(idx).map(|&i| self.items[i].clone());
                let Some(item) = item else {
                    return Some(Action::Keep);
                };
                match item.launch {
                    Launch::Exec(args) => Some(Action::Run(Job::Plain(args))),
                    Launch::Term(cmd) => Some(Action::Run(Job::Shell {
                        cwd: String::new(),
                        cmd,
                    })),
                    Launch::Tab(99) => Some(Action::Help),
                    Launch::Tab(n) => Some(Action::Tab(n)),
                    Launch::Nothing => Some(Action::Quit),
                }
            }
            _ => Some(Action::Keep),
        }
    }
}

fn score(hay: &str, needle: &str) -> Option<u32> {
    if needle.is_empty() {
        return Some(0);
    }
    if hay.starts_with(needle) {
        return Some(0);
    }
    if hay.contains(needle) {
        return Some(1);
    }
    // subsecuencia
    let mut it = hay.chars();
    let mut matched = 0usize;
    for c in needle.chars() {
        if it.any(|h| h == c) {
            matched += 1;
        } else {
            return None;
        }
    }
    if matched == needle.chars().count() {
        Some(2 + (hay.len() as u32).min(20))
    } else {
        None
    }
}

fn parse_desktop(path: &PathBuf) -> Option<Item> {
    let s = std::fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut exec = None;
    let mut comment = None;
    let mut terminal = false;
    let mut hidden = false;
    let mut keywords = String::new();
    let mut only_show = String::new();

    for line in s.lines() {
        if let Some(v) = line.strip_prefix("Name=") {
            if name.is_none() {
                name = Some(v.trim().to_string());
            }
        } else if let Some(v) = line.strip_prefix("Exec=") {
            exec = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Comment=") {
            comment = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Keywords=") {
            keywords = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("Terminal=") {
            terminal = v.trim() == "true";
        } else if let Some(v) = line.strip_prefix("Hidden=") {
            hidden = v.trim() == "true";
        } else if let Some(v) = line.strip_prefix("NoDisplay=") {
            hidden = hidden || v.trim() == "true";
        } else if let Some(v) = line.strip_prefix("OnlyShowIn=") {
            only_show = v.trim().to_string();
        }
    }
    if hidden || name.is_none() || exec.is_none() {
        return None;
    }
    if !only_show.is_empty() && !only_show.contains("GNOME") && !only_show.contains("X-GNOME") {
        // lo dejamos pasar igual: el escritorio puede ser cualquiera
    }
    let args = split_exec(exec.as_deref().unwrap_or(""));
    if args.is_empty() {
        return None;
    }
    let title = name.unwrap();
    let sub = comment.unwrap_or_else(|| keywords.replace(';', ", "));
    Some(Item {
        title,
        sub,
        kind: if terminal { "cmd" } else { "app" },
        launch: if terminal {
            Launch::Term(args.join(" "))
        } else {
            Launch::Exec(args)
        },
    })
}

/// Divide la línea Exec respetando comillas y quitando códigos de campo.
fn split_exec(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in s.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    // códigos de campo %f %F %u %U %i %c %k …
    out.retain(|a| !(a.len() == 2 && a.starts_with('%')));
    out
}

/// Ruta mostrada con `~` en lugar del home.
fn home_short(p: &std::path::Path) -> String {
    let full = p.to_string_lossy().to_string();
    let home = crate::config::home().to_string_lossy().to_string();
    if let Some(rest) = full.strip_prefix(&home) {
        if rest.is_empty() {
            "~".to_string()
        } else {
            format!("~{}", rest)
        }
    } else {
        full
    }
}
