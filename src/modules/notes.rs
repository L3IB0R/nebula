//! Notas: un bloc de texto sencillo con guardado.

use crate::action::Action;
use crate::app::Ctx;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

pub struct Notes {
    lines: Vec<String>,
    cur_line: usize,
    cur_col: usize,
    scroll: u16,
    editing: bool,
    loaded: bool,
    dirty: bool,
    path: std::path::PathBuf,
}

impl Default for Notes {
    fn default() -> Self {
        Notes {
            lines: vec![String::new()],
            cur_line: 0,
            cur_col: 0,
            scroll: 0,
            editing: false,
            loaded: false,
            dirty: false,
            path: crate::config::config_dir().join("notas.md"),
        }
    }
}

impl Notes {
    fn ensure_loaded(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        if let Ok(s) = std::fs::read_to_string(&self.path) {
            self.lines = s.lines().map(|l| l.to_string()).collect();
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }
        }
    }

    pub fn save(&mut self) -> Action {
        self.ensure_loaded();
        let dir = self.path.parent().map(|p| p.to_path_buf());
        if let Some(d) = dir {
            let _ = std::fs::create_dir_all(d);
        }
        match std::fs::write(&self.path, self.lines.join("\n")) {
            Ok(_) => {
                self.dirty = false;
                Action::Toast(format!("nota guardada · {}", self.path.display()))
            }
            Err(e) => Action::Toast(format!("no se pudo guardar: {e}")),
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.ensure_loaded();
        let t = ctx.theme;
        let mode = if self.editing { "editando" } else { "lectura" };
        let title = format!(
            " ✎  Notas · {} · {}{} ",
            ui::truncate(
                &self.path.file_name().unwrap_or_default().to_string_lossy(),
                24
            ),
            mode,
            if self.dirty {
                " · cambios sin guardar"
            } else {
                ""
            }
        );
        let rows = Layout::vertical([Constraint::Min(4), Constraint::Length(1)]).split(area);
        // ── Pie: posición del cursor + los atajos, siempre a la vista ──
        let mut bar_spans = vec![Span::styled(
            format!(" {}:{} ", self.cur_line + 1, self.cur_col + 1),
            Style::default().fg(t.accent),
        )];
        bar_spans.extend(
            ui::hint_line(
                t,
                if self.editing {
                    &[
                        ("esc", "lectura"),
                        ("ctrl+s", "guardar"),
                        ("enter", "salto"),
                        ("⌫", "borrar"),
                    ]
                } else {
                    &[
                        ("i", "editar"),
                        ("↑/↓", "mover"),
                        ("ctrl+s", "guardar"),
                        ("tab", "cambia de módulo"),
                    ]
                },
            )
            .spans,
        );
        f.render_widget(
            ratatui::widgets::Paragraph::new(Line::from(bar_spans)),
            rows[1],
        );

        let block = ui::panel_on(t, ctx.wp, rows[0], title, 0.82);
        let inner = block.inner(rows[0]);
        f.render_widget(block, rows[0]);
        if inner.height < 2 {
            return;
        }

        // mantener el cursor visible
        if (self.cur_line as i32) < self.scroll as i32 {
            self.scroll = self.cur_line as u16;
        }
        if self.cur_line as u16 >= self.scroll + inner.height {
            self.scroll = self.cur_line as u16 + 1 - inner.height;
        }

        let mut out: Vec<Line> = Vec::new();
        for (i, line) in self
            .lines
            .iter()
            .enumerate()
            .skip(self.scroll as usize)
            .take(inner.height as usize)
        {
            if i == self.cur_line {
                let chars: Vec<char> = line.chars().collect();
                let col = self.cur_col.min(chars.len());
                let before: String = chars[..col].iter().collect();
                let at = chars.get(col).copied().unwrap_or(' ');
                let after: String = chars.get(col + 1..).unwrap_or(&[]).iter().collect();
                out.push(Line::from(vec![
                    Span::styled(format!("{:>3} ", i + 1), Style::default().fg(t.fg_faint)),
                    Span::styled(before, Style::default().fg(t.fg)),
                    Span::styled(
                        at.to_string(),
                        Style::default()
                            .bg(t.accent)
                            .fg(t.bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(after, Style::default().fg(t.fg)),
                ]));
            } else {
                out.push(Line::from(vec![
                    Span::styled(format!("{:>3} ", i + 1), Style::default().fg(t.fg_faint)),
                    Span::styled(line.clone(), Style::default().fg(t.fg_dim)),
                ]));
            }
        }
        f.render_widget(ratatui::widgets::Paragraph::new(out), inner);
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Option<Action> {
        self.ensure_loaded();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // ── Modo lectura: casi todo lo deja pasar al corazón ──
        if !self.editing {
            return match key.code {
                KeyCode::Char('i') | KeyCode::Char('e') | KeyCode::Enter => {
                    self.editing = true;
                    Some(Action::Toast("editando · Esc para volver a lectura".into()))
                }
                KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                    self.move_cursor(key.code);
                    Some(Action::Keep)
                }
                KeyCode::PageUp => {
                    self.scroll = self.scroll.saturating_sub(5);
                    Some(Action::Keep)
                }
                KeyCode::PageDown => {
                    self.scroll = self.scroll.saturating_add(5);
                    Some(Action::Keep)
                }
                _ => None,
            };
        }

        // ── Modo edición ──
        if ctrl {
            return match key.code {
                KeyCode::Char('s') => Some(self.save()),
                KeyCode::Char('a') => {
                    self.cur_col = 0;
                    Some(Action::Keep)
                }
                KeyCode::Char('e') => {
                    self.cur_col = self.lines[self.cur_line].chars().count();
                    Some(Action::Keep)
                }
                _ => None,
            };
        }

        // Esc sale del modo edición (no de Nébula).
        if key.code == KeyCode::Esc {
            self.editing = false;
            return Some(Action::Toast("modo lectura · i para editar".into()));
        }

        match key.code {
            KeyCode::Char(c) => {
                let line = &mut self.lines[self.cur_line];
                let byte = line
                    .char_indices()
                    .nth(self.cur_col)
                    .map(|(i, _)| i)
                    .unwrap_or(line.len());
                line.insert(byte, c);
                self.cur_col += 1;
                self.dirty = true;
                Some(Action::Keep)
            }
            KeyCode::Enter => {
                let line = &mut self.lines[self.cur_line];
                let byte = line
                    .char_indices()
                    .nth(self.cur_col)
                    .map(|(i, _)| i)
                    .unwrap_or(line.len());
                let rest = line.split_off(byte);
                self.lines.insert(self.cur_line + 1, rest);
                self.cur_line += 1;
                self.cur_col = 0;
                self.dirty = true;
                Some(Action::Keep)
            }
            KeyCode::Backspace => {
                if self.cur_col > 0 {
                    let line = &mut self.lines[self.cur_line];
                    let idx = line
                        .char_indices()
                        .nth(self.cur_col - 1)
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    line.remove(idx);
                    self.cur_col -= 1;
                    self.dirty = true;
                } else if self.cur_line > 0 {
                    let cur = self.lines.remove(self.cur_line);
                    self.cur_line -= 1;
                    self.cur_col = self.lines[self.cur_line].chars().count();
                    self.lines[self.cur_line].push_str(&cur);
                    self.dirty = true;
                }
                Some(Action::Keep)
            }
            KeyCode::Left => {
                if self.cur_col > 0 {
                    self.cur_col -= 1;
                } else if self.cur_line > 0 {
                    self.cur_line -= 1;
                    self.cur_col = self.lines[self.cur_line].chars().count();
                }
                Some(Action::Keep)
            }
            KeyCode::Right => {
                let len = self.lines[self.cur_line].chars().count();
                if self.cur_col < len {
                    self.cur_col += 1;
                } else if self.cur_line + 1 < self.lines.len() {
                    self.cur_line += 1;
                    self.cur_col = 0;
                }
                Some(Action::Keep)
            }
            KeyCode::Up => {
                if self.cur_line > 0 {
                    self.cur_line -= 1;
                    self.cur_col = self.cur_col.min(self.lines[self.cur_line].chars().count());
                }
                Some(Action::Keep)
            }
            KeyCode::Down => {
                if self.cur_line + 1 < self.lines.len() {
                    self.cur_line += 1;
                    self.cur_col = self.cur_col.min(self.lines[self.cur_line].chars().count());
                }
                Some(Action::Keep)
            }
            KeyCode::Home => {
                self.cur_col = 0;
                Some(Action::Keep)
            }
            KeyCode::End => {
                self.cur_col = self.lines[self.cur_line].chars().count();
                Some(Action::Keep)
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(5);
                self.cur_line = self.cur_line.saturating_sub(5);
                Some(Action::Keep)
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(5);
                self.cur_line = (self.cur_line + 5).min(self.lines.len() - 1);
                Some(Action::Keep)
            }
            KeyCode::Tab => {
                let line = &mut self.lines[self.cur_line];
                let byte = line
                    .char_indices()
                    .nth(self.cur_col)
                    .map(|(i, _)| i)
                    .unwrap_or(line.len());
                line.insert_str(byte, "    ");
                self.cur_col += 4;
                self.dirty = true;
                Some(Action::Keep)
            }
            _ => None,
        }
    }

    /// Mueve el cursor de lectura/edición sin modificar el texto.
    fn move_cursor(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up => {
                if self.cur_line > 0 {
                    self.cur_line -= 1;
                    self.cur_col = self.cur_col.min(self.lines[self.cur_line].chars().count());
                }
            }
            KeyCode::Down => {
                if self.cur_line + 1 < self.lines.len() {
                    self.cur_line += 1;
                    self.cur_col = self.cur_col.min(self.lines[self.cur_line].chars().count());
                }
            }
            KeyCode::Left => {
                if self.cur_col > 0 {
                    self.cur_col -= 1;
                } else if self.cur_line > 0 {
                    self.cur_line -= 1;
                    self.cur_col = self.lines[self.cur_line].chars().count();
                }
            }
            KeyCode::Right => {
                let len = self.lines[self.cur_line].chars().count();
                if self.cur_col < len {
                    self.cur_col += 1;
                } else if self.cur_line + 1 < self.lines.len() {
                    self.cur_line += 1;
                    self.cur_col = 0;
                }
            }
            _ => {}
        }
    }
}
