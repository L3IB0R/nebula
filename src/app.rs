//! El corazón de Nébula: estado, bucle de eventos y dibujo.

use crate::action::{Action, Job};
use crate::anim::{ease_in_out, ease_out_cubic, Clock, Tween};
use crate::config::{Config, Theme};
use crate::data::*;
use crate::modules::dev::Dev;
use crate::modules::docker::Docker;
use crate::modules::fun::Fun;
use crate::modules::git::{GitMod, GitState};
use crate::modules::help::Help;
use crate::modules::home::Home;
use crate::modules::launcher::Launcher;
use crate::modules::music::Music;
use crate::modules::net::Net;
use crate::modules::notes::Notes;
use crate::modules::packages::Packages;
use crate::modules::projects::Projects;
use crate::modules::services::Services;
use crate::modules::settings::Settings;
use crate::modules::system::SystemMod;
use crate::ui;
use crate::wallpaper::{blend, Wallpaper};
use chrono::{Local, Timelike};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Widget};
use ratatui::Frame;
use std::sync::mpsc::{channel, Receiver, Sender};

pub mod tab {
    pub const HOME: usize = 0;
    pub const SYSTEM: usize = 1;
    pub const NET: usize = 2;
    pub const PACKAGES: usize = 3;
    pub const SERVICES: usize = 4;
    pub const DOCKER: usize = 5;
    pub const PROJECTS: usize = 6;
    pub const GIT: usize = 7;
    pub const DEV: usize = 8;
    pub const FUN: usize = 9;
    pub const NOTES: usize = 10;
    pub const SETTINGS: usize = 11;
    pub const COUNT: usize = 12;
}

pub const TABS: [(&str, &str); tab::COUNT] = [
    ("Inicio", "⌂"),
    ("Sistema", "◉"),
    ("Red", "⇄"),
    ("Paquetes", "⬡"),
    ("Servicios", "⚙"),
    ("Contenedores", "▣"),
    ("Proyectos", "❖"),
    ("Git", "⎇"),
    ("Desarrollo", "◈"),
    ("Entretenimiento", "★"),
    ("Notas", "✎"),
    ("Ajustes", "☾"),
];

/// Filas de la barra lateral: cabeceras de grupo y pestañas, en el mismo
/// orden que `TABS` (así los dígitos 1…0 siguen coincidiendo con lo que se
/// ve). Las cabeceras no son seleccionables: la navegación salta de una
/// pestaña a la siguiente y el resalte cae en su fila real.
pub enum SideRow {
    Head(&'static str),
    Tab(usize),
}

pub const SIDE: &[SideRow] = &[
    SideRow::Head("general"),
    SideRow::Tab(tab::HOME),
    SideRow::Head("sistema"),
    SideRow::Tab(tab::SYSTEM),
    SideRow::Tab(tab::NET),
    SideRow::Tab(tab::PACKAGES),
    SideRow::Tab(tab::SERVICES),
    SideRow::Tab(tab::DOCKER),
    SideRow::Head("código"),
    SideRow::Tab(tab::PROJECTS),
    SideRow::Tab(tab::GIT),
    SideRow::Tab(tab::DEV),
    SideRow::Head("ocio"),
    SideRow::Tab(tab::FUN),
    SideRow::Tab(tab::NOTES),
    SideRow::Head("configuración"),
    SideRow::Tab(tab::SETTINGS),
];

/// Fila visual (0-based) que ocupa la pestaña `n` en la lateral.
fn side_row(n: usize) -> u16 {
    let mut row = 0u16;
    for r in SIDE {
        match r {
            SideRow::Tab(i) if *i == n => return row,
            _ => row += 1,
        }
    }
    0
}

/// Pestaña que vive en la fila visual `row` (las cabeceras devuelven None).
fn tab_at_row(row: u16) -> Option<usize> {
    match SIDE.get(row as usize)? {
        SideRow::Tab(i) => Some(*i),
        SideRow::Head(_) => None,
    }
}

/// Siguiente fila con pestaña, saltando las cabeceras de grupo.
fn side_step(row: usize, dir: i32) -> usize {
    let rows = SIDE.len() as i32;
    let mut r = row as i32 + dir;
    while r >= 0 && r < rows {
        if matches!(SIDE[r as usize], SideRow::Tab(_)) {
            return r as usize;
        }
        r += dir;
    }
    row
}

fn first_tab_row() -> usize {
    SIDE.iter()
        .position(|r| matches!(r, SideRow::Tab(_)))
        .unwrap_or(0)
}

fn last_tab_row() -> usize {
    SIDE.iter()
        .rposition(|r| matches!(r, SideRow::Tab(_)))
        .unwrap_or(0)
}

/// Lo que todos los módulos necesitan leer.
pub struct Ctx<'a> {
    pub theme: &'a Theme,
    pub wp: &'a Wallpaper,
    pub cfg: &'a Config,
    pub sys: &'a SysSnap,
    pub music: &'a MusicSnap,
    pub vis: &'a Vec<f32>,
    pub projects: &'a Vec<ProjectInfo>,
    pub git: &'a GitState,
    pub net: &'a NetSnap,
    pub docker: &'a DockerSnap,
    pub pkgs: &'a PkgSnap,
    pub services: &'a Vec<ServiceInfo>,
    pub weather: &'a WeatherSnap,
    pub now: &'a chrono::DateTime<Local>,
    pub elapsed: f32,
}

macro_rules! ctx {
    ($s:expr) => {
        Ctx {
            theme: &$s.theme,
            wp: &$s.wp,
            cfg: &$s.cfg,
            sys: &$s.sys,
            music: &$s.music,
            vis: &$s.vis,
            projects: &$s.projects,
            git: &$s.git,
            net: &$s.net,
            docker: &$s.docker,
            pkgs: &$s.pkgs,
            services: &$s.services,
            weather: &$s.weather,
            now: &$s.now,
            elapsed: $s.clock.elapsed,
        }
    };
}

#[derive(Clone, Copy, PartialEq)]
enum Overlay {
    None,
    Launcher,
    Help,
    Confirm,
    /// Salida de un comando, dentro de Nébula.
    Output,
    /// Terminal embebido: cmus o cliamp, cada uno en su propia pty.
    Term,
}

pub struct App {
    pub cfg: Config,
    pub theme: Theme,
    theme_a: Theme,
    theme_b: Theme,
    theme_t: f32,
    pub wp: Wallpaper,
    pub size: (u16, u16),

    pub sys: SysSnap,
    pub music: MusicSnap,
    pub vis: Vec<f32>,
    pub projects: Vec<ProjectInfo>,
    /// Audio, brillo y batería.
    pub av: AvSnap,
    /// Clima (wttr.in).
    pub weather: WeatherSnap,
    /// Lugar del clima compartido con su hilo.
    place: Place,
    /// Modo Zen (F11): sin cromo, sólo contenido.
    pub zen: bool,
    pub git: GitState,
    pub net: NetSnap,
    pub docker: DockerSnap,
    pub pkgs: PkgSnap,
    pub services: Vec<ServiceInfo>,

    pub now: chrono::DateTime<Local>,
    pub tab: usize,
    side_y: Tween,
    /// ¿Barra lateral abierta? (Ctrl+B)
    pub sidebar_open: bool,
    /// Foco del menú lateral (Ctrl+E): ↑/↓ eligen, Intro abre.
    pub side_focus: bool,
    /// Índice que marcan las flechas con el foco lateral.
    pub side_sel: usize,
    /// Ancho animado de la barra lateral.
    side_w: Tween,
    slide: f32,
    boot: f32,
    /// Pantalla de arranque con el análisis real (Esc la salta).
    splash: bool,
    /// Líneas de progreso que ya ha mandado el análisis.
    boot_lines: Vec<String>,
    /// El análisis terminó: la cortina se retira sola en un momento.
    boot_end: f32,
    overlay: Overlay,
    confirm: Option<(String, Job)>,
    /// Visor interno de salida (git, apt, reproductores…).
    output: Option<crate::output::Output>,
    /// Turno del visor: el nº de comando más reciente.
    out_id: u64,
    /// Terminales embebidos: 0 = cmus, 1 = cliamp, 2 = consola ($SHELL -l).
    term: [Option<crate::term::Pty>; 3],
    /// Qué terminal está en pantalla cuando el panel está abierto.
    term_focus: usize,
    /// Comando que se ejecuta en la TTY al salir de Nébula (modo terminal).
    pub pending: Option<Vec<String>>,
    toast: Option<(String, f32)>,
    loaded: [bool; tab::COUNT],
    pending_git: bool,
    pub running: bool,
    clock: Clock,

    tx: Sender<Msg>,
    rx: Receiver<Msg>,

    pub m_home: Home,
    pub m_sys: SystemMod,
    pub m_projects: Projects,
    pub m_git: GitMod,
    pub m_dev: Dev,
    pub m_music: Music,
    pub m_net: Net,
    pub m_docker: Docker,
    pub m_packages: Packages,
    pub m_services: Services,
    pub m_notes: Notes,
    pub m_fun: Fun,
    pub m_settings: Settings,
    pub m_launcher: Launcher,
    pub m_help: Help,
    sidebar_state: ListState,
}

impl App {
    pub fn new(cfg: Config) -> App {
        let (tx, rx) = channel();
        // En la consola de 16 colores el tema viaja ya reducido.
        let theme = cfg.theme();
        let theme = if crate::config::tty_palette() {
            theme.for_tty()
        } else {
            theme
        };
        let size = crossterm::terminal::size().unwrap_or((120, 40));

        spawn_sampler(tx.clone());
        spawn_music(tx.clone());
        spawn_av(tx.clone());
        let place = place_cell(&cfg.weather.query());
        spawn_weather(tx.clone(), place.clone());
        spawn_projects(tx.clone(), cfg.projects.dirs.clone(), cfg.projects.max);
        spawn_boot(tx.clone());

        // Preferencia de arranque de la barra lateral.
        let sidebar_open = cfg.general.sidebar;
        let side_w0 = if sidebar_open { 22.0 } else { 4.0 };

        let mut app = App {
            theme_a: theme.clone(),
            theme_b: theme.clone(),
            theme_t: 1.0,
            theme,
            wp: Wallpaper::empty("solid", crate::config::parse_hex("#000000").unwrap()),
            size,
            cfg,
            sys: SysSnap::default(),
            music: MusicSnap::default(),
            vis: Vec::new(),
            projects: Vec::new(),
            av: AvSnap::default(),
            weather: WeatherSnap::default(),
            place,
            zen: false,
            git: GitState::default(),
            net: NetSnap::default(),
            docker: DockerSnap::default(),
            pkgs: PkgSnap::default(),
            services: Vec::new(),
            now: Local::now(),
            tab: tab::HOME,
            side_y: Tween::new(side_row(tab::HOME) as f32, 16.0),
            sidebar_open,
            side_focus: false,
            side_sel: side_row(tab::HOME) as usize,
            side_w: Tween::new(side_w0, 46.0),
            slide: 1.0,
            boot: 0.0,
            splash: true,
            boot_lines: Vec::new(),
            boot_end: 0.0,
            overlay: Overlay::None,
            confirm: None,
            output: None,
            out_id: 0,
            term: [None, None, None],
            term_focus: 0,
            pending: None,
            toast: None,
            loaded: [false; tab::COUNT],
            pending_git: false,
            running: true,
            clock: Clock::new(),
            tx,
            rx,
            m_home: Home,
            m_sys: SystemMod::default(),
            m_projects: Projects::default(),
            m_git: GitMod::default(),
            m_dev: Dev::default(),
            m_music: Music::default(),
            m_net: Net::default(),
            m_docker: Docker::default(),
            m_packages: Packages::default(),
            m_services: Services::default(),
            m_notes: Notes::default(),
            m_fun: Fun::default(),
            m_settings: Settings::default(),
            m_launcher: Launcher::default(),
            m_help: Help::default(),
            sidebar_state: ListState::default(),
        };
        app.loaded[tab::HOME] = true;
        app.loaded[tab::PROJECTS] = true;
        app.build_wallpaper();
        app.toast = Some(("Nébula lista ✦".into(), 3.0));
        app
    }

    // ── Fondo ─────────────────────────────────────────────────────────────

    pub fn build_wallpaper(&mut self) {
        let (w, h) = self.size;
        let mode = self.cfg.appearance.background.clone();
        let base = self.theme.bg;
        let dbg = |msg: String| {
            if std::env::var("NEBULA_DEBUG").is_ok() {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/nebula_wp.log")
                {
                    let _ = writeln!(f, "{msg}");
                }
            }
        };
        if mode == "image" {
            let path = if !self.cfg.appearance.wallpaper.is_empty() {
                Some(crate::config::expand(&self.cfg.appearance.wallpaper))
            } else {
                crate::config::wallpaper_candidates().into_iter().next()
            };
            dbg(format!("size={w}x{h} path={path:?}"));
            if let Some(p) = path {
                if p.is_file() {
                    let wp = Wallpaper::from_image(
                        &p,
                        w,
                        h,
                        base,
                        self.cfg.appearance.wallpaper_opacity,
                    );
                    dbg(format!(
                        "from_image -> mode={} cells={} avg={:?}",
                        wp.mode,
                        wp.width * wp.height,
                        wp.avg
                    ));
                    if wp.mode == "image" {
                        self.wp = wp;
                        return;
                    }
                }
            }
            self.wp = Wallpaper::empty("solid", base);
        } else {
            self.wp = Wallpaper::empty(&mode, base);
        }
    }

    fn set_theme(&mut self, name: String) {
        // El guard se fija en el tema *objetivo* (theme_b), no en cfg:
        // si mirara cfg, ApplyConfig ya habría copiado el valor y saldría
        // sin arrancar la transición (el cambio "no se veía").
        if name.eq_ignore_ascii_case(&self.theme_b.name) && self.theme_t >= 1.0 {
            return;
        }
        self.theme_a = self.theme.clone();
        let mut next = Theme::by_name(&name);
        if crate::config::tty_palette() {
            next = next.for_tty();
        }
        self.theme_b = next;
        self.theme_t = 0.0;
        self.cfg.general.theme = name;
        if self.cfg.appearance.background != "image" {
            self.build_wallpaper();
        }
    }

    // ── Mensajes ──────────────────────────────────────────────────────────

    pub fn drain(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Sys(s) => {
                    let cpu = s.cpu;
                    self.sys = s;
                    self.m_sys.on_sys(cpu);
                }
                Msg::Music(m) => self.music = m,
                Msg::Vis(v) => self.vis = v,
                Msg::Projects(p) => {
                    self.projects = p;
                    if self.pending_git {
                        self.pending_git = false;
                        self.load_git();
                    }
                }
                Msg::Git {
                    path,
                    repo,
                    files,
                    log,
                    diff,
                } => {
                    self.git = GitState {
                        path,
                        repo,
                        files,
                        log,
                        diff,
                        loading: false,
                    };
                }
                Msg::Services(v) => self.services = v,
                Msg::Pkgs(p) => self.pkgs = p,
                Msg::Docker(d) => self.docker = d,
                Msg::Net(n) => self.net = n,
                Msg::Db(r) => {
                    self.m_dev.results.retain(|x| x.name != r.name);
                    self.m_dev.results.push(r);
                }
                Msg::Note(s) => {
                    if !s.trim().is_empty() {
                        self.toast = Some((s, 5.0));
                    }
                }
                Msg::Av(a) => self.av = a,
                Msg::Weather(w) => self.weather = w,
                Msg::Boot(line) => self.boot_lines.push(line),
                Msg::BootEnd => {
                    if self.splash {
                        self.boot_end = 0.0001; // marca: empieza la retirada
                    }
                }
                Msg::GitRefresh => {
                    if self.tab == tab::GIT || self.loaded[tab::GIT] {
                        self.load_git();
                    }
                }
                Msg::Out { id, line } => {
                    if id == self.out_id {
                        if let Some(o) = self.output.as_mut() {
                            o.push(line);
                        }
                    }
                }
                Msg::OutEnd { id, ok } => {
                    if id != self.out_id {
                        continue; // turno de otro comando: se ignora
                    }
                    let closed = self.overlay != Overlay::Output;
                    if let Some(o) = self.output.as_mut() {
                        o.finish(ok);
                        let git = o.title.starts_with("git");
                        if closed {
                            let msg = o.summary(ok);
                            if !msg.trim().is_empty() {
                                self.toast = Some((msg, 5.0));
                            }
                        }
                        // Detrás del visor, la pestaña se pone al día.
                        if git {
                            self.load_git();
                        }
                    }
                    self.refresh_tab(self.tab);
                }
                Msg::Search { token, hits, error } => self.m_fun.on_search(token, hits, error),
                Msg::Stations {
                    token,
                    items,
                    error,
                } => self.m_fun.on_stations(token, items, error),
            }
        }
    }

    // ── Entrada ───────────────────────────────────────────────────────────

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        // 0) Pantalla de arranque: Esc la salta.
        if self.splash {
            if key.code == KeyCode::Esc {
                self.splash = false;
            }
            return;
        }
        // 1) Superposiciones
        match self.overlay {
            // El terminal se lleva las teclas: las de Nébula se interceptan
            // aquí y todo lo demás viaja a la aplicación embebida.
            Overlay::Term => {
                self.term_key(key);
                return;
            }
            Overlay::Launcher => {
                let a = self.m_launcher.on_key(key);
                match a {
                    Some(Action::Keep) => {}
                    Some(other) => {
                        self.close_overlay();
                        self.apply(other);
                    }
                    None => self.close_overlay(),
                }
                return;
            }
            Overlay::Help => {
                let a = self.m_help.on_key(key);
                match a {
                    Some(Action::Keep) => {}
                    Some(other) => {
                        self.close_overlay();
                        self.apply(other);
                    }
                    None => self.close_overlay(),
                }
                return;
            }
            Overlay::Confirm => {
                match key.code {
                    KeyCode::Char('s')
                    | KeyCode::Char('y')
                    | KeyCode::Char('S')
                    | KeyCode::Char('Y')
                    | KeyCode::Enter => {
                        let job = self.confirm.take().map(|(_, j)| j);
                        self.close_overlay();
                        if let Some(j) = job {
                            self.exec(j);
                        }
                    }
                    _ => {
                        self.confirm = None;
                        self.close_overlay();
                    }
                }
                return;
            }
            Overlay::Output => {
                let close = self.output.as_mut().map(|o| o.on_key(key)).unwrap_or(true);
                if close {
                    // El comando sigue en marcha por detrás: si termina
                    // después, su resultado saldrá como aviso.
                    self.overlay = Overlay::None;
                }
                return;
            }
            Overlay::None => {}
        }

        // 1.5) alt+t: la consola embebida, en cualquier pestaña y **antes**
        //     de que ningún módulo se coma la tecla (en Entretenimiento
        //     `t` ya significa otra cosa).
        if key.modifiers.contains(KeyModifiers::ALT)
            && !key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('t') | KeyCode::Char('T'))
        {
            self.open_term(2);
            return;
        }

        // 2) Volumen y brillo: funcionan en cualquier módulo.
        {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            let shift = key.modifiers.contains(KeyModifiers::SHIFT);
            if ctrl {
                match key.code {
                    KeyCode::Up if shift => {
                        set_brightness(5);
                        return;
                    }
                    KeyCode::Down if shift => {
                        set_brightness(-5);
                        return;
                    }
                    KeyCode::Up => {
                        set_volume(5);
                        return;
                    }
                    KeyCode::Down => {
                        set_volume(-5);
                        return;
                    }
                    _ => {}
                }
            }
        }

        // 2.5) Foco del menú lateral: elijo pestaña con ↑/↓ e Intro.
        // Las filas son las de la lateral (con cabeceras de grupo): las
        // flechas saltan de pestaña a pestaña y nunca caen en un título.
        if self.side_focus {
            match key.code {
                KeyCode::Up => {
                    self.side_sel = side_step(self.side_sel, -1);
                    return;
                }
                KeyCode::Down => {
                    self.side_sel = side_step(self.side_sel, 1);
                    return;
                }
                KeyCode::Home => {
                    self.side_sel = first_tab_row();
                    return;
                }
                KeyCode::End => {
                    self.side_sel = last_tab_row();
                    return;
                }
                KeyCode::Enter => {
                    if let Some(n) = tab_at_row(self.side_sel as u16) {
                        self.side_focus = false;
                        self.goto(n);
                    }
                    return;
                }
                KeyCode::Esc => {
                    self.side_focus = false;
                    return;
                }
                _ if crate::config::key_matches(&self.cfg.keys.sidebar_focus, key) => {
                    self.side_focus = false;
                    return;
                }
                _ => {}
            }
        }

        // 3) Módulo actual
        let action = {
            let ctx = ctx!(self);
            match self.tab {
                tab::HOME => self.m_home.on_key(key),
                tab::SYSTEM => self.m_sys.on_key(key, &ctx),
                tab::PROJECTS => self.m_projects.on_key(key, &ctx),
                tab::GIT => self.m_git.on_key(key, &ctx),
                tab::DEV => self.m_dev.on_key(key, &ctx),
                tab::NET => self.m_net.on_key(key),
                tab::DOCKER => self.m_docker.on_key(key, &ctx),
                tab::PACKAGES => self.m_packages.on_key(key, &ctx),
                tab::SERVICES => self.m_services.on_key(key, &ctx),
                tab::NOTES => self.m_notes.on_key(key),
                tab::FUN => self.m_fun.on_key(key, &mut self.m_music, &ctx),
                tab::SETTINGS => self.m_settings.on_key(key),
                _ => None,
            }
        };
        if let Some(a) = action {
            self.apply(a);
            return;
        }

        // 4) Globales
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('c') if ctrl => self.running = false,
            KeyCode::Char('q') => self.running = false,
            _ if crate::config::key_matches(&self.cfg.keys.launcher, key) => self.open_launcher(),
            _ if crate::config::key_matches(&self.cfg.keys.zen, key) => self.toggle_zen(),
            _ if crate::config::key_matches(&self.cfg.keys.mute, key) => toggle_mute(),
            _ if crate::config::key_matches(&self.cfg.keys.sidebar, key) => self.toggle_sidebar(),
            _ if crate::config::key_matches(&self.cfg.keys.sidebar_focus, key) => {
                // Entra al menú lateral (abriéndolo si hace falta) o sale de él.
                if self.side_focus {
                    self.side_focus = false;
                } else {
                    if !self.sidebar_open {
                        self.toggle_sidebar();
                    }
                    self.side_sel = side_row(self.tab) as usize;
                    self.side_focus = true;
                }
            }
            KeyCode::Char('p') if ctrl => self.open_launcher(),
            KeyCode::Char(' ') if !ctrl => self.open_launcher(),
            _ if crate::config::key_matches(&self.cfg.keys.help, key) => {
                self.overlay = Overlay::Help
            }
            KeyCode::F(1) => self.overlay = Overlay::Help,
            KeyCode::F(2) => self.goto(tab::SETTINGS),
            KeyCode::Tab => self.apply(Action::Next),
            KeyCode::BackTab => self.apply(Action::Prev),
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let n = if c == '0' {
                    9
                } else {
                    c as usize - '1' as usize
                };
                if n < tab::COUNT {
                    self.goto(n);
                }
            }
            KeyCode::Esc => {}
            _ => {}
        }
    }

    pub fn on_mouse(&mut self, m: MouseEvent) {
        // Dentro del terminal el ratón es de la aplicación embebida.
        if self.overlay == Overlay::Term {
            self.term_mouse(m);
            return;
        }
        match m.kind {
            MouseEventKind::ScrollUp => self.on_key(synth(KeyCode::Up)),
            MouseEventKind::ScrollDown => self.on_key(synth(KeyCode::Down)),
            MouseEventKind::Down(MouseButton::Left) => {
                if self.overlay != Overlay::None {
                    return;
                }
                let area = self.size;
                let side_w = if area.0 >= 76 {
                    self.side_w.value.round() as u16
                } else {
                    0
                };
                let header_h = 1;
                let status_h = 1;
                let body_top = header_h;
                let body_bottom = area.1.saturating_sub(status_h);
                // Botón ⟨/⟩: la columna reservada al borde de la lateral.
                if !self.zen && m.column == side_w && m.row >= body_top && m.row < body_bottom {
                    self.toggle_sidebar();
                    return;
                }
                if m.column < side_w && m.row >= body_top && m.row < body_bottom && side_w > 0 {
                    let idx = m.row - body_top;
                    if let Some(n) = tab_at_row(idx) {
                        self.goto(n);
                    }
                }
                // Chip ♪ de la barra de estado: un clic alterna play/pausa
                // desde cualquier pestaña.
                if !self.zen && m.row == area.1.saturating_sub(1) {
                    let x0 = TABS[self.tab].0.chars().count() as u16 + 6;
                    if m.column >= x0 && m.column < x0 + 4 {
                        self.apply(crate::modules::music::toggle_action(&self.music));
                    }
                }
            }
            _ => {}
        }
    }

    pub fn on_resize(&mut self, w: u16, h: u16) {
        self.size = (w, h);
        self.build_wallpaper();
    }

    fn open_launcher(&mut self) {
        let ctx = ctx!(self);
        self.m_launcher.open(&ctx);
        self.overlay = Overlay::Launcher;
    }

    fn close_overlay(&mut self) {
        self.overlay = Overlay::None;
        self.m_launcher.close();
        self.confirm = None;
    }

    // ── Acciones ──────────────────────────────────────────────────────────

    pub fn apply(&mut self, a: Action) {
        match a {
            Action::Keep => {}
            Action::Quit => self.running = false,
            Action::Tab(n) => self.goto(n),
            Action::Next => self.goto((self.tab + 1) % tab::COUNT),
            Action::Prev => self.goto((self.tab + tab::COUNT - 1) % tab::COUNT),
            Action::Launcher => self.open_launcher(),
            Action::Help => self.overlay = Overlay::Help,
            Action::Refresh => self.refresh_current(),
            Action::Run(job) => self.exec(job),
            Action::Confirm(msg, job) => {
                self.confirm = Some((msg, job));
                self.overlay = Overlay::Confirm;
            }
            Action::Toast(msg) => self.toast = Some((msg, 3.5)),
            Action::TestDb(i) => self.test_db(i),
            Action::Ping => self.ping(),
            Action::ApplyConfig(cfg) => {
                let theme_changed = cfg.general.theme != self.cfg.general.theme;
                let wp_changed = cfg.appearance != self.cfg.appearance;
                let place_changed = cfg.weather != self.cfg.weather;
                self.cfg = *cfg;
                self.sidebar_open = self.cfg.general.sidebar;
                if theme_changed {
                    let name = self.cfg.general.theme.clone();
                    self.set_theme(name);
                    self.toast = Some((format!("tema {}", self.cfg.general.theme), 2.5));
                }
                if wp_changed {
                    self.build_wallpaper();
                }
                if place_changed {
                    let q = self.cfg.weather.query();
                    if let Ok(mut g) = self.place.lock() {
                        *g = q.clone();
                    }
                    fetch_weather_now(self.tx.clone(), self.place.clone());
                    let label = if q.is_empty() {
                        "automática".to_string()
                    } else {
                        q
                    };
                    self.toast = Some((format!("clima · {label}"), 2.5));
                }
            }
            Action::SaveConfig(cfg) => {
                self.apply(Action::ApplyConfig(cfg));
                match self.cfg.save() {
                    Ok(_) => self.toast = Some(("config guardada ✦".into(), 3.0)),
                    Err(e) => self.toast = Some((format!("no se pudo guardar: {e}"), 4.0)),
                }
            }
            Action::ReloadWallpaper => {
                self.build_wallpaper();
                self.toast = Some(("fondo recargado".into(), 2.0));
            }
            Action::RefreshWeather => {
                // La ubicación nueva ya viaja en la celda compartida.
                fetch_weather_now(self.tx.clone(), self.place.clone());
                self.toast = Some(("consultando clima…".into(), 2.0));
            }
            Action::Search {
                token,
                anime,
                query,
            } => {
                if query.trim().is_empty() {
                    return;
                }
                if anime {
                    spawn_anime_search(self.tx.clone(), token, query);
                } else {
                    spawn_video_search(self.tx.clone(), token, query);
                }
                self.toast = Some(("buscando…".into(), 2.5));
            }
            // ── Fase 7: terminales embebidos, emisoras, descargas, TTY ──
            Action::Term(n) => self.open_term(n),
            Action::Stations => {
                self.goto(tab::FUN);
                let token = self.m_fun.open_stations();
                spawn_stations(self.tx.clone(), token);
            }
            Action::Station { provider, id, name } => {
                spawn_station_play(self.tx.clone(), provider, id, name)
            }
            Action::Download => {
                self.goto(tab::FUN);
                self.m_fun.open_download();
            }
            // Los dos reproductores se paran sin bloquear la interfaz:
            // el aviso llega por el canal cuando han terminado.
            Action::StopCmus => {
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let note = if !cmus_running() {
                        "cmus no estaba corriendo".to_string()
                    } else if stop_cmus() {
                        "cmus detenido".to_string()
                    } else {
                        "cmus no responde: pruébalo de nuevo".to_string()
                    };
                    let _ = tx.send(Msg::Note(note));
                });
            }
            Action::StopCliamp => {
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let note = if !cliamp_running() {
                        "cliamp no estaba corriendo".to_string()
                    } else if stop_cliamp() {
                        "cliamp detenido".to_string()
                    } else {
                        "cliamp no responde: pruébalo de nuevo".to_string()
                    };
                    let _ = tx.send(Msg::Note(note));
                });
            }
            Action::LoadLib => {
                let tx = self.tx.clone();
                let home = std::env::var("HOME").unwrap_or_else(|_| "~".into());
                let dir = format!("{home}/Música");
                std::thread::spawn(move || {
                    let (ok, msg) = cmus_add_library(&dir);
                    let note = if ok {
                        msg
                    } else {
                        format!("{msg} · {}", ui::truncate(&dir, 40))
                    };
                    let _ = tx.send(Msg::Note(note));
                });
            }
        }
    }

    pub fn goto(&mut self, n: usize) {
        if n >= tab::COUNT || n == self.tab {
            return;
        }
        self.tab = n;
        self.slide = 1.0;
        self.side_y.set(side_row(n) as f32);
        if !self.loaded[n] {
            self.loaded[n] = true;
            self.refresh_tab(n);
        }
    }

    fn refresh_current(&mut self) {
        self.refresh_tab(self.tab);
    }

    fn refresh_tab(&mut self, n: usize) {
        let tx = self.tx.clone();
        match n {
            tab::PROJECTS => {
                let dirs = self.cfg.projects.dirs.clone();
                let max = self.cfg.projects.max;
                spawn_projects(tx, dirs, max);
            }
            tab::GIT => self.load_git(),
            tab::SERVICES => spawn_services(tx),
            tab::PACKAGES => spawn_packages(tx),
            tab::DOCKER => spawn_docker(tx),
            tab::NET => spawn_net(tx),
            tab::DEV => {
                if self.projects.is_empty() {
                    spawn_projects(tx, self.cfg.projects.dirs.clone(), self.cfg.projects.max);
                }
            }
            tab::SYSTEM | tab::HOME => {
                self.toast = Some(("datos en vivo · muestreo cada 0,8 s".into(), 2.0));
            }
            _ => {}
        }
    }

    fn load_git(&mut self) {
        let ctx = ctx!(self);
        let path = self.m_git.selected_path(&ctx);
        if path.is_empty() {
            if self.projects.is_empty() {
                self.pending_git = true;
                spawn_projects(
                    self.tx.clone(),
                    self.cfg.projects.dirs.clone(),
                    self.cfg.projects.max,
                );
            } else {
                self.toast = Some(("no hay repositorios todavía".into(), 3.0));
            }
            return;
        }
        self.git.loading = true;
        spawn_git(self.tx.clone(), path);
    }

    /// Abre el visor interno con un turno nuevo y devuelve su id.
    fn open_output(&mut self, title: &str, cmd: &str) -> u64 {
        self.out_id += 1;
        self.output = Some(crate::output::Output::new(title, cmd));
        self.overlay = Overlay::Output;
        self.out_id
    }

    fn exec(&mut self, job: Job) {
        let tx = self.tx.clone();
        match job {
            // ── Modo terminal: salir y dejar el comando en la TTY ──
            // Nébula devuelve la consola antes de que pkexec pregunte su
            // contraseña, así que el cambio de sesión (o el reinicio) no
            // arrastra la interfaz por el camino.
            Job::Tty(args) => {
                if !args.is_empty() {
                    self.pending = Some(args);
                    self.running = false;
                }
            }
            // ── Con privilegios: sólo la contraseña sale de Nébula ──
            Job::Apt(args) => {
                if args.is_empty() {
                    return;
                }
                let title = format!("apt {}", args[0]);
                let shown = format!("pkexec apt {}", args.join(" "));
                let id = self.open_output(&title, &shown);
                std::thread::spawn(move || {
                    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                    spawn_priv_stream(tx, id, "apt", &refs);
                });
            }
            Job::Install { pkgs, cmds } => {
                if pkgs.is_empty() && cmds.is_empty() {
                    return;
                }
                let mut shown = String::new();
                if !pkgs.is_empty() {
                    shown = format!("pkexec apt install -y {}", pkgs.join(" "));
                }
                for (label, cmd) in &cmds {
                    if !shown.is_empty() {
                        shown.push_str("   ·   ");
                    }
                    shown.push_str(&format!("[{label}] {cmd}"));
                }
                let title = if pkgs.is_empty() {
                    format!("instalar {}", cmds[0].0)
                } else {
                    "instalación".to_string()
                };
                let id = self.open_output(&title, &shown);
                std::thread::spawn(move || {
                    let mut ok = true;
                    // Un sólo pkexec para todos los paquetes: una contraseña.
                    if !pkgs.is_empty() {
                        let mut args = vec!["install".to_string(), "-y".to_string()];
                        args.extend(pkgs);
                        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                        ok = priv_stream_run(&tx, id, "apt", &refs);
                        let _ = tx.send(Msg::Out {
                            id,
                            line: String::new(),
                        });
                    }
                    for (label, cmd) in &cmds {
                        let _ = tx.send(Msg::Out {
                            id,
                            line: format!("── {label} ──"),
                        });
                        ok = shell_stream_run(&tx, id, None, cmd) && ok;
                        let _ = tx.send(Msg::Out {
                            id,
                            line: String::new(),
                        });
                    }
                    crate::resources::refresh();
                    let _ = tx.send(Msg::OutEnd { id, ok });
                });
            }
            Job::Service(name, act) => {
                let title = format!("systemctl {act}");
                let shown = format!("pkexec systemctl {act} {name}");
                let id = self.open_output(&title, &shown);
                std::thread::spawn(move || {
                    spawn_priv_stream(tx, id, "systemctl", &[&act, &name]);
                });
            }
            Job::Docker(act, res) => {
                let title = format!("docker {act}");
                let shown = format!("docker {act} {res}");
                let id = self.open_output(&title, &shown);
                std::thread::spawn(move || {
                    spawn_stream(tx, id, "docker", &[&act, &res], None);
                });
            }
            Job::Kill(pid) => {
                let (ok, msg) = kill_process(pid);
                self.toast = Some((msg, if ok { 3.0 } else { 4.0 }));
            }
            Job::Privileged(args) => {
                if args.is_empty() {
                    return;
                }
                let title = if args[0] == "sh" && args.len() >= 3 {
                    ui::truncate(&args[2], 46)
                } else {
                    args.join(" ")
                };
                let shown = format!("pkexec {}", args.join(" "));
                let id = self.open_output(&title, &shown);
                std::thread::spawn(move || {
                    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                    spawn_priv_stream(tx, id, refs[0], &refs[1..]);
                });
            }
            // ── Aplicaciones: se abren fuera, sin ventana propia ──
            Job::Plain(args) => {
                if args.is_empty() {
                    return;
                }
                let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                spawn_detached(refs[0], &refs[1..], None);
                self.toast = Some((format!("lanzando {}", args[0]), 2.0));
            }
            // ── Git: commit, ramas y stash, con la salida aquí dentro ──
            Job::RunIn { cwd, cmd } => {
                if cmd.trim().is_empty() {
                    return;
                }
                let id = self.open_output("git", &cmd);
                let path = Some(std::path::PathBuf::from(&cwd));
                std::thread::spawn(move || {
                    let ok = shell_stream_run(&tx, id, path.as_deref(), &cmd);
                    let _ = tx.send(Msg::OutEnd { id, ok });
                    let _ = tx.send(Msg::GitRefresh);
                });
            }
            // ── Terminal de verdad: sólo cuando se pide a propósito ──
            Job::Shell { cwd, cmd } => {
                if crate::data::has_terminal_emulator() {
                    let path = if cwd.is_empty() {
                        None
                    } else {
                        Some(std::path::PathBuf::from(&cwd))
                    };
                    if cmd.is_empty() {
                        spawn_in_terminal("sh", &["-l"], path.as_deref());
                    } else {
                        spawn_in_terminal("sh", &["-c", &cmd], path.as_deref());
                    }
                    self.toast = Some((
                        if cwd.is_empty() {
                            "abriendo terminal".into()
                        } else {
                            format!("terminal en {}", ui::truncate(&cwd, 34))
                        },
                        2.0,
                    ));
                } else {
                    // Sin emulador (TTY de servidor o entorno mínimo) no
                    // hay ventana donde abrirlo: la orden va escrita en la
                    // consola embebida de Nébula.
                    self.toast = Some((
                        if cwd.is_empty() {
                            "consola embebida".into()
                        } else {
                            format!("consola en {}", ui::truncate(&cwd, 34))
                        },
                        2.0,
                    ));
                    self.open_term(2);
                    let line = if cmd.is_empty() {
                        String::new()
                    } else if cwd.is_empty() {
                        cmd
                    } else {
                        format!("cd {} && {cmd}", sh_quote(&cwd))
                    };
                    if !line.is_empty() {
                        if let Some(p) = self.term[2].as_mut() {
                            p.write(format!("{line}\n").as_bytes());
                        }
                    }
                }
            }
            // ── Visor interno: git, apt, scripts, instaladores… ──
            Job::View { title, cwd, cmd } => {
                if cmd.trim().is_empty() {
                    return;
                }
                let id = self.open_output(&title, &cmd);
                let path = if cwd.is_empty() {
                    None
                } else {
                    Some(std::path::PathBuf::from(cwd))
                };
                std::thread::spawn(move || {
                    spawn_shell_stream(tx, id, path.as_deref(), &cmd);
                });
            }
            // ── cliamp: levanta su daemon si hace falta y contesta ──
            Job::Cliamp(args) => {
                if args.is_empty() {
                    return;
                }
                let what = args.join(" ");
                self.toast = Some((format!("cliamp · {what}…"), 1.5));
                std::thread::spawn(move || {
                    let note = if !ensure_cliamp() {
                        "cliamp · no está instalado ni arranca".to_string()
                    } else {
                        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                        let (ok, out) = run_full("cliamp", &refs);
                        if ok {
                            let status = run("cliamp", &["status"]).unwrap_or_default();
                            cliamp_note(&status, &out)
                        } else {
                            summarize("cliamp", false, &out)
                        }
                    };
                    let _ = tx.send(Msg::Note(note));
                });
            }
            // ── cmus: en una pty invisible y controlado desde aquí ──
            Job::Cmus(args) => {
                self.toast = Some(("cmus…".into(), 1.5));
                std::thread::spawn(move || {
                    let note = if !ensure_cmus() {
                        "cmus · no está instalado o no arranca".to_string()
                    } else if args.is_empty() {
                        "cmus · corriendo en segundo plano".to_string()
                    } else if args.len() >= 2 && args[0] == "volume" {
                        // Volumen relativo: cmus sólo acepta valores absolutos.
                        let delta = args[1].parse::<i32>().unwrap_or(0);
                        let (ok, out) = cmus_volume(delta);
                        if ok {
                            format!("cmus · volumen {delta:+}")
                        } else {
                            summarize("cmus", false, &out)
                        }
                    } else if args.len() == 1 && args[0] == "-t" {
                        // cmus-remote no trae «toggle»: se mira el estado y
                        // se pausa o se reanuda en consecuencia.
                        let status = run("cmus-remote", &["-Q"]).unwrap_or_default();
                        let playing = status.lines().any(|l| l.trim() == "status playing");
                        let flag = if playing { "-u" } else { "-p" };
                        let (ok, out) = run_full("cmus-remote", &[flag]);
                        if ok {
                            let status = run("cmus-remote", &["-Q"]).unwrap_or_default();
                            cmus_note(&status, &out)
                        } else {
                            summarize("cmus", false, &out)
                        }
                    } else {
                        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                        let (ok, out) = run_full("cmus-remote", &refs);
                        if ok {
                            let status = run("cmus-remote", &["-Q"]).unwrap_or_default();
                            cmus_note(&status, &out)
                        } else {
                            summarize("cmus", false, &out)
                        }
                    };
                    let _ = tx.send(Msg::Note(note));
                });
            }
        }
    }

    // ── Terminales embebidos ──────────────────────────────────────────────

    /// Teclas dentro del panel del terminal: las de Nébula se interceptan
    /// aquí y el resto (q, Esc, dígitos, `?`…) viaja entero a la
    /// aplicación, que es la dueña de sus propios atajos.
    fn term_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            // Salir del panel: la pty sigue viva, sólo se oculta.
            KeyCode::Char('g') if ctrl => self.overlay = Overlay::None,
            // Cambiar de aplicación (cmus ↔ cliamp ↔ consola); si la otra
            // no está abierta, se abre en el momento.
            KeyCode::Char('o') | KeyCode::Tab if ctrl => {
                let other = (self.term_focus.min(2) + 1) % 3;
                self.open_term(other);
            }
            // Detener el proceso que está en pantalla.
            KeyCode::Char('t') if ctrl => self.stop_term(),
            // Ir directo a la consola embebida.
            KeyCode::Char('t') | KeyCode::Char('T') if alt => {
                self.open_term(2);
            }
            _ => {
                if let Some(bytes) = crate::term::key_bytes(key) {
                    if let Some(p) = self.term[self.term_focus.min(2)].as_mut() {
                        p.write(&bytes);
                    }
                }
            }
        }
    }

    /// Abre (o reabre) el terminal `n` y lo pone en pantalla:
    /// 0 = cmus, 1 = cliamp, 2 = consola (shell de login de `$SHELL`).
    fn open_term(&mut self, n: usize) {
        if n > 2 {
            return;
        }
        // Un pty que ya terminó se sustituye por uno nuevo.
        if self.term[n].as_ref().is_some_and(|p| !p.alive()) {
            self.term[n] = None;
        }
        if self.term[n].is_none() {
            // La consola usa el shell de la usuaria; las aplicaciones, su binario.
            let shell = std::env::var("SHELL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "sh".into());
            let (prog, args, label): (&str, &[&str], &'static str) = match n {
                0 => ("cmus", &[], "cmus"),
                1 => ("cliamp", &[], "cliamp"),
                _ => (shell.as_str(), &["-l"], "consola"),
            };
            if which(prog).is_none() {
                self.toast = Some((format!("{prog} no está instalado"), 3.5));
                return;
            }
            // Sólo las dos aplicaciones exigen ser la única instancia.
            if n == 0 {
                // cmus se niega a arrancar si ya escucha en su socket.
                if cmus_running() {
                    stop_cmus();
                }
            } else if n == 1 {
                // El TUI de cliamp no arranca con su daemon vivo: se para
                // y él retoma solo con su resume.json.
                stop_cliamp();
            }
            let (cols, rows) = term_inner(self.size);
            match crate::term::Pty::open(prog, args, cols, rows, label) {
                Some(p) => self.term[n] = Some(p),
                None => {
                    self.toast = Some((format!("{label} no arranca"), 3.5));
                    return;
                }
            }
            // Biblioteca en blanco: cmus arranca con el árbol vacío, así
            // que se siembra ~/Música en cuanto su socket esté dispuesto.
            if n == 0 && crate::data::cmus_lib_empty() {
                self.seed_cmus();
            }
        }
        self.term_focus = n;
        self.overlay = Overlay::Term;
    }

    /// Siembra la carpeta de música en la biblioteca de cmus recién abierto.
    /// cmus tarda un poco en escuchar en su socket: se reintenta unos
    /// segundos y, si cuaja, llega el aviso «biblioteca cargada».
    fn seed_cmus(&mut self) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let home = std::env::var("HOME").unwrap_or_default();
            let dir = format!("{home}/Música");
            if dir.is_empty() || !std::path::Path::new(&dir).is_dir() {
                return;
            }
            for _ in 0..30 {
                let (ok, _) = run_full("cmus-remote", &["-l", dir.as_str()]);
                if ok {
                    let _ = tx.send(Msg::Note("biblioteca cargada".into()));
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        });
    }

    /// Detiene el proceso del terminal en pantalla (Ctrl+T).
    fn stop_term(&mut self) {
        let n = self.term_focus.min(2);
        let Some(p) = self.term[n].as_mut() else {
            return;
        };
        let label = p.label;
        p.kill();
        self.toast = Some((format!("{label} · detenido"), 2.5));
    }

    /// Ratón dentro del panel: flechas si la app no pide ratón, y si lo
    /// pide, la secuencia SGR con coordenadas relativas al área útil.
    fn term_mouse(&mut self, m: MouseEvent) {
        let area = Rect {
            x: 0,
            y: 0,
            width: self.size.0,
            height: self.size.1,
        };
        let rect = term_rect(area);
        let inner = Rect {
            x: rect.x + 1,
            y: rect.y + 1,
            width: rect.width.saturating_sub(2),
            height: rect.height.saturating_sub(3),
        };
        let Some(p) = self.term[self.term_focus.min(2)].as_mut() else {
            return;
        };
        let inside = m.column >= inner.x
            && m.column < inner.x + inner.width
            && m.row >= inner.y
            && m.row < inner.y + inner.height;
        if !inside {
            return;
        }
        if p.wants_mouse() {
            let col = m.column - inner.x;
            let row = m.row - inner.y;
            if let Some(bytes) = crate::term::mouse_bytes(&m, col, row) {
                p.write(&bytes);
            }
            return;
        }
        // Sin ratón pedido: la rueda se convierte en flechas, como en el
        // resto de Nébula.
        let arrow = match m.kind {
            MouseEventKind::ScrollUp => b"\x1b[A",
            MouseEventKind::ScrollDown => b"\x1b[B",
            _ => return,
        };
        p.write(arrow);
    }

    /// El panel del terminal: título con estado, pantalla vt100 y pie de
    /// atajos.
    fn render_term(&mut self, f: &mut Frame, rect: Rect) {
        let t = self.theme.clone();
        let n = self.term_focus.min(2);
        let (label, alive) = match self.term[n].as_ref() {
            Some(p) => (p.label, p.alive()),
            None => (
                if n == 0 {
                    "cmus"
                } else if n == 1 {
                    "cliamp"
                } else {
                    "consola"
                },
                false,
            ),
        };
        let state = if self.term[n].is_none() {
            "sin abrir"
        } else if alive {
            "en marcha"
        } else {
            "finalizado"
        };
        let title = format!(" ▸ {label} · {state} ");
        let block = ui::panel_on(&t, &self.wp, rect, title, 0.97);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        if inner.width < 8 || inner.height < 4 {
            return;
        }
        let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
        let (term_area, foot) = (rows[0], rows[1]);
        ui::hint_bar(
            f,
            &t,
            foot,
            &[
                ("ctrl+g", "salir"),
                ("ctrl+tab / ctrl+o", "cambiar"),
                ("ctrl+t", "detener"),
                ("alt+t", "consola"),
            ],
        );

        let Some(p) = self.term[n].as_mut() else {
            let open = if n == 2 {
                "  ábrelo con  alt+t"
            } else {
                "  ábrelo desde Entretenimiento con  t"
            };
            let msg = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  aquí va ", Style::default().fg(t.fg_dim)),
                    Span::styled(label, Style::default().fg(t.accent)),
                    Span::styled("  con sus propios atajos", Style::default().fg(t.fg_dim)),
                ]),
                Line::from(""),
                Line::from(Span::styled(open, Style::default().fg(t.fg_faint))),
            ];
            f.render_widget(ratatui::widgets::Paragraph::new(msg), term_area);
            return;
        };
        p.resize(term_area.width, term_area.height);
        p.feed();
        f.render_widget(
            tui_term::widget::PseudoTerminal::new(p.screen())
                .cursor(tui_term::widget::Cursor::default()),
            term_area,
        );
        // vt100 manda «color por defecto» para las celdas sin color: aquí
        // ese defecto es el cristal del panel, no el fondo de la terminal.
        let bg = ui::glass(&t, &self.wp, term_area, 0.97);
        let buf = f.buffer_mut();
        for y in term_area.top()..term_area.bottom() {
            for x in term_area.left()..term_area.right() {
                let c = &mut buf[(x, y)];
                if c.bg == Color::Reset {
                    c.bg = bg;
                }
                if c.fg == Color::Reset {
                    c.fg = t.fg;
                }
            }
        }
    }

    fn test_db(&mut self, idx: usize) {
        let Some(c) = self.cfg.databases.get(idx).cloned() else {
            return;
        };
        let mut password = c.password.clone();
        if !c.password_env.is_empty() {
            if let Ok(v) = std::env::var(&c.password_env) {
                password = v;
            }
        }
        let tx = self.tx.clone();
        let label = c.name.clone();
        let _ = tx.send(Msg::Note(format!("conectando a {label}…")));
        spawn_db_test(tx, c.name, c.host, c.port, c.user, password, c.dbname);
        self.toast = Some((format!("probando {label}…"), 3.0));
    }

    fn ping(&mut self) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let (ok, out) = run_full("ping", &["-c", "3", "-W", "2", "1.1.1.1"]);
            let stats = out
                .lines()
                .find(|l| l.contains("rtt") || l.contains("min/avg/max"))
                .unwrap_or("")
                .to_string();
            let msg = if ok && !stats.is_empty() {
                format!("latencia · {}", stats.trim())
            } else {
                "sin respuesta de 1.1.1.1".to_string()
            };
            let _ = tx.send(Msg::Note(msg));
        });
        self.toast = Some(("midiendo latencia…".into(), 3.0));
    }

    // ── Reloj ─────────────────────────────────────────────────────────────

    pub fn tick(&mut self) {
        let dt = self.clock.advance();
        self.now = Local::now();

        if self.boot < 1.0 {
            self.boot = (self.boot + dt.as_secs_f32() / 0.75).min(1.0);
        }
        // La cortina del arranque se retira cuando el análisis termina.
        if self.splash && self.boot_end > 0.0 {
            self.boot_end += dt.as_secs_f32() / 0.45;
            if self.boot_end >= 1.0 {
                self.splash = false;
            }
        }
        if self.slide > 0.0 {
            self.slide = (self.slide - dt.as_secs_f32() * 5.0).max(0.0);
        }
        self.side_y.tick(dt);
        // Abierta 22 cols · cerrada deja una franja de 4 cols sólo con iconos.
        let want_side = if self.size.0 < 76 {
            0.0
        } else if self.sidebar_open {
            22.0
        } else {
            4.0
        };
        self.side_w.set(want_side);
        self.side_w.tick(dt);
        if self.theme_t < 1.0 {
            self.theme_t = (self.theme_t + dt.as_secs_f32() / 0.5).min(1.0);
            let mut th = Theme::lerp(&self.theme_a, &self.theme_b, ease_in_out(self.theme_t));
            if crate::config::tty_palette() {
                th = th.for_tty();
            }
            self.theme = th;
        }
        if let Some((_, ttl)) = self.toast.as_mut() {
            *ttl -= dt.as_secs_f32();
            if *ttl <= 0.0 {
                self.toast = None;
            }
        }
    }

    // ── Dibujo ────────────────────────────────────────────────────────────

    /// Abre o cierra la barra lateral (Ctrl+B o el botón ⟨/⟩).
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_open = !self.sidebar_open;
        if !self.sidebar_open {
            self.side_focus = false;
        }
        // Se mantiene la config en sincronía para que Ajustes lo refleje.
        self.cfg.general.sidebar = self.sidebar_open;
        self.toast = Some((
            if self.sidebar_open {
                "barra lateral abierta · ctrl+e para el menú".into()
            } else {
                "barra lateral oculta · pulsa ⟨ en el borde".into()
            },
            2.0,
        ));
    }

    /// Modo Zen (F11): sin barra superior, lateral ni pie; sólo contenido.
    pub fn toggle_zen(&mut self) {
        self.zen = !self.zen;
        ui::set_zen(self.zen);
        self.toast = Some((
            if self.zen {
                "modo zen · f11 para salir".into()
            } else {
                "modo zen desactivado".into()
            },
            2.5,
        ));
    }

    pub fn render(&mut self, f: &mut Frame) {
        let area = f.area();
        self.size = (area.width, area.height);

        // Fondo
        {
            let stars = self.cfg.appearance.stars;
            let tick = self.clock.tick;
            let vignette = self.cfg.appearance.vignette;
            let buf = f.buffer_mut();
            self.wp.render(buf, area, stars, tick);
            if vignette {
                ui::vignette(buf, area, 0.45);
            }
        }

        let mut status_area: Option<Rect> = None;
        let content = if self.zen {
            // Modo Zen: todo el área para el contenido.
            area
        } else {
            let side_w = if area.width >= 76 {
                self.side_w.value.round() as u16
            } else {
                0
            };
            let rows = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);
            // Una columna reservada siempre para el botón ⟨/⟩ de la lateral.
            let body = Layout::horizontal([
                Constraint::Length(side_w),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(rows[1]);

            self.header(f, rows[0]);
            if side_w > 0 {
                self.sidebar(f, body[0]);
            }
            if !self.zen {
                self.side_button(f, body[1]);
            }
            status_area = Some(rows[2]);
            body[2]
        };
        if content.width < 4 || content.height < 3 {
            return;
        }

        // Módulo actual
        {
            let ctx = ctx!(self);
            match self.tab {
                tab::HOME => self.m_home.render(f, content, &ctx),
                tab::SYSTEM => self.m_sys.render(f, content, &ctx),
                tab::PROJECTS => self.m_projects.render(f, content, &ctx),
                tab::GIT => self.m_git.render(f, content, &ctx),
                tab::DEV => self.m_dev.render(f, content, &ctx),
                tab::NET => self.m_net.render(f, content, &ctx),
                tab::DOCKER => self.m_docker.render(f, content, &ctx),
                tab::PACKAGES => self.m_packages.render(f, content, &ctx),
                tab::SERVICES => self.m_services.render(f, content, &ctx),
                tab::NOTES => self.m_notes.render(f, content, &ctx),
                tab::FUN => self.m_fun.render(f, content, &ctx, &mut self.m_music),
                tab::SETTINGS => self.m_settings.render(f, content, &ctx),
                _ => {}
            }
        }

        // Transición entre módulos
        if self.slide > 0.01 && self.cfg.appearance.animations {
            let alpha = ease_out_cubic(self.slide) * 0.92;
            let buf = f.buffer_mut();
            self.fade_to_wallpaper(buf, content, alpha);
        }

        if let Some(a) = status_area {
            self.status(f, a);
        }
        self.overlays(f, area);

        // Cortina de arranque
        if self.boot < 1.0 {
            let t = 1.0 - ease_out_cubic(self.boot);
            let buf = f.buffer_mut();
            ui::dim_area(buf, area, t * 0.98);
        }

        // Pantalla de arranque con el análisis real (Esc la salta).
        if self.splash {
            self.splash_screen(f, area);
        }

        // En una consola de 16 colores, el último pase de la pantalla
        // reduce a índices lo que aún viaje en RGB (el fondo interpolado,
        // los gráficos): así la TTY dibuja exactamente esos colores. El
        // tema ya venía cuantizado y esta operación es idempotente.
        if crate::config::tty_palette() {
            let buf = f.buffer_mut();
            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    let c = &mut buf[(x, y)];
                    c.fg = crate::config::quantize16(c.fg);
                    c.bg = crate::config::quantize16(c.bg);
                }
            }
        }
    }

    /// Lienzo de arranque: logotipo, análisis en curso y pista de salto.
    fn splash_screen(&mut self, f: &mut Frame, area: Rect) {
        if area.width < 30 || area.height < 10 {
            return;
        }
        let t = self.theme.clone();
        // El fondo se aclara a la vez que el panel se retira.
        let out = self.boot_end.clamp(0.0, 1.0);
        {
            let buf = f.buffer_mut();
            ui::dim_area(buf, area, (1.0 - out) * 0.62);
        }

        let n_lines = self
            .boot_lines
            .len()
            .min(area.height.saturating_sub(8) as usize);
        let h = (7 + n_lines as u16).min(area.height.saturating_sub(2));
        let w = (area.width * 70 / 100).clamp(40, 68).min(area.width);
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(w)) / 2,
            y: area.y + (area.height.saturating_sub(h)) / 2,
            width: w,
            height: h,
        };

        let block = ui::panel_on(&t, &self.wp, rect, " ✦ arranque ", 0.97);
        let inner = block.inner(rect);
        f.render_widget(block, rect);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(
            Line::from(Span::styled(
                "N É B U L A",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ))
            .centered(),
        );
        lines.push(
            Line::from(Span::styled(
                format!("centro de mando · v{}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(t.fg_faint),
            ))
            .centered(),
        );
        lines.push(Line::from(""));

        for l in self.boot_lines.iter().take(n_lines) {
            lines.push(Line::from(vec![
                Span::styled("  ✓ ", Style::default().fg(t.ok)),
                Span::styled(l.clone(), Style::default().fg(t.fg_dim)),
            ]));
        }
        // Línea viva mientras el análisis sigue.
        if self.boot_end <= 0.0 {
            let dots = match ((self.clock.elapsed * 3.0) as usize) % 4 {
                0 => "·",
                1 => "··",
                2 => "···",
                _ => " ",
            };
            lines.push(Line::from(vec![
                Span::styled("  ▸ ", Style::default().fg(t.accent2)),
                Span::styled(
                    format!("analizando {dots}"),
                    Style::default().fg(t.fg_faint),
                ),
            ]));
        }
        f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);

        // Pista de salto al pie del panel.
        if inner.height > 0 {
            let hint = Line::from(Span::styled(
                "  Esc para saltar  ",
                Style::default().fg(t.fg_faint),
            ))
            .centered();
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

        // Retirada: el panel se apaga con lo que queda de análisis.
        if out > 0.0 {
            let buf = f.buffer_mut();
            ui::dim_area(buf, rect, out * 0.98);
        }
    }

    fn fade_to_wallpaper(&self, buf: &mut Buffer, area: Rect, alpha: f32) {
        let base_fallback = self.theme.bg;
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let base = self.wp.cell(x, y).unwrap_or(base_fallback);
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_fg(blend(cell.fg, base, alpha));
                    cell.set_bg(blend(cell.bg, base, alpha));
                }
            }
        }
    }

    fn header(&self, f: &mut Frame, area: Rect) {
        let t = &self.theme;
        let now = self.now;
        let time = if self.cfg.general.clock_24h {
            format!("{:02}:{:02}", now.hour(), now.minute())
        } else {
            let (h, am) = match now.hour() {
                0..=11 => (now.hour(), "AM"),
                12 => (12, "PM"),
                _ => (now.hour() - 12, "PM"),
            };
            format!("{h:02}:{:02} {am}", now.minute())
        };
        let seconds = if self.cfg.general.show_seconds {
            format!(":{:02}", now.second())
        } else {
            String::new()
        };

        let playing = self.music.running && self.music.state == "playing";
        let mut spans: Vec<Span> = vec![
            Span::styled(
                " NÉBULA ",
                Style::default()
                    .fg(t.bg)
                    .bg(t.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(
                ui::truncate(&self.sys.distro, 34),
                Style::default().fg(t.fg_dim),
            ),
            Span::styled(" · ", Style::default().fg(t.border)),
            Span::styled(self.sys.hostname.clone(), Style::default().fg(t.fg_faint)),
        ];
        // Lo que suena, a la izquierda de la cabecera (el control está en
        // el chip ♪ de la barra de estado).
        if playing && !self.music.title.is_empty() {
            spans.push(Span::styled("  ♪ ", Style::default().fg(t.accent)));
            spans.push(Span::styled(
                ui::truncate(&self.music.title, 30),
                Style::default().fg(t.accent2),
            ));
        }
        let left_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();

        let right: Vec<Span> = vec![
            Span::styled(
                format!("{} ", crate::modules::home::fecha(&now)),
                Style::default().fg(t.fg_faint),
            ),
            Span::styled(
                time.clone(),
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                seconds,
                Style::default().fg(t.accent2).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   ", Style::default()),
            Span::styled(
                format!("◈ {}", self.cfg.general.theme),
                Style::default().fg(t.fg_dim),
            ),
        ];
        let right_len: usize = right.iter().map(|s| s.content.chars().count()).sum();
        let pad = (area.width as usize).saturating_sub(left_len + right_len);
        spans.push(Span::styled(" ".repeat(pad), Style::default()));
        spans.extend(right);

        f.render_widget(ratatui::widgets::Paragraph::new(Line::from(spans)), area);
    }

    /// Botón ⟨/⟩ pegado al borde de la barra lateral: siempre visible.
    fn side_button(&self, f: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let t = &self.theme;
        let sym = if self.sidebar_open { "⟨" } else { "⟩" };
        let y = area.y + area.height / 2;
        let buf = f.buffer_mut();
        // Tira vertical sutil con el símbolo en el centro.
        for row in area.y..area.bottom() {
            let (s, col) = if row == y {
                (sym, t.accent)
            } else if row == y.saturating_sub(1) || row == y + 1 {
                ("·", t.fg_faint)
            } else {
                ("│", t.border)
            };
            if let Some(c) = buf.cell_mut((area.x, row)) {
                c.set_symbol(s);
                c.set_fg(col);
            }
        }
    }

    fn sidebar(&mut self, f: &mut Frame, area: Rect) {
        let t = self.theme.clone();
        // Fondo sólido mezclado: sin semibloques colándose (anti-cebra).
        let collapsed = area.width <= 6;
        let block = ui::slab_solid(&t, &self.wp, area, 0.90);
        f.render_widget(block, area);

        // resalte animado (la fila de la pestaña actual, con sus grupos)
        let sel_y = self
            .side_y
            .value
            .round()
            .clamp(0.0, (SIDE.len() - 1) as f32) as u16;
        let hl = Rect {
            x: area.x,
            y: area.y.saturating_add(sel_y),
            width: area.width,
            height: 1,
        };
        if hl.y < area.bottom() {
            let buf = f.buffer_mut();
            for x in hl.left()..hl.right() {
                if let Some(c) = buf.cell_mut((x, hl.y)) {
                    c.set_bg(t.sel_bg);
                }
            }
        }

        let badges: [String; tab::COUNT] = [
            String::new(), // inicio
            String::new(), // sistema
            String::new(), // red
            if self.pkgs.upgradable.is_empty() {
                String::new()
            } else {
                format!("{}", self.pkgs.upgradable.len())
            }, // paquetes
            if self.services.is_empty() {
                String::new()
            } else {
                format!(
                    "{}",
                    self.services.iter().filter(|s| s.sub == "running").count()
                )
            }, // servicios
            if self.docker.containers.is_empty() {
                String::new()
            } else {
                format!("{}", self.docker.containers.len())
            }, // contenedores
            if self.projects.is_empty() {
                String::new()
            } else {
                format!("{}", self.projects.len())
            }, // proyectos
            if self.git.repo.is_some() {
                "●".into()
            } else {
                String::new()
            }, // git
            String::new(), // desarrollo
            if self.music.running && self.music.state == "playing" {
                "▶".into()
            } else {
                String::new()
            }, // ocio
            String::new(), // notas
            String::new(), // ajustes
        ];

        let items: Vec<ListItem> = SIDE
            .iter()
            .map(|row| match row {
                // Cabecera de grupo: nunca es seleccionable.
                SideRow::Head(name) => {
                    if collapsed {
                        ListItem::new(Line::from(""))
                    } else {
                        ListItem::new(Line::from(vec![
                            Span::styled("── ", Style::default().fg(t.border)),
                            Span::styled(
                                name.to_uppercase(),
                                Style::default().fg(t.accent2).add_modifier(Modifier::BOLD),
                            ),
                        ]))
                    }
                }
                SideRow::Tab(n) => {
                    let (name, icon) = TABS[*n];
                    let active = *n == self.tab;
                    if collapsed {
                        // Sólo icono, centrado en la franja estrecha.
                        let mut spans = vec![Span::styled(
                            icon.to_string(),
                            Style::default().fg(if active { t.accent } else { t.fg_faint }),
                        )];
                        let b = &badges[*n];
                        if !b.is_empty() {
                            spans.push(Span::styled(
                                b.chars().next().unwrap_or(' ').to_string(),
                                Style::default().fg(t.accent2),
                            ));
                        }
                        return ListItem::new(Line::from(spans).centered());
                    }
                    let mut spans = vec![Span::styled(
                        format!(" {icon} "),
                        Style::default().fg(if active { t.accent } else { t.fg_faint }),
                    )];
                    spans.push(Span::styled(
                        format!("{:<13}", name),
                        Style::default()
                            .fg(if active { t.sel_fg } else { t.fg_dim })
                            .add_modifier(if active {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ));
                    let b = &badges[*n];
                    if !b.is_empty() {
                        spans.push(Span::styled(
                            b.clone(),
                            Style::default().fg(if active { t.sel_fg } else { t.accent2 }),
                        ));
                    }
                    ListItem::new(Line::from(spans))
                }
            })
            .collect();

        let lw = List::new(items).style(Style::default());
        self.sidebar_state.select(None);
        f.render_stateful_widget(lw, area, &mut self.sidebar_state);

        // Cursor del foco (Ctrl+E): filo y flecha en la fila elegida.
        // (Se pinta tras la lista, que lo taparía.)
        if self.side_focus && !collapsed {
            let row = area.y.saturating_add(self.side_sel as u16);
            if row < area.bottom() {
                let buf = f.buffer_mut();
                for x in area.left()..area.right() {
                    if let Some(c) = buf.cell_mut((x, row)) {
                        c.set_bg(t.accent);
                        c.set_fg(t.bg);
                    }
                }
                if let Some(c) = buf.cell_mut((area.x, row)) {
                    c.set_symbol("▸");
                    c.set_fg(t.bg);
                }
            }
        }

        // pie del panel lateral (sólo con la lista desplegada)
        let rows_n = SIDE.len() as u16;
        if !collapsed && area.height > rows_n + 2 {
            let y = area.y + rows_n + 1;
            if y < area.bottom() {
                f.render_widget(
                    ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                        " ──────────────── ",
                        Style::default().fg(t.border),
                    ))),
                    Rect {
                        x: area.x,
                        y,
                        width: area.width,
                        height: 1,
                    },
                );
            }
        }
    }

    fn status(&self, f: &mut Frame, area: Rect) {
        let t = &self.theme;
        let (name, icon) = TABS[self.tab];
        // Mini indicador ♪: siempre a la vista, en cualquier pestaña.
        // Su ancho es fijo (4 celdas) para que el clic sepa dónde caer.
        let (mgl, mcol) = if !self.music.running {
            ("·", t.fg_faint)
        } else if self.music.state == "playing" {
            ("▶", t.ok)
        } else if self.music.state == "paused" {
            ("‖", t.warn)
        } else {
            ("■", t.fg_dim)
        };
        let mut spans: Vec<Span> = vec![
            Span::styled(
                format!(" {icon} {name} "),
                Style::default()
                    .fg(t.bg)
                    .bg(t.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("♪ {mgl} "),
                Style::default().fg(mcol).add_modifier(Modifier::BOLD),
            ),
        ];
        if let Some((msg, _)) = &self.toast {
            spans.push(Span::styled(
                ui::truncate(msg, (area.width as usize).saturating_sub(52)),
                Style::default().fg(t.accent2),
            ));
        } else {
            spans.push(Span::styled(
                format!("cpu {:.0}% · ram {:.0}%", self.sys.cpu, mem_pct(&self.sys)),
                Style::default().fg(t.fg_faint),
            ));
        }
        let left_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();

        let mut right: Vec<Span> = Vec::new();
        // Chips: volumen · brillo · batería
        if self.av.muted {
            right.push(Span::styled(" vol ✕ ", Style::default().fg(t.fg_faint)));
        } else {
            right.push(Span::styled(
                format!(" vol {:.0}% ", self.av.volume * 100.0),
                Style::default().fg(if self.av.volume > 0.0 {
                    t.accent2
                } else {
                    t.fg_faint
                }),
            ));
        }
        if let Some(b) = self.av.brightness {
            right.push(Span::styled(
                format!(" luz {:.0}% ", b * 100.0),
                Style::default().fg(t.fg_dim),
            ));
        }
        if let Some(bat) = &self.av.battery {
            let mut txt = format!(" bat {:.0}%", bat.pct);
            if bat.charging {
                txt.push('+');
            } else if let Some(secs) = bat.secs {
                txt.push_str(&format!(" · {}h{:02}", secs / 3600, (secs % 3600) / 60));
            }
            let col = if bat.charging {
                t.ok
            } else if bat.pct <= 20.0 {
                t.warn
            } else {
                t.fg_dim
            };
            right.push(Span::styled(txt, Style::default().fg(col)));
        }
        right.push(Span::styled("  ", Style::default()));
        right.extend([
            Span::styled(" q ", Style::default().fg(t.accent)),
            Span::styled("salir  ", Style::default().fg(t.fg_dim)),
            Span::styled("? ", Style::default().fg(t.accent)),
            Span::styled("ayuda  ", Style::default().fg(t.fg_dim)),
            Span::styled("ctrl+k ", Style::default().fg(t.accent)),
            Span::styled("lanzador", Style::default().fg(t.fg_dim)),
        ]);
        let right_len: usize = right.iter().map(|s| s.content.chars().count()).sum();
        let pad = (area.width as usize).saturating_sub(left_len + right_len);
        spans.push(Span::styled(" ".repeat(pad), Style::default()));
        spans.extend(right);

        f.render_widget(ratatui::widgets::Paragraph::new(Line::from(spans)), area);
    }

    fn overlays(&mut self, f: &mut Frame, area: Rect) {
        match self.overlay {
            Overlay::None => (),
            Overlay::Launcher => {
                let w = (area.width * 72 / 100).clamp(48, 96);
                let h = (area.height * 55 / 100).clamp(12, 22);
                let rect = Rect {
                    x: area.x + (area.width.saturating_sub(w)) / 2,
                    y: area.y + (area.height.saturating_sub(h)) / 4,
                    width: w.min(area.width),
                    height: h.min(area.height),
                };
                self.dim_behind(f, area);
                self.m_launcher.render(f, rect, &ctx!(self));
            }
            Overlay::Help => {
                let w = (area.width * 74 / 100).clamp(52, 104);
                let h = (area.height * 70 / 100).clamp(14, 34);
                let rect = Rect {
                    x: area.x + (area.width.saturating_sub(w)) / 2,
                    y: area.y + (area.height.saturating_sub(h)) / 2,
                    width: w.min(area.width),
                    height: h.min(area.height),
                };
                self.dim_behind(f, area);
                Clear.render(rect, f.buffer_mut());
                self.m_help.render(f, rect, &ctx!(self));
            }
            Overlay::Confirm => {
                let w = (area.width * 62 / 100).clamp(44, 84);
                let h = 9.min(area.height);
                let rect = Rect {
                    x: area.x + (area.width.saturating_sub(w)) / 2,
                    y: area.y + (area.height.saturating_sub(h)) / 2,
                    width: w.min(area.width),
                    height: h,
                };
                self.dim_behind(f, area);
                Clear.render(rect, f.buffer_mut());
                let t = self.theme.clone();
                let msg = self
                    .confirm
                    .as_ref()
                    .map(|(m, _)| m.clone())
                    .unwrap_or_default();
                let block = ui::panel_on(&t, &self.wp, rect, " ⚠  Confirmar ", 0.97);
                let inner = block.inner(rect);
                f.render_widget(block, rect);
                let lines = vec![
                    Line::from(Span::styled(
                        ui::truncate(&msg, (inner.width as usize).saturating_sub(2)),
                        Style::default().fg(t.fg),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            "   [s] sí, adelante   ",
                            Style::default()
                                .fg(t.bg)
                                .bg(t.ok)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("     ", Style::default()),
                        Span::styled(
                            " [n] no, mejor no ",
                            Style::default().fg(t.fg_dim).bg(t.bg_elevated),
                        ),
                    ]),
                ];
                f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
            }
            Overlay::Output => {
                let w = (area.width * 86 / 100).clamp(56, 150);
                let h = (area.height * 78 / 100).clamp(14, 46);
                let rect = Rect {
                    x: area.x + (area.width.saturating_sub(w)) / 2,
                    y: area.y + (area.height.saturating_sub(h)) / 2,
                    width: w.min(area.width),
                    height: h.min(area.height),
                };
                self.dim_behind(f, area);
                let ctx = ctx!(self);
                if let Some(o) = self.output.as_mut() {
                    o.render(f, rect, &ctx);
                }
            }
            Overlay::Term => {
                let rect = term_rect(area);
                self.dim_behind(f, area);
                Clear.render(rect, f.buffer_mut());
                self.render_term(f, rect);
            }
        }
    }

    fn dim_behind(&self, f: &mut Frame, area: Rect) {
        let buf = f.buffer_mut();
        ui::dim_area(buf, area, 0.55);
    }
}

fn synth(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Ruta entre comillas simples para incrustarla en un `cd …` de shell.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Rectángulo del panel del terminal dentro del área total. Es el mismo
/// en el dibujo y en el ratón, para que las coordenadas casen.
fn term_rect(area: Rect) -> Rect {
    let w = (area.width * 92 / 100).clamp(48, 220);
    let h = (area.height * 84 / 100).clamp(12, 64);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w.min(area.width),
        height: h.min(area.height),
    }
}

/// Tamaño útil del terminal (columnas, filas) para la ventana actual:
/// el interior del panel menos el borde y el pie de atajos.
fn term_inner(size: (u16, u16)) -> (u16, u16) {
    let r = term_rect(Rect {
        x: 0,
        y: 0,
        width: size.0,
        height: size.1,
    });
    (
        r.width.saturating_sub(2).max(2),
        r.height.saturating_sub(3).max(2),
    )
}

fn mem_pct(s: &SysSnap) -> f32 {
    if s.mem_total > 0 {
        (s.mem_used as f64 / s.mem_total as f64 * 100.0) as f32
    } else {
        0.0
    }
}

/// Aviso legible del estado de cliamp tras un comando: la usuaria ve
/// «reproduciendo «Lofi»» en vez de un silencio sospechoso.
fn cliamp_note(status: &str, out: &str) -> String {
    let field = |name: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let state = field("State: ");
    let track = field("Track: ");
    let st = match state.as_str() {
        "playing" => "reproduciendo",
        "paused" => "en pausa",
        "stopped" | "idle" => "detenido",
        "" => return summarize("cliamp", true, out),
        other => other,
    };
    if track.is_empty() || track == "-" {
        format!("cliamp · {st}")
    } else {
        format!("cliamp · {st} «{}»", ui::truncate(&track, 34))
    }
}

/// Igual que `cliamp_note` pero para cmus (`cmus-remote -Q`).
fn cmus_note(status: &str, out: &str) -> String {
    let field = |name: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let raw = field("status ");
    let st = match raw.as_str() {
        "playing" => "reproduciendo",
        "paused" => "en pausa",
        "stopped" => "detenido",
        "" => return summarize("cmus", true, out),
        other => other,
    };
    let title = field("title ");
    if title.is_empty() {
        format!("cmus · {st}")
    } else {
        format!("cmus · {st} «{}»", ui::truncate(&title, 34))
    }
}

pub(crate) fn summarize(what: &str, ok: bool, out: &str) -> String {
    let first = out
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let tail = if out.lines().count() > 1 { "…" } else { "" };
    if ok {
        if first.is_empty() {
            format!("{what} · hecho ✦")
        } else {
            format!("{what} · {}", ui::truncate(&format!("{first}{tail}"), 60))
        }
    } else {
        format!(
            "{what} · {}",
            ui::truncate(if first.is_empty() { "error" } else { first }, 60)
        )
    }
}
