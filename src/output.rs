//! Visor interno de salida.
//!
//! Nébula ejecuta sus comandos en un hilo (ver `data::spawn_stream`) y
//! pinta aquí el stdout y el stderr **línea a línea**: el resultado se ve
//! dentro de Nébula y no se abre ninguna otra terminal.
//!
//! Teclas: `esc`/`q` cierra · `↑`/`↓`/`rept` desplazan · `fin` vuelve a
//! seguir la última línea.

use crate::app::Ctx;
use crate::config::Theme;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Widget};
use ratatui::Frame;
use std::time::Instant;

/// Líneas que recuerda el visor (se recortan por el principio).
const MAX_LINES: usize = 4_000;

pub struct Output {
    pub title: String,
    cmd: String,
    lines: Vec<String>,
    running: bool,
    ok: bool,
    scroll: usize,
    /// Seguir la última línea cuando llega salida nueva.
    follow: bool,
    started: Instant,
}

impl Output {
    pub fn new(title: impl Into<String>, cmd: impl Into<String>) -> Output {
        Output {
            title: title.into(),
            cmd: cmd.into(),
            lines: Vec::new(),
            running: true,
            ok: true,
            scroll: 0,
            follow: true,
            started: Instant::now(),
        }
    }

    /// Añade una línea de salida (las más antiguas se descartan).
    pub fn push(&mut self, line: String) {
        self.lines.push(line);
        if self.lines.len() > MAX_LINES {
            let cut = self.lines.len() - MAX_LINES;
            self.lines.drain(..cut);
            self.scroll = self.scroll.saturating_sub(cut);
        }
    }

    /// El comando terminó: `ok` = salió con código cero.
    pub fn finish(&mut self, ok: bool) {
        self.running = false;
        self.ok = ok;
    }

    /// Aviso para la barra de estado, para cuando el visor está cerrado.
    pub fn summary(&self, ok: bool) -> String {
        let last = self
            .lines
            .iter()
            .rev()
            .find(|l| !l.trim().is_empty())
            .cloned()
            .unwrap_or_default();
        crate::app::summarize(self.title.trim(), ok, &last)
    }

    /// Teclas del visor. Devuelve `true` cuando hay que cerrarlo.
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return true,
            KeyCode::Up => {
                self.follow = false;
                self.scroll = self.scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                self.follow = false;
                self.scroll = self.scroll.saturating_add(1);
            }
            KeyCode::PageUp => {
                self.follow = false;
                self.scroll = self.scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.follow = false;
                self.scroll = self.scroll.saturating_add(10);
            }
            KeyCode::Home => {
                self.follow = false;
                self.scroll = 0;
            }
            KeyCode::End | KeyCode::Char('f') => self.follow = true,
            _ => {}
        }
        false
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let w = (area.width * 86 / 100).clamp(56, 150).min(area.width);
        let h = (area.height * 78 / 100).clamp(14, 46).min(area.height);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(w)) / 2,
            y: area.y + (area.height.saturating_sub(h)) / 2,
            width: w,
            height: h,
        };
        Clear.render(rect, f.buffer_mut());
        let title = format!(" {} ", self.title);
        let block = ui::panel_on(t, ctx.wp, rect, title, 0.97);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        if inner.height < 3 || inner.width < 10 {
            return;
        }

        let width = inner.width as usize;
        let body = (inner.height as usize).saturating_sub(2); // cabecera + pie
        let max_scroll = self.lines.len().saturating_sub(body);
        if self.follow {
            self.scroll = max_scroll;
        } else {
            self.scroll = self.scroll.min(max_scroll);
        }

        let mut rows: Vec<Line> = Vec::with_capacity(inner.height as usize);
        rows.push(Line::from(vec![
            Span::styled(" $ ", Style::default().fg(t.accent2)),
            Span::styled(
                ui::truncate(&self.cmd, width.saturating_sub(3)),
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
        ]));
        for line in self.lines.iter().skip(self.scroll).take(body) {
            rows.push(Line::from(Span::styled(
                ui::truncate(line, width),
                line_style(line, t),
            )));
        }
        while rows.len() < body + 1 {
            rows.push(Line::from(""));
        }
        rows.push(self.footer(t, width));
        f.render_widget(ratatui::widgets::Paragraph::new(rows), inner);
    }

    /// Pie: estado a la izquierda, posición y atajos a la derecha.
    fn footer(&self, t: &Theme, width: usize) -> Line<'static> {
        let (status, color) = if self.running {
            (
                format!("● en marcha · {:.1}s", self.started.elapsed().as_secs_f32()),
                t.accent,
            )
        } else if self.ok {
            ("✓ terminado".to_string(), t.ok)
        } else {
            ("✗ terminado con errores".to_string(), t.err)
        };
        let pos = format!(" {}/{} ", self.scroll + 1, self.lines.len().max(1));
        let hint = " ↑/↓ desplazar · esc cerrar ";
        let pad = width
            .saturating_sub(status.chars().count() + pos.chars().count() + hint.chars().count());
        Line::from(vec![
            Span::styled(
                status,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(pos, Style::default().fg(t.fg_faint)),
            Span::raw(" ".repeat(pad)),
            Span::styled(hint, Style::default().fg(t.fg_faint)),
        ])
    }
}

/// Pinta de rojo lo que parece un error y de ámbar lo que es un aviso.
fn line_style(line: &str, t: &Theme) -> Style {
    let low = line.to_lowercase();
    if low.starts_with("e:")
        || low.contains("error")
        || low.contains("fatal")
        || low.contains("failed")
        || low.contains("not found")
        || low.contains("no se pudo")
        || low.contains("denied")
    {
        Style::default().fg(t.err)
    } else if low.starts_with("w:") || low.contains("warning") || low.contains("aviso") {
        Style::default().fg(t.warn)
    } else {
        Style::default().fg(t.fg)
    }
}
