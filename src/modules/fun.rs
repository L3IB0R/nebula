//! Entretenimiento: anime, vídeo, música local e imágenes desde la TUI.
//!
//! Las búsquedas ocurren **dentro** de Nébula —AniList para anime, yt-dlp
//! para vídeo—: se escribe el nombre, se elige de la lista y la
//! reproducción arranca desde aquí, con la salida en el visor interno.
//! Nada se abre en otra terminal.

use crate::action::{Action, Job};
use crate::app::Ctx;
use crate::data::{run, which, Hit, Station};
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;
use std::sync::atomic::{AtomicU64, Ordering};

/// Comillas simples seguras para `sh -c`.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Nº de consulta en vuelo: descarta respuestas de búsquedas antiguas.
fn next_token() -> u64 {
    static TOKEN: AtomicU64 = AtomicU64::new(1);
    TOKEN.fetch_add(1, Ordering::Relaxed)
}

/// Binarios que faltan de una lista (son también nombres de paquete).
fn missing(bins: &[&str]) -> Vec<String> {
    bins.iter()
        .filter(|b| which(b).is_none())
        .map(|b| (*b).to_string())
        .collect()
}

/// Qué hace una fila al pulsar Intro.
#[derive(Clone, Copy)]
enum Do {
    /// Abrir la búsqueda de anime dentro de Nébula.
    Anime,
    /// `ani-cli -c -S 1`: seguir lo último visto.
    Continue,
    /// `pkexec ani-cli -U`: actualizar el script con sus permisos.
    Update,
    /// Abrir la búsqueda de vídeo dentro de Nébula.
    Video,
    /// Arrancar cmus en segundo plano y controlarlo desde aquí.
    Cmus,
    /// `cmus-remote <args>` (arranca cmus si aún no corre).
    Remote(&'static [&'static str]),
    /// cliamp (online): subcomandos como `toggle`, `next`, `status`.
    /// Vacío = levantar el daemon y contar su estado.
    Cliamp(&'static [&'static str]),
    /// timg es un visor de imagen *de terminal*: no puede vivir en un panel.
    Timg,
    /// Terminal embebido: 0 = cmus, 1 = cliamp (cada uno con sus atajos).
    Term(usize),
    /// Detener la instancia de cmus.
    StopCmus,
    /// Detener el daemon de cliamp (para abrir su TUI u otra cosa).
    StopCliamp,
    /// Cargar la carpeta de música en la biblioteca de cmus.
    LoadLib,
    /// Menú nativo de emisoras y géneros.
    Stations,
    /// Descargador de música de YouTube por URL.
    Download,
}

/// Una fila del menú: cabecera de grupo o acción ejecutable.
enum Row {
    Group(&'static str),
    Act {
        icon: &'static str,
        label: &'static str,
        /// Binario que tiene que existir (None = siempre disponible).
        bin: Option<&'static str>,
        /// Paquetes apt a instalar si falta.
        pkgs: &'static [&'static str],
        do_: Do,
    },
}

use Row::{Act, Group};

const MENU: &[Row] = &[
    Group("anime"),
    Act {
        icon: "✦",
        label: "buscar anime",
        bin: Some("ani-cli"),
        pkgs: &["ani-cli", "mpv", "fzf"],
        do_: Do::Anime,
    },
    Act {
        icon: "▶",
        label: "continuar lo último",
        bin: Some("ani-cli"),
        pkgs: &["ani-cli", "mpv", "fzf"],
        do_: Do::Continue,
    },
    Act {
        icon: "⟳",
        label: "actualizar ani-cli",
        bin: Some("ani-cli"),
        pkgs: &["ani-cli", "mpv", "fzf"],
        do_: Do::Update,
    },
    Group("vídeo"),
    Act {
        icon: "✦",
        label: "buscar vídeo",
        bin: Some("yt-dlp"),
        pkgs: &["yt-dlp", "mpv"],
        do_: Do::Video,
    },
    Group("♪ cliamp · online"),
    Act {
        icon: "◉",
        label: "arrancar cliamp (daemon)",
        bin: Some("cliamp"),
        pkgs: &[],
        do_: Do::Cliamp(&[]),
    },
    Act {
        icon: "◆",
        label: "estado",
        bin: Some("cliamp"),
        pkgs: &[],
        do_: Do::Cliamp(&["status"]),
    },
    Act {
        icon: "▶",
        label: "reproducir / pausa",
        bin: Some("cliamp"),
        pkgs: &[],
        do_: Do::Cliamp(&["toggle"]),
    },
    Act {
        icon: "⏭",
        label: "siguiente",
        bin: Some("cliamp"),
        pkgs: &[],
        do_: Do::Cliamp(&["next"]),
    },
    Act {
        icon: "⏮",
        label: "anterior",
        bin: Some("cliamp"),
        pkgs: &[],
        do_: Do::Cliamp(&["prev"]),
    },
    Act {
        icon: "+",
        label: "subir volumen",
        bin: Some("cliamp"),
        pkgs: &[],
        do_: Do::Cliamp(&["volume", "+2"]),
    },
    Act {
        icon: "−",
        label: "bajar volumen",
        bin: Some("cliamp"),
        pkgs: &[],
        do_: Do::Cliamp(&["volume", "-2"]),
    },
    Group("música local"),
    Act {
        icon: "♫",
        label: "abrir cmus (segundo plano)",
        bin: Some("cmus"),
        pkgs: &["cmus"],
        do_: Do::Cmus,
    },
    Act {
        icon: "▶",
        label: "reproducir",
        bin: Some("cmus-remote"),
        pkgs: &["cmus"],
        do_: Do::Remote(&["-p"]),
    },
    Act {
        icon: "⏸",
        label: "pausa",
        bin: Some("cmus-remote"),
        pkgs: &["cmus"],
        do_: Do::Remote(&["-u"]),
    },
    Act {
        icon: "⏭",
        label: "siguiente pista",
        bin: Some("cmus-remote"),
        pkgs: &["cmus"],
        do_: Do::Remote(&["-n"]),
    },
    Act {
        icon: "⏮",
        label: "pista anterior",
        bin: Some("cmus-remote"),
        pkgs: &["cmus"],
        do_: Do::Remote(&["-r"]),
    },
    Group("imágenes"),
    Act {
        icon: "▣",
        label: "ver el fondo con timg",
        bin: Some("timg"),
        pkgs: &["timg"],
        do_: Do::Timg,
    },
    Group("terminales embebidos"),
    Act {
        icon: "▸",
        label: "abrir terminal de cmus",
        bin: Some("cmus"),
        pkgs: &["cmus"],
        do_: Do::Term(0),
    },
    Act {
        icon: "▸",
        label: "abrir terminal de cliamp",
        bin: Some("cliamp"),
        pkgs: &[],
        do_: Do::Term(1),
    },
    Act {
        icon: "▸",
        label: "abrir consola (shell)",
        bin: None,
        pkgs: &[],
        do_: Do::Term(2),
    },
    Act {
        icon: "■",
        label: "detener cmus",
        bin: Some("cmus-remote"),
        pkgs: &["cmus"],
        do_: Do::StopCmus,
    },
    Act {
        icon: "■",
        label: "detener cliamp (daemon)",
        bin: Some("cliamp"),
        pkgs: &[],
        do_: Do::StopCliamp,
    },
    Act {
        icon: "♫",
        label: "cargar ~/Música en cmus",
        bin: Some("cmus-remote"),
        pkgs: &["cmus"],
        do_: Do::LoadLib,
    },
    Group("radio y descargas"),
    Act {
        icon: "◉",
        label: "emisoras y géneros",
        bin: Some("cliamp"),
        pkgs: &[],
        do_: Do::Stations,
    },
    Act {
        icon: "↓",
        label: "descargar música (URL)",
        bin: Some("yt-dlp"),
        pkgs: &["yt-dlp"],
        do_: Do::Download,
    },
];

/// Índices planos de las acciones (sin cabeceras).
fn act_rows() -> Vec<usize> {
    MENU.iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, Act { .. }))
        .map(|(i, _)| i)
        .collect()
}

// ── Búsqueda dentro de Nébula ────────────────────────────────────────────

/// Fuente de la búsqueda.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Anime,
    Video,
}

/// Fase de la búsqueda abierta.
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    /// Escribiendo la consulta.
    Query,
    /// Eligiendo entre los resultados.
    Results,
    /// Escribiendo el episodio (sólo anime).
    Episode,
}

/// Qué hacer tras una tecla de la búsqueda.
enum SAct {
    /// Gestionada; la búsqueda sigue abierta.
    Keep,
    /// Cerrar la búsqueda.
    Close,
    /// Gestionada, con una acción para el corazón.
    Run(Action),
}

/// Búsqueda en marcha, con su lista de resultados.
struct Search {
    kind: Kind,
    phase: Phase,
    query: String,
    /// Episodio a reproducir (sólo anime).
    ep: String,
    hits: Vec<Hit>,
    error: String,
    sel: usize,
    /// Versión doblada (`--dub`).
    dub: bool,
    /// Nº de consulta en vuelo (casa con `Msg::Search`).
    token: u64,
}

impl Search {
    fn new(kind: Kind) -> Search {
        Search {
            kind,
            phase: Phase::Query,
            query: String::new(),
            ep: "1".to_string(),
            hits: Vec::new(),
            error: String::new(),
            sel: 0,
            dub: false,
            token: 0,
        }
    }

    fn label(&self) -> &'static str {
        match self.kind {
            Kind::Anime => "buscar anime",
            Kind::Video => "buscar vídeo",
        }
    }

    /// Teclas de la búsqueda.
    fn on_key(&mut self, key: KeyEvent) -> SAct {
        match self.phase {
            Phase::Query => match key.code {
                KeyCode::Esc => SAct::Close,
                KeyCode::Backspace => {
                    self.query.pop();
                    SAct::Keep
                }
                KeyCode::Enter => {
                    let q = self.query.trim().to_string();
                    if q.is_empty() {
                        return SAct::Run(Action::Toast("escribe algo para buscar".into()));
                    }
                    self.token = next_token();
                    self.hits.clear();
                    self.error.clear();
                    self.sel = 0;
                    SAct::Run(Action::Search {
                        token: self.token,
                        anime: self.kind == Kind::Anime,
                        query: q,
                    })
                }
                KeyCode::Char(c) if !c.is_control() => {
                    self.query.push(c);
                    SAct::Keep
                }
                _ => SAct::Keep,
            },
            Phase::Episode => match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.phase = Phase::Results;
                    SAct::Keep
                }
                KeyCode::Backspace => {
                    self.ep.pop();
                    SAct::Keep
                }
                KeyCode::Char(c) if c.is_ascii_digit() && self.ep.len() < 3 => {
                    self.ep.push(c);
                    SAct::Keep
                }
                _ => SAct::Keep,
            },
            Phase::Results => match key.code {
                KeyCode::Esc => {
                    self.phase = Phase::Query;
                    SAct::Keep
                }
                KeyCode::Up => {
                    self.sel = self.sel.saturating_sub(1);
                    SAct::Keep
                }
                KeyCode::Down => {
                    self.sel = (self.sel + 1).min(self.hits.len().saturating_sub(1));
                    SAct::Keep
                }
                KeyCode::PageUp => {
                    self.sel = self.sel.saturating_sub(5);
                    SAct::Keep
                }
                KeyCode::PageDown => {
                    self.sel = (self.sel + 5).min(self.hits.len().saturating_sub(1));
                    SAct::Keep
                }
                KeyCode::Home => {
                    self.sel = 0;
                    SAct::Keep
                }
                KeyCode::End => {
                    self.sel = self.hits.len().saturating_sub(1);
                    SAct::Keep
                }
                KeyCode::Char('e') => {
                    self.phase = Phase::Episode;
                    SAct::Keep
                }
                KeyCode::Char('d') | KeyCode::Char('D') if self.kind == Kind::Anime => {
                    self.dub = !self.dub;
                    SAct::Keep
                }
                KeyCode::Enter => self.play(),
                _ => SAct::Keep,
            },
        }
    }

    /// Reproduce la fila elegida (ofreciendo instalar lo que haga falta).
    fn play(&self) -> SAct {
        let Some(hit) = self.hits.get(self.sel).cloned() else {
            return SAct::Run(Action::Toast("no hay ningún resultado".into()));
        };
        match self.kind {
            Kind::Anime => {
                let falta = missing(&["ani-cli", "mpv", "fzf"]);
                if !falta.is_empty() {
                    return SAct::Run(Action::Confirm(
                        format!(
                            "«{}» necesita {} paquete(s) para reproducir. ¿Instalarlos?",
                            ui::truncate(&hit.title, 30),
                            falta.len()
                        ),
                        Job::Install {
                            pkgs: falta,
                            cmds: Vec::new(),
                        },
                    ));
                }
                let ep = if self.ep.trim().is_empty() {
                    "1".to_string()
                } else {
                    self.ep.trim().to_string()
                };
                let mut cmd = String::from("ani-cli");
                if self.dub {
                    cmd.push_str(" --dub");
                }
                // `-S 1` elige el primer resultado sin menú interactivo y
                // `-e` evita la pregunta de episodio: todo sin terminal.
                cmd.push_str(&format!(" -S 1 -e {ep} {}", quote(&hit.title)));
                SAct::Run(Action::Run(Job::View {
                    title: format!("anime · {}", ui::truncate(&hit.title, 34)),
                    cwd: String::new(),
                    cmd,
                }))
            }
            Kind::Video => {
                if let Some(falta) = missing(&["mpv"]).first() {
                    return SAct::Run(Action::Confirm(
                        format!("¿Instalar {falta} para ver los vídeos? (pide privilegios)"),
                        Job::Install {
                            pkgs: vec![falta.clone()],
                            cmds: Vec::new(),
                        },
                    ));
                }
                let url = format!("https://www.youtube.com/watch?v={}", hit.id);
                SAct::Run(Action::Run(Job::Plain(vec![
                    "mpv".into(),
                    "--no-terminal".into(),
                    url,
                ])))
            }
        }
    }
}

// ── Menú nativo de emisoras y géneros ────────────────────────────────────

/// El menú de radio de cliamp, abierto dentro de Nébula (`g`).
struct Stations {
    /// Emisoras (catálogo `radio`) y géneros (`cliamp`), en ese orden.
    items: Vec<Station>,
    /// Texto del filtro (`/`).
    filter: String,
    /// ¿Se está escribiendo el filtro?
    filtering: bool,
    sel: usize,
    error: String,
    /// Nº de consulta en vuelo (casa con `Msg::Stations`).
    token: u64,
    loading: bool,
}

impl Stations {
    fn new(token: u64) -> Stations {
        Stations {
            items: Vec::new(),
            filter: String::new(),
            filtering: false,
            sel: 0,
            error: String::new(),
            token,
            loading: true,
        }
    }

    /// Filas que pasan el filtro (sin copiarlas).
    fn visible(&self) -> Vec<&Station> {
        let f = self.filter.to_lowercase();
        self.items
            .iter()
            .filter(|s| {
                f.is_empty()
                    || s.name.to_lowercase().contains(&f)
                    || s.detail.to_lowercase().contains(&f)
            })
            .collect()
    }
}

// ── Descargador de música ────────────────────────────────────────────────

/// Formatos de audio que ofrece el descargador.
const FORMATS: [&str; 3] = ["mp3", "m4a", "opus"];

/// Diálogo de descarga por URL (`d`).
struct Download {
    url: String,
    /// Índice en `FORMATS`.
    fmt: usize,
    /// ¿Cargar la carpeta en cmus al terminar?
    add: bool,
    error: String,
}

impl Download {
    fn new() -> Download {
        Download {
            url: String::new(),
            fmt: 0,
            add: true,
            error: String::new(),
        }
    }

    /// Carpeta de destino: `~/Música/Nébula`.
    fn dir() -> String {
        let home = std::env::var("HOME").unwrap_or_else(|_| "~".into());
        format!("{home}/Música/Nébula")
    }

    /// Comando completo: descarga, y si toca, carga en cmus.
    fn cmd(&self) -> String {
        let dir = Self::dir();
        let fmt = FORMATS[self.fmt.min(FORMATS.len() - 1)];
        // El «artista» de la etiqueta es el canal (uploader).
        let mut c = format!(
            "yt-dlp -f bestaudio/best -x --audio-format {fmt} --audio-quality 0 \
             --embed-metadata --parse-metadata '%(uploader|)s:%(artist)s' \
             --no-playlist --newline -o '{dir}/%(title)s.%(ext)s' -- {}",
            quote(&self.url)
        );
        if self.add {
            // Al terminar, la carpeta entera a la biblioteca de cmus.
            c.push_str(&format!(
                " && if command -v cmus-remote >/dev/null 2>&1 && cmus-remote -Q >/dev/null 2>&1; \
                 then cmus-remote -l '{dir}' && echo '✓ música cargada en cmus'; \
                 else echo 'cmus no está corriendo: la carpeta queda en {dir}'; fi"
            ));
        }
        c
    }
}

#[derive(Default)]
pub struct Fun {
    /// Selección sobre las acciones (0..act_rows().len()).
    sel: usize,
    /// Estado de cmus visto la última vez (None = sin consultar).
    cmus: Option<String>,
    cmus_at: Option<std::time::Instant>,
    /// Búsqueda abierta (anime o vídeo).
    search: Option<Search>,
    /// Menú de emisoras y géneros abierto.
    stations: Option<Stations>,
    /// Descargador abierto.
    download: Option<Download>,
}

impl Fun {
    /// Llegan los resultados de una búsqueda (o el motivo de su fallo).
    pub fn on_search(&mut self, token: u64, hits: Vec<Hit>, error: String) {
        let Some(s) = self.search.as_mut() else {
            return;
        };
        if s.token != token {
            return; // era la respuesta de otra consulta
        }
        s.hits = hits;
        s.error = error;
        s.sel = 0;
        s.phase = Phase::Results;
    }

    /// Abre (o refresca) el menú de emisoras y géneros. Devuelve el token
    /// de la consulta para que el corazón la lance.
    pub fn open_stations(&mut self) -> u64 {
        let token = next_token();
        match self.stations.as_mut() {
            Some(s) => {
                // Se conserva lo ya cargado: sólo se pide una actualización.
                s.token = token;
                s.loading = s.items.is_empty();
                s.error.clear();
                s.filtering = false;
                s.filter.clear();
                s.sel = 0;
            }
            None => self.stations = Some(Stations::new(token)),
        }
        token
    }

    /// Abre el diálogo de descarga (si aún no está).
    pub fn open_download(&mut self) {
        if self.download.is_none() {
            self.download = Some(Download::new());
        }
    }

    /// Llegan las emisoras y los géneros (o el motivo de su fallo).
    pub fn on_stations(&mut self, token: u64, items: Vec<Station>, error: String) {
        let Some(s) = self.stations.as_mut() else {
            return;
        };
        if s.token != token {
            return; // era la respuesta de otra consulta
        }
        s.loading = false;
        if error.is_empty() {
            s.items = items;
            s.sel = 0;
        }
        s.error = error;
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        ctx: &Ctx,
        music: &mut crate::modules::music::Music,
    ) {
        let t = ctx.theme;
        let rows = Layout::vertical([Constraint::Min(6), Constraint::Length(1)]).split(area);
        let cols =
            Layout::horizontal([Constraint::Percentage(58), Constraint::Min(34)]).split(rows[0]);
        // Columna derecha: arriba lo que suena, abajo las herramientas.
        let side = Layout::vertical([Constraint::Percentage(56), Constraint::Percentage(44)])
            .split(cols[1]);

        // ── Menú ──
        let block = ui::panel_on(t, ctx.wp, cols[0], " ★  Entretenimiento ", 0.84);
        let inner = block.inner(cols[0]);
        f.render_widget(block, cols[0]);

        let acts = act_rows();
        let sel = self.sel.min(acts.len().saturating_sub(1));
        let mut lines: Vec<Line> = Vec::new();
        let mut act_i = 0;
        for row in MENU {
            match row {
                Group(name) => {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(format!("── {name} ──"), Style::default().fg(t.accent2)),
                    ]));
                }
                Act {
                    icon, label, bin, ..
                } => {
                    let have = bin.map(|b| which(b).is_some()).unwrap_or(true);
                    let selected = act_i == sel;
                    act_i += 1;
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!(" {} ", if selected { "▸" } else { " " }),
                            Style::default().fg(t.accent),
                        ),
                        Span::styled(format!("{icon} "), Style::default().fg(t.fg_faint)),
                        Span::styled(
                            format!("{:<26}", label),
                            Style::default()
                                .fg(if selected { t.fg } else { t.fg_dim })
                                .add_modifier(if selected {
                                    Modifier::BOLD
                                } else {
                                    Modifier::empty()
                                }),
                        ),
                        Span::styled(
                            if have { "✓" } else { "✗" },
                            Style::default().fg(if have { t.ok } else { t.warn }),
                        ),
                    ]));
                }
            }
        }
        f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);

        // ── Lo que suena (el reproductor vive aquí desde la Fase 6) ──
        music.render(f, side[0], ctx);

        // ── Estado ──
        let block = ui::panel_on(t, ctx.wp, side[1], " ⚙  herramientas ", 0.84);
        let inner2 = block.inner(side[1]);
        f.render_widget(block, side[1]);
        let tools: [(&str, &str); 7] = [
            ("cliamp", "cliamp"),
            ("cmus", "cmus"),
            ("ani-cli", "ani-cli"),
            ("yt-dlp", "yt-dlp"),
            ("ytfzf", "ytfzf"),
            ("mpv", "mpv"),
            ("timg", "timg"),
        ];
        let mut st: Vec<Line> = Vec::new();
        for (label, bin) in tools {
            let have = which(bin).is_some();
            st.push(Line::from(vec![
                Span::styled(
                    format!(" {} ", if have { "✓" } else { "✗" }),
                    Style::default().fg(if have { t.ok } else { t.fg_faint }),
                ),
                Span::styled(
                    format!("{:<12}", label),
                    Style::default().fg(if have { t.fg_dim } else { t.fg_faint }),
                ),
                Span::styled(
                    if have { "listo" } else { "no instalado" },
                    Style::default().fg(t.fg_faint),
                ),
            ]));
        }
        // Estado de cliamp y de cmus (consultas con caché corta).
        st.push(Line::from(""));
        st.push(Line::from(Span::styled(
            " cliamp ",
            Style::default().fg(t.accent2),
        )));
        let cstate = if which("cliamp").is_none() {
            "sin instalar".to_string()
        } else if ctx.music.engine == "cliamp" && ctx.music.running {
            let st_name = match ctx.music.state.as_str() {
                "playing" => "reproduciendo",
                "paused" => "en pausa",
                _ => "detenido",
            };
            format!("{st_name} · {}", ui::truncate(&ctx.music.title, 20))
        } else if crate::data::cliamp_running() {
            "daemon activo · sin reproducción".to_string()
        } else {
            "parado · «arrancar cliamp» lo levanta".to_string()
        };
        st.push(Line::from(Span::styled(
            cstate,
            Style::default().fg(t.fg_dim),
        )));

        st.push(Line::from(""));
        st.push(Line::from(Span::styled(
            " cmus ",
            Style::default().fg(t.accent2),
        )));
        let now = std::time::Instant::now();
        let stale = self
            .cmus_at
            .map(|at| at.elapsed() > std::time::Duration::from_secs(3))
            .unwrap_or(true);
        if stale {
            self.cmus = if which("cmus-remote").is_some() {
                run("cmus-remote", &["-Q"])
                    .map(|out| {
                        let status = out
                            .lines()
                            .find(|l| l.starts_with("status "))
                            .map(|l| l.trim_start_matches("status ").to_string())
                            .unwrap_or_else(|| "sin reproducción".into());
                        let title = out
                            .lines()
                            .find(|l| l.starts_with("tag title ") || l.starts_with("file "))
                            .map(|l| {
                                l.trim_start_matches("tag title ")
                                    .trim_start_matches("file ")
                                    .to_string()
                            });
                        match title {
                            Some(t2) => format!("{status} · {}", ui::truncate(&t2, 22)),
                            None => status,
                        }
                    })
                    .or_else(|| Some("cmus no está corriendo".into()))
            } else {
                Some("cmus sin instalar".into())
            };
            self.cmus_at = Some(now);
        }
        st.push(Line::from(Span::styled(
            self.cmus.clone().unwrap_or_default(),
            Style::default().fg(t.fg_dim),
        )));
        f.render_widget(ratatui::widgets::Paragraph::new(st), inner2);

        // ── Pie ──
        ui::hint_bar(
            f,
            t,
            rows[1],
            &[
                ("↑/↓", "mover"),
                ("intro", "ejecutar"),
                ("espacio", "play/pausa"),
                ("t", "terminal"),
                ("g", "emisoras"),
                ("d", "descargar"),
            ],
        );

        // ── Búsqueda abierta (encima de todo) ──
        if let Some(s) = &self.search {
            self.render_search(f, area, s, ctx);
        }
        // ── Menú de emisoras y descargador ──
        if let Some(s) = &self.stations {
            self.render_stations(f, area, s, ctx);
        }
        if let Some(d) = &self.download {
            self.render_download(f, area, d, ctx);
        }
    }

    /// Ventana del menú nativo de emisoras y géneros.
    fn render_stations(&self, f: &mut Frame, area: Rect, s: &Stations, ctx: &Ctx) {
        let t = ctx.theme;
        let w = (area.width * 78 / 100).clamp(58, 120);
        let h = (area.height * 78 / 100).clamp(12, 36);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(w)) / 2,
            y: area.y + (area.height.saturating_sub(h)) / 2,
            width: w.min(area.width),
            height: h.min(area.height),
        };
        {
            let buf = f.buffer_mut();
            ui::dim_area(buf, area, 0.55);
        }
        let title = if s.loading {
            " ◉  emisoras y géneros · cargando… ".to_string()
        } else {
            format!(" ◉  emisoras y géneros · {} ", s.visible().len())
        };
        let block = ui::panel_on(t, ctx.wp, rect, title, 0.97);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        if inner.height < 4 {
            return;
        }

        let vis = s.visible();
        let mut v: Vec<Line> = Vec::new();
        // Línea de filtro o de instrucción.
        if s.filtering {
            v.push(Line::from(vec![
                Span::styled("  filtrar ▸ ", Style::default().fg(t.accent)),
                Span::styled(
                    format!("{}▏", s.filter),
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                ),
            ]));
        } else if !s.error.is_empty() {
            v.push(Line::from(Span::styled(
                format!("  {}", s.error),
                Style::default().fg(t.warn),
            )));
        } else {
            v.push(Line::from(Span::styled(
                "  emisoras en directo y géneros de cliamp radio",
                Style::default().fg(t.fg_faint),
            )));
        }
        v.push(Line::from(""));

        if vis.is_empty() {
            if s.loading {
                let dots = ".".repeat((ctx.elapsed as usize % 3) + 1);
                v.push(Line::from(Span::styled(
                    format!("  pidiendo las listas{dots}"),
                    Style::default().fg(t.fg_dim),
                )));
            } else {
                v.push(Line::from(Span::styled(
                    "  nada por aquí · pulsa r para volver a mirar",
                    Style::default().fg(t.fg_dim),
                )));
            }
        } else {
            // Ventana de filas centrada en la selección.
            let room = (inner.height as usize).saturating_sub(5).max(3);
            let start = s
                .sel
                .saturating_sub(room / 2)
                .min(vis.len().saturating_sub(room));
            let mut last_provider: Option<&str> = None;
            for (i, st) in vis.iter().enumerate().skip(start).take(room) {
                if last_provider != Some(st.provider.as_str()) {
                    let head = if st.provider == "radio" {
                        "emisoras"
                    } else {
                        "géneros"
                    };
                    v.push(Line::from(Span::styled(
                        format!("── {head} ──"),
                        Style::default().fg(t.accent2),
                    )));
                    last_provider = Some(st.provider.as_str());
                }
                let on = i == s.sel;
                v.push(Line::from(vec![
                    Span::styled(
                        format!(" {} ", if on { "▸" } else { " " }),
                        Style::default().fg(t.accent),
                    ),
                    Span::styled("◉ ", Style::default().fg(t.fg_faint)),
                    Span::styled(
                        format!("{:<34}", ui::truncate(&st.name, 34)),
                        Style::default()
                            .fg(if on { t.fg } else { t.fg_dim })
                            .add_modifier(if on {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(
                        ui::truncate(&st.detail, 22),
                        Style::default().fg(t.fg_faint),
                    ),
                ]));
            }
        }
        f.render_widget(ratatui::widgets::Paragraph::new(v), inner);

        // Pie: la última fila del panel.
        let last = inner.bottom().saturating_sub(1);
        let hint = if s.filtering {
            "  escribe para filtrar · tab terminar · esc cancelar"
        } else {
            "  intro reproducir · / filtrar · r actualizar · esc cerrar"
        };
        f.render_widget(
            ratatui::widgets::Paragraph::new(vec![Line::from(Span::styled(
                hint,
                Style::default().fg(t.fg_faint),
            ))]),
            Rect {
                x: inner.x,
                y: last,
                width: inner.width,
                height: 1,
            },
        );
    }

    /// Ventana del descargador de música por URL.
    fn render_download(&self, f: &mut Frame, area: Rect, d: &Download, ctx: &Ctx) {
        let t = ctx.theme;
        let w = (area.width * 74 / 100).clamp(56, 104);
        let h = 15.min(area.height);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(w)) / 2,
            y: area.y + (area.height.saturating_sub(h)) / 2,
            width: w.min(area.width),
            height: h,
        };
        {
            let buf = f.buffer_mut();
            ui::dim_area(buf, area, 0.55);
        }
        let block = ui::panel_on(t, ctx.wp, rect, " ↓  descargar música ", 0.97);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        if inner.height < 6 {
            return;
        }
        let fmt = FORMATS[d.fmt.min(FORMATS.len() - 1)];
        let mut v: Vec<Line> = vec![
            Line::from(Span::styled(
                "  pega la URL de la canción (YouTube)",
                Style::default().fg(t.fg_faint),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  ▸ ", Style::default().fg(t.accent)),
                Span::styled(
                    format!("{}▏", d.url),
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  formato   ", Style::default().fg(t.fg_dim)),
                Span::styled(
                    format!("◀ {fmt} ▶"),
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled("   ←/→ para cambiar", Style::default().fg(t.fg_faint)),
            ]),
            Line::from(vec![
                Span::styled("  en cmus   ", Style::default().fg(t.fg_dim)),
                Span::styled(
                    if d.add {
                        "[✓] cargar al terminar"
                    } else {
                        "[ ] cargar al terminar"
                    },
                    Style::default().fg(if d.add { t.ok } else { t.fg_dim }),
                ),
                Span::styled("   tab", Style::default().fg(t.fg_faint)),
            ]),
        ];
        if !d.error.is_empty() {
            v.push(Line::from(""));
            v.push(Line::from(Span::styled(
                format!("  {}", d.error),
                Style::default().fg(t.warn),
            )));
        }
        v.push(Line::from(Span::styled(
            format!("  destino · {}", ui::truncate(&Download::dir(), 44)),
            Style::default().fg(t.fg_faint),
        )));
        f.render_widget(ratatui::widgets::Paragraph::new(v), inner);

        let last = inner.bottom().saturating_sub(1);
        f.render_widget(
            ratatui::widgets::Paragraph::new(vec![Line::from(Span::styled(
                "  intro descargar · esc cancelar",
                Style::default().fg(t.fg_faint),
            ))]),
            Rect {
                x: inner.x,
                y: last,
                width: inner.width,
                height: 1,
            },
        );
    }

    /// Ventana centrada con la consulta o con los resultados.
    fn render_search(&self, f: &mut Frame, area: Rect, s: &Search, ctx: &Ctx) {
        let t = ctx.theme;
        let (title, mut lines, hint): (String, Vec<Line>, String) = match s.phase {
            Phase::Query => (
                format!(" ✦  {} ", s.label()),
                vec![
                    Line::from(Span::styled(
                        "  escribe el nombre y pulsa intro",
                        Style::default().fg(t.fg_faint),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("  ▸ ", Style::default().fg(t.accent)),
                        Span::styled(
                            format!("{}▏", s.query),
                            Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                        ),
                    ]),
                ],
                "  intro buscar · esc cancelar".to_string(),
            ),
            Phase::Episode => (
                " ✦  episodio ".to_string(),
                vec![
                    Line::from(Span::styled(
                        "  nº de episodio a ver",
                        Style::default().fg(t.fg_faint),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("  ▸ ", Style::default().fg(t.accent)),
                        Span::styled(
                            format!("{}▏", s.ep),
                            Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                        ),
                    ]),
                ],
                "  intro guardar · esc volver".to_string(),
            ),
            Phase::Results => {
                let mut v: Vec<Line> = Vec::new();
                v.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(ui::truncate(&s.query, 38), Style::default().fg(t.accent2)),
                    if s.kind == Kind::Anime {
                        Span::styled(
                            if s.dub {
                                "  · doblado"
                            } else {
                                "  · subtitulado"
                            },
                            Style::default().fg(t.fg_faint),
                        )
                    } else {
                        Span::raw("")
                    },
                ]));
                if !s.error.is_empty() {
                    v.push(Line::from(""));
                    v.push(Line::from(Span::styled(
                        format!("  {}", s.error),
                        Style::default().fg(t.warn),
                    )));
                    v.push(Line::from(Span::styled(
                        "  esc para probar con otra cosa",
                        Style::default().fg(t.fg_faint),
                    )));
                } else if s.hits.is_empty() {
                    let dots = ".".repeat((ctx.elapsed as usize % 3) + 1);
                    v.push(Line::from(""));
                    v.push(Line::from(Span::styled(
                        format!("  buscando{dots}"),
                        Style::default().fg(t.fg_dim),
                    )));
                } else {
                    v.push(Line::from(Span::styled(
                        format!("── {} resultados ──", s.hits.len()),
                        Style::default().fg(t.accent2),
                    )));
                    // Ventana de filas centrada en la selección.
                    let max = (area.height as usize).saturating_sub(7).max(3);
                    let start = s
                        .sel
                        .saturating_sub(max / 2)
                        .min(s.hits.len().saturating_sub(max));
                    for (i, hit) in s.hits.iter().enumerate().skip(start).take(max) {
                        let on = i == s.sel;
                        v.push(Line::from(vec![
                            Span::styled(
                                format!(" {} ", if on { "▸" } else { " " }),
                                Style::default().fg(t.accent),
                            ),
                            Span::styled(
                                format!("{:<30}", ui::truncate(&hit.title, 30)),
                                Style::default()
                                    .fg(if on { t.fg } else { t.fg_dim })
                                    .add_modifier(if on {
                                        Modifier::BOLD
                                    } else {
                                        Modifier::empty()
                                    }),
                            ),
                            Span::styled(
                                ui::truncate(&hit.sub, 32),
                                Style::default().fg(t.fg_faint),
                            ),
                        ]));
                    }
                    v.push(Line::from(""));
                    v.push(Line::from(Span::styled(
                        format!("  {} de {}", s.sel + 1, s.hits.len()),
                        Style::default().fg(t.fg_faint),
                    )));
                }
                let h = if s.kind == Kind::Anime {
                    "  intro reproducir · e episodio · D doblado · esc volver"
                } else {
                    "  intro reproducir · esc volver"
                };
                (format!(" ✦  {} ", s.label()), v, h.to_string())
            }
        };

        let h = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
        let w = (area.width * 68 / 100).clamp(48, 88).min(area.width);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(w)) / 2,
            y: area.y + (area.height.saturating_sub(h)) / 2,
            width: w,
            height: h,
        };
        let buf = f.buffer_mut();
        ui::dim_area(buf, area, 0.55);
        let block = ui::panel_on(t, ctx.wp, rect, title, 0.97);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
        // El pie ocupa la última línea del panel.
        let last = inner.bottom().saturating_sub(1);
        lines = vec![Line::from(Span::styled(
            hint,
            Style::default().fg(t.fg_faint),
        ))];
        f.render_widget(
            ratatui::widgets::Paragraph::new(lines),
            Rect {
                x: inner.x,
                y: last,
                width: inner.width,
                height: 1,
            },
        );
    }

    pub fn on_key(
        &mut self,
        key: KeyEvent,
        music: &mut crate::modules::music::Music,
        ctx: &Ctx,
    ) -> Option<Action> {
        // ── Búsqueda abierta ──
        if self.search.is_some() {
            return self.search_key(key);
        }
        // ── Menú de emisoras o descargador abiertos ──
        if self.stations.is_some() {
            return self.stations_key(key);
        }
        if self.download.is_some() {
            return self.download_key(key);
        }
        // Con el menú del reproductor abierto manda él (↑/↓, intro, esc).
        if music.menu.open {
            return music.on_key(key, ctx);
        }

        // ── Atajos de la Fase 7: van antes que el reproductor, porque
        //    `t` era su tecla de «detener» y ahora abre su terminal. ──
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('t') if !ctrl => {
                return Some(Action::Term(usize::from(ctx.music.engine == "cliamp")));
            }
            KeyCode::Char('g') if !ctrl => return Some(Action::Stations),
            KeyCode::Char('d') if !ctrl => {
                self.open_download();
                return Some(Action::Keep);
            }
            _ => {}
        }

        let acts = act_rows();
        let max = acts.len().saturating_sub(1);
        match key.code {
            KeyCode::Up => {
                self.sel = self.sel.saturating_sub(1);
                Some(Action::Keep)
            }
            KeyCode::Down => {
                self.sel = (self.sel + 1).min(max);
                Some(Action::Keep)
            }
            KeyCode::PageUp => {
                self.sel = self.sel.saturating_sub(5);
                Some(Action::Keep)
            }
            KeyCode::PageDown => {
                self.sel = (self.sel + 5).min(max);
                Some(Action::Keep)
            }
            KeyCode::Enter => self.run_sel(),
            // Instalar las herramientas que falten de todo el menú.
            KeyCode::Char('i') => {
                let mut pkgs: Vec<String> = Vec::new();
                for row in MENU {
                    if let Act { bin, pkgs: p, .. } = row {
                        let have = bin.map(|b| which(b).is_some()).unwrap_or(true);
                        if !have {
                            for x in p.iter() {
                                if !pkgs.iter().any(|y| y == *x) {
                                    pkgs.push((*x).to_string());
                                }
                            }
                        }
                    }
                }
                if pkgs.is_empty() {
                    Some(Action::Toast("✓ entretenimiento listo".into()))
                } else {
                    Some(Action::Confirm(
                        format!(
                            "¿Instalar {} paquete(s) de Entretenimiento? (pide privilegios)",
                            pkgs.len()
                        ),
                        Job::Install {
                            pkgs,
                            cmds: Vec::new(),
                        },
                    ))
                }
            }
            // Todo lo demás se lo pasamos al reproductor embebido:
            // espacio, n/p, volumen, shuffle, repeat, m…
            _ => music.on_key(key, ctx),
        }
    }

    /// Teclas de la búsqueda abierta.
    fn search_key(&mut self, key: KeyEvent) -> Option<Action> {
        // Se saca el estado para no pelear con los préstamos.
        let mut s = self.search.take()?;
        match s.on_key(key) {
            SAct::Close => Some(Action::Keep),
            SAct::Keep => {
                self.search = Some(s);
                Some(Action::Keep)
            }
            SAct::Run(a) => {
                self.search = Some(s);
                Some(a)
            }
        }
    }

    /// Teclas del menú de emisoras y géneros.
    fn stations_key(&mut self, key: KeyEvent) -> Option<Action> {
        let mut s = self.stations.take()?;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let n = s.visible().len();
        let mut close = false;
        let mut act: Option<Action> = None;
        match key.code {
            KeyCode::Esc => {
                if s.filtering {
                    s.filtering = false;
                    s.filter.clear();
                    s.sel = 0;
                } else {
                    close = true;
                }
            }
            KeyCode::Char('/') if !s.filtering => {
                s.filtering = true;
                s.filter.clear();
                s.sel = 0;
            }
            KeyCode::Char('r') | KeyCode::Char('R') if !s.filtering && !ctrl => {
                act = Some(Action::Stations)
            }
            KeyCode::Char(c) if s.filtering && !c.is_control() => {
                s.filter.push(c);
                s.sel = 0;
            }
            KeyCode::Backspace if s.filtering => {
                s.filter.pop();
                s.sel = 0;
            }
            KeyCode::Tab if s.filtering => s.filtering = false,
            KeyCode::Up => s.sel = s.sel.saturating_sub(1),
            KeyCode::Down => s.sel = (s.sel + 1).min(n.saturating_sub(1)),
            KeyCode::PageUp => s.sel = s.sel.saturating_sub(5),
            KeyCode::PageDown => s.sel = (s.sel + 5).min(n.saturating_sub(1)),
            KeyCode::Home => s.sel = 0,
            KeyCode::End => s.sel = n.saturating_sub(1),
            KeyCode::Enter => {
                s.sel = s.sel.min(n.saturating_sub(1));
                match s.visible().get(s.sel) {
                    Some(st) => {
                        act = Some(Action::Station {
                            provider: st.provider.clone(),
                            id: st.id.clone(),
                            name: st.name.clone(),
                        });
                        close = true;
                    }
                    None => act = Some(Action::Toast("elige una emisora o un género".into())),
                }
            }
            _ => {}
        }
        // El menú se cierra sólo si no queda nada que contarle al corazón.
        if !close {
            self.stations = Some(s);
        }
        act.or(Some(Action::Keep))
    }

    /// Teclas del descargador de música.
    fn download_key(&mut self, key: KeyEvent) -> Option<Action> {
        let mut d = self.download.take()?;
        let mut close = false;
        let mut act: Option<Action> = None;
        match key.code {
            KeyCode::Esc => close = true,
            KeyCode::Backspace => {
                d.url.pop();
                d.error.clear();
            }
            KeyCode::Left => d.fmt = (d.fmt + FORMATS.len() - 1) % FORMATS.len(),
            KeyCode::Right => d.fmt = (d.fmt + 1) % FORMATS.len(),
            KeyCode::Tab => d.add = !d.add,
            KeyCode::Enter => {
                let url = d.url.trim().to_string();
                if url.is_empty() {
                    d.error = "pega la URL de la canción".into();
                } else if !url.starts_with("http") {
                    d.error = "la URL tiene que empezar por http".into();
                } else if which("yt-dlp").is_none() {
                    // Sin yt-dlp se ofrece instalarlo, con el diálogo a mano.
                    act = Some(Action::Confirm(
                        "«descargar música» necesita yt-dlp. ¿Instalarlo? (pide privilegios)"
                            .into(),
                        Job::Install {
                            pkgs: vec!["yt-dlp".into()],
                            cmds: Vec::new(),
                        },
                    ));
                } else {
                    let dir = Download::dir();
                    if std::fs::create_dir_all(&dir).is_err() {
                        d.error = format!("no se pudo crear {dir}");
                    } else {
                        act = Some(Action::Run(Job::View {
                            title: format!("descargar · {}", ui::truncate(&url, 38)),
                            cwd: dir,
                            cmd: d.cmd(),
                        }));
                        close = true;
                    }
                }
            }
            KeyCode::Char(c) if !c.is_control() => {
                d.url.push(c);
                d.error.clear();
            }
            _ => {}
        }
        if !close {
            self.download = Some(d);
        }
        act.or(Some(Action::Keep))
    }

    /// Ejecuta la acción seleccionada (o ofrece instalar su herramienta).
    fn run_sel(&mut self) -> Option<Action> {
        let acts = act_rows();
        let &idx = acts.get(self.sel)?;
        let Row::Act {
            bin,
            pkgs,
            do_,
            label,
            ..
        } = &MENU[idx]
        else {
            return None;
        };

        // Si falta la herramienta, ofrecemos instalarla. Las búsquedas no
        // la necesitan: sólo hacen falta al reproducir (allí se ofrecen).
        if !matches!(do_, Do::Anime | Do::Video) {
            let falta: Vec<String> = pkgs
                .iter()
                .filter(|p| which(p).is_none())
                .map(|p| (*p).to_string())
                .collect();
            if !falta.is_empty() {
                return Some(Action::Confirm(
                    format!(
                        "«{label}» necesita {} paquete(s). ¿Instalarlos? (pide privilegios)",
                        falta.len()
                    ),
                    Job::Install {
                        pkgs: falta,
                        cmds: Vec::new(),
                    },
                ));
            }
        }
        let _ = bin;

        match do_ {
            // Las búsquedas se abren aquí dentro, sin terminal.
            Do::Anime => {
                self.search = Some(Search::new(Kind::Anime));
                Some(Action::Keep)
            }
            Do::Video => {
                self.search = Some(Search::new(Kind::Video));
                Some(Action::Keep)
            }
            Do::Continue => Some(Action::Run(Job::View {
                title: "ani-cli · continuar".into(),
                cwd: String::new(),
                cmd: "ani-cli -c -S 1".into(),
            })),
            Do::Update => Some(Action::Run(Job::View {
                title: "ani-cli · actualizar".into(),
                cwd: String::new(),
                cmd: "pkexec ani-cli -U".into(),
            })),
            Do::Cmus => Some(Action::Run(Job::Cmus(Vec::new()))),
            Do::Remote(args) => Some(Action::Run(Job::Cmus(
                args.iter().map(|s| (*s).to_string()).collect(),
            ))),
            Do::Cliamp(args) => Some(Action::Run(Job::Cliamp(
                args.iter().map(|s| (*s).to_string()).collect(),
            ))),
            // ── Fase 7: terminales, detener, emisoras y descargas ──
            Do::Term(n) => Some(Action::Term(*n)),
            Do::StopCmus => Some(Action::StopCmus),
            Do::StopCliamp => Some(Action::StopCliamp),
            Do::LoadLib => Some(Action::LoadLib),
            Do::Stations => Some(Action::Stations),
            Do::Download => Some(Action::Download),
            Do::Timg => {
                let cfg = crate::config::Config::load();
                let path = if cfg.appearance.wallpaper.is_empty() {
                    crate::config::wallpaper_candidates().into_iter().next()
                } else {
                    Some(crate::config::expand(&cfg.appearance.wallpaper))
                };
                match path {
                    // timg dibuja con arte ANSI: eso es cosa de un terminal.
                    Some(p) if p.is_file() => {
                        let s = p.to_string_lossy().to_string();
                        crate::data::spawn_in_terminal("timg", &[&s], None);
                        Some(Action::Toast(
                            "timg · se abre en tu terminal (es un visor ANSI)".into(),
                        ))
                    }
                    _ => Some(Action::Toast(
                        "no hay ninguna imagen de fondo elegida".into(),
                    )),
                }
            }
        }
    }
}
