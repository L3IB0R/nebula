//! Inicio: reloj grande, saludo y widgets del escritorio.

use crate::action::Action;
use crate::app::Ctx;
use crate::ui;
use chrono::{Datelike, Timelike};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Sparkline;
use ratatui::Frame;

pub struct Home;

impl Home {
    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        if area.height < 12 || area.width < 40 {
            let msg = paragraph_center(vec![Line::from(Span::styled(
                "Nébula necesita un terminal más grande ✦",
                Style::default().fg(t.fg_dim),
            ))]);
            f.render_widget(msg, area);
            return;
        }

        // Altura de la cabecera: depende del estilo y el tamaño del reloj.
        let hdr = hdr_height(ctx, area);

        // ── Banda de clima (si está activo) ──
        let clima_on = ctx.cfg.home.on("clima");
        let clima_h = if clima_on {
            if ctx.cfg.home.style_of("clima") == "linea" {
                3
            } else {
                5
            }
        } else {
            0
        };

        // ── Resto de widgets, en el orden elegido ──
        let rest: Vec<&str> = ctx
            .cfg
            .home
            .widgets
            .iter()
            .map(|s| s.as_str())
            .filter(|w| *w != "clima")
            .collect();

        let mut cons = vec![Constraint::Length(hdr)];
        if clima_on {
            cons.push(Constraint::Length(clima_h));
        }
        if !rest.is_empty() {
            cons.push(Constraint::Min(6));
        }
        // Pie de atajos: siempre la última fila.
        cons.push(Constraint::Length(1));
        let rows = Layout::vertical(cons).split(area);

        self.header(f, rows[0], ctx);
        let mut next = 1;
        if clima_on {
            weather_widget(f, rows[next], ctx);
            next += 1;
        }
        ui::hint_bar(
            f,
            t,
            rows[rows.len() - 1],
            &[
                ("intro", "lanzador"),
                ("tab", "secciones"),
                ("r", "refrescar"),
                ("?", "ayuda"),
            ],
        );
        if rest.is_empty() {
            return;
        }
        grid(f, rows[next], &rest, ctx);
    }

    fn header(&self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let now = ctx.now;
        if area.height == 0 {
            return;
        }

        // ── Saludo (siempre en la primera fila) ──
        let greet = match now.hour() {
            0..=4 => "Buenas noches",
            5..=11 => "Buenos días",
            12..=19 => "Buenas tardes",
            _ => "Buenas noches",
        };
        let name = if ctx.cfg.general.name.is_empty() {
            "cariño".to_string()
        } else {
            ctx.cfg.general.name.clone()
        };
        let greeting = Line::from(vec![
            Span::styled(format!("{greet}, "), Style::default().fg(t.fg_dim)),
            Span::styled(
                name,
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ✦", Style::default().fg(t.accent2)),
        ])
        .centered();
        f.render_widget(
            ratatui::widgets::Paragraph::new(greeting),
            Rect { height: 1, ..area },
        );

        // ── Cuerpo del reloj (debajo del saludo) ──
        let body = Rect {
            y: area.y + 1,
            height: area.height - 1,
            ..area
        };
        if body.height == 0 {
            return;
        }
        match ctx.cfg.general.clock_style.as_str() {
            "linea" => clock_linea(f, body, ctx),
            "flip" => clock_flip(f, body, ctx),
            "reloj" => clock_reloj(f, body, ctx),
            "minimal" => clock_minimal(f, body, ctx),
            _ => clock_digital(f, body, ctx),
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Enter => Some(Action::Launcher),
            KeyCode::Char('r') => Some(Action::Refresh),
            _ => None,
        }
    }
}

/// Tarjeta de clima (wttr.in, en hilo aparte con caché de 10 min).
fn weather_widget(f: &mut Frame, area: Rect, ctx: &Ctx) {
    let t = ctx.theme;
    // Variante "linea": todo en una sola fila, sin panel.
    if ctx.cfg.home.style_of("clima") == "linea" {
        let w = ctx.weather;
        let spans = if w.ok {
            vec![
                Span::styled(format!("{} ", w.icon), Style::default().fg(t.accent)),
                Span::styled(
                    format!("{} °C", w.temp),
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ·  {}  ·  {}", w.desc, w.place),
                    Style::default().fg(t.fg_dim),
                ),
            ]
        } else {
            vec![Span::styled(
                " consultando el clima…",
                Style::default().fg(t.fg_faint),
            )]
        };
        f.render_widget(ratatui::widgets::Paragraph::new(Line::from(spans)), area);
        return;
    }
    let block = ui::panel_on(t, ctx.wp, area, "☀  Clima", 0.82);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let w = ctx.weather;
    if !w.ok {
        // Spinner suave mientras consulta.
        let dots = ".".repeat(1 + (ctx.elapsed as usize % 3));
        let line = Line::from(Span::styled(
            format!(" consultando el clima{dots}"),
            Style::default().fg(t.fg_faint),
        ));
        f.render_widget(ratatui::widgets::Paragraph::new(line), inner);
        return;
    }
    let l1 = Line::from(vec![
        Span::styled(
            format!(" {}  ", w.icon),
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} °C", w.temp),
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  ·  {}", w.desc), Style::default().fg(t.fg)),
        if w.temp != w.feels {
            Span::styled(
                format!("  (se siente {}°)", w.feels),
                Style::default().fg(t.fg_dim),
            )
        } else {
            Span::styled("", Style::default())
        },
    ]);
    f.render_widget(ratatui::widgets::Paragraph::new(l1), inner);

    if inner.height >= 2 {
        let l2 = Line::from(vec![
            Span::styled(
                format!("humedad {}%", w.humidity),
                Style::default().fg(t.fg_dim),
            ),
            Span::styled("   ·   ", Style::default().fg(t.fg_faint)),
            Span::styled(
                format!("viento {:.0} km/h", w.wind),
                Style::default().fg(t.fg_dim),
            ),
            Span::styled("   ·   ", Style::default().fg(t.fg_faint)),
            Span::styled(w.place.clone(), Style::default().fg(t.fg_dim)),
        ]);
        f.render_widget(
            ratatui::widgets::Paragraph::new(l2),
            Rect {
                x: inner.x,
                y: inner.y + 1,
                width: inner.width,
                height: 1,
            },
        );
    }
    if inner.height >= 3 {
        let l3 = Line::from(Span::styled(
            format!("actualizado a las {}", w.at),
            Style::default().fg(t.fg_faint),
        ));
        f.render_widget(
            ratatui::widgets::Paragraph::new(l3),
            Rect {
                x: inner.x,
                y: inner.y + 2,
                width: inner.width,
                height: 1,
            },
        );
    }
}

fn paragraph_center(lines: Vec<Line<'_>>) -> ratatui::widgets::Paragraph<'_> {
    ratatui::widgets::Paragraph::new(lines).alignment(Alignment::Center)
}

fn panel<'a>(ctx: &Ctx<'a>, area: Rect, title: &str) -> ui::Panel<'a> {
    ui::panel_on(ctx.theme, ctx.wp, area, title.to_string(), 0.78)
}

// ── Rejilla dinámica de widgets ───────────────────────────────────────────

/// Reparte `names` por el área en columnas equilibradas y pinta cada uno.
///
/// El número de columnas crece con la cantidad de widgets para que ninguno
/// quede aplastado ni sobre dimensionado.
fn grid(f: &mut Frame, area: Rect, names: &[&str], ctx: &Ctx) {
    if names.is_empty() || area.width < 20 || area.height < 4 {
        return;
    }
    let n = names.len();
    let cols = match n {
        1 => 1,
        2..=3 => 2,
        4..=6 => 3,
        _ => 4,
    };
    let rows_n = n.div_ceil(cols);

    let col_w = area.width / cols as u16;
    let row_h = area.height / rows_n as u16;
    if col_w < 12 || row_h < 4 {
        // Sin sitio: apilamos en una sola columna con lo que quepa.
        let h = area.height / n as u16;
        for (i, name) in names.iter().enumerate() {
            let r = Rect {
                y: area.y + h * i as u16,
                height: h,
                ..area
            };
            draw_named(f, r, name, ctx);
        }
        return;
    }

    for (i, name) in names.iter().enumerate() {
        let row = i / cols;
        let col = i % cols;
        // La última fila puede quedar corta: estiramos su último widget.
        let in_row = (n - row * cols).min(cols);
        let w = if col == in_row - 1 {
            area.width - col_w * col as u16
        } else {
            col_w
        };
        let h = if row == rows_n - 1 {
            area.height - row_h * row as u16
        } else {
            row_h
        };
        let r = Rect {
            x: area.x + col_w * col as u16,
            y: area.y + row_h * row as u16,
            width: w,
            height: h,
        };
        draw_named(f, r, name, ctx);
    }
}

/// Pinta el widget `name` (desconocidos se ignoran en silencio).
fn draw_named(f: &mut Frame, area: Rect, name: &str, ctx: &Ctx) {
    if area.width < 8 || area.height < 3 {
        return;
    }
    match name {
        "clima" => weather_widget(f, area, ctx),
        "musica" => music_widget(f, area, ctx),
        "sistema" => sys_widget(f, area, ctx),
        "proyectos" => projects_widget(f, area, ctx),
        "red" => net_widget(f, area, ctx),
        "disco" => disk_widget(f, area, ctx),
        "mundo" => world_widget(f, area, ctx),
        _ => {}
    }
}

// ── Widgets nuevos ────────────────────────────────────────────────────────

/// Uso de los discos: una barra por partición montada.
fn disk_widget(f: &mut Frame, area: Rect, ctx: &Ctx) {
    let t = ctx.theme;
    let style = ctx.cfg.home.style_of("disco");
    let title = " ▤  Discos ";
    let block = panel(ctx, area, title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let disks = &ctx.sys.disks;
    if disks.is_empty() {
        f.render_widget(
            ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                "sin particiones legibles",
                Style::default().fg(t.fg_faint),
            ))),
            inner,
        );
        return;
    }
    let w = inner.width as usize;
    let mut lines = Vec::new();
    for d in disks.iter().take(inner.height as usize) {
        let used = d.total.saturating_sub(d.avail);
        let pct = if d.total > 0 {
            used as f32 / d.total as f32
        } else {
            0.0
        };
        let col = level_color(t, pct);
        if style == "lista" {
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {:<10}", ui::truncate(&d.mount, 10)),
                    Style::default().fg(t.fg),
                ),
                Span::styled(
                    format!("{:>8}", ui::human_bytes(d.avail)),
                    Style::default().fg(col).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" libres", Style::default().fg(t.fg_faint)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {:<10}", ui::truncate(&d.mount, 10)),
                    Style::default().fg(t.fg),
                ),
                Span::styled(
                    format!("{:>5.0}%", pct * 100.0),
                    Style::default().fg(col).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", Style::default()),
                Span::styled(ui::bar(pct, w.saturating_sub(22)), Style::default().fg(col)),
            ]));
        }
    }
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

/// Relojes del mundo: varias ciudades con su hora local fija por huso.
fn world_widget(f: &mut Frame, area: Rect, ctx: &Ctx) {
    let t = ctx.theme;
    let style = ctx.cfg.home.style_of("mundo");
    let block = panel(ctx, area, " ◷  Mundo ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    // Husos fijos (sin horario de verano): suficiente para una lectura rápida.
    let cities: [(&str, i32); 5] = [
        ("UTC", 0),
        ("Madrid", 1),
        ("Caracas", -4),
        ("Ciudad de México", -6),
        ("Tokio", 9),
    ];
    let now = ctx.now;
    let mut lines = Vec::new();
    for (i, (city, off)) in cities.iter().enumerate() {
        if i >= inner.height as usize {
            break;
        }
        let local = *now + chrono::Duration::hours(*off as i64);
        let hhmm = format!("{:02}:{:02}", local.hour(), local.minute());
        if style == "linea" {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", ui::truncate(city, 8)),
                    Style::default().fg(t.fg_dim),
                ),
                Span::styled(
                    hhmm,
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {:<16}", ui::truncate(city, 16)),
                    Style::default().fg(t.fg_dim),
                ),
                Span::styled(
                    hhmm,
                    Style::default().fg(t.accent2).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  UTC{off:+}"), Style::default().fg(t.fg_faint)),
            ]));
        }
    }
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

// ── Widgets ───────────────────────────────────────────────────────────────

fn music_widget(f: &mut Frame, area: Rect, ctx: &Ctx) {
    let t = ctx.theme;
    let m = ctx.music;
    // Variante "linea": una sola fila con lo esencial.
    if ctx.cfg.home.style_of("musica") == "linea" {
        let spans = if !m.running {
            vec![Span::styled(
                "♪ sin reproducción activa",
                Style::default().fg(t.fg_faint),
            )]
        } else {
            let icon = match m.state.as_str() {
                "playing" => "▶",
                "paused" => "⏸",
                _ => "■",
            };
            let mut v = vec![
                Span::styled(format!("{icon} "), Style::default().fg(t.ok)),
                Span::styled(
                    ui::truncate(&m.title, (area.width as usize).saturating_sub(6)),
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                ),
            ];
            if !m.artist.is_empty() {
                v.push(Span::styled("  ·  ", Style::default().fg(t.fg_faint)));
                v.push(Span::styled(
                    ui::truncate(&m.artist, (area.width as usize).saturating_sub(24)),
                    Style::default().fg(t.fg_dim),
                ));
            }
            v
        };
        f.render_widget(ratatui::widgets::Paragraph::new(Line::from(spans)), area);
        return;
    }
    let block = panel(ctx, area, " ♪  Música ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 {
        return;
    }
    let mut lines = Vec::new();
    if !m.running {
        lines.push(Line::from(Span::styled(
            "sin reproducción activa",
            Style::default().fg(t.fg_faint),
        )));
        lines.push(Line::from(Span::styled(
            "abre el reproductor con ♪",
            Style::default().fg(t.fg_dim),
        )));
    } else {
        let icon = match m.state.as_str() {
            "playing" => "▶",
            "paused" => "⏸",
            _ => "■",
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{icon} "), Style::default().fg(t.ok)),
            Span::styled(
                ui::truncate(&m.title, (inner.width as usize).saturating_sub(4)),
                Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
            ),
        ]));
        if !m.artist.is_empty() {
            lines.push(Line::from(Span::styled(
                ui::truncate(&m.artist, inner.width as usize),
                Style::default().fg(t.fg_dim),
            )));
        }
        if m.duration > 0 {
            let pct = (m.position as f32 / m.duration as f32).clamp(0.0, 1.0);
            lines.push(Line::from(vec![
                Span::styled(
                    ui::thin_bar(pct, (inner.width as usize).saturating_sub(15)),
                    Style::default().fg(t.accent),
                ),
                Span::styled(
                    format!(" {}", ui::human_time(m.position)),
                    Style::default().fg(t.fg_faint),
                ),
            ]));
        }
        if !m.provider.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("vía {}", m.provider),
                Style::default().fg(t.fg_faint),
            )));
        }
    }
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

fn sys_widget(f: &mut Frame, area: Rect, ctx: &Ctx) {
    let t = ctx.theme;
    let s = ctx.sys;
    let style = ctx.cfg.home.style_of("sistema");
    // Variante "mini": una sola línea con CPU y RAM.
    if style == "mini" {
        let mem_pct = if s.mem_total > 0 {
            (s.mem_used as f64 / s.mem_total as f64) as f32
        } else {
            0.0
        };
        let spans = vec![
            Span::styled("cpu ", Style::default().fg(t.fg_dim)),
            Span::styled(
                format!("{:.0}%", s.cpu),
                Style::default()
                    .fg(level_color(t, s.cpu / 100.0))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ·  ram ", Style::default().fg(t.fg_dim)),
            Span::styled(
                format!("{:.0}%", mem_pct * 100.0),
                Style::default()
                    .fg(level_color(t, mem_pct))
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        f.render_widget(ratatui::widgets::Paragraph::new(Line::from(spans)), area);
        return;
    }
    let block = panel(ctx, area, " ◉  Sistema ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 {
        return;
    }
    let w = inner.width as usize;
    let mem_pct = if s.mem_total > 0 {
        (s.mem_used as f64 / s.mem_total as f64) as f32
    } else {
        0.0
    };
    let cpu_col = level_color(t, s.cpu / 100.0);
    let mem_col = level_color(t, mem_pct);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("CPU  ", Style::default().fg(t.fg_dim)),
            Span::styled(
                format!("{:>5.1}%", s.cpu),
                Style::default().fg(cpu_col).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(
                ui::bar(s.cpu / 100.0, w.saturating_sub(16)),
                Style::default().fg(cpu_col),
            ),
        ]),
        Line::from(vec![
            Span::styled("RAM  ", Style::default().fg(t.fg_dim)),
            Span::styled(
                format!("{:>5.1}%", mem_pct * 100.0),
                Style::default().fg(mem_col).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(
                ui::bar(mem_pct, w.saturating_sub(16)),
                Style::default().fg(mem_col),
            ),
        ]),
    ];
    // En modo "compacto" nos quedamos sólo con las dos barras.
    if style != "compacto" {
        lines.push(Line::from(Span::styled(
            format!(
                "{} · {} · up {}",
                ui::human_bytes(s.mem_used),
                ui::human_bytes(s.mem_total),
                uptime(s.uptime)
            ),
            Style::default().fg(t.fg_faint),
        )));
        if let Some((label, temp)) = s.temps.first() {
            lines.push(Line::from(Span::styled(
                format!("🌡 {} {:.0}°C", ui::truncate(label, 18), temp),
                Style::default().fg(if *temp > 75.0 { t.warn } else { t.fg_dim }),
            )));
        }
    }
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

fn projects_widget(f: &mut Frame, area: Rect, ctx: &Ctx) {
    let t = ctx.theme;
    // Variante "conteo": sólo el número, sin listado.
    if ctx.cfg.home.style_of("proyectos") == "conteo" {
        let n = ctx.projects.len();
        let dirty: i32 = ctx.projects.iter().map(|p| p.dirty).sum();
        let spans = vec![
            Span::styled(
                format!("{n}"),
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" proyectos", Style::default().fg(t.fg_dim)),
            if dirty > 0 {
                Span::styled(format!("  ·  {dirty} cambios"), Style::default().fg(t.warn))
            } else {
                Span::styled("  ·  todo limpio", Style::default().fg(t.ok))
            },
        ];
        f.render_widget(ratatui::widgets::Paragraph::new(Line::from(spans)), area);
        return;
    }
    let block = panel(ctx, area, " ❖  Proyectos ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 2 {
        return;
    }
    let mut lines = Vec::new();
    if ctx.projects.is_empty() {
        lines.push(Line::from(Span::styled(
            "escaneando…",
            Style::default().fg(t.fg_faint),
        )));
    }
    for p in ctx.projects.iter().take(inner.height as usize) {
        let mut spans = vec![Span::styled("▸ ", Style::default().fg(t.accent))];
        spans.push(Span::styled(
            ui::truncate(&p.name, (inner.width as usize).saturating_sub(8)),
            Style::default().fg(t.fg),
        ));
        if p.dirty > 0 {
            spans.push(Span::styled(
                format!(" ●{}", p.dirty),
                Style::default().fg(t.warn),
            ));
        }
        if let Some(k) = p.kinds.first() {
            spans.push(Span::styled(
                format!(" [{k}]"),
                Style::default().fg(t.accent2),
            ));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

fn net_widget(f: &mut Frame, area: Rect, ctx: &Ctx) {
    let t = ctx.theme;
    let n = &ctx.sys.net;
    let style = ctx.cfg.home.style_of("red");
    // Variante "mini": una sola fila con las dos tasas.
    if style == "mini" {
        let spans = vec![
            Span::styled("↓ ", Style::default().fg(t.accent)),
            Span::styled(ui::human_rate(n.down_bps), Style::default().fg(t.fg)),
            Span::styled("   ↑ ", Style::default().fg(t.accent2)),
            Span::styled(ui::human_rate(n.up_bps), Style::default().fg(t.fg)),
        ];
        f.render_widget(ratatui::widgets::Paragraph::new(Line::from(spans)), area);
        return;
    }
    let block = panel(ctx, area, " ⇄  Red ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 {
        return;
    }
    let mut lines = vec![Line::from(vec![
        Span::styled("↓ ", Style::default().fg(t.accent)),
        Span::styled(
            format!("{:>12}", ui::human_rate(n.down_bps)),
            Style::default().fg(t.fg),
        ),
        Span::styled("    ↑ ", Style::default().fg(t.accent2)),
        Span::styled(
            format!("{:>12}", ui::human_rate(n.up_bps)),
            Style::default().fg(t.fg),
        ),
    ])];
    if let Some(ifc) = n.ifaces.first() {
        lines.push(Line::from(Span::styled(
            format!("{} · {}", ifc.name, ifc.kind),
            Style::default().fg(t.fg_faint),
        )));
    }
    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);

    if inner.height > 4 && style != "filas" {
        let spark_area = Rect {
            x: inner.x,
            y: inner.y + 3,
            width: inner.width,
            height: inner.height - 3,
        };
        let down_h = (spark_area.height / 2).max(1);
        let up_h = spark_area.height.saturating_sub(down_h).max(1);
        let data: Vec<u64> = ui::spark(&n.history_down, spark_area.width as usize);
        if data.iter().any(|v| *v > 0) {
            let spark = Sparkline::default()
                .data(&data)
                .style(Style::default().fg(t.accent));
            f.render_widget(
                spark,
                Rect {
                    x: spark_area.x,
                    y: spark_area.y,
                    width: spark_area.width,
                    height: down_h,
                },
            );
        }
        let data_up: Vec<u64> = ui::spark(&n.history_up, spark_area.width as usize);
        if data_up.iter().any(|v| *v > 0) {
            let spark = Sparkline::default()
                .data(&data_up)
                .style(Style::default().fg(t.accent2));
            f.render_widget(
                spark,
                Rect {
                    x: spark_area.x,
                    y: spark_area.y + down_h,
                    width: spark_area.width,
                    height: up_h,
                },
            );
        }
    }
}

// ── Aux ───────────────────────────────────────────────────────────────────

pub fn level_color(t: &crate::config::Theme, ratio: f32) -> Color {
    if ratio > 0.9 {
        t.err
    } else if ratio > 0.7 {
        t.warn
    } else {
        t.ok
    }
}

pub fn uptime(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

pub fn fecha(now: &chrono::DateTime<chrono::Local>) -> String {
    const DIAS: [&str; 7] = [
        "domingo",
        "lunes",
        "martes",
        "miércoles",
        "jueves",
        "viernes",
        "sábado",
    ];
    const MESES: [&str; 12] = [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ];
    let dia = DIAS[now.weekday().num_days_from_sunday() as usize];
    let mes = MESES[(now.month0() % 12) as usize];
    format!("{}, {} de {} de {}", dia, now.day(), mes, now.year())
}

// ── Estilos de reloj ──────────────────────────────────────────────────────

/// Altura que necesita la cabecera según el estilo elegido.
fn hdr_height(ctx: &Ctx, area: Rect) -> u16 {
    let g = &ctx.cfg.general;
    let h = match g.clock_style.as_str() {
        // saludo + hueco + línea + fecha
        "linea" => 4,
        // saludo + hueco + hora
        "minimal" => 3,
        // saludo + hueco + tablero + fecha
        "flip" => 3 + ui::flip_height(flip_scale(g.clock_size.as_str(), area.width)),
        // saludo + hueco + cara + hora + fecha
        "reloj" => 5 + analog_radius(g.clock_size.as_str(), area),
        _ => digital_height(g.clock_size.as_str(), area),
    };
    h.min(area.height.saturating_sub(6)).max(3)
}

/// Altura del reloj digital (tipografía grande), según el tamaño pedido.
fn digital_height(size: &str, area: Rect) -> u16 {
    match size {
        "bar" => 3,
        "small" => 9,
        "medium" => 14,
        "large" => 19,
        // auto: mediano por defecto, grande sólo en terminales muy holgados
        _ => {
            if area.height >= 30 && area.width >= 140 {
                19
            } else if area.height >= 17 && area.width >= 56 {
                14
            } else {
                9
            }
        }
    }
}

/// Escala del reloj de tablero, limitada por el ancho disponible.
fn flip_scale(size: &str, width: u16) -> u16 {
    let mut s = match size {
        "small" | "bar" => 1,
        "medium" => 2,
        "large" => 3,
        _ => {
            if width >= 130 {
                3
            } else if width >= 95 {
                2
            } else {
                1
            }
        }
    };
    while s > 1 && ui::flip_width(8, s) + 4 > width {
        s -= 1;
    }
    s
}

/// Radio del reloj analógico, limitado por el área disponible.
fn analog_radius(size: &str, area: Rect) -> u16 {
    let want = match size {
        "small" | "bar" => 4,
        "medium" => 6,
        "large" => 9,
        _ => {
            if area.height >= 30 {
                9
            } else if area.height >= 17 {
                6
            } else {
                4
            }
        }
    };
    // Debe caber en ancho y dejar sitio para saludo, hora y fecha.
    let max_w = area.width.saturating_sub(1) / 2;
    let max_h = area.height.saturating_sub(9);
    want.min(max_w).min(max_h).max(3)
}

/// Hora formateada según `clock_24h` y `show_seconds`.
fn hora_txt(now: &chrono::DateTime<chrono::Local>, clock_24h: bool, secs: bool) -> String {
    let (h, suffix) = if clock_24h {
        (now.hour(), String::new())
    } else {
        let (h, am) = match now.hour() {
            0..=11 => (now.hour(), "AM"),
            12 => (12, "PM"),
            _ => (now.hour() - 12, "PM"),
        };
        (h, format!(" {am}"))
    };
    if secs {
        format!("{h:02}:{:02}:{:02}{suffix}", now.minute(), now.second())
    } else {
        format!("{h:02}:{:02}{suffix}", now.minute())
    }
}

/// Partes de la hora para los estilos que dibujan cifras sueltas.
fn hora_piezas(now: &chrono::DateTime<chrono::Local>, clock_24h: bool) -> (String, String, String) {
    let h = if clock_24h {
        now.hour()
    } else {
        match now.hour() {
            0..=11 => now.hour(),
            12 => 12,
            _ => now.hour() - 12,
        }
    };
    let suffix = if clock_24h {
        String::new()
    } else if now.hour() < 12 {
        "AM".into()
    } else {
        "PM".into()
    };
    (format!("{h:02}"), format!("{:02}", now.minute()), suffix)
}

// ── digital: la tipografía grande de toda la vida ──
fn clock_digital(f: &mut Frame, body: Rect, ctx: &Ctx) {
    let t = ctx.theme;
    let now = ctx.now;
    let size = ctx.cfg.general.clock_size.as_str();
    let want = digital_height(
        size,
        Rect {
            height: body.height + 1,
            ..body
        },
    );
    if want <= 3 {
        // Modo barra: una sola línea
        let line = Line::from(vec![
            Span::styled(
                hora_txt(now, ctx.cfg.general.clock_24h, true),
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ·  ", Style::default().fg(t.fg_faint)),
            Span::styled(fecha(now), Style::default().fg(t.fg_dim)),
        ])
        .centered();
        f.render_widget(
            ratatui::widgets::Paragraph::new(line),
            Rect { height: 1, ..body },
        );
        return;
    }

    let (hh, mm, suffix) = hora_piezas(now, ctx.cfg.general.clock_24h);
    let show_secs = ctx.cfg.general.show_seconds;
    let time = if show_secs {
        format!("{hh}:{mm}:{:02}", now.second())
    } else {
        format!("{hh}:{mm}")
    };

    // Escala pedida o automática
    let mut scale = match size {
        "small" => 1,
        "medium" => 2,
        "large" => 3,
        _ => {
            if body.height >= 15 && body.width >= 96 {
                3
            } else if body.height >= 9 {
                2
            } else {
                1
            }
        }
    };
    let full = if suffix.is_empty() {
        time.clone()
    } else {
        format!("{time} {suffix}")
    };
    while scale > 1 && ui::big_text_width(&full, scale) + 4 > body.width {
        scale -= 1;
    }

    let w = ui::big_text_width(&full, scale);
    if w + 2 > body.width || 5 * scale + 2 > body.height {
        return;
    }
    let x0 = body.x + (body.width - w) / 2;
    let y0 = body.y + 1;
    let buf = f.buffer_mut();
    // horas:minutos con degradado, segundos más apagados
    let left = format!("{hh}:{mm}");
    let lw = ui::big_text_width(&left, scale);
    ui::draw_big_text(buf, x0, y0, &left, scale, t.accent, t.accent2);
    let mut cx = x0 + lw;
    if show_secs {
        cx += scale / 2;
        cx += ui::draw_big_text(
            buf,
            cx,
            y0,
            &format!(":{:02}", now.second()),
            scale,
            t.fg_dim,
            t.fg_faint,
        );
    }
    if !suffix.is_empty() {
        cx += scale;
        ui::draw_big_text(buf, cx, y0, &suffix, scale, t.fg_dim, t.fg_faint);
    }

    // Fecha
    f.render_widget(
        ratatui::widgets::Paragraph::new(
            Line::from(Span::styled(fecha(now), Style::default().fg(t.fg_dim))).centered(),
        ),
        Rect {
            y: y0 + 5 * scale + 1,
            height: 1,
            ..body
        },
    );
}

// ── línea: una regla elegante con la hora embebida ──
fn clock_linea(f: &mut Frame, body: Rect, ctx: &Ctx) {
    let t = ctx.theme;
    if body.height < 3 {
        return;
    }
    let time = hora_txt(
        ctx.now,
        ctx.cfg.general.clock_24h,
        ctx.cfg.general.show_seconds,
    );
    let date = fecha(ctx.now);
    let content_w = time.chars().count() + date.chars().count() + 7;
    let width = body.width as usize;
    if width < 8 {
        return;
    }
    let pad = width.saturating_sub(content_w) / 2;
    let rest = width - pad - content_w;

    let mut spans = vec![
        Span::styled("─".repeat(pad), Style::default().fg(t.border)),
        Span::styled(" ✦ ", Style::default().fg(t.accent2)),
        Span::styled(
            time,
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  ", Style::default().fg(t.fg_faint)),
        Span::styled(date, Style::default().fg(t.fg_dim)),
        Span::styled(" ", Style::default()),
        Span::styled("✦ ", Style::default().fg(t.accent2)),
        Span::styled(
            "─".repeat(rest.saturating_sub(3)),
            Style::default().fg(t.border),
        ),
    ];
    if rest < 3 {
        spans.pop();
        spans.pop();
        spans.pop();
    }
    f.render_widget(
        ratatui::widgets::Paragraph::new(Line::from(spans)),
        Rect {
            y: body.y + 1,
            height: 1,
            ..body
        },
    );
}

// ── flip: tablero split-flap de aeropuerto ──
fn clock_flip(f: &mut Frame, body: Rect, ctx: &Ctx) {
    let t = ctx.theme;
    let now = ctx.now;
    let (hh, mm, suffix) = hora_piezas(now, ctx.cfg.general.clock_24h);
    let mut text = format!("{hh}:{mm}");
    if ctx.cfg.general.show_seconds {
        text.push_str(&format!(":{:02}", now.second()));
    }
    if !suffix.is_empty() {
        text.push_str(&suffix);
    }

    // Escala que cabe en el alto y el ancho del cuerpo
    let mut scale = flip_scale(ctx.cfg.general.clock_size.as_str(), body.width);
    while scale > 1 && 2 + ui::flip_height(scale) > body.height {
        scale -= 1;
    }
    let w = ui::flip_width(text.chars().count(), scale);
    let h = ui::flip_height(scale);
    if 2 + h > body.height || w + 2 > body.width {
        // Ni siquiera el mínimo cabe: caemos a la línea.
        clock_linea(f, body, ctx);
        return;
    }
    let x0 = body.x + (body.width - w) / 2;
    let y0 = body.y + 1;
    ui::draw_flip(
        f.buffer_mut(),
        x0,
        y0,
        &text,
        scale,
        (t.accent, t.accent2),
        t.border,
    );
    f.render_widget(
        ratatui::widgets::Paragraph::new(
            Line::from(Span::styled(fecha(now), Style::default().fg(t.fg_dim))).centered(),
        ),
        Rect {
            y: y0 + h + 1,
            height: 1,
            ..body
        },
    );
}

// ── reloj: cara analógica dibujada a mano ──
fn clock_reloj(f: &mut Frame, body: Rect, ctx: &Ctx) {
    let t = ctx.theme;
    let now = ctx.now;
    if body.height < 5 {
        clock_linea(f, body, ctx);
        return;
    }
    let rows = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(body);
    let r = analog_radius(
        ctx.cfg.general.clock_size.as_str(),
        Rect {
            height: rows[0].height + 4,
            ..rows[0]
        },
    );
    // El área de la cara se centra según el radio pedido
    let (aw, ah) = ui::analog_size(r);
    let face = Rect {
        width: aw.min(rows[0].width),
        height: ah.min(rows[0].height),
        ..rows[0]
    };
    let face = Rect {
        x: rows[0].x + (rows[0].width.saturating_sub(face.width)) / 2,
        y: rows[0].y + (rows[0].height.saturating_sub(face.height)) / 2,
        ..face
    };
    ui::draw_analog(
        f.buffer_mut(),
        face,
        now.hour(),
        now.minute(),
        now.second(),
        (t.fg_faint, t.fg, t.accent),
    );
    f.render_widget(
        ratatui::widgets::Paragraph::new(
            Line::from(Span::styled(
                hora_txt(now, ctx.cfg.general.clock_24h, ctx.cfg.general.show_seconds),
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ))
            .centered(),
        ),
        rows[1],
    );
    f.render_widget(
        ratatui::widgets::Paragraph::new(
            Line::from(Span::styled(fecha(now), Style::default().fg(t.fg_dim))).centered(),
        ),
        rows[2],
    );
}

// ── minimal: sólo la hora, en calma ──
fn clock_minimal(f: &mut Frame, body: Rect, ctx: &Ctx) {
    let t = ctx.theme;
    if body.height < 1 {
        return;
    }
    let line = Line::from(Span::styled(
        hora_txt(
            ctx.now,
            ctx.cfg.general.clock_24h,
            ctx.cfg.general.show_seconds,
        ),
        Style::default().fg(t.accent),
    ))
    .centered();
    f.render_widget(
        ratatui::widgets::Paragraph::new(line),
        Rect {
            y: body.y + 1,
            height: 1,
            ..body
        },
    );
}
