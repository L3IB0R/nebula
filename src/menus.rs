//! Patrón común de submenú navegable con teclado.
//!
//! Todas las secciones usan la misma ventana y las mismas teclas:
//! `m` abre (o cierra), ↑/↓ eligen, Intro ejecuta, Esc cierra.

use crate::config::Theme;
use crate::ui;
use crate::wallpaper::Wallpaper;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

/// Resultado de pasar una tecla al menú.
pub enum Nav {
    /// La tecla no era del menú: que la sección la procese.
    Pass,
    /// Se movió la selección o se abrió/cerró (ya gestionado).
    Handled,
    /// Intro en la fila `n`.
    Run(usize),
}

#[derive(Default)]
pub struct Menu {
    pub open: bool,
    pub sel: usize,
}

impl Menu {
    /// Teclas del menú. `n` = nº de filas.
    pub fn on_key(&mut self, key: KeyEvent, n: usize) -> Nav {
        if !self.open {
            return if matches!(key.code, KeyCode::Char('m')) {
                self.open = true;
                self.sel = 0;
                Nav::Handled
            } else {
                Nav::Pass
            };
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('m') => {
                self.open = false;
                Nav::Handled
            }
            KeyCode::Up => {
                self.sel = self.sel.saturating_sub(1);
                Nav::Handled
            }
            KeyCode::Down => {
                self.sel = (self.sel + 1).min(n.saturating_sub(1));
                Nav::Handled
            }
            KeyCode::Home => {
                self.sel = 0;
                Nav::Handled
            }
            KeyCode::End => {
                self.sel = n.saturating_sub(1);
                Nav::Handled
            }
            KeyCode::Enter => {
                self.open = false;
                Nav::Run(self.sel)
            }
            _ => Nav::Pass,
        }
    }

    /// Ventana centrada con las etiquetas indicadas.
    pub fn render(
        &self,
        f: &mut Frame,
        area: Rect,
        title: &str,
        items: &[&str],
        theme: &Theme,
        wp: &Wallpaper,
    ) {
        if !self.open || items.is_empty() {
            return;
        }
        let t = theme;
        let mut lines: Vec<Line> = Vec::new();
        for (i, label) in items.iter().enumerate() {
            let s = i == self.sel.min(items.len() - 1);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", if s { "▸" } else { " " }),
                    Style::default().fg(t.accent),
                ),
                Span::styled(
                    ui::truncate(label, 44),
                    Style::default()
                        .fg(if s { t.fg } else { t.fg_dim })
                        .add_modifier(if s { Modifier::BOLD } else { Modifier::empty() }),
                ),
            ]));
        }
        let h = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
        let w = (area.width * 56 / 100).clamp(36, 60).min(area.width);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(w)) / 2,
            y: area.y + (area.height.saturating_sub(h)) / 2,
            width: w,
            height: h,
        };
        let buf = f.buffer_mut();
        ui::dim_area(buf, area, 0.55);
        let block = ui::panel_on(t, wp, rect, title, 0.97);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
        let hint = Line::from(Span::styled(
            "  ↑/↓ elegir · intro ejecutar · m/esc cerrar",
            Style::default().fg(t.fg_faint),
        ));
        f.render_widget(
            ratatui::widgets::Paragraph::new(hint),
            Rect {
                x: inner.x,
                y: inner.bottom().saturating_sub(1),
                width: inner.width,
                height: 1,
            },
        );
    }
}
