//! Configuración y temas de Nébula.
//! Todo se guarda en ~/.config/nebula/config.toml

use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Rutas ────────────────────────────────────────────────────────────────

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "~".to_string()))
}

pub fn config_dir() -> PathBuf {
    let xdg = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty());
    match xdg {
        Some(d) => PathBuf::from(d).join("nebula"),
        None => home().join(".config").join("nebula"),
    }
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

// ── Colores hex ⇄ Color ──────────────────────────────────────────────────

pub fn parse_hex(s: &str) -> Option<Color> {
    let t = s.trim().trim_start_matches('#');
    if t.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&t[0..2], 16).ok()?;
    let g = u8::from_str_radix(&t[2..4], 16).ok()?;
    let b = u8::from_str_radix(&t[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

pub fn color_hex(c: &Color) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        other => format!("{other:?}"),
    }
}

mod hex_color {
    use super::{color_hex, parse_hex};
    use ratatui::style::Color;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(c: &Color, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&color_hex(c))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Color, D::Error> {
        let s = String::deserialize(d)?;
        parse_hex(&s).ok_or_else(|| serde::de::Error::custom(format!("color inválido: {s}")))
    }
}

/// Luminancia simple (0..=255) para elegir texto sobre un color.
fn luminance(c: Color) -> f32 {
    match c {
        Color::Rgb(r, g, b) => 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32,
        _ => 128.0,
    }
}

/// Si Pywal está activo, los acentos del tema salen de su colors.json.
/// Si el fichero no existe, el tema se queda como estaba (degradación limpia).
fn apply_pywal(t: &mut Theme) {
    let Some(home) = std::env::var("HOME").ok() else {
        return;
    };
    let path = std::path::Path::new(&home).join(".cache/wal/colors.json");
    let Ok(txt) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
        return;
    };
    let hex = |key: &str| -> Option<Color> {
        v.get("colors")
            .and_then(|c| c.get(key))
            .and_then(|x| x.as_str())
            .and_then(parse_hex)
    };
    let special = |key: &str| -> Option<Color> {
        v.get("special")
            .and_then(|c| c.get(key))
            .and_then(|x| x.as_str())
            .and_then(parse_hex)
    };

    let bg = special("background")
        .or_else(|| hex("color0"))
        .unwrap_or(t.bg);
    let fg = special("foreground")
        .or_else(|| hex("color7"))
        .unwrap_or(t.fg);
    t.bg = bg;
    t.fg = fg;
    t.bg_panel = crate::wallpaper::blend(bg, fg, 0.07);
    t.bg_elevated = crate::wallpaper::blend(bg, fg, 0.13);
    t.fg_dim = crate::wallpaper::blend(fg, bg, 0.35);
    t.fg_faint = crate::wallpaper::blend(fg, bg, 0.60);
    t.accent = hex("color4").unwrap_or(t.accent);
    t.accent2 = hex("color13")
        .or_else(|| hex("color12"))
        .unwrap_or(t.accent2);
    t.ok = hex("color2").unwrap_or(t.ok);
    t.warn = hex("color3").unwrap_or(t.warn);
    t.err = hex("color1").unwrap_or(t.err);
    t.info = hex("color6").unwrap_or(t.info);
    t.border = crate::wallpaper::blend(fg, bg, 0.78);
    t.border_focus = t.accent;
    t.sel_bg = t.accent;
    t.sel_fg = if luminance(t.accent) < 140.0 {
        Color::Rgb(255, 255, 255)
    } else {
        Color::Rgb(12, 12, 12)
    };
    t.name = format!("{} · pywal", t.name.replace(" · pywal", ""));
}

// ── Tema ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    #[serde(with = "hex_color")]
    pub bg: Color,
    #[serde(with = "hex_color")]
    pub bg_panel: Color,
    #[serde(with = "hex_color")]
    pub bg_elevated: Color,
    #[serde(with = "hex_color")]
    pub fg: Color,
    #[serde(with = "hex_color")]
    pub fg_dim: Color,
    #[serde(with = "hex_color")]
    pub fg_faint: Color,
    #[serde(with = "hex_color")]
    pub accent: Color,
    #[serde(with = "hex_color")]
    pub accent2: Color,
    #[serde(with = "hex_color")]
    pub ok: Color,
    #[serde(with = "hex_color")]
    pub warn: Color,
    #[serde(with = "hex_color")]
    pub err: Color,
    #[serde(with = "hex_color")]
    pub info: Color,
    #[serde(with = "hex_color")]
    pub border: Color,
    #[serde(with = "hex_color")]
    pub border_focus: Color,
    #[serde(with = "hex_color")]
    pub sel_bg: Color,
    #[serde(with = "hex_color")]
    pub sel_fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        presets()[0].clone()
    }
}

impl Theme {
    pub fn by_name(name: &str) -> Theme {
        presets()
            .into_iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| presets()[0].clone())
    }

    /// Interpola linealmente entre dos temas (para transiciones suaves).
    pub fn lerp(a: &Theme, b: &Theme, t: f32) -> Theme {
        let t = t.clamp(0.0, 1.0);
        let mix = |x: Color, y: Color| -> Color {
            // Un color de consola (índice) se mezcla como su vecino VGA:
            // así la transición sigue siendo un degradado en la TTY.
            let x = index_rgb(x);
            let y = index_rgb(y);
            match (x, y) {
                (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => Color::Rgb(
                    (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8,
                    (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8,
                    (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8,
                ),
                (x, _) => x,
            }
        };
        Theme {
            name: if t < 0.5 {
                a.name.clone()
            } else {
                b.name.clone()
            },
            bg: mix(a.bg, b.bg),
            bg_panel: mix(a.bg_panel, b.bg_panel),
            bg_elevated: mix(a.bg_elevated, b.bg_elevated),
            fg: mix(a.fg, b.fg),
            fg_dim: mix(a.fg_dim, b.fg_dim),
            fg_faint: mix(a.fg_faint, b.fg_faint),
            accent: mix(a.accent, b.accent),
            accent2: mix(a.accent2, b.accent2),
            ok: mix(a.ok, b.ok),
            warn: mix(a.warn, b.warn),
            err: mix(a.err, b.err),
            info: mix(a.info, b.info),
            border: mix(a.border, b.border),
            border_focus: mix(a.border_focus, b.border_focus),
            sel_bg: mix(a.sel_bg, b.sel_bg),
            sel_fg: mix(a.sel_fg, b.sel_fg),
        }
    }

    /// El mismo tema, reducido a los 16 colores de una consola.
    ///
    /// En una TTY (`TERM=linux`) un RGB se dibuja mal: la consola lo
    /// aproxima a su paleta y el resultado es un color vecino imprevisible.
    /// Aquí cada color cae en el más cercano de la rejilla VGA y viaja como
    /// índice, que es lo que la consola pinta exactamente.
    pub fn for_tty(&self) -> Theme {
        let q = quantize16;
        Theme {
            name: self.name.clone(),
            bg: q(self.bg),
            bg_panel: q(self.bg_panel),
            bg_elevated: q(self.bg_elevated),
            fg: q(self.fg),
            fg_dim: q(self.fg_dim),
            fg_faint: q(self.fg_faint),
            accent: q(self.accent),
            accent2: q(self.accent2),
            ok: q(self.ok),
            warn: q(self.warn),
            err: q(self.err),
            info: q(self.info),
            border: q(self.border),
            border_focus: q(self.border_focus),
            sel_bg: q(self.sel_bg),
            sel_fg: q(self.sel_fg),
        }
    }
}

// ── Consola de 16 colores (TTY) ───────────────────────────────────────────

/// Paleta VGA nominal: los 16 colores de una consola, en orden canónico.
const VGA: [[u8; 3]; 16] = [
    [0, 0, 0],
    [128, 0, 0],
    [0, 128, 0],
    [128, 128, 0],
    [0, 0, 128],
    [128, 0, 128],
    [0, 128, 128],
    [192, 192, 192],
    [128, 128, 128],
    [255, 0, 0],
    [0, 255, 0],
    [255, 255, 0],
    [0, 0, 255],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
];

/// Un color de consola (índice 0..15) como su RGB nominal; el resto, igual.
fn index_rgb(c: Color) -> Color {
    match c {
        Color::Indexed(i) => {
            let v = VGA.get(usize::from(i)).copied().unwrap_or([255, 255, 255]);
            Color::Rgb(v[0], v[1], v[2])
        }
        other => other,
    }
}

/// Acerca un color a la rejilla de los 16 colores de una consola y lo
/// devuelve como índice (lo que una TTY dibuja sin approximaciones).
pub fn quantize16(c: Color) -> Color {
    let (r, g, b) = match c {
        // Los nominales ya son el índice de su color.
        Color::Black => return Color::Indexed(0),
        Color::Red => return Color::Indexed(1),
        Color::Green => return Color::Indexed(2),
        Color::Yellow => return Color::Indexed(3),
        Color::Blue => return Color::Indexed(4),
        Color::Magenta => return Color::Indexed(5),
        Color::Cyan => return Color::Indexed(6),
        Color::Gray => return Color::Indexed(7),
        Color::DarkGray => return Color::Indexed(8),
        Color::LightRed => return Color::Indexed(9),
        Color::LightGreen => return Color::Indexed(10),
        Color::LightYellow => return Color::Indexed(11),
        Color::LightBlue => return Color::Indexed(12),
        Color::LightMagenta => return Color::Indexed(13),
        Color::LightCyan => return Color::Indexed(14),
        Color::White => return Color::Indexed(15),
        Color::Indexed(i) => return Color::Indexed(i.min(15)),
        Color::Rgb(r, g, b) => (r, g, b),
        other => return other, // Reset y compañía: no se tocan
    };
    let mut best = 0usize;
    let mut dist = i32::MAX;
    for (i, v) in VGA.iter().enumerate() {
        let d = (i32::from(r) - i32::from(v[0])).pow(2)
            + (i32::from(g) - i32::from(v[1])).pow(2)
            + (i32::from(b) - i32::from(v[2])).pow(2);
        if d < dist {
            dist = d;
            best = i;
        }
    }
    Color::Indexed(best as u8)
}

/// ¿La salida es una consola de 16 colores (la TTY de un servidor)?
pub fn tty_palette() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal() && std::env::var("TERM").is_ok_and(|t| t == "linux")
}

pub fn presets() -> Vec<Theme> {
    let mk = |name: &str,
              bg: &str,
              bg_panel: &str,
              bg_elevated: &str,
              fg: &str,
              fg_dim: &str,
              fg_faint: &str,
              accent: &str,
              accent2: &str,
              ok: &str,
              warn: &str,
              err: &str,
              info: &str,
              border: &str,
              border_focus: &str,
              sel_bg: &str,
              sel_fg: &str|
     -> Theme {
        let p = parse_hex;
        Theme {
            name: name.into(),
            bg: p(bg).unwrap(),
            bg_panel: p(bg_panel).unwrap(),
            bg_elevated: p(bg_elevated).unwrap(),
            fg: p(fg).unwrap(),
            fg_dim: p(fg_dim).unwrap(),
            fg_faint: p(fg_faint).unwrap(),
            accent: p(accent).unwrap(),
            accent2: p(accent2).unwrap(),
            ok: p(ok).unwrap(),
            warn: p(warn).unwrap(),
            err: p(err).unwrap(),
            info: p(info).unwrap(),
            border: p(border).unwrap(),
            border_focus: p(border_focus).unwrap(),
            sel_bg: p(sel_bg).unwrap(),
            sel_fg: p(sel_fg).unwrap(),
        }
    };

    vec![
        // Azul-violeta profundo, elegante y futurista
        mk(
            "Aurora", "#0a0e1a", "#101728", "#16203a", "#e6edf7", "#8fa0c0", "#4a5878", "#7ee8fa",
            "#a78bfa", "#7ef0a8", "#f5c97a", "#ff7b8a", "#8ecdf8", "#2a3854", "#7ee8fa", "#1c2942",
            "#e6edf7",
        ),
        // Cian puro sobre negro azulado — limpio y tecnológico
        mk(
            "Cobalto", "#060d14", "#0b1622", "#101f30", "#e8f4fb", "#7fa8c4", "#3f5f7a", "#38d6f0",
            "#5aa9ff", "#5df2b0", "#ffd166", "#ff6b8b", "#62c8ff", "#17304a", "#38d6f0", "#0f2338",
            "#e8f4fb",
        ),
        // Naranja cálido sobre carbón — estilo panel de instrumentos
        mk(
            "Ámbar", "#12100c", "#1b1812", "#242018", "#f7efe2", "#bfae92", "#6f6350", "#ffb454",
            "#ff8a5c", "#9fe08a", "#ffd166", "#ff7b6b", "#ffc98a", "#3a3226", "#ffb454", "#2a2418",
            "#f7efe2",
        ),
        // Sepia antiguo con acentos dorados — nostálgico y cálido
        mk(
            "Sepia", "#14100b", "#1e1810", "#282016", "#f2e8d8", "#b3a288", "#6b5f4c", "#e8b86d",
            "#d99a5a", "#a8c98a", "#e8c87a", "#d97a6a", "#c9a87a", "#3a3020", "#e8b86d", "#2a2216",
            "#f2e8d8",
        ),
        // Verde esmeralda sobre verde oscuro — tipo terminal serena
        mk(
            "Bosque", "#0a1210", "#0f1b17", "#14251f", "#e7f5ee", "#8fb8a6", "#4a6f60", "#5fe8b0",
            "#8ad97a", "#7ef0a8", "#d9e87a", "#ff8a8a", "#7ad9c0", "#1e3a30", "#5fe8b0", "#14302a",
            "#e7f5ee",
        ),
        // Rosa y violeta sobre púrpura profundo — suave y luminosa
        mk(
            "Orquídea",
            "#120a18",
            "#1b1026",
            "#241634",
            "#f6ecfa",
            "#b99fc9",
            "#6c5580",
            "#f79bd3",
            "#b18af5",
            "#8af0c8",
            "#f5d97a",
            "#ff8ab5",
            "#c9a0f0",
            "#38244a",
            "#f79bd3",
            "#28183a",
            "#f6ecfa",
        ),
    ]
}

// ── Configuración ────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub theme: String,
    /// Nombre para el saludo del Inicio.
    pub name: String,
    pub clock_24h: bool,
    pub show_seconds: bool,
    /// Tamaño del reloj de Inicio:
    /// "auto" | "bar" | "small" | "medium" | "large"
    pub clock_size: String,
    /// Visualización del reloj:
    /// "digital" | "linea" | "flip" | "reloj" | "minimal"
    pub clock_style: String,
    /// ¿Arrancar con la barra lateral abierta?
    pub sidebar: bool,
}

impl Default for General {
    fn default() -> Self {
        let name = std::env::var("USER").unwrap_or_default();
        Self {
            theme: "Aurora".into(),
            name,
            clock_24h: true,
            show_seconds: true,
            clock_size: "auto".into(),
            clock_style: "digital".into(),
            sidebar: true,
        }
    }
}

/// Opciones del tamaño del reloj, en el orden en que las recorre el ajuste.
pub const CLOCK_SIZES: [&str; 5] = ["auto", "bar", "small", "medium", "large"];

/// Opciones de visualización del reloj.
pub const CLOCK_STYLES: [&str; 5] = ["digital", "linea", "flip", "reloj", "minimal"];

/// Etiqueta legible de cada tamaño.
pub fn clock_label(v: &str) -> &'static str {
    match v {
        "bar" => "línea",
        "small" => "pequeño",
        "medium" => "mediano",
        "large" => "grande",
        _ => "auto",
    }
}

/// Etiqueta legible de cada estilo de reloj.
pub fn clock_style_label(v: &str) -> &'static str {
    match v {
        "linea" => "una línea",
        "flip" => "tablero",
        "reloj" => "analógico",
        "minimal" => "mínimo",
        _ => "digital",
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// "image" | "transparent" | "solid"
    pub background: String,
    /// Ruta del fondo; vacío = autodetectar.
    pub wallpaper: String,
    /// Opacidad del fondo 0.1 ..= 1.0 (1 = sin oscurecer).
    pub wallpaper_opacity: f32,
    pub vignette: bool,
    pub stars: bool,
    /// "glass" (paneles translúcidos) | "solid" (paneles opacos).
    pub panel: String,
    pub animations: bool,
    /// Tomar los acentos de Pywal (~/.cache/wal/colors.json) si existe.
    pub pywal: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            background: "image".into(),
            wallpaper: String::new(),
            wallpaper_opacity: 0.55,
            vignette: true,
            stars: true,
            panel: "glass".into(),
            animations: true,
            pywal: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Projects {
    /// Directorios donde buscar proyectos.
    pub dirs: Vec<String>,
    /// Editor para abrir proyectos.
    pub editor: String,
    /// Nombre máximo de proyectos a mostrar.
    pub max: usize,
}

impl Default for Projects {
    fn default() -> Self {
        Self {
            dirs: vec![
                "~".into(),
                "~/Documentos".into(),
                "~/Proyectos".into(),
                "~/proyectos".into(),
                "~/Escritorio".into(),
            ],
            editor: "gnome-text-editor".into(),
            max: 120,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DbConn {
    pub name: String,
    #[serde(default = "d_host")]
    pub host: String,
    #[serde(default = "d_port")]
    pub port: u16,
    #[serde(default = "d_user")]
    pub user: String,
    pub dbname: String,
    #[serde(default)]
    pub password: String,
    /// Nombre de variable de entorno con la contraseña (preferido sobre `password`).
    #[serde(default)]
    pub password_env: String,
}

fn d_host() -> String {
    "localhost".into()
}
fn d_port() -> u16 {
    5432
}
fn d_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "postgres".into())
}

/// Widgets del Inicio: cuáles se ven, en qué orden y con qué forma.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Home {
    /// Nombres de los widgets visibles, en el orden en que se pintan.
    /// Disponibles: "clima", "musica", "sistema", "proyectos", "red",
    /// "disco" y "mundo". Un widget que no esté aquí no se muestra.
    pub widgets: Vec<String>,
    /// Forma de cada widget: nombre → variante
    /// (p. ej. `sistema = "compacto"`, `red = "grafico"`).
    pub styles: std::collections::BTreeMap<String, String>,
}

impl Default for Home {
    fn default() -> Self {
        let mut styles = std::collections::BTreeMap::new();
        styles.insert("sistema".into(), "completo".into());
        styles.insert("red".into(), "grafico".into());
        styles.insert("musica".into(), "tarjeta".into());
        styles.insert("proyectos".into(), "lista".into());
        styles.insert("clima".into(), "tarjeta".into());
        styles.insert("disco".into(), "barras".into());
        styles.insert("mundo".into(), "tabla".into());
        Home {
            widgets: vec![
                "clima".into(),
                "musica".into(),
                "sistema".into(),
                "proyectos".into(),
                "red".into(),
            ],
            styles,
        }
    }
}

impl Home {
    /// ¿Está el widget activo?
    pub fn on(&self, name: &str) -> bool {
        self.widgets.iter().any(|w| w == name)
    }

    /// Forma elegida para un widget (o la por defecto).
    pub fn style_of(&self, name: &str) -> &str {
        self.styles.get(name).map(|s| s.as_str()).unwrap_or("")
    }

    /// Cicla la forma de un widget entre sus variantes.
    pub fn cycle_style(&mut self, name: &str) {
        let variants = style_variants(name);
        if variants.is_empty() {
            return;
        }
        let cur = self.style_of(name);
        let pos = variants.iter().position(|v| *v == cur).unwrap_or(0);
        self.styles.insert(
            name.into(),
            variants[(pos + 1) % variants.len()].to_string(),
        );
    }
}

/// Variantes de forma de cada widget (la primera es la por defecto).
pub fn style_variants(widget: &str) -> Vec<&'static str> {
    match widget {
        "clima" => vec!["tarjeta", "linea"],
        "musica" => vec!["tarjeta", "linea"],
        "sistema" => vec!["completo", "compacto", "mini"],
        "proyectos" => vec!["lista", "conteo"],
        "red" => vec!["grafico", "filas", "mini"],
        "disco" => vec!["barras", "lista"],
        "mundo" => vec!["tabla", "linea"],
        _ => vec![],
    }
}

/// Widgets disponibles, en el orden en que los recorre el ajuste.
pub const WIDGETS: [&str; 7] = [
    "clima",
    "musica",
    "sistema",
    "proyectos",
    "red",
    "disco",
    "mundo",
];

/// Nombre legible de cada widget.
pub fn widget_label(w: &str) -> &'static str {
    match w {
        "clima" => "clima",
        "musica" => "música",
        "sistema" => "sistema",
        "proyectos" => "proyectos",
        "red" => "red",
        "disco" => "discos",
        "mundo" => "mundo",
        _ => "widget",
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Keys {
    /// Ej.: "ctrl+k", "alt+p", "f5"
    pub launcher: String,
    pub help: String,
    pub zen: String,
    pub sidebar: String,
    /// Entra y sale del foco del menú lateral.
    pub sidebar_focus: String,
    pub mute: String,
}

impl Default for Keys {
    fn default() -> Self {
        Keys {
            launcher: "ctrl+k".into(),
            help: "?".into(),
            zen: "f11".into(),
            sidebar: "ctrl+b".into(),
            sidebar_focus: "ctrl+e".into(),
            mute: "ctrl+space".into(),
        }
    }
}

/// ¿Coincide la tecla con una especificación tipo "ctrl+shift+f5", "?" o "space"?
///
/// Los modificadores que no se piden se ignoran (así "?" funciona con o sin mayús),
/// pero los que sí se piden deben estar.
pub fn key_matches(spec: &str, key: crossterm::event::KeyEvent) -> bool {
    use crossterm::event::{KeyCode, KeyModifiers};
    let raw = spec.trim().to_lowercase();
    if raw.is_empty() {
        return false;
    }
    let parts: Vec<&str> = raw.split('+').collect();
    let Some((base, mods)) = parts.split_last() else {
        return false;
    };
    let base = *base;
    let want_ctrl = mods.iter().any(|m| matches!(*m, "ctrl" | "control"));
    let want_alt = mods.iter().any(|m| matches!(*m, "alt" | "meta" | "super"));
    let want_shift = mods.contains(&"shift");

    let has = |f: KeyModifiers| key.modifiers.contains(f);
    if has(KeyModifiers::CONTROL) != want_ctrl {
        return false;
    }
    if has(KeyModifiers::ALT) != want_alt {
        return false;
    }
    if want_shift && !has(KeyModifiers::SHIFT) {
        return false;
    }

    match base {
        "space" => key.code == KeyCode::Char(' '),
        "tab" => key.code == KeyCode::Tab,
        "enter" | "return" => key.code == KeyCode::Enter,
        "esc" | "escape" => key.code == KeyCode::Esc,
        "backspace" => key.code == KeyCode::Backspace,
        "delete" => key.code == KeyCode::Delete,
        b if b.starts_with('f') && b[1..].parse::<u8>().is_ok() => {
            key.code == KeyCode::F(b[1..].parse().unwrap())
        }
        b if b.chars().count() == 1 => {
            let c = b.chars().next().unwrap();
            matches!(key.code, KeyCode::Char(x) if x.eq_ignore_ascii_case(&c))
        }
        _ => false,
    }
}

/// Ubicación del widget de clima.
///
/// `place` tiene prioridad si no está vacío (se pasa tal cual a wttr.in:
/// "Madrid", "Carrizal,VE", "Bogotá,CO"…). Si está vacío se arma a partir
/// de `country` + `city`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Weather {
    /// Lugar libre, escrito a mano.
    pub place: String,
    /// Código ISO del país (p. ej. "VE").
    pub country: String,
    /// Ciudad del catálogo para ese país.
    pub city: String,
}

impl Weather {
    /// Lugar que se le pide a wttr.in (vacío = detección automática).
    pub fn query(&self) -> String {
        if !self.place.trim().is_empty() {
            self.place.trim().to_string()
        } else if !self.city.trim().is_empty() {
            if self.country.trim().is_empty() {
                self.city.trim().to_string()
            } else {
                format!("{},{}", self.city.trim(), self.country.trim())
            }
        } else {
            String::new()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub appearance: Appearance,
    /// Widgets del Inicio: presencia, orden y forma.
    pub home: Home,
    pub projects: Projects,
    /// Atajos personalizables (ver [keys] en config.toml).
    pub keys: Keys,
    /// Ubicación del clima (campo libre o país → ciudad).
    pub weather: Weather,
    pub databases: Vec<DbConn>,
}

impl Config {
    pub fn load() -> Config {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(s) => match toml::from_str::<Config>(&s) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("config inválido ({}): {e}", path.display());
                    Config::default()
                }
            },
            Err(_) => {
                let cfg = Config::default();
                let _ = cfg.save();
                cfg
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = config_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let s = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(config_path(), s).map_err(|e| e.to_string())
    }

    pub fn theme(&self) -> Theme {
        let mut t = Theme::by_name(&self.general.theme);
        if self.appearance.pywal {
            apply_pywal(&mut t);
        }
        t
    }
}

// ── Fondos disponibles ───────────────────────────────────────────────────

pub fn wallpaper_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut push = |p: PathBuf| {
        if p.is_file() && !out.contains(&p) {
            out.push(p);
        }
    };

    if let Ok(cfg) = std::fs::read_to_string(config_path()) {
        if let Ok(c) = toml::from_str::<Config>(&cfg) {
            if !c.appearance.wallpaper.is_empty() {
                push(expand(&c.appearance.wallpaper));
            }
        }
    }
    push(home().join(".config/background"));

    for dir in [
        home().join(".local/share/backgrounds"),
        PathBuf::from("/usr/share/backgrounds"),
        home().join("Imágenes"),
        home().join("Pictures"),
    ] {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut files: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    matches!(
                        p.extension().and_then(|e| e.to_str()),
                        Some("jpg" | "jpeg" | "png" | "webp")
                    )
                })
                .collect();
            files.sort();
            for f in files {
                push(f);
            }
        }
    }
    out
}

pub fn expand(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        home().join(rest)
    } else if s == "~" {
        home()
    } else {
        PathBuf::from(s)
    }
}
