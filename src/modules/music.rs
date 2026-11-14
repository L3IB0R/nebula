//! Música: reproductor, progreso y visualizador.

use crate::action::{Action, Job};
use crate::app::Ctx;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

#[derive(Default)]
pub struct Music {
    pub scroll: u16,
    /// Menú local (tecla m).
    pub menu: crate::menus::Menu,
}

/// Filas del menú local de Música.
const MUS_MENU: &[&str] = &[
    "reproducir / pausar",
    "siguiente",
    "anterior",
    "detener",
    "subir volumen",
    "bajar volumen",
    "mezclar (shuffle)",
    "repetir (repeat)",
    "arrancar cmus",
    "instalar cmus",
    "instalar cliamp",
];

/// Orden de transporte traducida al motor elegido.
///
/// cliamp usa subcomandos (`toggle`, `next`…); cmus-remote usa banderas
/// (`-t`, `-n`…). El volumen lleva la misma forma en ambos: «volume» más
/// un delta (cliamp lo toma en dB, cmus en porcentaje).
fn cmd_for(engine: &str, what: &str) -> Vec<String> {
    let table: &[(&str, &[&str])] = if engine == "cmus" {
        &[
            ("toggle", &["-t"]),
            ("next", &["-n"]),
            ("prev", &["-r"]),
            ("stop", &["-s"]),
            ("vol+", &["volume", "+2"]),
            ("vol-", &["volume", "-2"]),
            ("shuffle", &["-S"]),
            ("repeat", &["-R"]),
        ]
    } else {
        &[
            ("toggle", &["toggle"]),
            ("next", &["next"]),
            ("prev", &["prev"]),
            ("stop", &["stop"]),
            ("vol+", &["volume", "+2"]),
            ("vol-", &["volume", "-2"]),
            ("shuffle", &["shuffle"]),
            ("repeat", &["repeat"]),
        ]
    };
    table
        .iter()
        .find(|(k, _)| *k == what)
        .map(|(_, a)| a.iter().map(|s| (*s).to_string()).collect())
        .unwrap_or_default()
}

/// Motor que manda: el que está sonando, o el instalado por defecto.
fn engine_for(m: &crate::data::MusicSnap) -> &'static str {
    match m.engine.as_str() {
        "cmus" => "cmus",
        "cliamp" => "cliamp",
        _ if crate::data::which("cliamp").is_some() => "cliamp",
        _ => "cmus",
    }
}

/// Alterna reproducir/pausa con el motor que esté mandando.
///
/// Es lo que ejecuta el chip ♪ de la barra de estado: control de la música
/// desde cualquier pestaña, sin salir de donde estés.
pub fn toggle_action(m: &crate::data::MusicSnap) -> Action {
    let engine = engine_for(m);
    let args = cmd_for(engine, "toggle");
    Action::Run(if engine == "cmus" {
        Job::Cmus(args)
    } else {
        Job::Cliamp(args)
    })
}

impl Music {
    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let m = ctx.music;
        if area.height < 8 {
            return;
        }
        let rows = Layout::vertical([
            Constraint::Length(7),
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(area);

        // ── Estado vacío: toda la zona, con calma ──
        if !m.running {
            let block = ui::panel_on(t, ctx.wp, area, " ♪  Música ", 0.86);
            let inner = block.inner(area);
            f.render_widget(block, area);
            if inner.height < 6 {
                return;
            }
            let has_cliamp = crate::data::which("cliamp").is_some();
            let has_cmus = crate::data::which("cmus-remote").is_some();
            let pad = (inner.height.saturating_sub(7)) / 2;

            let mut lines: Vec<Line> = (0..pad).map(|_| Line::from("")).collect();
            lines.push(Line::from(Span::styled("♪", Style::default().fg(t.accent))).centered());
            lines.push(
                Line::from(Span::styled(
                    "Nada sonando ahora mismo",
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                ))
                .centered(),
            );
            lines.push(Line::from(""));
            lines.push(
                Line::from(Span::styled(
                    "Nébula escucha cliamp (online) y cmus (offline).",
                    Style::default().fg(t.fg_dim),
                ))
                .centered(),
            );
            lines.push(
                Line::from(Span::styled(
                    format!(
                        "detectados · cliamp {} · cmus {}",
                        if has_cliamp { "✓" } else { "✗" },
                        if has_cmus { "✓" } else { "✗" }
                    ),
                    Style::default().fg(t.fg_faint),
                ))
                .centered(),
            );
            lines.push(Line::from(""));

            // Sólo se ofrece instalar lo que de verdad falta.
            if !has_cmus || !has_cliamp {
                let mut opts: Vec<Span> = Vec::new();
                if !has_cmus {
                    opts.push(Span::styled(
                        "  [i] ",
                        Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                    ));
                    opts.push(Span::styled(
                        "instalar cmus (apt)     ",
                        Style::default().fg(t.fg_dim),
                    ));
                }
                if !has_cliamp {
                    if !has_cmus {
                        opts.push(Span::styled("  ", Style::default()));
                    }
                    opts.push(Span::styled(
                        "[I] ",
                        Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                    ));
                    opts.push(Span::styled(
                        "instalar cliamp",
                        Style::default().fg(t.fg_dim),
                    ));
                }
                lines.push(Line::from(opts).centered());
            } else {
                lines.push(
                    Line::from(Span::styled(
                        "✓ ambos instalados — pulsa espacio para arrancar cliamp",
                        Style::default().fg(t.ok),
                    ))
                    .centered(),
                );
            }
            f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
            // El menú se dibuja también con el estado vacío: antes de la
            // Fase 6 este `return` se lo comía y la tecla m nunca hacía nada.
            self.menu
                .render(f, area, " ♪  Música ", MUS_MENU, t, ctx.wp);
            return;
        }

        // ── Ahora suena ──
        let block = ui::panel_on(t, ctx.wp, rows[0], " ♪  Ahora suena ", 0.8);
        let inner = block.inner(rows[0]);
        f.render_widget(block, rows[0]);

        let state_icon = match m.state.as_str() {
            "playing" => ("▶", t.ok),
            "paused" => ("⏸", t.warn),
            _ => ("■", t.fg_dim),
        };
        let title_w = (inner.width as usize).saturating_sub(6);
        let title = marquee(&m.title, title_w, ctx.elapsed, m.state == "playing");

        let mut lines = vec![Line::from(vec![
            Span::styled(
                format!(" {} ", state_icon.0),
                Style::default().fg(state_icon.1),
            ),
            Span::styled(
                title,
                Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
            ),
        ])];
        lines.push(Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled(
                ui::truncate(&m.artist, title_w),
                Style::default().fg(t.accent),
            ),
            Span::styled("  ·  ", Style::default().fg(t.fg_faint)),
            Span::styled(
                ui::truncate(&m.album, title_w),
                Style::default().fg(t.fg_dim),
            ),
        ]));
        if !m.provider.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("   vía {}   motor {}", m.provider, m.engine),
                Style::default().fg(t.fg_faint),
            )));
        }
        let flags = format!(
            "{} shuffle · repeat:{} · vol {:.1} dB",
            if m.shuffle { "◉" } else { "○" },
            if m.repeat.is_empty() {
                "off"
            } else {
                &m.repeat
            },
            m.volume
        );
        lines.push(Line::from(Span::styled(
            flags,
            Style::default().fg(t.fg_dim),
        )));
        f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);

        // ── Progreso ──
        let block = ui::panel_on(t, ctx.wp, rows[1], "", 0.75);
        let inner = block.inner(rows[1]);
        f.render_widget(block, rows[1]);
        if inner.height >= 1 && m.duration > 0 {
            let pct = (m.position as f64 / m.duration as f64).clamp(0.0, 1.0);
            let w = (inner.width as usize).saturating_sub(16);
            let bar = ui::bar(pct as f32, w);
            f.render_widget(
                ratatui::widgets::Paragraph::new(Line::from(vec![
                    Span::styled(
                        format!("{} ", ui::human_time(m.position)),
                        Style::default().fg(t.fg_dim),
                    ),
                    Span::styled(bar, Style::default().fg(t.accent)),
                    Span::styled(
                        format!(" {}", ui::human_time(m.duration)),
                        Style::default().fg(t.fg_dim),
                    ),
                    Span::styled(
                        format!(" {:>3.0}%", pct * 100.0),
                        Style::default().fg(t.accent2),
                    ),
                ])),
                inner,
            );
        } else if inner.height >= 1 {
            f.render_widget(
                ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                    "duración desconocida",
                    Style::default().fg(t.fg_faint),
                ))),
                inner,
            );
        }

        // ── Visualizador ──
        let block = ui::panel_on(t, ctx.wp, rows[2], " ▁▂▃  Visualizador ", 0.75);
        let inner = block.inner(rows[2]);
        f.render_widget(block, rows[2]);
        self.vis(f, inner, ctx);

        // ── Atajos ──
        ui::hint_bar(
            f,
            t,
            rows[3],
            &[
                ("espacio", "play/pausa"),
                ("n/p", "siguiente/anterior"),
                ("+/-", "volumen"),
                ("s", "shuffle"),
                ("r", "repeat"),
                ("a", "arrancar cmus"),
                ("m", "menú"),
            ],
        );

        // ── Menú local (tecla m) ──
        self.menu
            .render(f, area, " ♪  Música ", MUS_MENU, t, ctx.wp);
    }

    fn vis(&self, f: &mut Frame, inner: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        if inner.height < 2 || inner.width < 4 {
            return;
        }
        let rows_n = inner.height as usize;
        let cols = inner.width as usize;
        let bands = ctx.vis;

        let lines: Vec<Line> = if bands.len() >= 4 {
            let max = bands.iter().cloned().fold(0.0f32, f32::max).max(0.001);
            (0..rows_n)
                .map(|r| {
                    let level = (rows_n - r) as f32 / rows_n as f32; // 1 arriba
                    let mut s = String::new();
                    for c in 0..cols {
                        let idx = c * bands.len() / cols;
                        let v = bands[idx] / max;
                        s.push(if v >= level {
                            '█'
                        } else if v >= level - 0.12 {
                            '▄'
                        } else {
                            ' '
                        });
                    }
                    let shade = 1.0 - (r as f32 / rows_n as f32) * 0.55;
                    Line::from(Span::styled(
                        s,
                        Style::default().fg(crate::wallpaper::tint(t.accent, shade * 0.35)),
                    ))
                })
                .collect()
        } else {
            // Sin datos: onda ambiente suave mientras suena.
            let playing = ctx.music.state == "playing";
            (0..rows_n)
                .map(|r| {
                    let mut s = String::new();
                    for c in 0..cols {
                        let phase = ctx.elapsed * 1.6 + c as f32 * 0.28 + r as f32 * 0.5;
                        let wave = (phase.sin() * 0.5 + 0.5) * (rows_n as f32 - 1.0);
                        let mid = (rows_n - 1 - r) as f32;
                        let hit = (wave - mid).abs() < 0.6 && playing;
                        s.push(if hit { '·' } else { ' ' });
                    }
                    Line::from(Span::styled(s, Style::default().fg(t.fg_faint)))
                })
                .collect()
        };
        f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
    }

    /// Lanza una orden de transporte al motor elegido (sin salir de aquí).
    fn transport(&self, ctx: &Ctx, what: &str) -> Option<Action> {
        let engine = engine_for(ctx.music);
        let args = cmd_for(engine, what);
        if args.is_empty() {
            return Some(Action::Keep);
        }
        Some(Action::Run(if engine == "cmus" {
            Job::Cmus(args)
        } else {
            Job::Cliamp(args)
        }))
    }

    pub fn on_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Option<Action> {
        // ── Menú local (m) ──
        if self.menu.open || matches!(key.code, KeyCode::Char('m')) {
            match self.menu.on_key(key, MUS_MENU.len()) {
                crate::menus::Nav::Pass => {}
                crate::menus::Nav::Handled => return Some(Action::Keep),
                crate::menus::Nav::Run(n) => {
                    return match n {
                        0 => self.transport(ctx, "toggle"),
                        1 => self.transport(ctx, "next"),
                        2 => self.transport(ctx, "prev"),
                        3 => self.transport(ctx, "stop"),
                        4 => self.transport(ctx, "vol+"),
                        5 => self.transport(ctx, "vol-"),
                        6 => self.transport(ctx, "shuffle"),
                        7 => self.transport(ctx, "repeat"),
                        8 => Some(Action::Run(Job::Cmus(Vec::new()))),
                        9 => Some(Action::Confirm(
                            "¿Instalar cmus con apt? (pide privilegios)".into(),
                            Job::Apt(vec!["install".into(), "-y".into(), "cmus".into()]),
                        )),
                        _ => Some(Action::Confirm(
                            "¿Instalar cliamp? (la salida saldrá aquí)".into(),
                            Job::View {
                                title: "instalar cliamp".into(),
                                cwd: String::new(),
                                cmd: "curl -fsSL https://cliamp.dev/install.sh | sh".into(),
                            },
                        )),
                    };
                }
            }
        }

        match key.code {
            KeyCode::Char(' ') => self.transport(ctx, "toggle"),
            KeyCode::Char('n') => self.transport(ctx, "next"),
            KeyCode::Char('p') => self.transport(ctx, "prev"),
            KeyCode::Char('t') => self.transport(ctx, "stop"),
            KeyCode::Char('+') | KeyCode::Char('=') => self.transport(ctx, "vol+"),
            KeyCode::Char('-') => self.transport(ctx, "vol-"),
            KeyCode::Char('s') => self.transport(ctx, "shuffle"),
            KeyCode::Char('r') => self.transport(ctx, "repeat"),
            // Arrancar cmus en su pty invisible y controlarlo desde aquí.
            KeyCode::Char('a') => Some(Action::Run(Job::Cmus(Vec::new()))),
            KeyCode::Char('i') if !crate::data::has("cmus-remote", "cmus") => {
                Some(Action::Confirm(
                    "¿Instalar cmus con apt? (pide privilegios)".into(),
                    Job::Apt(vec!["install".into(), "-y".into(), "cmus".into()]),
                ))
            }
            KeyCode::Char('I') if crate::data::which("cliamp").is_none() => Some(Action::Confirm(
                "¿Instalar cliamp? (la salida saldrá aquí)".into(),
                Job::View {
                    title: "instalar cliamp".into(),
                    cwd: String::new(),
                    cmd: "curl -fsSL https://cliamp.dev/install.sh | sh".into(),
                },
            )),
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll = self.scroll.saturating_add(3);
                Some(Action::Keep)
            }
            _ => None,
        }
    }
}

/// Título en marquesina: se desliza cuando no cabe.
fn marquee(text: &str, width: usize, elapsed: f32, playing: bool) -> String {
    if width == 0 {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return text.to_string();
    }
    let pad = 6;
    let total = chars.len() + pad;
    let offset = if playing {
        ((elapsed * 6.0) as usize) % total
    } else {
        0
    };
    let mut out = String::new();
    for i in 0..width {
        let idx = (offset + i) % total;
        if idx < chars.len() {
            out.push(chars[idx]);
        } else {
            out.push(' ');
        }
    }
    out
}
