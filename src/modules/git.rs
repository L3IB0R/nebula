//! Git: repositorios, estado, archivos, bitácora y menú local
//! (commits con tipo en español, ramas y stash).

use crate::action::{Action, Job};
use crate::app::Ctx;
use crate::config::Theme;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

/// Tipos de commit, en español: prefijo del mensaje y descripción.
const KINDS: &[(&str, &str)] = &[
    ("añadir", "nueva funcionalidad"),
    ("corregir", "arreglo de un fallo"),
    ("ajustar", "ajuste o mejora"),
    ("refactorizar", "reorganizar código"),
    ("documentar", "docs y comentarios"),
    ("probar", "pruebas"),
    ("limpiar", "limpieza"),
];

/// Submenú abierto dentro de la pestaña Git.
pub enum Menu {
    Commit {
        kind: usize,
        /// 0 = tipo · 1 = título · 2 = descripción.
        field: usize,
        title: String,
        desc: String,
    },
    /// `new` = escribiendo el nombre de la rama nueva.
    Branch {
        sel: usize,
        new: bool,
        name: String,
        list: Vec<String>,
    },
    Stash {
        sel: usize,
        list: Vec<String>,
    },
    /// Acciones rápidas (tecla m): sólo atajos, todo lo demás está a la vista.
    Actions {
        sel: usize,
    },
    /// Selector con casillas para añadir rutas al área de preparación
    /// (tecla a). Cada fila se añade **por su ruta**, jamás `git add .`.
    Add {
        sel: usize,
        /// (status, ruta) de cada archivo que se puede añadir.
        files: Vec<(String, String)>,
        checks: Vec<bool>,
    },
    /// Clonar un repositorio por URL dentro de Nébula (tecla k).
    Clone {
        url: String,
        /// Dónde caerá el clon.
        dest: String,
    },
}

/// Qué hace cada fila del menú de acciones.
enum ActCmd {
    /// Ejecutar tal cual, con la salida en el visor.
    Run(&'static str),
    /// `git pull` / `git push`: van confirmados.
    Pull,
    Push,
    /// Terminal de verdad, si se quiere trabajar a mano.
    Term,
}

/// Acciones rápidas de git (tecla m).
const ACTIONS: &[(&str, ActCmd)] = &[
    ("estatus (git status)", ActCmd::Run("git status")),
    ("diferencias (git diff)", ActCmd::Run("git diff")),
    (
        "bitácora (git log)",
        ActCmd::Run("git log --oneline --decorate -20"),
    ),
    ("actualizar (git pull)", ActCmd::Pull),
    ("subir (git push)", ActCmd::Push),
    ("traer (git fetch)", ActCmd::Run("git fetch --all --prune")),
    ("terminal en el repositorio", ActCmd::Term),
];

/// Comillas simples seguras para `sh -c`.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Tira de acciones: **todo** lo que git puede hacer, a la vista siempre.
/// (La tecla m queda sólo como atajo a la lista de comandos sueltos.)
fn strip(t: &Theme) -> Vec<Line<'static>> {
    vec![
        ui::hint_line(
            t,
            &[
                ("intro", "estatus"),
                ("a", "añadir"),
                ("u", "sin stage"),
                ("c", "commit"),
                ("b", "ramas"),
                ("s", "stash"),
            ],
        ),
        ui::hint_line(
            t,
            &[
                ("l", "log"),
                ("f", "fetch"),
                ("d", "diff"),
                ("p", "pull"),
                ("P", "push"),
                ("k", "clonar"),
            ],
        ),
    ]
}

#[derive(Default)]
pub struct GitState {
    pub path: String,
    pub repo: Option<crate::data::GitRepo>,
    pub files: Vec<crate::data::GitFile>,
    pub log: Vec<crate::data::GitCommit>,
    pub diff: String,
    pub loading: bool,
}

#[derive(Default)]
pub struct GitMod {
    pub state: ListState,
    /// Submenú local: commit, ramas o stash.
    pub menu: Option<Menu>,
}

impl GitMod {
    pub fn repos<'a>(&self, ctx: &'a Ctx<'a>) -> Vec<&'a crate::data::ProjectInfo> {
        ctx.projects.iter().filter(|p| p.has_git).collect()
    }

    pub fn selected_path(&self, ctx: &Ctx) -> String {
        let repos = self.repos(ctx);
        repos
            .get(self.state.selected().unwrap_or(0))
            .map(|p| p.path.clone())
            .unwrap_or_default()
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
        // ── Tira de acciones: todo a la vista, sin menús escondidos ──
        f.render_widget(ratatui::widgets::Paragraph::new(strip(t)), rows[0]);

        let repos = self.repos(ctx);
        if repos.is_empty() {
            let msg = if crate::data::which("git").is_none() {
                "git no está instalado en este sistema.\n\nInstálalo con:\n  sudo apt install git"
            } else {
                "No he encontrado repositorios git todavía.\n pulsa r para reescanear tus proyectos."
            };
            f.render_widget(
                ratatui::widgets::Paragraph::new(msg).style(Style::default().fg(t.fg_dim)),
                rows[1],
            );
            ui::hint_bar(
                f,
                t,
                rows[2],
                &[
                    ("↑/↓", "repos"),
                    ("r", "refrescar"),
                    ("k", "clonar"),
                    ("m", "atajos"),
                ],
            );
            return;
        }

        let cols = Layout::horizontal([Constraint::Percentage(32), Constraint::Percentage(68)])
            .split(rows[1]);

        // ── Lista de repos ──
        let block = ui::panel_on(
            t,
            ctx.wp,
            cols[0],
            format!(" ⎇  Repos · {} ", repos.len()),
            0.8,
        );
        let inner = block.inner(cols[0]);
        f.render_widget(block, cols[0]);
        let items: Vec<ListItem> = repos
            .iter()
            .map(|p| {
                ListItem::new(Line::from(vec![
                    Span::styled(" ▸ ", Style::default().fg(t.accent)),
                    Span::styled(
                        ui::truncate(&p.name, 20),
                        Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                    ),
                    if p.dirty > 0 {
                        Span::styled(format!(" ●{}", p.dirty), Style::default().fg(t.warn))
                    } else {
                        Span::styled(" ✓", Style::default().fg(t.ok))
                    },
                ]))
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
        if self.state.selected().unwrap_or(0) >= repos.len() {
            self.state.select(Some(0));
        }
        f.render_stateful_widget(listw, inner, &mut self.state);

        // ── Detalle ──
        let g = &ctx.git;
        let right = Layout::vertical([
            Constraint::Length(7),
            Constraint::Percentage(45),
            Constraint::Min(5),
        ])
        .split(cols[1]);

        let title = if g.loading {
            " ⎇  Estado · leyendo… ".to_string()
        } else {
            format!(" ⎇  Estado · {} ", ui::truncate(&g.path, 40))
        };
        let block = ui::panel_on(t, ctx.wp, right[0], title, 0.8);
        let inner = block.inner(right[0]);
        f.render_widget(block, right[0]);

        match &g.repo {
            None => {
                let msg = if g.loading {
                    "leyendo el repositorio…".to_string()
                } else if crate::data::which("git").is_none() {
                    "git no está instalado.\n\n  sudo apt install git\n\npulsa r cuando lo tengas."
                        .to_string()
                } else {
                    "sin datos todavía · pulsa r".to_string()
                };
                f.render_widget(
                    ratatui::widgets::Paragraph::new(msg).style(Style::default().fg(t.fg_dim)),
                    inner,
                );
            }
            Some(r) => {
                let mut lines = vec![
                    Line::from(vec![
                        Span::styled(" rama    ", Style::default().fg(t.fg_faint)),
                        Span::styled(
                            ui::truncate(&r.branch, 30),
                            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                        ),
                        if r.upstream.is_empty() {
                            Span::styled("  (sin upstream)", Style::default().fg(t.fg_faint))
                        } else {
                            Span::styled(
                                format!("  → {}", ui::truncate(&r.upstream, 26)),
                                Style::default().fg(t.fg_dim),
                            )
                        },
                        if r.ahead > 0 {
                            Span::styled(format!("  ↑{}", r.ahead), Style::default().fg(t.ok))
                        } else {
                            Span::raw("")
                        },
                        if r.behind > 0 {
                            Span::styled(format!("  ↓{}", r.behind), Style::default().fg(t.warn))
                        } else {
                            Span::raw("")
                        },
                    ]),
                    Line::from(vec![
                        Span::styled(" cambios ", Style::default().fg(t.fg_faint)),
                        Span::styled(format!("●{} staged ", r.staged), Style::default().fg(t.ok)),
                        Span::styled(
                            format!("●{} sin stage ", r.unstaged),
                            Style::default().fg(t.warn),
                        ),
                        Span::styled(
                            format!("●{} sin track ", r.untracked),
                            Style::default().fg(t.accent2),
                        ),
                        if r.conflicts > 0 {
                            Span::styled(
                                format!("●{} conflictos", r.conflicts),
                                Style::default().fg(t.err),
                            )
                        } else {
                            Span::raw("")
                        },
                    ]),
                    Line::from(vec![
                        Span::styled(" último   ", Style::default().fg(t.fg_faint)),
                        Span::styled(
                            ui::truncate(&r.last_commit, (inner.width as usize).saturating_sub(12)),
                            Style::default().fg(t.fg),
                        ),
                    ]),
                    Line::from(Span::styled(
                        format!("          {}", r.last_time),
                        Style::default().fg(t.fg_faint),
                    )),
                ];
                if !g.diff.is_empty() {
                    for l in g.diff.lines().take(2) {
                        lines.push(Line::from(Span::styled(
                            ui::truncate(l, inner.width as usize),
                            Style::default().fg(t.fg_dim),
                        )));
                    }
                }
                f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
            }
        }

        // ── Archivos ──
        let block = ui::panel_on(
            t,
            ctx.wp,
            right[1],
            format!(" ▧  Archivos · {} ", g.files.len()),
            0.8,
        );
        let inner = block.inner(right[1]);
        f.render_widget(block, right[1]);
        let flines: Vec<Line> = g
            .files
            .iter()
            .take(inner.height as usize)
            .map(|f| {
                let color = if f.status.contains('?') {
                    t.accent2
                } else if f.status.starts_with('A') || f.status.contains('M') {
                    t.ok
                } else if f.status.contains('D') {
                    t.err
                } else {
                    t.warn
                };
                Line::from(vec![
                    Span::styled(
                        format!(" {} ", f.status),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        ui::truncate(&f.path, (inner.width as usize).saturating_sub(6)),
                        Style::default().fg(t.fg),
                    ),
                ])
            })
            .collect();
        f.render_widget(
            ratatui::widgets::Paragraph::new(if flines.is_empty() {
                vec![Line::from(Span::styled(
                    "árbol limpio ✦",
                    Style::default().fg(t.ok),
                ))]
            } else {
                flines
            }),
            inner,
        );

        // ── Bitácora ──
        let block = ui::panel_on(t, ctx.wp, right[2], " ⌗  Bitácora ", 0.8);
        let inner = block.inner(right[2]);
        f.render_widget(block, right[2]);
        let llines: Vec<Line> = g
            .log
            .iter()
            .take(inner.height as usize)
            .map(|c| {
                Line::from(vec![
                    Span::styled(
                        format!(" {} ", ui::truncate(&c.hash, 7)),
                        Style::default().fg(t.accent2),
                    ),
                    Span::styled(
                        ui::truncate(&c.msg, (inner.width as usize).saturating_sub(20)),
                        Style::default().fg(t.fg),
                    ),
                    Span::styled(
                        format!(" {}", ui::truncate(&c.author, 10)),
                        Style::default().fg(t.fg_faint),
                    ),
                ])
            })
            .collect();
        f.render_widget(ratatui::widgets::Paragraph::new(llines), inner);

        // ── Pie: los atajos, siempre a la vista ──
        ui::hint_bar(
            f,
            t,
            rows[2],
            &[
                ("↑/↓", "repositorio"),
                ("r", "refrescar"),
                ("k", "clonar"),
                ("m", "atajos"),
            ],
        );

        // ── Submenú local (commit · ramas · stash · añadir · clonar) ──
        if let Some(menu) = &self.menu {
            self.render_menu(f, rows[1], menu, ctx);
        }
    }

    /// Dibuja el submenú como ventana centrada sobre la pestaña.
    fn render_menu(&self, f: &mut Frame, area: Rect, menu: &Menu, ctx: &Ctx) {
        let t = ctx.theme;
        let (title, lines): (String, Vec<Line>) = match menu {
            Menu::Commit {
                kind,
                field,
                title,
                desc,
            } => {
                let mut v: Vec<Line> = Vec::new();
                // La etiqueta del campo activo se enciende.
                let lab = |n: usize, text: &str| {
                    let on = *field == n;
                    Span::styled(
                        format!("  {text} "),
                        Style::default()
                            .fg(if on { t.accent } else { t.fg_faint })
                            .add_modifier(if on {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    )
                };
                v.push(Line::from(vec![lab(0, "tipo")]));
                for (i, (prefix, kd)) in KINDS.iter().enumerate() {
                    let on = i == *kind;
                    v.push(Line::from(vec![
                        Span::styled(
                            format!("  {} ", if on { "▸" } else { " " }),
                            Style::default().fg(t.accent),
                        ),
                        Span::styled(
                            format!("{prefix:<14}"),
                            Style::default()
                                .fg(if on { t.fg } else { t.fg_dim })
                                .add_modifier(if on {
                                    Modifier::BOLD
                                } else {
                                    Modifier::empty()
                                }),
                        ),
                        Span::styled(*kd, Style::default().fg(t.fg_faint)),
                        Span::styled(
                            if on && *field == 0 { "  ▏" } else { "" },
                            Style::default().fg(t.accent),
                        ),
                    ]));
                }
                v.push(Line::from(""));
                v.push(Line::from(vec![
                    lab(1, "título"),
                    Span::styled(
                        if *field == 1 {
                            format!("{title}▏")
                        } else {
                            title.clone()
                        },
                        Style::default()
                            .fg(if *field == 1 { t.fg } else { t.fg_dim })
                            .add_modifier(if *field == 1 {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                ]));
                v.push(Line::from(vec![
                    lab(2, "descripción"),
                    Span::styled(
                        if *field == 2 {
                            format!("{desc}▏")
                        } else {
                            desc.clone()
                        },
                        Style::default().fg(if *field == 2 { t.fg } else { t.fg_dim }),
                    ),
                ]));
                v.push(Line::from(""));
                v.push(Line::from(Span::styled(
                    "  ↑/↓ tipo · tab campo · intro crear · esc cancelar",
                    Style::default().fg(t.fg_faint),
                )));
                (" ✎  Nuevo commit ".to_string(), v)
            }
            Menu::Branch {
                sel,
                new,
                name,
                list,
            } => {
                let mut v: Vec<Line> = Vec::new();
                if *new {
                    v.push(Line::from(vec![
                        Span::styled("  nombre   ", Style::default().fg(t.fg_faint)),
                        Span::styled(format!("{name}▏"), Style::default().fg(t.fg)),
                    ]));
                    v.push(Line::from(Span::styled(
                        "  intro crear · esc volver",
                        Style::default().fg(t.fg_faint),
                    )));
                } else {
                    v.push(Line::from(vec![
                        Span::styled(
                            format!(" {} ", if *sel == 0 { "▸" } else { " " }),
                            Style::default().fg(t.accent),
                        ),
                        Span::styled("+ rama nueva…", Style::default().fg(t.accent2)),
                    ]));
                    for (i, b) in list.iter().enumerate() {
                        let s = i + 1 == *sel;
                        v.push(Line::from(vec![
                            Span::styled(
                                format!(" {} ", if s { "▸" } else { " " }),
                                Style::default().fg(t.accent),
                            ),
                            Span::styled(
                                ui::truncate(b, 34),
                                Style::default()
                                    .fg(if s { t.fg } else { t.fg_dim })
                                    .add_modifier(if s {
                                        Modifier::BOLD
                                    } else {
                                        Modifier::empty()
                                    }),
                            ),
                        ]));
                    }
                    if list.is_empty() {
                        v.push(Line::from(Span::styled(
                            "  sin ramas aún",
                            Style::default().fg(t.fg_faint),
                        )));
                    }
                }
                (" ⎇  Ramas ".to_string(), v)
            }
            Menu::Stash { sel, list } => {
                let mut v: Vec<Line> = Vec::new();
                for (i, label) in ["guardar cambios", "aplicar el último"].iter().enumerate() {
                    let s = i == *sel;
                    v.push(Line::from(vec![
                        Span::styled(
                            format!(" {} ", if s { "▸" } else { " " }),
                            Style::default().fg(t.accent),
                        ),
                        Span::styled(
                            *label,
                            Style::default()
                                .fg(if s { t.fg } else { t.fg_dim })
                                .add_modifier(if s { Modifier::BOLD } else { Modifier::empty() }),
                        ),
                    ]));
                }
                if !list.is_empty() {
                    v.push(Line::from(Span::styled(
                        "  ── guardados ──",
                        Style::default().fg(t.accent2),
                    )));
                }
                for (i, entry) in list.iter().enumerate() {
                    let s = i + 2 == *sel;
                    v.push(Line::from(vec![
                        Span::styled(
                            format!(" {} ", if s { "▸" } else { " " }),
                            Style::default().fg(t.accent),
                        ),
                        Span::styled(
                            ui::truncate(entry, 40),
                            Style::default().fg(if s { t.fg } else { t.fg_dim }),
                        ),
                    ]));
                }
                (" ⌗  Stash ".to_string(), v)
            }
            Menu::Actions { sel } => {
                let mut v: Vec<Line> = Vec::new();
                for (i, (label, _)) in ACTIONS.iter().enumerate() {
                    let s = i == *sel;
                    v.push(Line::from(vec![
                        Span::styled(
                            format!(" {} ", if s { "▸" } else { " " }),
                            Style::default().fg(t.accent),
                        ),
                        Span::styled(
                            *label,
                            Style::default()
                                .fg(if s { t.fg } else { t.fg_dim })
                                .add_modifier(if s { Modifier::BOLD } else { Modifier::empty() }),
                        ),
                    ]));
                }
                v.push(Line::from(""));
                v.push(Line::from(Span::styled(
                    "  intro ejecutar · esc cerrar",
                    Style::default().fg(t.fg_faint),
                )));
                (" ⚡  Acciones ".to_string(), v)
            }
            Menu::Add { sel, files, checks } => {
                let mut v: Vec<Line> = Vec::new();
                v.push(Line::from(Span::styled(
                    "  cada archivo se añade por su ruta (nunca «git add .»)",
                    Style::default().fg(t.fg_faint),
                )));
                v.push(Line::from(""));
                if files.is_empty() {
                    v.push(Line::from(Span::styled(
                        "  no hay nada que añadir · árbol limpio ✦",
                        Style::default().fg(t.ok),
                    )));
                }
                for (i, (status, path)) in files.iter().enumerate() {
                    let s = i == *sel;
                    let on = checks.get(i).copied().unwrap_or(false);
                    v.push(Line::from(vec![
                        Span::styled(
                            format!(" {} ", if s { "▸" } else { " " }),
                            Style::default().fg(t.accent),
                        ),
                        Span::styled(
                            format!("{} ", if on { "☑" } else { "☐" }),
                            Style::default().fg(if on { t.ok } else { t.fg_faint }),
                        ),
                        Span::styled(
                            format!("{status:<3}"),
                            Style::default().fg(if s { t.accent2 } else { t.fg_dim }),
                        ),
                        Span::styled(
                            ui::truncate(path, 44),
                            Style::default()
                                .fg(if s { t.fg } else { t.fg_dim })
                                .add_modifier(if s { Modifier::BOLD } else { Modifier::empty() }),
                        ),
                    ]));
                }
                v.push(Line::from(""));
                v.push(Line::from(Span::styled(
                    "  ↑/↓ elegir · espacio marcar · intro git add · esc cerrar",
                    Style::default().fg(t.fg_faint),
                )));
                (" ▧  Añadir al área de preparación ".to_string(), v)
            }
            Menu::Clone { url, dest } => {
                let v = vec![
                    Line::from(vec![
                        Span::styled("  url     ", Style::default().fg(t.fg_faint)),
                        Span::styled(
                            if url.is_empty() {
                                "▏".to_string()
                            } else {
                                format!("{url}▏")
                            },
                            Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  destino ", Style::default().fg(t.fg_faint)),
                        Span::styled(ui::truncate(dest, 46), Style::default().fg(t.accent2)),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        "  intro clonar · esc cancelar",
                        Style::default().fg(t.fg_faint),
                    )),
                ];
                (" ⤓  Clonar repositorio ".to_string(), v)
            }
        };

        let h = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
        let w = (area.width * 58 / 100).clamp(38, 64).min(area.width);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(w)) / 2,
            y: area.y + (area.height.saturating_sub(h)) / 2,
            width: w,
            height: h,
        };
        let buf = f.buffer_mut();
        ui::dim_area(buf, area, 0.55);
        let block = ui::panel_on(t, ctx.wp, rect, title.as_str(), 0.97);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
    }

    pub fn on_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Option<Action> {
        // ── Submenús abiertos ──
        if self.menu.is_some() {
            return self.menu_key(key, ctx);
        }

        let repos = self.repos(ctx);
        let len = repos.len();
        match key.code {
            KeyCode::Up => {
                let i = self.state.selected().unwrap_or(0);
                self.state.select(Some(i.saturating_sub(1)));
                Some(Action::Refresh)
            }
            KeyCode::Down => {
                let i = self.state.selected().unwrap_or(0);
                self.state.select(Some((i + 1).min(len.saturating_sub(1))));
                Some(Action::Refresh)
            }
            KeyCode::Char('r') => Some(Action::Refresh),
            // ── Añadir al área de preparación, ruta por ruta ──
            KeyCode::Char('a') => {
                let path = self.selected_path(ctx);
                if path.is_empty() {
                    return Some(Action::Toast("elige un repositorio primero".into()));
                }
                // Sólo lo que se puede añadir: sin stage aún.
                let files: Vec<(String, String)> = ctx
                    .git
                    .files
                    .iter()
                    .filter(|f| {
                        let y = f.status.chars().nth(1).unwrap_or(' ');
                        y == ' ' || y == '?'
                    })
                    .map(|f| (f.status.clone(), f.path.clone()))
                    .collect();
                if files.is_empty() {
                    return Some(Action::Toast(
                        "no hay nada que añadir · árbol limpio".into(),
                    ));
                }
                let checks = vec![true; files.len()];
                self.menu = Some(Menu::Add {
                    sel: 0,
                    files,
                    checks,
                });
                Some(Action::Keep)
            }
            // ── Quitar del área de preparación ──
            KeyCode::Char('u') => {
                let path = self.selected_path(ctx);
                if path.is_empty() {
                    return Some(Action::Toast("elige un repositorio primero".into()));
                }
                let staged: Vec<String> = ctx
                    .git
                    .files
                    .iter()
                    .filter(|f| {
                        let x = f.status.chars().next().unwrap_or(' ');
                        x != ' ' && x != '?'
                    })
                    .map(|f| f.path.clone())
                    .collect();
                if staged.is_empty() {
                    return Some(Action::Toast(
                        "no hay nada en el área de preparación".into(),
                    ));
                }
                let mut cmd = String::from("git restore --staged --");
                for p in &staged {
                    cmd.push(' ');
                    cmd.push_str(&quote(p));
                }
                Some(Action::Run(Job::RunIn { cwd: path, cmd }))
            }
            // ── Bitácora completa ──
            KeyCode::Char('l') => {
                let path = self.selected_path(ctx);
                if path.is_empty() {
                    return Some(Action::Keep);
                }
                Some(Action::Run(Job::View {
                    title: "git log".into(),
                    cwd: path,
                    cmd: "git log --graph --decorate --oneline -40".into(),
                }))
            }
            // ── Traer del remoto ──
            KeyCode::Char('f') => {
                let path = self.selected_path(ctx);
                if path.is_empty() {
                    return Some(Action::Keep);
                }
                Some(Action::Run(Job::View {
                    title: "git fetch".into(),
                    cwd: path,
                    cmd: "git fetch --all --prune".into(),
                }))
            }
            // ── Clonar una URL dentro de Nébula ──
            KeyCode::Char('k') => {
                let dest = clone_dest(ctx);
                self.menu = Some(Menu::Clone {
                    url: String::new(),
                    dest,
                });
                Some(Action::Keep)
            }
            // Menú local: sólo atajos (todo lo demás está en la tira).
            KeyCode::Char('c') => {
                if self.selected_path(ctx).is_empty() {
                    return Some(Action::Keep);
                }
                self.menu = Some(Menu::Commit {
                    kind: 0,
                    field: 0,
                    title: String::new(),
                    desc: String::new(),
                });
                Some(Action::Keep)
            }
            KeyCode::Char('b') => {
                let path = self.selected_path(ctx);
                if path.is_empty() {
                    return Some(Action::Keep);
                }
                let list = git_branches(&path);
                self.menu = Some(Menu::Branch {
                    sel: 0,
                    new: false,
                    name: String::new(),
                    list,
                });
                Some(Action::Keep)
            }
            KeyCode::Char('s') => {
                let path = self.selected_path(ctx);
                if path.is_empty() {
                    return Some(Action::Keep);
                }
                let list = git_stashes(&path);
                self.menu = Some(Menu::Stash { sel: 0, list });
                Some(Action::Keep)
            }
            KeyCode::Char('m') => {
                if self.selected_path(ctx).is_empty() {
                    return Some(Action::Keep);
                }
                self.menu = Some(Menu::Actions { sel: 0 });
                Some(Action::Keep)
            }
            KeyCode::Enter => {
                let path = self.selected_path(ctx);
                if path.is_empty() {
                    return Some(Action::Keep);
                }
                // Estatus dentro de Nébula: sin terminal por fuera.
                Some(Action::Run(Job::View {
                    title: "git status".into(),
                    cwd: path,
                    cmd: "git status --short --branch".into(),
                }))
            }
            KeyCode::Char('p') => {
                let path = self.selected_path(ctx);
                if path.is_empty() {
                    return Some(Action::Keep);
                }
                Some(Action::Confirm(
                    format!("¿git pull en {}?", ui::truncate(&path, 40)),
                    Job::View {
                        title: "git pull".into(),
                        cwd: path,
                        cmd: "git pull --ff-only".into(),
                    },
                ))
            }
            KeyCode::Char('P') => {
                let path = self.selected_path(ctx);
                if path.is_empty() {
                    return Some(Action::Keep);
                }
                Some(Action::Confirm(
                    format!("¿git push en {}?", ui::truncate(&path, 40)),
                    Job::View {
                        title: "git push".into(),
                        cwd: path,
                        cmd: "git push".into(),
                    },
                ))
            }
            KeyCode::Char('d') => {
                let path = self.selected_path(ctx);
                if path.is_empty() {
                    return Some(Action::Keep);
                }
                Some(Action::Run(Job::View {
                    title: "git diff".into(),
                    cwd: path,
                    cmd: "git diff --stat && git diff".into(),
                }))
            }
            _ => None,
        }
    }

    /// Teclas de los submenús (commit · ramas · stash).
    fn menu_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Option<Action> {
        let path = self.selected_path(ctx);
        // Se saca el menú para poder cerrarlo sin pelear con los préstamos.
        let mut menu = self.menu.take()?;
        let (keep, action) = menu_on_key(&mut menu, key, &path);
        if keep {
            self.menu = Some(menu);
        }
        action
    }
}

// ── Submenús locales ──────────────────────────────────────────────────────

/// Teclas de un submenú. Devuelve (seguir abierto, acción a ejecutar).
fn menu_on_key(menu: &mut Menu, key: KeyEvent, path: &str) -> (bool, Option<Action>) {
    match menu {
        Menu::Commit {
            kind,
            field,
            title,
            desc,
        } => match key.code {
            KeyCode::Esc => (false, Some(Action::Keep)),
            KeyCode::Tab => {
                *field = (*field + 1) % 3;
                (true, Some(Action::Keep))
            }
            KeyCode::BackTab => {
                *field = (*field + 2) % 3;
                (true, Some(Action::Keep))
            }
            KeyCode::Up => {
                *kind = kind.saturating_sub(1);
                (true, Some(Action::Keep))
            }
            KeyCode::Down => {
                *kind = (*kind + 1).min(KINDS.len() - 1);
                (true, Some(Action::Keep))
            }
            KeyCode::Backspace => {
                // Sólo se borra en los campos de texto.
                match *field {
                    1 => {
                        title.pop();
                    }
                    2 => {
                        desc.pop();
                    }
                    _ => {}
                }
                (true, Some(Action::Keep))
            }
            KeyCode::Char(ch) => {
                // El tipo se elige con ↑/↓; aquí sólo se escribe texto.
                match *field {
                    1 => title.push(ch),
                    2 => desc.push(ch),
                    _ => {}
                }
                (true, Some(Action::Keep))
            }
            KeyCode::Enter => {
                let head = title.trim().to_string();
                if head.is_empty() {
                    return (
                        false,
                        Some(Action::Toast("commit sin título · cancelado".into())),
                    );
                }
                if path.is_empty() {
                    return (
                        false,
                        Some(Action::Toast("sin repositorio seleccionado".into())),
                    );
                }
                let (prefix, _) = KINDS[*kind];
                let full = format!("{prefix}: {head}");
                // Aviso claro si git no tiene autor configurado.
                if crate::data::run_in(
                    "git",
                    &["config", "--get", "user.email"],
                    Some(std::path::Path::new(path)),
                )
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
                {
                    return (
                        false,
                        Some(Action::Toast(
                            "git no tiene autor (user.email) · commit cancelado".into(),
                        )),
                    );
                }
                let d = desc.trim().to_string();
                // Se commitea **lo preparado** (con a): nada de `git add -A`
                // ni `git add .` escondidos detrás de un botón.
                let staged = crate::data::run_in(
                    "git",
                    &["diff", "--cached", "--name-only"],
                    Some(std::path::Path::new(path)),
                )
                .unwrap_or_default();
                if staged.trim().is_empty() {
                    return (
                        false,
                        Some(Action::Toast(
                            "nada en el área de preparación · pulsa a".into(),
                        )),
                    );
                }
                (
                    false,
                    Some(Action::Run(Job::RunIn {
                        cwd: path.to_string(),
                        cmd: format!("git commit -m {} -m {}", quote(&full), quote(&d)),
                    })),
                )
            }
            _ => (true, Some(Action::Keep)),
        },
        Menu::Branch {
            sel,
            new,
            name,
            list,
        } => {
            // Escribiendo el nombre de la rama nueva.
            if *new {
                return match key.code {
                    KeyCode::Esc => {
                        *new = false;
                        name.clear();
                        (true, Some(Action::Keep))
                    }
                    KeyCode::Backspace => {
                        name.pop();
                        (true, Some(Action::Keep))
                    }
                    KeyCode::Char(ch) => {
                        if ch.is_alphanumeric() || ch == '-' || ch == '_' || ch == '/' {
                            name.push(ch);
                        }
                        (true, Some(Action::Keep))
                    }
                    KeyCode::Enter => {
                        let n = name.trim().to_string();
                        if n.is_empty() || path.is_empty() {
                            return (true, Some(Action::Toast("nombre de rama vacío".into())));
                        }
                        (
                            false,
                            Some(Action::Run(Job::RunIn {
                                cwd: path.to_string(),
                                cmd: format!("git switch -c {}", quote(&n)),
                            })),
                        )
                    }
                    _ => (true, Some(Action::Keep)),
                };
            }
            // Navegación: 0 = «rama nueva…» + lista de ramas.
            let rows = list.len() + 1;
            match key.code {
                KeyCode::Esc => (false, Some(Action::Keep)),
                KeyCode::Up => {
                    *sel = sel.saturating_sub(1);
                    (true, Some(Action::Keep))
                }
                KeyCode::Down => {
                    *sel = (*sel + 1).min(rows - 1);
                    (true, Some(Action::Keep))
                }
                KeyCode::Char('n') => {
                    *new = true;
                    name.clear();
                    (true, Some(Action::Keep))
                }
                KeyCode::Enter => {
                    if path.is_empty() {
                        return (
                            false,
                            Some(Action::Toast("sin repositorio seleccionado".into())),
                        );
                    }
                    if *sel == 0 {
                        return (
                            true,
                            Some(Action::Toast("pulsa n para escribir la rama".into())),
                        );
                    }
                    let Some(br) = list.get(*sel - 1).cloned() else {
                        return (true, None);
                    };
                    (
                        false,
                        Some(Action::Run(Job::RunIn {
                            cwd: path.to_string(),
                            cmd: format!("git switch {}", quote(&br)),
                        })),
                    )
                }
                _ => (true, Some(Action::Keep)),
            }
        }
        Menu::Stash { sel, list } => {
            // 0 = guardar · 1 = aplicar el último · 2+ = entradas concretas.
            let rows = list.len() + 2;
            match key.code {
                KeyCode::Esc => (false, Some(Action::Keep)),
                KeyCode::Up => {
                    *sel = sel.saturating_sub(1);
                    (true, Some(Action::Keep))
                }
                KeyCode::Down => {
                    *sel = (*sel + 1).min(rows - 1);
                    (true, Some(Action::Keep))
                }
                KeyCode::Enter => {
                    if path.is_empty() {
                        return (
                            false,
                            Some(Action::Toast("sin repositorio seleccionado".into())),
                        );
                    }
                    let cmd = match *sel {
                        0 => "git stash push -m nebula".to_string(),
                        1 => "git stash pop".to_string(),
                        i => {
                            let Some(entry) = list.get(i - 2) else {
                                return (true, None);
                            };
                            // «stash@{0}: mensaje» → sólo la referencia.
                            let refid = entry
                                .split(':')
                                .next()
                                .unwrap_or("stash@{0}")
                                .trim()
                                .to_string();
                            format!("git stash apply {}", quote(&refid))
                        }
                    };
                    (
                        false,
                        Some(Action::Run(Job::RunIn {
                            cwd: path.to_string(),
                            cmd,
                        })),
                    )
                }
                _ => (true, Some(Action::Keep)),
            }
        }
        Menu::Add { sel, files, checks } => {
            let rows = files.len();
            match key.code {
                KeyCode::Esc => (false, Some(Action::Keep)),
                KeyCode::Up => {
                    *sel = sel.saturating_sub(1);
                    (true, Some(Action::Keep))
                }
                KeyCode::Down => {
                    *sel = (*sel + 1).min(rows.saturating_sub(1));
                    (true, Some(Action::Keep))
                }
                // Marcar / desmarcar la casilla.
                KeyCode::Char(' ') => {
                    if let Some(c) = checks.get_mut(*sel) {
                        *c = !*c;
                    }
                    (true, Some(Action::Keep))
                }
                KeyCode::Char(ch) if ch.is_ascii_digit() && rows > 0 => {
                    // Atajo: 1-9 marca esa fila.
                    let n = ch as usize - '1' as usize;
                    if let Some(c) = checks.get_mut(n) {
                        *c = !*c;
                        *sel = n;
                    }
                    (true, Some(Action::Keep))
                }
                KeyCode::Enter => {
                    if path.is_empty() {
                        return (
                            false,
                            Some(Action::Toast("sin repositorio seleccionado".into())),
                        );
                    }
                    let chosen: Vec<&String> = files
                        .iter()
                        .zip(checks.iter())
                        .filter(|(_, on)| **on)
                        .map(|((_, p), _)| p)
                        .collect();
                    if chosen.is_empty() {
                        return (
                            true,
                            Some(Action::Toast("marca al menos un archivo (espacio)".into())),
                        );
                    }
                    // Rutas explícitas, una a una. Jamás `git add .`.
                    let mut cmd = String::from("git add --");
                    for p in &chosen {
                        cmd.push(' ');
                        cmd.push_str(&quote(p));
                    }
                    (
                        false,
                        Some(Action::Run(Job::RunIn {
                            cwd: path.to_string(),
                            cmd,
                        })),
                    )
                }
                _ => (true, Some(Action::Keep)),
            }
        }
        Menu::Clone { url, dest } => match key.code {
            KeyCode::Esc => (false, Some(Action::Keep)),
            KeyCode::Backspace => {
                url.pop();
                (true, Some(Action::Keep))
            }
            KeyCode::Char(ch) if !ch.is_control() => {
                url.push(ch);
                (true, Some(Action::Keep))
            }
            KeyCode::Enter => {
                let u = url.trim().to_string();
                if u.is_empty() {
                    return (
                        true,
                        Some(Action::Toast("escribe la URL del repositorio".into())),
                    );
                }
                if dest.is_empty() {
                    return (true, Some(Action::Toast("sin dónde clonar".into())));
                }
                (
                    false,
                    Some(Action::Confirm(
                        format!("¿clonar {} en {}?", ui::truncate(&u, 44), dest),
                        Job::View {
                            title: "git clone".into(),
                            cwd: dest.clone(),
                            cmd: format!("git clone {}", quote(&u)),
                        },
                    )),
                )
            }
            _ => (true, Some(Action::Keep)),
        },
        Menu::Actions { sel } => {
            let rows = ACTIONS.len();
            match key.code {
                KeyCode::Esc => (false, Some(Action::Keep)),
                KeyCode::Up => {
                    *sel = sel.saturating_sub(1);
                    (true, Some(Action::Keep))
                }
                KeyCode::Down => {
                    *sel = (*sel + 1).min(rows - 1);
                    (true, Some(Action::Keep))
                }
                KeyCode::Enter => {
                    if path.is_empty() {
                        return (
                            false,
                            Some(Action::Toast("sin repositorio seleccionado".into())),
                        );
                    }
                    let Some((_, cmd)) = ACTIONS.get(*sel) else {
                        return (true, None);
                    };
                    match cmd {
                        ActCmd::Run(c) => (
                            false,
                            Some(Action::Run(Job::View {
                                title: (*c).to_string(),
                                cwd: path.to_string(),
                                cmd: (*c).to_string(),
                            })),
                        ),
                        ActCmd::Pull => (
                            false,
                            Some(Action::Confirm(
                                format!("¿git pull en {}?", ui::truncate(path, 40)),
                                Job::View {
                                    title: "git pull".into(),
                                    cwd: path.to_string(),
                                    cmd: "git pull --ff-only".into(),
                                },
                            )),
                        ),
                        ActCmd::Push => (
                            false,
                            Some(Action::Confirm(
                                format!("¿git push en {}?", ui::truncate(path, 40)),
                                Job::View {
                                    title: "git push".into(),
                                    cwd: path.to_string(),
                                    cmd: "git push".into(),
                                },
                            )),
                        ),
                        ActCmd::Term => (
                            false,
                            Some(Action::Run(Job::Shell {
                                cwd: path.to_string(),
                                cmd: String::new(),
                            })),
                        ),
                    }
                }
                _ => (true, Some(Action::Keep)),
            }
        }
    }
}

/// Dónde caerá un clon nuevo: el primer directorio de proyectos que exista
/// (o la casa, si ninguno).
fn clone_dest(ctx: &Ctx) -> String {
    for d in &ctx.cfg.projects.dirs {
        let p = crate::config::expand(d);
        if p.is_dir() {
            return p.to_string_lossy().to_string();
        }
    }
    crate::config::home().to_string_lossy().to_string()
}

/// Ramas del repositorio (una por línea).
fn git_branches(path: &str) -> Vec<String> {
    crate::data::run_in(
        "git",
        &["branch", "--format=%(refname:short)"],
        Some(std::path::Path::new(path)),
    )
    .map(|s| {
        s.lines()
            .map(|l| l.trim().trim_start_matches("* ").to_string())
            .filter(|l| !l.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// Entradas del stash («stash@{0}: mensaje»).
fn git_stashes(path: &str) -> Vec<String> {
    crate::data::run_in("git", &["stash", "list"], Some(std::path::Path::new(path)))
        .map(|s| {
            s.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}
