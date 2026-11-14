//! Red: dispositivos, conexiones, puertos y latencia.

use crate::action::Action;
use crate::app::Ctx;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

#[derive(Default)]
pub struct Net {
    pub scroll: u16,
    /// Menú local (tecla m): ping · refrescar.
    pub menu: crate::menus::Menu,
    /// Filtro de la lista de sockets (tecla f).
    filter: Filter,
    /// Texto escrito en el filtro «buscar».
    query: String,
    /// ¿Se está escribiendo el filtro?
    typing: bool,
}

/// Filas del menú local de Red.
const NET_MENU: &[&str] = &["hacer ping de prueba", "refrescar"];

/// Qué muestra la lista de sockets.
#[derive(Clone, Copy, PartialEq, Default)]
pub enum Filter {
    #[default]
    Todas,
    Escuchando,
    Establecidas,
    Buscar,
}

impl Filter {
    fn next(self) -> Self {
        match self {
            Filter::Todas => Filter::Escuchando,
            Filter::Escuchando => Filter::Establecidas,
            Filter::Establecidas => Filter::Buscar,
            Filter::Buscar => Filter::Todas,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Filter::Todas => "todas",
            Filter::Escuchando => "escuchando",
            Filter::Establecidas => "establecidas",
            Filter::Buscar => "buscar",
        }
    }
}

impl Net {
    /// Sockets que pasa el filtro activo.
    fn filtered<'a>(&self, n: &'a crate::data::NetSnap) -> Vec<&'a crate::data::Sock> {
        let q = self.query.to_lowercase();
        n.socks
            .iter()
            .filter(|s| match self.filter {
                Filter::Todas => true,
                Filter::Escuchando => s.listening(),
                Filter::Establecidas => s.established(),
                Filter::Buscar => {
                    q.is_empty()
                        || s.addr.to_lowercase().contains(&q)
                        || s.proc.to_lowercase().contains(&q)
                        || s.proto.to_lowercase().contains(&q)
                        || s.state.to_lowercase().contains(&q)
                }
            })
            .collect()
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let n = &ctx.net;
        let rows = Layout::vertical([
            Constraint::Length(8),
            Constraint::Percentage(36),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

        // ── Estado ──
        let block = ui::panel_on(t, ctx.wp, rows[0], " ⇄  Enlace ", 0.8);
        let inner = block.inner(rows[0]);
        f.render_widget(block, rows[0]);
        let info = &ctx.sys.net;
        let mut lines = vec![
            Line::from(vec![
                Span::styled("↓ bajada  ", Style::default().fg(t.accent)),
                Span::styled(
                    format!("{:>14}", ui::human_rate(info.down_bps)),
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled("     ↑ subida  ", Style::default().fg(t.accent2)),
                Span::styled(
                    format!("{:>14}", ui::human_rate(info.up_bps)),
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("totales  ", Style::default().fg(t.fg_faint)),
                Span::styled(
                    format!(
                        "↓ {}   ↑ {}",
                        ui::human_bytes(info.total_down),
                        ui::human_bytes(info.total_up)
                    ),
                    Style::default().fg(t.fg_dim),
                ),
            ]),
        ];
        for d in n.devices.iter().take(3) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {:<12}", ui::truncate(&d.name, 12)),
                    Style::default().fg(t.accent),
                ),
                Span::styled(format!("{:<10}", d.kind), Style::default().fg(t.fg_faint)),
                Span::styled(
                    if d.up { "● conectado" } else { "○ caído" },
                    Style::default().fg(if d.up { t.ok } else { t.err }),
                ),
                Span::styled(
                    format!("  {}", ui::truncate(&d.ip, 30)),
                    Style::default().fg(t.fg_dim),
                ),
            ]));
        }
        f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);

        // ── Conexiones guardadas ──
        let block = ui::panel_on(
            t,
            ctx.wp,
            rows[1],
            format!(" ⌁  Conexiones · {} ", n.conns.len()),
            0.8,
        );
        let inner = block.inner(rows[1]);
        f.render_widget(block, rows[1]);
        let clines: Vec<Line> = n
            .conns
            .iter()
            .take(inner.height as usize)
            .map(|(name, kind, dev)| {
                Line::from(vec![
                    Span::styled(" ▸ ", Style::default().fg(t.accent)),
                    Span::styled(
                        ui::truncate(name, (inner.width as usize).saturating_sub(30)),
                        Style::default().fg(t.fg),
                    ),
                    Span::styled(format!(" {kind}"), Style::default().fg(t.accent2)),
                    Span::styled(
                        format!(" · {}", ui::truncate(dev, 12)),
                        Style::default().fg(t.fg_faint),
                    ),
                ])
            })
            .collect();
        f.render_widget(
            ratatui::widgets::Paragraph::new(if clines.is_empty() {
                vec![Line::from(Span::styled(
                    "nmcli no respondió",
                    Style::default().fg(t.fg_faint),
                ))]
            } else {
                clines
            }),
            inner,
        );

        // ── Sockets, con el filtro activo ──
        let socks = self.filtered(n);
        let title = if self.typing {
            format!(" ⌾  Buscar · {}▏ ", self.query)
        } else if matches!(self.filter, Filter::Buscar) && !self.query.is_empty() {
            format!(" ⌾  «{}» · {} ", ui::truncate(&self.query, 18), socks.len())
        } else {
            format!(" ⌾  {} · {} ", self.filter.label(), socks.len())
        };
        let block = ui::panel_on(t, ctx.wp, rows[2], title, 0.8);
        let inner = block.inner(rows[2]);
        f.render_widget(block, rows[2]);
        let start = (self.scroll as usize).min(socks.len().saturating_sub(1));
        let plines: Vec<Line> = socks
            .iter()
            .skip(start)
            .take(inner.height as usize)
            .map(|s| {
                let col = if s.established() {
                    t.ok
                } else if s.listening() {
                    t.accent2
                } else {
                    t.fg_faint
                };
                Line::from(vec![
                    Span::styled(
                        format!(" {:<5}", ui::truncate(&s.proto, 5)),
                        Style::default().fg(col),
                    ),
                    Span::styled(
                        format!("{:<9}", ui::truncate(&s.state, 9)),
                        Style::default().fg(col),
                    ),
                    Span::styled(
                        format!("{:<26}", ui::truncate(&s.addr, 26)),
                        Style::default().fg(t.fg),
                    ),
                    Span::styled(
                        ui::truncate(&s.proc, (inner.width as usize).saturating_sub(43)),
                        Style::default().fg(t.fg_dim),
                    ),
                ])
            })
            .collect();
        f.render_widget(
            ratatui::widgets::Paragraph::new(if plines.is_empty() {
                vec![Line::from(Span::styled(
                    if !n.ss_ok {
                        "ss no disponible"
                    } else if self.filter == Filter::Buscar && !self.query.is_empty() {
                        "nada coincide con la búsqueda"
                    } else {
                        "nada con este filtro"
                    },
                    Style::default().fg(t.fg_faint),
                ))]
            } else {
                plines
            }),
            inner,
        );

        // ── Menú local (tecla m) ──
        self.menu.render(f, area, " ⇄  Red ", NET_MENU, t, ctx.wp);

        // ── Pie: los atajos, siempre a la vista ──
        ui::hint_bar(
            f,
            t,
            rows[3],
            &[
                ("f", "filtro"),
                ("/", "buscar puerto/nombre"),
                ("p", "ping"),
                ("r", "refrescar"),
                ("m", "menú"),
            ],
        );
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Option<Action> {
        // ── Escribiendo el filtro ──
        if self.typing {
            return match key.code {
                KeyCode::Esc => {
                    self.typing = false;
                    self.filter = Filter::Todas;
                    self.query.clear();
                    self.scroll = 0;
                    Some(Action::Keep)
                }
                KeyCode::Enter => {
                    self.typing = false;
                    self.scroll = 0;
                    Some(Action::Keep)
                }
                KeyCode::Backspace => {
                    self.query.pop();
                    self.scroll = 0;
                    Some(Action::Keep)
                }
                KeyCode::Char(c) if !c.is_control() => {
                    self.query.push(c);
                    self.scroll = 0;
                    Some(Action::Keep)
                }
                _ => Some(Action::Keep),
            };
        }

        // ── Menú local (m) ──
        if self.menu.open || matches!(key.code, KeyCode::Char('m')) {
            match self.menu.on_key(key, NET_MENU.len()) {
                crate::menus::Nav::Pass => {}
                crate::menus::Nav::Handled => return Some(Action::Keep),
                crate::menus::Nav::Run(n) => {
                    return Some(if n == 0 {
                        Action::Ping
                    } else {
                        Action::Refresh
                    });
                }
            }
        }

        match key.code {
            KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                Some(Action::Keep)
            }
            KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1);
                Some(Action::Keep)
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(8);
                Some(Action::Keep)
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(8);
                Some(Action::Keep)
            }
            // Ciclo de filtros: todas · escuchando · establecidas · buscar.
            KeyCode::Char('f') => {
                self.filter = self.filter.next();
                self.scroll = 0;
                if self.filter == Filter::Buscar {
                    self.typing = true;
                    self.query.clear();
                } else {
                    self.query.clear();
                }
                Some(Action::Keep)
            }
            KeyCode::Char('/') => {
                self.filter = Filter::Buscar;
                self.typing = true;
                self.query.clear();
                self.scroll = 0;
                Some(Action::Keep)
            }
            KeyCode::Char('p') => Some(Action::Ping),
            KeyCode::Char('r') => Some(Action::Refresh),
            _ => None,
        }
    }
}
