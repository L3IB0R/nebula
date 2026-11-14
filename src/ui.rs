//! Helpers de dibujo compartidos: paneles "glass", tipografía grande, barras.

use crate::config::Theme;
use crate::wallpaper::{blend, shade, Wallpaper};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget};
use ratatui::Frame;
use std::sync::atomic::{AtomicBool, Ordering};

/// Modo Zen (F11): los paneles pierden borde y título.
pub static ZEN: AtomicBool = AtomicBool::new(false);

pub fn set_zen(v: bool) {
    ZEN.store(v, Ordering::Relaxed);
}

/// Color de fondo de un panel: el color del tema mezclado con el wallpaper
/// que hay debajo (efecto cristal esmerilado).
pub fn glass(theme: &Theme, wp: &Wallpaper, area: Rect, strength: f32) -> Color {
    if wp.mode == "transparent" {
        return blend(theme.bg_panel, theme.bg, strength);
    }
    blend(theme.bg_panel, wp.avg_of(area), strength)
}

/// Panel opaco: limpia su interior (los semibloques del fondo) y luego
/// pinta el borde de cristal. Así el texto siempre se lee perfecto.
pub struct Panel<'a>(pub Block<'a>);

impl<'a> Panel<'a> {
    pub fn inner(&self, area: Rect) -> Rect {
        self.0.inner(area)
    }
}

impl<'a> Widget for Panel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        self.0.render(area, buf);
    }
}

/// Panel con fondo "glass" (mezclado con el wallpaper que hay debajo).
pub fn panel_on<'a>(
    theme: &Theme,
    wp: &Wallpaper,
    area: Rect,
    title: impl Into<ratatui::text::Line<'a>>,
    strength: f32,
) -> Panel<'a> {
    let bg = glass(theme, wp, area, strength);
    let base = Block::new().style(Style::default().bg(bg).fg(theme.fg));
    if ZEN.load(Ordering::Relaxed) {
        // Zen: sólo el cristal, sin bordes ni títulos.
        return Panel(base);
    }
    Panel(
        base.borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border))
            .title(title)
            .title_alignment(Alignment::Left)
            .title_style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
    )
}

/// Bloque de fondo "glass" sin borde.
/// Barra lateral: limpia los semibloques del fondo y pinta un color sólido
/// mezclado con el wallpaper. Si no, los `▀` se cuelan y se leen como cebra.
pub struct Slab<'a>(pub Block<'a>);

impl<'a> Widget for Slab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        self.0.render(area, buf);
    }
}

pub fn slab_solid<'a>(theme: &Theme, wp: &Wallpaper, area: Rect, strength: f32) -> Slab<'a> {
    let bg = glass(theme, wp, area, strength);
    Slab(Block::new().style(Style::default().bg(bg).fg(theme.fg)))
}

/// Funde una zona hacia `base` (alpha 1 = invisible).
pub fn fade_area(buf: &mut Buffer, area: Rect, alpha: f32, base: Color) {
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.001 {
        return;
    }
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_fg(blend(cell.fg, base, a));
                cell.set_bg(blend(cell.bg, base, a));
            }
        }
    }
}

/// Oscurece una zona hacia negro (t = 1 = negro puro).
pub fn dim_area(buf: &mut Buffer, area: Rect, t: f32) {
    fade_area(buf, area, t, Color::Rgb(0, 0, 0))
}

// ── Tipografía grande (reloj / cifras) ────────────────────────────────────

// Fuente 3×5. Cada glifo: 5 filas de 3 celdas.
const FONT: &[(&str, [&str; 5])] = &[
    ("0", ["111", "101", "101", "101", "111"]),
    ("1", ["010", "110", "010", "010", "111"]),
    ("2", ["111", "001", "111", "100", "111"]),
    ("3", ["111", "001", "111", "001", "111"]),
    ("4", ["101", "101", "111", "001", "001"]),
    ("5", ["111", "100", "111", "001", "111"]),
    ("6", ["111", "100", "111", "101", "111"]),
    ("7", ["111", "001", "001", "001", "001"]),
    ("8", ["111", "101", "111", "101", "111"]),
    ("9", ["111", "101", "111", "001", "111"]),
    (":", ["000", "010", "000", "010", "000"]),
    (" ", ["000", "000", "000", "000", "000"]),
    ("A", ["111", "101", "111", "101", "101"]),
    ("P", ["111", "101", "111", "100", "100"]),
    ("M", ["101", "111", "111", "101", "101"]),
    ("V", ["101", "101", "101", "101", "010"]),
    ("?", ["111", "001", "011", "000", "010"]),
];

fn glyph(ch: char) -> [&'static str; 5] {
    let key = ch.to_ascii_uppercase();
    FONT.iter()
        .find(|(c, _)| c.starts_with(key))
        .map(|(_, g)| *g)
        .unwrap_or(["000"; 5])
}

/// Dibuja texto grande con degradado vertical. Devuelve el ancho ocupado.
pub fn draw_big_text(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    text: &str,
    scale: u16,
    top: Color,
    bottom: Color,
) -> u16 {
    let total_rows = 5 * scale;
    let mut cx = x;
    for ch in text.chars() {
        let g = glyph(ch);
        for (gy, row) in g.iter().enumerate() {
            for (gx, bit) in row.chars().enumerate() {
                if bit != '1' {
                    continue;
                }
                for sy in 0..scale {
                    let py = y + (gy as u16 * scale) + sy;
                    // degradado por fila
                    let t = (py - y) as f32 / total_rows.max(1) as f32;
                    let col = blend(top, bottom, t);
                    for sx in 0..scale {
                        let px = cx + (gx as u16 * scale) + sx;
                        if let Some(cell) = buf.cell_mut((px, py)) {
                            cell.set_symbol("█");
                            cell.set_fg(col);
                            cell.set_bg(Color::Reset);
                        }
                    }
                }
            }
        }
        cx += 3 * scale + scale / 2;
    }
    cx - x
}

/// Ancho que ocupará `draw_big_text`.
pub fn big_text_width(text: &str, scale: u16) -> u16 {
    let n = text.chars().count() as u16;
    if n == 0 {
        return 0;
    }
    n * (3 * scale + scale / 2) - scale / 2
}

// ── Reloj de tablero (split-flap) ─────────────────────────────────────────

/// Ancho que ocupa un reloj de tablero con `n` piezas (dígitos y `:`).
pub fn flip_width(n: usize, scale: u16) -> u16 {
    if n == 0 {
        return 0;
    }
    let piece = 3 * scale + 4; // interior 3·scale + 2 bordes + 2 de relleno
    (n as u16) * piece + (n as u16 - 1)
}

/// Altura de una fila del reloj de tablero.
pub fn flip_height(scale: u16) -> u16 {
    5 * scale + 2
}

/// Dibuja un reloj estilo split-flap: cada carácter en su propia casilla
/// con bisel, como los relojes de aeropuerto de los 60.
///
/// `text` puede contener dígitos y `:`. Devuelve el ancho ocupado.
pub fn draw_flip(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    text: &str,
    scale: u16,
    grad: (Color, Color),
    edge: Color,
) -> u16 {
    let (top, bottom) = grad;
    let h = flip_height(scale);
    let mut cx = x;
    for ch in text.chars() {
        let w = 3 * scale + 4;
        let g = glyph(ch);
        // Borde superior
        set_cell(buf, cx, y, "┌", edge);
        for i in 1..w - 1 {
            set_cell(buf, cx + i, y, "─", edge);
        }
        set_cell(buf, cx + w - 1, y, "┐", edge);
        // Interior: 5·scale filas de glifo
        for (gy, grow) in g.iter().enumerate() {
            for sy in 0..scale {
                let py = y + 1 + (gy as u16 * scale) + sy;
                let t = (py - y) as f32 / (h as f32);
                let col = blend(top, bottom, t);
                set_cell(buf, cx, py, "│", edge);
                set_cell(buf, cx + w - 1, py, "│", edge);
                for gx in 0..3usize {
                    let on = grow.as_bytes()[gx] == b'1';
                    for sx in 0..scale {
                        let px = cx + 2 + (gx as u16 * scale) + sx;
                        set_cell(
                            buf,
                            px,
                            py,
                            if on { "█" } else { " " },
                            if on { col } else { Color::Reset },
                        );
                    }
                }
            }
        }
        // Borde inferior
        set_cell(buf, cx, y + h - 1, "└", edge);
        for i in 1..w - 1 {
            set_cell(buf, cx + i, y + h - 1, "─", edge);
        }
        set_cell(buf, cx + w - 1, y + h - 1, "┘", edge);
        cx += w + 1;
    }
    cx - x
}

// ── Reloj analógico ───────────────────────────────────────────────────────

/// Ancho/alto mínimos de una cara de reloj con radio `r` (el alto va a la
/// mitad: una celda del terminal es el doble de alta que de ancha).
pub fn analog_size(r: u16) -> (u16, u16) {
    (2 * r + 1, r + 1)
}

/// Dibuja una cara de reloj analógica centrada en `area` con las manecillas
/// apuntando a `hour` (0..12) y `minute` (0..60). Devuelve el centro dibujado.
pub fn draw_analog(
    buf: &mut Buffer,
    area: Rect,
    hour: u32,
    minute: u32,
    second: u32,
    cols: (Color, Color, Color),
) -> Option<(u16, u16)> {
    let (face, hands, accent) = cols;
    if area.width < 7 || area.height < 5 {
        return None;
    }
    // Radio: el círculo debe caber en ancho y alto.
    let r = ((area.width.saturating_sub(1) / 2).min(area.height.saturating_sub(1) * 2)) as i32;
    if r < 3 {
        return None;
    }
    let cx = area.x + area.width / 2;
    let cy = area.y + area.height / 2;

    // Aro: para cada celda, ¿está a distancia r del centro?
    let rx = r as f32;
    let ry = (r as f32) / 2.0;
    for dy in -(r / 2) - 1..=r / 2 + 1 {
        for dx in -r - 1..=r + 1 {
            let fx = dx as f32 / rx;
            let fy = dy as f32 / ry;
            let d = (fx * fx + fy * fy).sqrt();
            if (d - 1.0).abs() > 0.16 {
                continue;
            }
            // Marcas cardinales más nítidas
            let cardinal = (d - 1.0).abs() < 0.16
                && (dx == 0 || dy == 0)
                && (dx.abs() * 2 >= r * 3 / 4 || dy.abs() * 4 >= r * 3 / 4);
            let sym = if cardinal { "◆" } else { "·" };
            let col = if cardinal { accent } else { face };
            if let Some(cell) = buf.cell_mut((
                cx.saturating_add_signed(dx as i16),
                cy.saturating_add_signed(dy as i16),
            )) {
                cell.set_symbol(sym);
                cell.set_fg(col);
            }
        }
    }

    // Manecillas: se trazan como puntos desde el centro hacia el destino.
    let mut hand = |frac: f32, len: f32, col: Color, sym: &str| {
        let steps = ((len * 6.0) as i32).max(1);
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let px = (rx * len * t * frac.sin()) as i32;
            let py = (-ry * len * t * frac.cos()) as i32;
            if let Some(cell) = buf.cell_mut((
                cx.saturating_add_signed(px as i16),
                cy.saturating_add_signed(py as i16),
            )) {
                cell.set_symbol(sym);
                cell.set_fg(col);
            }
        }
    };
    let tau = std::f32::consts::TAU;
    // Minutero (largo) y segundero (finísimo)
    let m_ang = (minute as f32 + second as f32 / 60.0) / 60.0 * tau;
    hand(m_ang, 0.86, hands, "•");
    let s_ang = second as f32 / 60.0 * tau;
    hand(s_ang, 0.92, accent, "·");
    // Horario (corto y grueso)
    let h_ang = ((hour % 12) as f32 + minute as f32 / 60.0) / 12.0 * tau;
    hand(h_ang, 0.55, hands, "▪");

    // Centro
    if let Some(cell) = buf.cell_mut((cx, cy)) {
        cell.set_symbol("◉");
        cell.set_fg(accent);
    }
    Some((cx, cy))
}

/// Escribe un símbolo en una celda si existe (sin tocar el resto del estilo).
fn set_cell(buf: &mut Buffer, x: u16, y: u16, sym: &str, col: Color) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_symbol(sym);
        cell.set_fg(col);
    }
}

// ── Barras ────────────────────────────────────────────────────────────────

/// Barra de progreso con bloques degradados: `░▒▓█`.
pub fn bar(percent: f32, width: usize) -> String {
    let p = percent.clamp(0.0, 1.0);
    let filled = (p * width as f32).round() as usize;
    let mut s = String::new();
    for i in 0..width {
        if i < filled {
            s.push('█');
        } else {
            s.push('·');
        }
    }
    s
}

/// Barra fina con caracteres medios: útil para filas compactas.
pub fn thin_bar(percent: f32, width: usize) -> String {
    let p = percent.clamp(0.0, 1.0);
    let filled = (p * width as f32).round() as usize;
    let mut s = String::new();
    for i in 0..width {
        s.push(if i < filled { '━' } else { '─' });
    }
    s
}

// ── Pie de atajos ─────────────────────────────────────────────────────────

/// Pie estándar de atajos: la tecla en acento en negrita y la descripción
/// en tenue. Todos los módulos dibujan el suyo con esto, para que la barra
/// se leiga siempre igual.
pub fn hint_line(theme: &Theme, items: &[(&str, &str)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (k, d)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("   ", Style::default()));
        }
        spans.push(Span::styled(
            format!(" {k} "),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            (*d).to_string(),
            Style::default().fg(theme.fg_dim),
        ));
    }
    Line::from(spans)
}

/// Dibuja el pie de atajos en `area` (una fila).
pub fn hint_bar(f: &mut Frame, theme: &Theme, area: Rect, items: &[(&str, &str)]) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    f.render_widget(Paragraph::new(hint_line(theme, items)), area);
}

// ── Texto ────────────────────────────────────────────────────────────────

/// Serie para `Sparkline`: alineada a la derecha (lo más reciente, a la derecha)
/// y con el hueco relleno con el primer valor para que el panel nunca quede vacío.
pub fn spark(hist: &[u64], width: usize) -> Vec<u64> {
    if width == 0 {
        return Vec::new();
    }
    if hist.is_empty() {
        return vec![0; width];
    }
    let tail: Vec<u64> = hist.iter().rev().take(width).rev().copied().collect();
    let fill = tail.iter().copied().min().unwrap_or(0).max(1);
    let mut out = vec![fill; width - tail.len()];
    out.extend(tail);
    out
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}

/// Formatea bytes en unidades legibles.
pub fn human_bytes(b: u64) -> String {
    const U: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{v:.0} {}", U[i])
    } else {
        format!("{v:.1} {}", U[i])
    }
}

pub fn human_rate(bytes_per_sec: f64) -> String {
    human_bytes(bytes_per_sec as u64) + "/s"
}

/// Duración en segundos → mm:ss / h:mm:ss
pub fn human_time(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Sombra suave alrededor de un área (oscurece el borde exterior).
pub fn vignette(buf: &mut Buffer, area: Rect, strength: f32) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let dx = (x as f32 - (area.width as f32 / 2.0)).abs() / (area.width as f32 / 2.0);
            let dy = (y as f32 - (area.height as f32 / 2.0)).abs() / (area.height as f32 / 2.0);
            let d = (dx.max(dy)).clamp(0.0, 1.0);
            let t = (d * d) * strength;
            if t < 0.02 {
                continue;
            }
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_bg(shade(cell.bg, t));
                if cell.fg != cell.bg {
                    cell.set_fg(shade(cell.fg, t * 0.6));
                }
            }
        }
    }
}
