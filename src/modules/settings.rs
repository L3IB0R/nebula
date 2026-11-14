//! Ajustes: categorías con panel de opciones, widgets del Inicio y
//! la instalación de recursos opcionales.

use crate::action::{Action, Job};
use crate::app::Ctx;
use crate::config::{
    clock_label, clock_style_label, presets, style_variants, wallpaper_candidates, widget_label,
    Config, CLOCK_SIZES, CLOCK_STYLES, WIDGETS,
};
use crate::resources;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

/// Categorías del panel izquierdo.
const CATS: [&str; 8] = [
    "Temas",
    "Apariencia",
    "Reloj",
    "Widgets",
    "Clima",
    "Sistema",
    "Recursos",
    "Terminal",
];
const ICONS: [&str; 8] = ["☾", "✦", "◷", "▦", "☀", "⚙", "❖", "❯"];

/// Fila del catálogo **Terminal**: estado de la sesión, los dos terminales
/// embebidos, cómo pararlos, el brillo y el modo terminal.
const TERMINAL: [&str; 8] = [
    "sesión",
    "terminal de cmus",
    "terminal de cliamp",
    "detener cmus",
    "detener cliamp",
    "brillo",
    "modo terminal",
    "reiniciar equipo",
];

const APPEARANCE: [&str; 8] = [
    "fondo",
    "wallpaper",
    "opacidad",
    "viñeta",
    "estrellas",
    "paneles",
    "animaciones",
    "pywal",
];
const CLOCK: [&str; 4] = ["estilo", "tamaño", "reloj 24 h", "segundos"];
const CLIMA: [&str; 4] = ["lugar libre", "país", "ciudad", "consultar ahora"];
const SYSTEM: [&str; 4] = ["nombre", "lateral", "recargar fondo", "guardar ahora"];

#[derive(Default)]
pub struct Settings {
    pub draft: Option<Config>,
    /// Categoría activa (0..8).
    pub cat: usize,
    /// Índice seleccionado dentro de cada categoría.
    pub sel: [usize; 8],
    pub editing_name: bool,
    /// Escribiendo el lugar libre del clima.
    pub editing_place: bool,
    /// cmus visto por última vez (evita preguntarle a cada frame).
    cmus_seen: Option<(bool, std::time::Instant)>,
    /// Ídem para el daemon de cliamp.
    cliamp_seen: Option<(bool, std::time::Instant)>,
}

impl Settings {
    /// ¿cmus está corriendo? Como mucho una consulta por segundo.
    fn cmus_alive(&mut self) -> bool {
        if !self
            .cmus_seen
            .is_some_and(|(_, at)| at.elapsed().as_secs() < 1)
        {
            let v = crate::data::cmus_running();
            self.cmus_seen = Some((v, std::time::Instant::now()));
        }
        self.cmus_seen.map(|(v, _)| v).unwrap_or(false)
    }

    /// ¿El daemon de cliamp está vivo? Como mucho una consulta por segundo.
    fn cliamp_alive(&mut self) -> bool {
        if !self
            .cliamp_seen
            .is_some_and(|(_, at)| at.elapsed().as_secs() < 1)
        {
            let v = crate::data::cliamp_running();
            self.cliamp_seen = Some((v, std::time::Instant::now()));
        }
        self.cliamp_seen.map(|(v, _)| v).unwrap_or(false)
    }
}

impl Settings {
    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        if self.draft.is_none() {
            self.draft = Some(ctx.cfg.clone());
        }
        let t = ctx.theme;
        // El pie ocupa exactamente la última fila: el mismo sitio que en el
        // resto de módulos (si dejamos dos, la fila del fondo queda sin pintar).
        let rows = Layout::vertical([Constraint::Min(8), Constraint::Length(1)]).split(area);
        let cols = Layout::horizontal([Constraint::Length(24), Constraint::Min(30)]).split(rows[0]);

        // ── Categorías ──
        let block = ui::panel_on(t, ctx.wp, cols[0], " ☰  Ajustes ", 0.84);
        let inner = block.inner(cols[0]);
        f.render_widget(block, cols[0]);
        let cats: Vec<Line> = CATS
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let selected = self.cat == i;
                Line::from(vec![
                    Span::styled(
                        format!(" {} ", if selected { "▸" } else { " " }),
                        Style::default().fg(t.accent),
                    ),
                    Span::styled(
                        format!("{} ", ICONS[i]),
                        Style::default().fg(if selected { t.accent2 } else { t.fg_faint }),
                    ),
                    Span::styled(
                        format!("{name:<11}"),
                        Style::default()
                            .fg(if selected { t.fg } else { t.fg_dim })
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    if i == 6 {
                        Span::styled(
                            format!("{}/{}", resources::installed_count(), resources::total()),
                            Style::default().fg(if selected { t.ok } else { t.fg_faint }),
                        )
                    } else {
                        Span::styled("", Style::default())
                    },
                ])
            })
            .collect();
        f.render_widget(ratatui::widgets::Paragraph::new(cats), inner);

        // ── Panel de opciones ──
        let title = format!(" {} {} ◀ ", ICONS[self.cat], CATS[self.cat]);
        let block = ui::panel_on(t, ctx.wp, cols[1], title.as_str(), 0.84);
        let inner = block.inner(cols[1]);
        f.render_widget(block, cols[1]);
        if inner.height == 0 {
            return;
        }
        let cfg = self.draft.as_ref().unwrap();
        match self.cat {
            0 => render_themes(f, inner, self, cfg, t),
            1 => render_appearance(f, inner, self, cfg, t),
            2 => render_clock(f, inner, self, cfg, t),
            3 => render_widgets(f, inner, self, cfg, t),
            4 => render_clima(f, inner, self, cfg, t),
            5 => render_system(f, inner, self, cfg, t),
            6 => render_resources(f, inner, self, t),
            _ => render_terminal(f, inner, self, t),
        }

        // ── Pie: los atajos, siempre a la vista ──
        let items: &[(&str, &str)] = match self.cat {
            3 => &[
                ("↑/↓", "widget"),
                ("espacio", "mostrar/ocultar"),
                ("f", "forma"),
                ("←/→", "categoría"),
                ("w", "guardar"),
            ],
            4 => &[
                ("↑/↓", "opción"),
                ("intro", "elegir / escribir"),
                ("←/→", "categoría"),
                ("w", "guardar"),
            ],
            6 => &[
                ("↑/↓", "recurso"),
                ("espacio", "instalar sección"),
                ("a", "instalar todo"),
                ("←/→", "categoría"),
                ("w", "guardar"),
            ],
            7 => &[
                ("↑/↓", "opción"),
                ("intro", "ejecutar"),
                ("+ / −", "brillo"),
                ("←/→", "categoría"),
                ("w", "guardar"),
            ],
            _ => &[
                ("←/→", "categoría"),
                ("↑/↓", "opción"),
                ("espacio", "cambiar"),
                ("w", "guardar"),
            ],
        };
        let mut spans = ui::hint_line(t, items).spans;
        // La ruta sólo si cabe entera: mejor un pie limpio que uno cortado.
        let base: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        let ruta = format!(" ruta: {}", crate::config::config_path().display());
        if base + ruta.chars().count() <= rows[1].width as usize {
            spans.push(Span::styled(" ruta: ", Style::default().fg(t.fg_faint)));
            spans.push(Span::styled(
                crate::config::config_path().display().to_string(),
                Style::default().fg(t.fg_faint),
            ));
        }
        f.render_widget(ratatui::widgets::Paragraph::new(Line::from(spans)), rows[1]);
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Option<Action> {
        // ── Edición del nombre ──
        if self.editing_name {
            return match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.editing_name = false;
                    Some(Action::ApplyConfig(Box::new(self.draft.clone().unwrap())))
                }
                KeyCode::Backspace => {
                    if let Some(c) = self.draft.as_mut() {
                        c.general.name.pop();
                    }
                    Some(Action::ApplyConfig(Box::new(self.draft.clone().unwrap())))
                }
                KeyCode::Char(ch) => {
                    if let Some(c) = self.draft.as_mut() {
                        c.general.name.push(ch);
                    }
                    Some(Action::ApplyConfig(Box::new(self.draft.clone().unwrap())))
                }
                _ => Some(Action::Keep),
            };
        }

        // ── Edición del lugar libre del clima ──
        if self.editing_place {
            return match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.editing_place = false;
                    Some(Action::ApplyConfig(Box::new(self.draft.clone().unwrap())))
                }
                KeyCode::Backspace => {
                    if let Some(c) = self.draft.as_mut() {
                        c.weather.place.pop();
                    }
                    Some(Action::Keep)
                }
                KeyCode::Char(ch) => {
                    if let Some(c) = self.draft.as_mut() {
                        c.weather.place.push(ch);
                    }
                    Some(Action::Keep)
                }
                _ => Some(Action::Keep),
            };
        }

        let cfg = self.draft.as_mut()?;
        let max = match self.cat {
            0 => presets().len().saturating_sub(1),
            1 => APPEARANCE.len() - 1,
            2 => CLOCK.len() - 1,
            3 => WIDGETS.len() - 1,
            4 => CLIMA.len() - 1,
            5 => SYSTEM.len() - 1,
            6 => res_rows().len().saturating_sub(1),
            _ => TERMINAL.len() - 1,
        };

        match key.code {
            // Brillo: el deslizador de la categoría Terminal se mueve con
            // las teclas físicas de un portátil (y con ←/→, aquí).
            KeyCode::Left | KeyCode::Right if self.cat == 7 && self.sel[7] == 5 => {
                crate::data::set_brightness(if key.code == KeyCode::Right { 10 } else { -10 });
                Some(Action::Keep)
            }
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Char('-') | KeyCode::Char('_')
                if self.cat == 7 =>
            {
                crate::data::set_brightness(match key.code {
                    KeyCode::Char('-') | KeyCode::Char('_') => -10,
                    _ => 10,
                });
                Some(Action::Keep)
            }
            KeyCode::Left => {
                self.cat = self.cat.saturating_sub(1);
                Some(Action::Keep)
            }
            KeyCode::Right => {
                self.cat = (self.cat + 1).min(CATS.len() - 1);
                Some(Action::Keep)
            }
            KeyCode::Up => {
                self.sel[self.cat] = self.sel[self.cat].saturating_sub(1);
                Some(Action::Keep)
            }
            KeyCode::Down => {
                self.sel[self.cat] = (self.sel[self.cat] + 1).min(max);
                Some(Action::Keep)
            }
            // Widgets: espacio muestra/oculta, f (o Intro) cambia la forma.
            KeyCode::Char(' ') if self.cat == 3 => {
                let w = WIDGETS[self.sel[3]].to_string();
                let on = cfg.home.on(&w);
                if on {
                    cfg.home.widgets.retain(|x| *x != w);
                } else {
                    cfg.home.widgets.push(w);
                }
                Some(Action::ApplyConfig(Box::new(self.draft.clone().unwrap())))
            }
            KeyCode::Char('f') | KeyCode::Enter if self.cat == 3 => {
                let w = WIDGETS[self.sel[3]];
                cfg.home.cycle_style(w);
                Some(Action::ApplyConfig(Box::new(self.draft.clone().unwrap())))
            }
            // Recursos: instalar la sección elegida, o todo.
            KeyCode::Char(' ') | KeyCode::Enter if self.cat == 6 => {
                let rows = res_rows();
                let (sec, _) = rows.get(self.sel[6]).copied().unwrap_or((0, None));
                install_section(sec)
            }
            KeyCode::Char('a') if self.cat == 6 => install_all(),
            // Terminal: los dos embebidos, pararlos, la TTY y el reinicio.
            KeyCode::Char(' ') | KeyCode::Enter if self.cat == 7 => {
                match self.sel[7] {
                    1 => Some(Action::Term(0)),
                    2 => Some(Action::Term(1)),
                    3 => Some(Action::StopCmus),
                    4 => Some(Action::StopCliamp),
                    // La sesión sólo se muestra; el brillo va con +/−.
                    6 => {
                        // En la TTY la acción es volver a la interfaz.
                        let s = crate::data::session();
                        if s.graphical() {
                            Some(Action::Confirm(
                                "Modo terminal: Nébula termina y el comando queda en la TTY. ¿Parar la interfaz gráfica?"
                                    .into(),
                                Job::Tty(vec![
                                    "pkexec".into(),
                                    "systemctl".into(),
                                    "isolate".into(),
                                    "multi-user.target".into(),
                                ]),
                            ))
                        } else {
                            Some(Action::Confirm(
                                "¿Volver a arrancar la interfaz gráfica?".into(),
                                Job::Tty(vec![
                                    "pkexec".into(),
                                    "systemctl".into(),
                                    "isolate".into(),
                                    "graphical.target".into(),
                                ]),
                            ))
                        }
                    }
                    7 => Some(Action::Confirm(
                        "¿Reiniciar el equipo?".into(),
                        Job::Tty(vec!["pkexec".into(), "systemctl".into(), "reboot".into()]),
                    )),
                    _ => Some(Action::Keep),
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                let i = self.sel[self.cat];
                match self.cat {
                    0 => {
                        if let Some(th) = presets().get(i) {
                            cfg.general.theme = th.name.clone();
                        }
                    }
                    1 => match i {
                        0 => {
                            let order = ["image", "transparent", "solid"];
                            let cur = order
                                .iter()
                                .position(|m| *m == cfg.appearance.background)
                                .unwrap_or(0);
                            cfg.appearance.background = order[(cur + 1) % order.len()].to_string();
                        }
                        1 => {
                            let list = wallpaper_candidates();
                            if !list.is_empty() {
                                let cur = if cfg.appearance.wallpaper.is_empty() {
                                    0
                                } else {
                                    list.iter()
                                        .position(|p| {
                                            p.to_string_lossy() == cfg.appearance.wallpaper
                                        })
                                        .unwrap_or(0)
                                };
                                let next = &list[(cur + 1) % list.len()];
                                cfg.appearance.wallpaper = next.to_string_lossy().to_string();
                            }
                        }
                        2 => {
                            let v = cfg.appearance.wallpaper_opacity + 0.05;
                            cfg.appearance.wallpaper_opacity = if v > 1.01 {
                                0.20
                            } else {
                                (v * 100.0).round() / 100.0
                            };
                        }
                        3 => cfg.appearance.vignette = !cfg.appearance.vignette,
                        4 => cfg.appearance.stars = !cfg.appearance.stars,
                        5 => {
                            cfg.appearance.panel = if cfg.appearance.panel == "glass" {
                                "solid"
                            } else {
                                "glass"
                            }
                            .to_string();
                        }
                        6 => cfg.appearance.animations = !cfg.appearance.animations,
                        7 => cfg.appearance.pywal = !cfg.appearance.pywal,
                        _ => {}
                    },
                    2 => match i {
                        0 => {
                            let cur = CLOCK_STYLES
                                .iter()
                                .position(|v| *v == cfg.general.clock_style)
                                .unwrap_or(0);
                            cfg.general.clock_style =
                                CLOCK_STYLES[(cur + 1) % CLOCK_STYLES.len()].to_string();
                        }
                        1 => {
                            let cur = CLOCK_SIZES
                                .iter()
                                .position(|v| *v == cfg.general.clock_size)
                                .unwrap_or(0);
                            cfg.general.clock_size =
                                CLOCK_SIZES[(cur + 1) % CLOCK_SIZES.len()].to_string();
                        }
                        2 => cfg.general.clock_24h = !cfg.general.clock_24h,
                        3 => cfg.general.show_seconds = !cfg.general.show_seconds,
                        _ => {}
                    },
                    4 => match i {
                        // Lugar libre: se escribe como el nombre.
                        0 => self.editing_place = true,
                        // País: al cambiarlo, la ciudad vuelve a la primera.
                        1 => {
                            let cur = crate::geo::COUNTRIES
                                .iter()
                                .position(|c| c.iso == cfg.weather.country)
                                .unwrap_or(0);
                            let next =
                                &crate::geo::COUNTRIES[(cur + 1) % crate::geo::COUNTRIES.len()];
                            cfg.weather.country = next.iso.to_string();
                            cfg.weather.city =
                                next.cities.first().copied().unwrap_or_default().to_string();
                            cfg.weather.place.clear();
                        }
                        // Ciudad: se recorre el catálogo del país elegido.
                        2 => {
                            let cities = crate::geo::cities_of(&cfg.weather.country);
                            if cities.is_empty() {
                                return Some(Action::Toast("elige primero un país".into()));
                            }
                            let cur = cities
                                .iter()
                                .position(|c| *c == cfg.weather.city)
                                .unwrap_or(0);
                            cfg.weather.city = cities[(cur + 1) % cities.len()].to_string();
                            cfg.weather.place.clear();
                            return Some(Action::ApplyConfig(Box::new(
                                self.draft.clone().unwrap(),
                            )));
                        }
                        3 => return Some(Action::RefreshWeather),
                        _ => {}
                    },
                    5 => match i {
                        0 => self.editing_name = true,
                        1 => cfg.general.sidebar = !cfg.general.sidebar,
                        2 => return Some(Action::ReloadWallpaper),
                        3 => {
                            return Some(Action::SaveConfig(Box::new(self.draft.clone().unwrap())))
                        }
                        _ => {}
                    },
                    _ => {}
                }
                Some(Action::ApplyConfig(Box::new(self.draft.clone().unwrap())))
            }
            KeyCode::Char('w') => Some(Action::SaveConfig(Box::new(self.draft.clone().unwrap()))),
            KeyCode::Char('r') => Some(Action::ReloadWallpaper),
            _ => None,
        }
    }
}

// ── Render por categoría ──────────────────────────────────────────────────

fn render_themes(
    f: &mut Frame,
    inner: Rect,
    st: &Settings,
    cfg: &Config,
    t: &crate::config::Theme,
) {
    let all = presets();
    let lines: Vec<Line> = all
        .iter()
        .enumerate()
        .map(|(i, th)| {
            let selected = st.sel[0] == i;
            let active = th.name == cfg.general.theme;
            Line::from(vec![
                Span::styled(
                    format!(" {} ", if selected { "▸" } else { " " }),
                    Style::default().fg(t.accent),
                ),
                Span::styled("●", Style::default().fg(th.accent)),
                Span::styled(" ", Style::default()),
                Span::styled("●", Style::default().fg(th.accent2)),
                Span::styled(" ", Style::default()),
                Span::styled("●", Style::default().fg(th.ok)),
                Span::styled(
                    format!("  {:<12}", th.name),
                    Style::default()
                        .fg(if selected { t.fg } else { t.fg_dim })
                        .add_modifier(if selected || active {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    format!("#{:02x}{:02x}{:02x}", r(th.bg), g(th.bg), b(th.bg)),
                    Style::default().fg(t.fg_faint),
                ),
                if active {
                    Span::styled("  ◀ activo", Style::default().fg(t.ok))
                } else {
                    Span::styled("", Style::default())
                },
            ])
        })
        .collect();
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

fn render_appearance(
    f: &mut Frame,
    inner: Rect,
    st: &Settings,
    cfg: &Config,
    t: &crate::config::Theme,
) {
    let wp_list = wallpaper_candidates();
    let wp_label = if cfg.appearance.wallpaper.is_empty() {
        let first = wp_list
            .first()
            .map(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })
            .unwrap_or_else(|| "ninguno".into());
        format!("auto ({first})")
    } else {
        cfg.appearance.wallpaper.clone()
    };
    let values = [
        cfg.appearance.background.clone(),
        ui::truncate(&wp_label, 44),
        slider(cfg.appearance.wallpaper_opacity),
        onoff(cfg.appearance.vignette),
        onoff(cfg.appearance.stars),
        cfg.appearance.panel.clone(),
        onoff(cfg.appearance.animations),
        onoff(cfg.appearance.pywal),
    ];
    let lines: Vec<Line> = APPEARANCE
        .iter()
        .zip(values.iter())
        .enumerate()
        .map(|(i, (k, v))| row_line(st.sel[1] == i, k, v, t))
        .collect();
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

fn render_clock(f: &mut Frame, inner: Rect, st: &Settings, cfg: &Config, t: &crate::config::Theme) {
    let values = [
        format!("{} ◀▶", clock_style_label(&cfg.general.clock_style)),
        format!("{} ◀▶", clock_label(&cfg.general.clock_size)),
        onoff(cfg.general.clock_24h),
        onoff(cfg.general.show_seconds),
    ];
    let lines: Vec<Line> = CLOCK
        .iter()
        .zip(values.iter())
        .enumerate()
        .map(|(i, (k, v))| row_line(st.sel[2] == i, k, v, t))
        .collect();
    let mut lines = lines;
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  los cinco estilos: digital · una línea · tablero · analógico · mínimo",
        Style::default().fg(t.fg_faint),
    )));
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

fn render_widgets(
    f: &mut Frame,
    inner: Rect,
    st: &Settings,
    cfg: &Config,
    t: &crate::config::Theme,
) {
    let lines: Vec<Line> = WIDGETS
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let selected = st.sel[3] == i;
            let on = cfg.home.on(w);
            let variants = style_variants(w);
            let cur = cfg.home.style_of(w);
            let pos = variants.iter().position(|v| *v == cur).unwrap_or(0);
            let style_txt = if on && variants.len() > 1 {
                format!("{} ◀▶", cur)
            } else if on {
                cur.to_string()
            } else {
                "oculto".to_string()
            };
            Line::from(vec![
                Span::styled(
                    format!(" {} ", if selected { "▸" } else { " " }),
                    Style::default().fg(t.accent),
                ),
                Span::styled(
                    format!("{} ", if on { "◉" } else { "○" }),
                    Style::default().fg(if on { t.ok } else { t.fg_faint }),
                ),
                Span::styled(
                    format!("{:<11}", widget_label(w)),
                    Style::default()
                        .fg(if selected { t.fg } else { t.fg_dim })
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    style_txt,
                    Style::default().fg(if on { t.accent2 } else { t.fg_faint }),
                ),
                if variants.len() > 1 && on {
                    Span::styled(
                        format!("  {}/{}", pos + 1, variants.len()),
                        Style::default().fg(t.fg_faint),
                    )
                } else {
                    Span::styled("", Style::default())
                },
            ])
        })
        .collect();
    let mut lines = lines;
    if inner.height > lines.len() as u16 + 2 {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  espacio: mostrar u ocultar · f o intro: cambiar la forma",
            Style::default().fg(t.fg_faint),
        )));
    }
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

fn render_clima(f: &mut Frame, inner: Rect, st: &Settings, cfg: &Config, t: &crate::config::Theme) {
    let country = crate::geo::COUNTRIES
        .iter()
        .find(|c| c.iso == cfg.weather.country)
        .map(|c| c.name)
        .unwrap_or("(sin país)");
    let city = if cfg.weather.city.is_empty() {
        "(sin ciudad)".to_string()
    } else {
        cfg.weather.city.clone()
    };
    let place = if st.editing_place {
        format!("{}▏", cfg.weather.place)
    } else if cfg.weather.place.is_empty() {
        "(vacío)".to_string()
    } else {
        cfg.weather.place.clone()
    };
    let values = [
        place,
        format!("{country} ◀▶"),
        format!("{city} ◀▶"),
        "consultar".to_string(),
    ];
    let lines: Vec<Line> = CLIMA
        .iter()
        .zip(values.iter())
        .enumerate()
        .map(|(i, (k, v))| row_line(st.sel[4] == i, k, v, t))
        .collect();
    let mut lines = lines;
    // Lo que de verdad se le pide a wttr.in.
    let q = cfg.weather.query();
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  consulta: ", Style::default().fg(t.fg_faint)),
        Span::styled(
            if q.is_empty() {
                "automática (por IP)".to_string()
            } else {
                q
            },
            Style::default().fg(t.accent2),
        ),
    ]));
    if inner.height > lines.len() as u16 + 2 {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  el lugar libre manda sobre país/ciudad · intro para escribirlo",
            Style::default().fg(t.fg_faint),
        )));
    }
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

/// Filas de la categoría Terminal: en qué sesión vivimos, los dos
/// terminales embebidos, cómo pararlos, el brillo y la salida a la TTY.
fn render_terminal(f: &mut Frame, inner: Rect, st: &mut Settings, t: &crate::config::Theme) {
    let s = crate::data::session();
    let dm = if s.dm.is_empty() {
        "—"
    } else {
        s.dm.as_str()
    };
    let cmus = if st.cmus_alive() {
        "en marcha"
    } else {
        "parado"
    };
    let cliamp = if st.cliamp_alive() {
        "daemon activo"
    } else {
        "parado"
    };
    let values = [
        format!("{} · {}", s.kind, dm),
        format!("▸ abrir · {cmus}"),
        format!("▸ abrir · {cliamp}"),
        "■ detener".to_string(),
        "■ detener".to_string(),
        match crate::data::brightness_pct() {
            Some(v) => slider(v / 100.0),
            None => "no regulable".to_string(),
        },
        if s.graphical() {
            "salir y dejar la TTY".to_string()
        } else {
            "volver a la interfaz".to_string()
        },
        "pkexec reboot".to_string(),
    ];
    let mut lines: Vec<Line> = TERMINAL
        .iter()
        .zip(values.iter())
        .enumerate()
        .map(|(i, (k, v))| row_line(st.sel[7] == i, k, v, t))
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            "  objetivo {} · tty {}",
            s.target,
            crate::ui::truncate(&crate::data::tty_name(), 34)
        ),
        Style::default().fg(t.fg_faint),
    )));
    lines.push(Line::from(Span::styled(
        "  el brillo también se mueve con + y − (teclas del portátil)",
        Style::default().fg(t.fg_faint),
    )));
    lines.push(Line::from(Span::styled(
        "  al salir a la TTY, pkexec pregunta en una terminal de verdad",
        Style::default().fg(t.fg_faint),
    )));
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

fn render_system(
    f: &mut Frame,
    inner: Rect,
    st: &Settings,
    cfg: &Config,
    t: &crate::config::Theme,
) {
    let values = [
        if st.editing_name {
            format!("{}▏", cfg.general.name)
        } else {
            format!("{} ◀▶", cfg.general.name)
        },
        onoff(cfg.general.sidebar),
        "recargar".to_string(),
        "guardar".to_string(),
    ];
    let lines: Vec<Line> = SYSTEM
        .iter()
        .zip(values.iter())
        .enumerate()
        .map(|(i, (k, v))| row_line(st.sel[5] == i, k, v, t))
        .collect();
    let mut lines = lines;
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  la lateral también se abre y cierra con ctrl+b",
        Style::default().fg(t.fg_faint),
    )));
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

/// Filas planas del catálogo de recursos: (sección, item) · item = None → cabecera.
fn res_rows() -> Vec<(usize, Option<usize>)> {
    let mut out = Vec::new();
    for (si, s) in resources::SECTIONS.iter().enumerate() {
        out.push((si, None));
        for ii in 0..s.items.len() {
            out.push((si, Some(ii)));
        }
    }
    out
}

fn render_resources(f: &mut Frame, inner: Rect, st: &Settings, t: &crate::config::Theme) {
    let rows = res_rows();
    let sel = st.sel[6].min(rows.len().saturating_sub(1));
    let mut lines: Vec<Line> = Vec::new();
    for (i, (si, item)) in rows.iter().enumerate() {
        let selected = i == sel;
        let sec = &resources::SECTIONS[*si];
        let marker = Span::styled(
            format!(" {} ", if selected { "▸" } else { " " }),
            Style::default().fg(t.accent),
        );
        match item {
            None => {
                let (have, tot) = {
                    let mut have = 0;
                    for ii in 0..sec.items.len() {
                        if resources::is_installed(*si, ii) {
                            have += 1;
                        }
                    }
                    (have, sec.items.len())
                };
                lines.push(Line::from(vec![
                    marker,
                    Span::styled(
                        format!("{} {}", sec.icon, sec.name),
                        Style::default()
                            .fg(if selected { t.fg } else { t.accent2 })
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  {have}/{tot}"),
                        Style::default().fg(if have == tot { t.ok } else { t.warn }),
                    ),
                ]));
            }
            Some(ii) => {
                let it = &sec.items[*ii];
                let ok = resources::is_installed(*si, *ii);
                lines.push(Line::from(vec![
                    marker,
                    Span::styled("   ", Style::default()),
                    Span::styled(
                        format!("{} ", if ok { "✓" } else { "✗" }),
                        Style::default().fg(if ok { t.ok } else { t.fg_faint }),
                    ),
                    Span::styled(
                        format!("{:<20}", it.label),
                        Style::default()
                            .fg(if selected { t.fg } else { t.fg_dim })
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(
                        if it.apt.is_empty() {
                            "instalador propio".to_string()
                        } else {
                            format!("apt {}", it.apt)
                        },
                        Style::default().fg(t.fg_faint),
                    ),
                ]));
            }
        }
        // Se dibuja por páginas si no cabe todo.
        if lines.len() as u16 >= inner.height {
            break;
        }
    }
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

// ── Ayudas ────────────────────────────────────────────────────────────────

fn row_line(selected: bool, k: &str, v: &str, t: &crate::config::Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {} ", if selected { "▸" } else { " " }),
            Style::default().fg(t.accent),
        ),
        Span::styled(
            format!("{k:<16}"),
            Style::default().fg(if selected { t.fg } else { t.fg_dim }),
        ),
        Span::styled(
            v.to_string(),
            Style::default()
                .fg(if selected { t.accent2 } else { t.fg_faint })
                .add_modifier(if selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
    ])
}

/// Slider proporcional de 0..=1 a 10 bloques.
fn slider(v: f32) -> String {
    let n = ((v.clamp(0.0, 1.0)) * 10.0).round() as usize;
    format!("{}{}", "█".repeat(n), "░".repeat(10 - n))
}

fn onoff(v: bool) -> String {
    if v {
        "◉ sí".into()
    } else {
        "○ no".into()
    }
}

fn r(c: ratatui::style::Color) -> u8 {
    match c {
        ratatui::style::Color::Rgb(r, _, _) => r,
        _ => 0,
    }
}
fn g(c: ratatui::style::Color) -> u8 {
    match c {
        ratatui::style::Color::Rgb(_, g, _) => g,
        _ => 0,
    }
}
fn b(c: ratatui::style::Color) -> u8 {
    match c {
        ratatui::style::Color::Rgb(_, _, b) => b,
        _ => 0,
    }
}

// ── Instalación de recursos ───────────────────────────────────────────────

/// Pide confirmación para instalar lo que falta en una sección.
fn install_section(section: usize) -> Option<Action> {
    let pkgs = resources::missing_in(section);
    let cmds = resources::missing_cmds_in(section);
    if pkgs.is_empty() && cmds.is_empty() {
        return Some(Action::Toast(
            "✓ esa sección ya tiene todo instalado".into(),
        ));
    }
    let name = resources::SECTIONS
        .get(section)
        .map(|s| s.name)
        .unwrap_or("recurso");
    let msg = if pkgs.is_empty() {
        format!("¿Instalar {} con su instalador propio?", cmds[0].0)
    } else {
        format!(
            "¿Instalar {} paquete(s) de «{name}» con apt? (pide privilegios)",
            pkgs.len()
        )
    };
    Some(Action::Confirm(msg, Job::Install { pkgs, cmds }))
}

/// Pide confirmación para instalar todo lo que falta del catálogo.
fn install_all() -> Option<Action> {
    let pkgs = resources::missing_pkgs();
    let cmds = resources::missing_cmds();
    if pkgs.is_empty() && cmds.is_empty() {
        return Some(Action::Toast(
            "✓ Nébula no detecta nada que instalar".into(),
        ));
    }
    let n = pkgs.len() + cmds.len();
    Some(Action::Confirm(
        format!("¿Instalar los {n} recursos que faltan? (un solo pkexec)"),
        Job::Install { pkgs, cmds },
    ))
}
