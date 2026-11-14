//! Fondo de pantalla dentro del TUI.
//!
//! Tres modos:
//!  * `image`      — la imagen se reduce a la resolución de celdas del terminal y se
//!    dibuja con semibloques (`▀` con fg = píxel superior, bg = inferior).
//!    Funciona en cualquier terminal con truecolor y se ve precioso.
//!  * `transparent`— no se pinta nada: se ve el fondo real del terminal.
//!  * `solid`      — un color plano del tema.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

#[derive(Clone)]
pub struct Wallpaper {
    /// Un color por celda: (parte superior, parte inferior) del semibloque.
    cells: Vec<(Color, Color)>,
    pub width: u16,
    pub height: u16,
    /// Media de los colores de la imagen, para mezclar con los paneles "glass".
    pub avg: Color,
    /// Modo activo: image | transparent | solid
    pub mode: String,
    /// Color base cuando no hay imagen (solid / transparent + tema).
    pub base: Color,
}

impl Wallpaper {
    pub fn empty(mode: &str, base: Color) -> Self {
        Wallpaper {
            cells: Vec::new(),
            width: 0,
            height: 0,
            avg: base,
            mode: mode.into(),
            base,
        }
    }

    /// Construye el wallpaper a partir de una ruta de imagen.
    pub fn from_image(path: &std::path::Path, w: u16, h: u16, base: Color, opacity: f32) -> Self {
        let mut wp = Wallpaper::empty("image", base);
        if w < 4 || h < 4 {
            return wp;
        }
        let decoded = image::ImageReader::open(path)
            .and_then(|r| r.with_guessed_format())
            .map_err(|e| e.to_string())
            .and_then(|r| r.decode().map_err(|e| e.to_string()));
        let img = match decoded {
            Ok(i) => i,
            Err(e) => {
                if std::env::var("NEBULA_DEBUG").is_ok() {
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("/tmp/nebula_wp.log")
                    {
                        let _ = writeln!(f, "open error for {path:?}: {e}");
                    }
                }
                return Wallpaper::empty("solid", base);
            }
        };
        let img = img.to_rgb8();
        // Semibloque = 1 celda de ancho × 2 píxeles de alto.
        let tw = w as u32;
        let th = (h as u32) * 2;
        let small = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle);

        let mut cells = Vec::with_capacity((w * h) as usize);
        let mut sr = 0u32;
        let mut sg = 0u32;
        let mut sb = 0u32;
        for y in 0..h as u32 {
            for x in 0..w as u32 {
                let top = small.get_pixel(x, y * 2).0;
                let bot = small.get_pixel(x, y * 2 + 1).0;
                let (t, b) = (dim(top, opacity), dim(bot, opacity));
                sr += t[0] as u32 + b[0] as u32;
                sg += t[1] as u32 + b[1] as u32;
                sb += t[2] as u32 + b[2] as u32;
                cells.push((Color::Rgb(t[0], t[1], t[2]), Color::Rgb(b[0], b[1], b[2])));
            }
        }
        let n = (cells.len() * 2) as u32;
        wp.avg = Color::Rgb((sr / n) as u8, (sg / n) as u8, (sb / n) as u8);
        wp.cells = cells;
        wp.width = w;
        wp.height = h;
        wp
    }

    /// Color promedio de una región (para el fondo de los paneles glass).
    pub fn avg_of(&self, area: Rect) -> Color {
        if self.cells.is_empty() || self.width == 0 {
            return self.avg;
        }
        let mut r = 0u32;
        let mut g = 0u32;
        let mut b = 0u32;
        let mut n = 0u32;
        let x0 = area.x as usize;
        let y0 = area.y as usize;
        let x1 = (area.x + area.width).min(self.width) as usize;
        let y1 = (area.y + area.height).min(self.height) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some((t, bo)) = self.cells.get(y * self.width as usize + x) {
                    for c in [t, bo] {
                        if let Color::Rgb(rr, gg, bb) = c {
                            r += *rr as u32;
                            g += *gg as u32;
                            b += *bb as u32;
                            n += 1;
                        }
                    }
                }
            }
        }
        match (r.checked_div(n), g.checked_div(n), b.checked_div(n)) {
            (Some(r), Some(g), Some(b)) => Color::Rgb(r as u8, g as u8, b as u8),
            _ => self.avg,
        }
    }

    /// Color de fondo (parte superior) en una celda — útil para mezclar.
    pub fn cell(&self, x: u16, y: u16) -> Option<Color> {
        if self.width == 0 {
            return None;
        }
        self.cells
            .get(y as usize * self.width as usize + x as usize)
            .map(|(t, _)| *t)
    }

    /// Dibuja el fondo en el buffer. `stars` añade puntitos titilando.
    pub fn render(&self, buf: &mut Buffer, area: Rect, stars: bool, tick: u64) {
        if self.mode.as_str() == "transparent" {
            return;
        }
        if self.cells.is_empty() {
            // Modo solid: color plano.
            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_symbol(" ");
                        cell.set_bg(self.base);
                        cell.set_fg(self.base);
                    }
                }
            }
            if stars {
                self.draw_stars(buf, area, tick);
            }
            return;
        }

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let idx = y as usize * self.width as usize + x as usize;
                let Some((top, bot)) = self.cells.get(idx) else {
                    continue;
                };
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol("▀");
                    cell.set_fg(*top);
                    cell.set_bg(*bot);
                }
            }
        }
        if stars {
            self.draw_stars(buf, area, tick);
        }
    }

    /// Estrellas muy sutiles que parpadean lento.
    fn draw_stars(&self, buf: &mut Buffer, area: Rect, tick: u64) {
        // Determinista: semilla fija por celda.
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let h = (x as u32)
                    .wrapping_mul(374761393)
                    .wrapping_add((y as u32).wrapping_mul(668265263));
                if !h.is_multiple_of(977) {
                    continue;
                }
                let phase = (h.wrapping_add((tick as u32).wrapping_mul(3)) % 24) as i32;
                let bright = if phase < 12 { phase } else { 24 - phase };
                let a = (bright as f32 / 12.0) * 0.55;
                let star = blend(
                    Color::Rgb(230, 237, 247),
                    self.cell(x, y).unwrap_or(self.base),
                    a,
                );
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol("·");
                    cell.set_fg(star);
                }
            }
        }
    }
}

fn dim(p: [u8; 3], opacity: f32) -> [u8; 3] {
    let o = opacity.clamp(0.05, 1.0);
    [
        (p[0] as f32 * o) as u8,
        (p[1] as f32 * o) as u8,
        (p[2] as f32 * o) as u8,
    ]
}

/// Mezcla dos colores: `t` = peso del segundo color (0..1).
pub fn blend(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => Color::Rgb(
            (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8,
            (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8,
            (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8,
        ),
        (x, _) => x,
    }
}

/// Oscurece un color hacia negro.
pub fn shade(c: Color, t: f32) -> Color {
    blend(c, Color::Rgb(0, 0, 0), t)
}

/// Aclara un color hacia blanco.
pub fn tint(c: Color, t: f32) -> Color {
    blend(c, Color::Rgb(255, 255, 255), t)
}
