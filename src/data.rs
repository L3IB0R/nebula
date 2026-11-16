//! Datos de fondo: capturas de estado y hilos que no bloquean la interfaz.

use crate::app::summarize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::Duration;

// ── Utilidades de comandos ────────────────────────────────────────────────

pub fn which(prog: &str) -> Option<PathBuf> {
    if prog.contains('/') {
        let p = PathBuf::from(prog);
        return p.is_file().then_some(p);
    }
    // 1) $PATH del proceso…
    if let Ok(path) = std::env::var("PATH") {
        if let Some(p) = path
            .split(':')
            .filter(|d| !d.is_empty())
            .map(|d| PathBuf::from(d).join(prog))
            .find(|p| p.is_file())
        {
            return Some(p);
        }
    }
    // 2) …y por si acaso los directorios típicos de usuario (cliamp vive en
    //    ~/.local/bin y muchas terminales no lo traen en el PATH heredado).
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        for dir in [
            home.join(".local/bin"),
            home.join(".cargo/bin"),
            PathBuf::from("/usr/local/bin"),
        ] {
            let p = dir.join(prog);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// ¿Está el paquete dpkg `name` instalado? (`false` si no hay dpkg o no está)
///
/// Es el respaldo de [`which`] para binarios con nombre distinto al paquete
/// (p. ej. `cliamp` es un binario de Node, `cmus-remote` viene de `cmus`).
pub fn pkg_installed(name: &str) -> bool {
    run("dpkg-query", &["-W", "-f=${Status}", name])
        .map(|s| s.contains("install ok installed"))
        .unwrap_or(false)
}

/// ¿Se puede usar `prog` (binario en el PATH) **o** está instalado como
/// paquete dpkg `pkg`? Útil cuando binario y paquete no se llaman igual.
pub fn has(prog: &str, pkg: &str) -> bool {
    which(prog).is_some() || pkg_installed(pkg)
}

/// Ejecuta un comando y devuelve stdout (recortado). `None` si falla.
pub fn run(prog: &str, args: &[&str]) -> Option<String> {
    run_in(prog, args, None)
}

pub fn run_in(prog: &str, args: &[&str], cwd: Option<&Path>) -> Option<String> {
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    let mut child = cmd.spawn().ok()?;
    let mut out = String::new();
    if let Some(mut so) = child.stdout.take() {
        let _ = so.read_to_string(&mut out);
    }
    let _ = child.wait();
    Some(out)
}

/// Igual que `run` pero capturando también stderr (para errores de apt, etc).
pub fn run_full(prog: &str, args: &[&str]) -> (bool, String) {
    run_full_in(prog, args, None)
}

/// Igual que `run_full` pero con directorio de trabajo.
pub fn run_full_in(prog: &str, args: &[&str], cwd: Option<&Path>) -> (bool, String) {
    let mut cmd = Command::new(prog);
    cmd.args(args).stdin(Stdio::null());
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    match cmd.output() {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).to_string();
            let e = String::from_utf8_lossy(&o.stderr).to_string();
            if !e.trim().is_empty() {
                if !s.trim().is_empty() {
                    s.push('\n');
                }
                s.push_str(&e);
            }
            (o.status.success(), s)
        }
        Err(_) => (false, format!("no se pudo ejecutar {prog}")),
    }
}

/// Lanza un programa sin esperarlo (para abrir apps).
pub fn spawn_detached(prog: &str, args: &[&str], cwd: Option<&Path>) {
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    // Desacoplar de la terminal para que Nébula siga viva.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let _ = cmd.spawn();
}

/// ¿Hay con qué abrir una ventana de terminal? Es la misma detección que
/// usa `spawn_in_terminal`: `$TERMINAL` si está, o un emulador conocido.
/// Si no hay nada, los comandos interactivos van a la consola embebida.
pub fn has_terminal_emulator() -> bool {
    if !std::env::var("TERMINAL").unwrap_or_default().is_empty() {
        return true;
    }
    [
        "x-terminal-emulator",
        "xdg-terminal-exec",
        "ptyxis",
        "sensible-terminal",
        "xterm",
    ]
    .iter()
    .any(|t| which(t).is_some())
}

/// Lanza en un terminal emulador (para comandos interactivos).
///
/// Cada emulador tiene su sintaxis para ejecutar un comando (`-e`, `--`,
/// directa…), así que se detecta el binario real (siguiendo el enlace de
/// `/etc/alternatives`) y se arma la línea que le toca. Ej. ptyxis quiere
/// `ptyxis -d DIR -- programa argumentos`.
pub fn spawn_in_terminal(prog: &str, args: &[&str], cwd: Option<&Path>) {
    let mut term = std::env::var("TERMINAL").unwrap_or_default();
    if term.is_empty() {
        for t in [
            "x-terminal-emulator",
            "xdg-terminal-exec",
            "ptyxis",
            "sensible-terminal",
            "xterm",
        ] {
            if which(t).is_some() {
                term = t.into();
                break;
            }
        }
    }
    if term.is_empty() {
        spawn_detached(prog, args, cwd);
        return;
    }

    // Nombre real del emulador: /etc/alternatives/x-terminal-emulator → ptyxis.
    let base = std::fs::canonicalize(&term)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        .unwrap_or_else(|| {
            std::path::Path::new(&term)
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        });

    // Argumentos previos al comando, según el emulador.
    let mut argv: Vec<String> = Vec::new();
    if let Some(dir) = cwd.filter(|_| base.contains("ptyxis")) {
        argv.push("-d".into());
        argv.push(dir.to_string_lossy().into_owned());
    }
    match base.as_str() {
        b if b.contains("ptyxis") => argv.push("--".into()),
        "gnome-terminal" => {
            if let Some(c) = cwd {
                argv.push(format!("--working-directory={}", c.display()));
            }
            argv.push("--".into());
        }
        "wezterm" => {
            argv.push("start".into());
            argv.push("--".into());
        }
        "kitty" | "xdg-terminal-exec" => {} // comando directo, sin marca
        "sensible-terminal" => argv.push("--".into()), // reenvía a x-terminal-emulator
        // Forma clásica: xterm, alacritty, foot, urxvt, konsole…
        _ => argv.push("-e".into()),
    }
    argv.push(prog.to_string());
    argv.extend(args.iter().map(|s| s.to_string()));

    let refs: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
    spawn_detached(&term, &refs, cwd);
}

/// Lanza un programa en una **pseudo-terminal invisible** en segundo plano.
///
/// Hay reproductores (cmus) que exigen un TTY de verdad: sin él mueren con
/// un error interno de malloc. Aquí les damos uno propio, sin ventana, y
/// drenamos su salida para que no se atasque al llenarse el búfer. Nébula
/// los controla después con `cmus-remote`.
pub fn spawn_in_pty(prog: &str, args: &[&str]) -> bool {
    use std::os::unix::io::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;

    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return false;
    }
    // Tamaño útil: con 0×0 los TUI se dibujan en una esquina.
    let ws = libc::winsize {
        ws_row: 40,
        ws_col: 120,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        libc::ioctl(master, libc::TIOCSWINSZ, &ws as *const libc::winsize);
    }

    let owner = unsafe { std::fs::File::from_raw_fd(slave) };
    let clones = (owner.try_clone(), owner.try_clone(), owner.try_clone());
    let (Ok(s0), Ok(s1), Ok(s2)) = clones else {
        // Cierra también el maestro: no queremos descriptores huérfanos.
        unsafe {
            libc::close(master);
        }
        return false;
    };
    let tty_fd = owner.as_raw_fd();

    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdin(Stdio::from(s0))
        .stdout(Stdio::from(s1))
        .stderr(Stdio::from(s2));
    unsafe {
        cmd.pre_exec(move || {
            // Sesión propia: el hijo no recibe las señales de Nébula.
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            // Su terminal de control es el pty (así cmus cree tener TTY).
            libc::ioctl(tty_fd, libc::TIOCSCTTY, 0);
            Ok(())
        });
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => {
            unsafe {
                libc::close(master);
            }
            return false;
        }
    };
    drop(owner); // el hijo ya tiene sus copias: sin esto el pty no da EOF

    // Un hilo drena la salida (evita que el hijo se bloquee) y al llegar
    // al EOF recoge al hijo para que no quede como zombi.
    std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = [0u8; 4096];
        // El descriptor maestro vive aquí dentro hasta que el hijo muera.
        let mut master_file = unsafe { std::fs::File::from_raw_fd(master) };
        loop {
            match master_file.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
        drop(master_file);
        let _ = child.wait();
    });
    true
}

// ── Capturas de estado ────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct SysSnap {
    pub cpu: f32,
    pub per_cpu: Vec<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
    /// Historial de uso de memoria (0..=1), para la sparkline.
    pub mem_hist: Vec<u64>,
    pub swap_used: u64,
    pub swap_total: u64,
    pub load: [f64; 3],
    pub uptime: u64,
    pub disks: Vec<DiskInfo>,
    pub temps: Vec<(String, f32)>,
    pub procs: Vec<ProcInfo>,
    pub net: NetInfo,
    pub hostname: String,
    pub kernel: String,
    pub distro: String,
    pub cpu_model: String,
    pub cpu_freq: u64,
    pub gpu: String,
}

#[derive(Clone, Debug, Default)]
pub struct DiskInfo {
    pub mount: String,
    pub fs: String,
    pub total: u64,
    pub avail: u64,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct ProcInfo {
    pub pid: i32,
    pub name: String,
    pub cpu: f32,
    pub mem: u64,
    pub user: String,
    pub status: String,
    pub cmd: String,
}

#[derive(Clone, Debug, Default)]
pub struct NetInfo {
    pub up_bps: f64,
    pub down_bps: f64,
    pub total_up: u64,
    pub total_down: u64,
    pub ifaces: Vec<IfInfo>,
    pub history_down: Vec<u64>,
    pub history_up: Vec<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct IfInfo {
    pub name: String,
    pub ip: String,
    pub up: bool,
    pub kind: String,
}

#[derive(Clone, Debug, Default)]
pub struct MusicSnap {
    pub engine: String,
    pub running: bool,
    pub state: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub provider: String,
    pub position: u64,
    pub duration: u64,
    pub volume: f64,
    pub shuffle: bool,
    pub repeat: String,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectInfo {
    pub path: String,
    pub name: String,
    pub kinds: Vec<String>,
    pub branch: String,
    pub dirty: i32,
    pub last_commit: String,
    pub last_time: String,
    pub scripts: Vec<(String, String)>,
    pub has_git: bool,
}

#[derive(Clone, Debug, Default)]
pub struct GitRepo {
    pub branch: String,
    pub upstream: String,
    pub ahead: i32,
    pub behind: i32,
    pub staged: i32,
    pub unstaged: i32,
    pub untracked: i32,
    pub conflicts: i32,
    pub last_commit: String,
    pub last_time: String,
}

#[derive(Clone, Debug, Default)]
pub struct GitFile {
    pub status: String,
    pub path: String,
}

#[derive(Clone, Debug, Default)]
pub struct GitCommit {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub msg: String,
}

#[derive(Clone, Debug, Default)]
pub struct ServiceInfo {
    pub name: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub desc: String,
}

#[derive(Clone, Debug, Default)]
pub struct PkgSnap {
    pub upgradable: Vec<(String, String)>,
    pub installed: usize,
    pub flatpaks: Vec<(String, String)>,
    pub apt_ok: bool,
    pub last_run: String,
}

#[derive(Clone, Debug, Default)]
pub struct DockerSnap {
    pub available: bool,
    /// Motor detectado: "docker" o "podman" (vacío si no hay ninguno).
    pub engine: String,
    pub containers: Vec<DockerRow>,
    pub images: Vec<DockerRow>,
    pub server: String,
}

#[derive(Clone, Debug, Default)]
pub struct DockerRow {
    pub a: String,
    pub b: String,
    pub c: String,
    pub d: String,
}

/// Audio, brillo y batería (encuesta cada 2 s).
#[derive(Clone, Debug, Default)]
pub struct AvSnap {
    /// Volumen 0..=1.
    pub volume: f32,
    pub muted: bool,
    /// Brillo 0..=1, si hay backlight legible.
    pub brightness: Option<f32>,
    /// ¿Está brightnessctl instalado? (para poder subir/bajar)
    pub has_brightnessctl: bool,
    pub battery: Option<BatSnap>,
}

#[derive(Clone, Debug, Default)]
pub struct BatSnap {
    /// 0..=100
    pub pct: f32,
    pub charging: bool,
    /// Autonomía estimada en segundos, si hay datos.
    pub secs: Option<u64>,
}

/// Clima vía wttr.in (hilo aparte, caché de 10 min).
#[derive(Clone, Debug, Default)]
pub struct WeatherSnap {
    /// false = aún no hemos recibido respuesta.
    pub ok: bool,
    pub temp: i32,
    pub feels: i32,
    pub humidity: u32,
    /// km/h
    pub wind: f32,
    pub desc: String,
    /// Icono (☀ ⛅ ☁ 🌧 ⛈ ❄ 🌫 🌡)
    pub icon: String,
    pub place: String,
    /// Hora local del último éxito (HH:MM).
    pub at: String,
}

// Los mensajes viajan por canal: el tamaño extra se paga una sola vez al enviar.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum Msg {
    Sys(SysSnap),
    Music(MusicSnap),
    Vis(Vec<f32>),
    Projects(Vec<ProjectInfo>),
    Git {
        path: String,
        repo: Option<GitRepo>,
        files: Vec<GitFile>,
        log: Vec<GitCommit>,
        diff: String,
    },
    Services(Vec<ServiceInfo>),
    Pkgs(PkgSnap),
    Docker(DockerSnap),
    Net(NetSnap),
    Db(DbResult),
    Note(String),
    Av(AvSnap),
    Weather(WeatherSnap),
    /// Línea de progreso de la pantalla de arranque.
    Boot(String),
    /// El análisis de arranque terminó.
    BootEnd,
    /// Un comando de git terminó: recargar el estado.
    GitRefresh,
    /// Línea de salida de un comando del visor interno (`id` del visor).
    Out {
        id: u64,
        line: String,
    },
    /// El comando `id` del visor terminó (`ok` = salió bien).
    OutEnd {
        id: u64,
        ok: bool,
    },
    /// Resultados de la búsqueda de vídeo de Entretenimiento (yt-dlp).
    Search {
        token: u64,
        hits: Vec<Hit>,
        /// Vacío si todo fue bien; si no, el motivo para mostrarla.
        error: String,
    },
    /// Emisoras y géneros del menú nativo (`token` casa la respuesta con
    /// la consulta que la lanzó).
    Stations {
        token: u64,
        items: Vec<Station>,
        /// Vacío si todo fue bien; si no, el motivo para mostrarlo.
        error: String,
    },
}

/// Una fila de resultado de búsqueda de vídeo.
#[derive(Clone, Debug)]
pub struct Hit {
    /// Título tal cual se pasa al reproductor.
    pub title: String,
    /// Detalle visible (año, episodios, duración…).
    pub sub: String,
    /// Identificador (id de YouTube).
    pub id: String,
}

/// Una fila de `ss`: qué protocolo, en qué estado, en qué puerto y qué
/// proceso la mantiene.
#[derive(Clone, Debug, Default)]
pub struct Sock {
    pub proto: String,
    /// `LISTEN`, `UNCONN`, `ESTAB`, `TIME-WAIT`…
    pub state: String,
    /// Dirección local (ip:puerto).
    pub addr: String,
    pub proc: String,
}

#[derive(Clone, Debug, Default)]
pub struct NetSnap {
    pub devices: Vec<IfInfo>,
    pub conns: Vec<(String, String, String)>, // nombre, tipo, estado
    /// Sockets: escuchando, establecidos y el resto (para los filtros).
    pub socks: Vec<Sock>,
    pub ss_ok: bool,
}

impl Sock {
    /// ¿Es un socket a la escucha? (`ss -l`: tcp LISTEN y udp UNCONN)
    pub fn listening(&self) -> bool {
        self.state == "LISTEN" || self.state == "UNCONN"
    }

    /// ¿Hay una conexión viva por él?
    pub fn established(&self) -> bool {
        self.state == "ESTAB"
    }
}

// ── Hilo de sistema ──────────────────────────────────────────────────────

pub fn spawn_sampler(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        use sysinfo::{Components, Disks, Networks, ProcessRefreshKind, RefreshKind, System};

        let mut sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(sysinfo::CpuRefreshKind::everything())
                .with_memory(sysinfo::MemoryRefreshKind::everything())
                .with_processes(ProcessRefreshKind::everything()),
        )
        .expect("no se pudo leer el sistema");
        let mut disks = Disks::new_with_refreshed_list().ok();
        let mut comps = Components::new_with_refreshed_list().ok();
        let mut nets = Networks::new_with_refreshed_list().ok();

        let hostname = System::host_name().unwrap_or_default();
        let kernel = System::kernel_version().unwrap_or_default();
        let distro = distro_name();
        let cpu_model = sys
            .cpus()
            .first()
            .map(|c| c.name().to_string())
            .unwrap_or_default();

        let mut last_net = (0u64, 0u64);
        let mut last_t = std::time::Instant::now();
        let mut hist_down: Vec<u64> = Vec::new();
        let mut hist_up: Vec<u64> = Vec::new();
        let mut hist_mem: Vec<u64> = Vec::new();

        loop {
            sys.refresh_memory();
            sys.refresh_cpu_usage();
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            if let Some(d) = disks.as_mut() {
                d.refresh(false);
            }
            if let Some(c) = comps.as_mut() {
                c.refresh(false);
            }
            if let Some(n) = nets.as_mut() {
                n.refresh(true);
            }

            // Red: derivamos el caudal de los totales.
            let mut down = 0u64;
            let mut up = 0u64;
            let mut ifaces = Vec::new();
            if let Some(n) = nets.as_ref() {
                for (name, nd) in n {
                    down += nd.total_received();
                    up += nd.total_transmitted();
                    let name = name.to_string_lossy().to_string();
                    if name == "lo" {
                        continue;
                    }
                    ifaces.push(IfInfo {
                        name,
                        ip: String::new(),
                        up: nd.total_received() > 0 || nd.total_transmitted() > 0,
                        kind: "ethernet".into(),
                    });
                }
            }
            let now = std::time::Instant::now();
            let dt = now.duration_since(last_t).as_secs_f64().max(0.001);
            let d_down = down.saturating_sub(last_net.0) as f64 / dt;
            let d_up = up.saturating_sub(last_net.1) as f64 / dt;
            last_net = (down, up);
            last_t = now;

            hist_down.push(d_down as u64);
            hist_up.push(d_up as u64);
            if hist_down.len() > 90 {
                hist_down.remove(0);
                hist_up.remove(0);
            }

            // Memoria: porcentaje 0..=100 para la sparkline.
            let mem_pct = if sys.total_memory() > 0 {
                ((sys.total_memory() - sys.available_memory()) as f64 * 100.0
                    / sys.total_memory() as f64) as u64
            } else {
                0
            };
            hist_mem.push(mem_pct.clamp(0, 100));
            if hist_mem.len() > 90 {
                hist_mem.remove(0);
            }

            let temps: Vec<(String, f32)> = comps
                .as_ref()
                .map(|c| {
                    c.iter()
                        .filter_map(|c| c.temperature().map(|t| (c.label().to_string(), t)))
                        .collect()
                })
                .unwrap_or_default();

            let load = System::load_average()
                .map(|l| [l.one, l.five, l.fifteen])
                .unwrap_or([0.0, 0.0, 0.0]);

            let users = sysinfo::Users::new_with_refreshed_list().ok();
            let mut procs: Vec<ProcInfo> = sys
                .processes()
                .values()
                .map(|p| ProcInfo {
                    pid: p.pid().as_u32() as i32,
                    name: p.name().to_string_lossy().to_string(),
                    cpu: p.cpu_usage(),
                    mem: p.memory(),
                    user: p
                        .user_id()
                        .and_then(|u| users.as_ref().and_then(|us| us.get_user_by_id(u)))
                        .map(|u| u.name().to_string())
                        .unwrap_or_else(|| "-".into()),
                    status: format!("{:?}", p.status()),
                    cmd: p
                        .cmd()
                        .iter()
                        .map(|s| s.to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                        .join(" "),
                })
                .collect();
            procs.sort_by(|a, b| {
                b.cpu
                    .partial_cmp(&a.cpu)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let snap = SysSnap {
                cpu: sys.global_cpu_usage(),
                per_cpu: sys.cpus().iter().map(|c| c.usage()).collect(),
                mem_used: sys.total_memory().saturating_sub(sys.available_memory()),
                mem_total: sys.total_memory(),
                mem_hist: hist_mem.clone(),
                swap_used: sys.used_swap(),
                swap_total: sys.total_swap(),
                load,
                uptime: System::uptime().unwrap_or(0),
                disks: disks
                    .as_ref()
                    .map(|d| {
                        d.iter()
                            .map(|d| DiskInfo {
                                mount: d.mount_point().to_string_lossy().to_string(),
                                fs: d.file_system().to_string_lossy().to_string(),
                                total: d.total_space(),
                                avail: d.available_space(),
                                name: d.name().to_string_lossy().to_string(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                temps,
                procs: procs.into_iter().take(60).collect(),
                net: NetInfo {
                    up_bps: d_up,
                    down_bps: d_down,
                    total_up: up,
                    total_down: down,
                    ifaces,
                    history_down: hist_down.clone(),
                    history_up: hist_up.clone(),
                },
                hostname: hostname.clone(),
                kernel: kernel.clone(),
                distro: distro.clone(),
                cpu_model: cpu_model.clone(),
                cpu_freq: sys.cpus().first().map(|c| c.frequency()).unwrap_or(0),
                gpu: gpu_name(),
            };

            if tx.send(Msg::Sys(snap)).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(800));
        }
    });
}

fn distro_name() -> String {
    if let Ok(s) = std::fs::read_to_string("/etc/os-release") {
        for line in s.lines() {
            if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
                return v.trim_matches('"').to_string();
            }
        }
    }
    String::new()
}

fn gpu_name() -> String {
    let out = run(
        "sh",
        &[
            "-c",
            "lspci 2>/dev/null | grep -i 'vga\\|3d\\|display' | head -3",
        ],
    )
    .unwrap_or_default();
    out.lines()
        .map(|l| {
            l.split(':')
                .nth(1)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| l.to_string())
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

// ── Hilo de música ───────────────────────────────────────────────────────

pub fn spawn_music(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        loop {
            let snap = read_music();
            if tx.send(Msg::Music(snap.clone())).is_err() {
                break;
            }
            // Visualizador por streaming si cliamp lo permite.
            if snap.engine == "cliamp" && snap.running && snap.state == "playing" {
                stream_vis(&tx, &snap);
            }
            std::thread::sleep(Duration::from_millis(1200));
        }
    });
}

fn stream_vis(tx: &Sender<Msg>, _snap: &MusicSnap) {
    use std::io::BufRead;
    let Some(cliamp) = which("cliamp") else {
        return;
    };
    let child = Command::new(cliamp)
        .args(["visstream"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return };
    let Some(out) = child.stdout.take() else {
        return;
    };
    let reader = std::io::BufReader::new(out);
    let mut frames = 0;
    for line in reader.lines().map_while(Result::ok) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            let bands: Vec<f32> = if let Some(arr) = v.as_array() {
                arr.iter()
                    .filter_map(|x| x.as_f64())
                    .map(|x| x as f32)
                    .collect()
            } else if let Some(arr) = v.get("bands").and_then(|b| b.as_array()) {
                arr.iter()
                    .filter_map(|x| x.as_f64())
                    .map(|x| x as f32)
                    .collect()
            } else {
                continue;
            };
            if bands.is_empty() {
                continue;
            }
            if tx.send(Msg::Vis(bands)).is_err() {
                break;
            }
            frames += 1;
            if frames > 400 {
                break;
            }
        }
    }
    let _ = child.kill();
}

fn read_music() -> MusicSnap {
    let mut snap = MusicSnap::default();
    if let Some(cliamp) = which("cliamp") {
        if run(&cliamp.to_string_lossy(), &["status"]).is_some() || cliamp_status_alive() {
            if let Some(out) = run(&cliamp.to_string_lossy(), &["status", "--json"]) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) {
                    snap.engine = "cliamp".into();
                    snap.running = true;
                    pick(&v, &["state", "status", "playback_state"], &mut snap.state);
                    if snap.state.is_empty() {
                        snap.state = "playing".into();
                    }
                    pick(&v, &["title", "track_title", "name"], &mut snap.title);
                    pick(&v, &["artist", "track_artist"], &mut snap.artist);
                    pick(&v, &["album", "track_album"], &mut snap.album);
                    pick(&v, &["provider", "source"], &mut snap.provider);
                    if let Some(t) = num(&v, &["position", "elapsed", "seek_position"]) {
                        snap.position = t as u64;
                    }
                    if let Some(t) = num(&v, &["duration", "total", "length"]) {
                        snap.duration = t as u64;
                    }
                    if let Some(t) = num(&v, &["volume", "vol"]) {
                        snap.volume = t;
                    }
                    if let Some(b) = v.get("shuffle").and_then(|x| x.as_bool()) {
                        snap.shuffle = b;
                    }
                    pick(&v, &["repeat"], &mut snap.repeat);
                    // Algunos formatos anidan la pista.
                    if snap.title.is_empty() {
                        if let Some(t) = v.get("track").or(v.get("current")) {
                            pick(t, &["title", "name"], &mut snap.title);
                            pick(t, &["artist", "creator"], &mut snap.artist);
                            pick(t, &["album"], &mut snap.album);
                            if let Some(d) = num(t, &["duration"]) {
                                snap.duration = d as u64;
                            }
                        }
                    }
                    if snap.title.is_empty() {
                        snap.title = "(sin reproducción)".into();
                    }
                    return snap;
                }
            }
        }
    }
    if which("cmus-remote").is_some() {
        // Sólo cuenta si cmus contesta de verdad: `run` devuelve salida
        // aunque el proceso falle, y con eso basta para creer que corre.
        let (ok, out) = run_full("cmus-remote", &["-Q"]);
        if ok && out.lines().any(|l| l.starts_with("status ")) {
            snap.engine = "cmus".into();
            snap.running = true;
            for line in out.lines() {
                if let Some(v) = line.strip_prefix("status ") {
                    snap.state = v.trim().into();
                } else if let Some(v) = line.strip_prefix("title ") {
                    snap.title = v.trim().into();
                } else if let Some(v) = line.strip_prefix("artist ") {
                    snap.artist = v.trim().into();
                } else if let Some(v) = line.strip_prefix("album ") {
                    snap.album = v.trim().into();
                } else if let Some(v) = line.strip_prefix("position ") {
                    snap.position = v.trim().parse().unwrap_or(0);
                } else if let Some(v) = line.strip_prefix("duration ") {
                    snap.duration = v.trim().parse().unwrap_or(0);
                } else if let Some(v) = line.strip_prefix("file ") {
                    if snap.title.is_empty() {
                        snap.title = v.rsplit('/').next().unwrap_or("").to_string();
                    }
                }
            }
            if snap.title.is_empty() {
                snap.title = "(sin reproducción)".into();
            }
            return snap;
        }
    }
    snap.title = String::new();
    snap
}

fn cliamp_status_alive() -> bool {
    let sock = crate::config::home().join(".config/cliamp/cliamp.sock");
    sock.exists()
}

fn pick(v: &serde_json::Value, keys: &[&str], out: &mut String) {
    if !out.is_empty() {
        return;
    }
    for k in keys {
        if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
            if !s.is_empty() {
                *out = s.to_string();
                return;
            }
        }
    }
}

fn num(v: &serde_json::Value, keys: &[&str]) -> Option<f64> {
    for k in keys {
        if let Some(x) = v.get(*k) {
            if let Some(n) = x.as_f64() {
                return Some(n);
            }
            if let Some(n) = x.as_str().and_then(|s| s.parse::<f64>().ok()) {
                return Some(n);
            }
            // "mm:ss"
            if let Some(s) = x.as_str() {
                let parts: Vec<&str> = s.split(':').collect();
                if parts.len() >= 2 {
                    let m: f64 = parts[0].parse().ok()?;
                    let sec: f64 = parts[1].parse().ok()?;
                    return Some(m * 60.0 + sec);
                }
            }
        }
    }
    None
}

// ── Escaneo de proyectos ─────────────────────────────────────────────────

pub fn scan_projects(dirs: &[String], max: usize) -> Vec<ProjectInfo> {
    let mut out = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    for d in dirs {
        let root = crate::config::expand(d);
        if !root.is_dir() {
            continue;
        }
        walk(&root, 0, 4, max, &mut seen, &mut out);
        if out.len() >= max {
            break;
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.path.len()));
    out.truncate(max);
    out
}

fn walk(
    dir: &Path,
    depth: u32,
    max_depth: u32,
    max: usize,
    seen: &mut Vec<PathBuf>,
    out: &mut Vec<ProjectInfo>,
) {
    if out.len() >= max || depth > max_depth {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if name.starts_with('.') && name != ".config" {
            continue;
        }
        if matches!(
            name.as_str(),
            "node_modules" | "target" | "build" | "dist" | "__pycache__" | ".cache"
        ) {
            continue;
        }
        let has_marker = p.join(".git").exists()
            || p.join("package.json").exists()
            || p.join("schema.prisma").exists()
            || p.join("Cargo.toml").exists()
            || p.join("pyproject.toml").exists()
            || p.join("go.mod").exists()
            || p.join("index.html").exists()
            || p.join("composer.json").exists()
            || p.join("Gemfile").exists();
        if has_marker && !seen.contains(&p) {
            seen.push(p.clone());
            out.push(describe(&p));
            if out.len() >= max {
                return;
            }
        }
        subdirs.push(p);
    }
    for s in subdirs {
        walk(&s, depth + 1, max_depth, max, seen, out);
    }
}

fn describe(path: &Path) -> ProjectInfo {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut kinds = Vec::new();
    let mut scripts = Vec::new();

    if let Ok(s) = std::fs::read_to_string(path.join("package.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            let deps = v.get("dependencies").cloned().unwrap_or_default();
            let dev = v.get("devDependencies").cloned().unwrap_or_default();
            let has = |k: &str| deps.get(k).is_some() || dev.get(k).is_some();
            if has("react") {
                kinds.push("react".into());
            }
            if has("next") {
                kinds.push("next".into());
            }
            if has("vite") {
                kinds.push("vite".into());
            }
            if has("typescript") {
                kinds.push("ts".into());
            }
            if let Some(sc) = v.get("scripts").and_then(|x| x.as_object()) {
                for (k, val) in sc {
                    scripts.push((k.clone(), val.as_str().unwrap_or("").to_string()));
                }
            }
        }
    }
    if path.join("schema.prisma").exists() {
        kinds.push("prisma".into());
    }
    if path.join("Cargo.toml").exists() {
        kinds.push("rust".into());
    }
    if path.join("pyproject.toml").exists() || path.join("requirements.txt").exists() {
        kinds.push("python".into());
    }
    if path.join("go.mod").exists() {
        kinds.push("go".into());
    }

    let has_git = path.join(".git").exists();
    let (branch, last_commit, last_time) = if has_git && which("git").is_some() {
        let b = run_in("git", &["rev-parse", "--abbrev-ref", "HEAD"], Some(path))
            .unwrap_or_default()
            .trim()
            .to_string();
        let log = run_in("git", &["log", "-1", "--format=%s|%cr"], Some(path)).unwrap_or_default();
        let mut it = log.splitn(2, '|');
        let c = it.next().unwrap_or("").to_string();
        let t = it.next().unwrap_or("").to_string();
        (b, c, t)
    } else {
        (String::new(), String::new(), String::new())
    };

    let dirty = if has_git && which("git").is_some() {
        run_in("git", &["status", "--porcelain"], Some(path))
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count() as i32
    } else {
        0
    };

    ProjectInfo {
        path: path.to_string_lossy().to_string(),
        name,
        kinds,
        branch,
        dirty,
        last_commit,
        last_time,
        scripts,
        has_git,
    }
}

pub fn spawn_projects(tx: Sender<Msg>, dirs: Vec<String>, max: usize) {
    std::thread::spawn(move || {
        let list = scan_projects(&dirs, max);
        let _ = tx.send(Msg::Projects(list));
    });
}

// ── Git ──────────────────────────────────────────────────────────────────

pub fn spawn_git(tx: Sender<Msg>, path: String) {
    std::thread::spawn(move || {
        let p = PathBuf::from(&path);
        if which("git").is_none() {
            let _ = tx.send(Msg::Note(
                "git no está instalado. Instálalo con: sudo apt install git".into(),
            ));
            let _ = tx.send(Msg::Git {
                path,
                repo: None,
                files: vec![],
                log: vec![],
                diff: String::new(),
            });
            return;
        }
        let run_g = |args: &[&str]| run_in("git", args, Some(&p)).unwrap_or_default();

        let branch = run_g(&["rev-parse", "--abbrev-ref", "HEAD"])
            .trim()
            .to_string();
        let upstream = run_g(&["rev-parse", "--abbrev-symbol", "--abbrev-ref", "HEAD@{u}"])
            .trim()
            .to_string();
        let (upstream, ahead, behind) = if upstream.is_empty() {
            (String::new(), 0, 0)
        } else {
            let ab = run_g(&["rev-list", "--left-right", "--count", "HEAD...@{u}"]);
            let mut it = ab.split_whitespace();
            let a: i32 = it.next().unwrap_or("0").parse().unwrap_or(0);
            let b: i32 = it.next().unwrap_or("0").parse().unwrap_or(0);
            (upstream, a, b)
        };

        let mut staged = 0;
        let mut unstaged = 0;
        let mut untracked = 0;
        let mut conflicts = 0;
        let mut files = Vec::new();
        for line in run_g(&["status", "--porcelain"]).lines() {
            if line.len() < 3 {
                continue;
            }
            let x = line.chars().next().unwrap_or(' ');
            let y = line.chars().nth(1).unwrap_or(' ');
            let path = line[3..].trim().to_string();
            if x == 'U' || y == 'U' || (x == 'A' && y == 'A') {
                conflicts += 1;
            }
            if x != ' ' && x != '?' {
                staged += 1;
            }
            if y != ' ' && y != '?' {
                unstaged += 1;
            }
            if x == '?' && y == '?' {
                untracked += 1;
            }
            files.push(GitFile {
                status: format!("{x}{y}"),
                path,
            });
        }

        let log_line = run_g(&["log", "-1", "--format=%h|%an|%cr|%s"]);
        let mut it = log_line.splitn(4, '|');
        let last = GitCommit {
            hash: it.next().unwrap_or("").to_string(),
            author: it.next().unwrap_or("").to_string(),
            date: it.next().unwrap_or("").to_string(),
            msg: it.next().unwrap_or("").to_string(),
        };

        let mut log = Vec::new();
        for l in run_g(&["log", "-25", "--format=%h|%an|%cr|%s"]).lines() {
            let mut it = l.splitn(4, '|');
            log.push(GitCommit {
                hash: it.next().unwrap_or("").to_string(),
                author: it.next().unwrap_or("").to_string(),
                date: it.next().unwrap_or("").to_string(),
                msg: it.next().unwrap_or("").to_string(),
            });
        }

        let diff = run_g(&["diff", "--stat"]);
        let repo = GitRepo {
            branch,
            upstream,
            ahead,
            behind,
            staged,
            unstaged,
            untracked,
            conflicts,
            last_commit: last.msg,
            last_time: last.date,
        };

        let _ = tx.send(Msg::Git {
            path,
            repo: Some(repo),
            files,
            log,
            diff,
        });
    });
}

// ── Servicios, paquetes, docker, red ──────────────────────────────────────

pub fn spawn_services(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(s) = run(
            "systemctl",
            &[
                "list-units",
                "--type=service",
                "--all",
                "--no-pager",
                "--no-legend",
                "--plain",
            ],
        ) {
            for line in s.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 4 {
                    continue;
                }
                out.push(ServiceInfo {
                    name: parts[0].trim_end_matches('.').to_string(),
                    load: parts[1].to_string(),
                    active: parts[2].to_string(),
                    sub: parts[3].to_string(),
                    desc: parts[4..].join(" "),
                });
            }
        }
        out.sort_by(|a, b| {
            let rank = |s: &str| if s == "running" { 0 } else { 1 };
            rank(&a.active)
                .cmp(&rank(&b.active))
                .then(a.name.cmp(&b.name))
        });
        let _ = tx.send(Msg::Services(out));
    });
}

pub fn spawn_packages(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let snap = build_pkgs();
        let _ = tx.send(Msg::Pkgs(snap));
    });
}

/// Snapshot de paquetes (apt + flatpak). También lo usa el arranque.
pub fn build_pkgs() -> PkgSnap {
    let mut snap = PkgSnap::default();
    if which("apt").is_some() {
        snap.apt_ok = true;
        if let Some(out) = run("apt", &["list", "--upgradable"]) {
            for line in out.lines().skip(1) {
                if let Some((pkg, rest)) = line.split_once('/') {
                    let ver = rest
                        .split_whitespace()
                        .nth(1)
                        .map(|s| s.trim_end_matches(',').to_string())
                        .unwrap_or_default();
                    snap.upgradable.push((pkg.to_string(), ver));
                }
            }
        }
        if let Some(out) = run("dpkg-query", &["-W", "-f=${Package}\\n"]) {
            snap.installed = out.lines().filter(|l| !l.is_empty()).count();
        }
    }
    if let Some(flatpak) = which("flatpak") {
        if let Some(out) = run(
            &flatpak.to_string_lossy(),
            &["list", "--app", "--columns=application,version"],
        ) {
            for line in out.lines() {
                let mut it = line.splitn(2, '\t');
                let a = it.next().unwrap_or("").to_string();
                let b = it.next().unwrap_or("").to_string();
                if !a.is_empty() {
                    snap.flatpaks.push((a, b));
                }
            }
        }
    }
    snap.last_run = chrono::Local::now().format("%H:%M:%S").to_string();
    snap
}

pub fn spawn_docker(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let mut snap = DockerSnap::default();
        // Vale docker o podman: el panel promete que "también vale podman".
        let engine = if which("docker").is_some() {
            "docker"
        } else if which("podman").is_some() {
            "podman"
        } else {
            let _ = tx.send(Msg::Docker(snap));
            return;
        };
        snap.available = true;
        snap.engine = engine.into();
        if let Some(out) = run(
            engine,
            &[
                "ps",
                "-a",
                "--format",
                "{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}",
            ],
        ) {
            for line in out.lines() {
                let mut it = line.splitn(4, '\t');
                snap.containers.push(DockerRow {
                    a: it.next().unwrap_or("").into(),
                    b: it.next().unwrap_or("").into(),
                    c: it.next().unwrap_or("").into(),
                    d: it.next().unwrap_or("").into(),
                });
            }
        }
        if let Some(out) = run(
            engine,
            &[
                "images",
                "--format",
                "{{.Repository}}:{{.Tag}}\t{{.Size}}\t{{.CreatedSince}}",
            ],
        ) {
            for line in out.lines() {
                let mut it = line.splitn(3, '\t');
                snap.images.push(DockerRow {
                    a: it.next().unwrap_or("").into(),
                    b: it.next().unwrap_or("").into(),
                    c: it.next().unwrap_or("").into(),
                    d: String::new(),
                });
            }
        }
        if let Some(out) = run(engine, &["version", "--format", "{{.Server.Version}}"]) {
            snap.server = out.trim().to_string();
        }
        let _ = tx.send(Msg::Docker(snap));
    });
}

pub fn spawn_net(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let _ = tx.send(Msg::Net(build_net()));
    });
}

/// Snapshot de red. También lo usa el análisis de arranque.
pub fn build_net() -> NetSnap {
    let mut snap = NetSnap::default();
    if let Some(out) = run(
        "nmcli",
        &["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "dev", "status"],
    ) {
        for line in out.lines() {
            let p: Vec<&str> = line.split(':').collect();
            if p.len() >= 4 {
                snap.devices.push(IfInfo {
                    name: p[0].into(),
                    kind: p[1].into(),
                    up: p[2] == "connected",
                    ip: p[3].into(),
                });
            }
        }
    }
    if let Some(out) = run(
        "nmcli",
        &["-t", "-f", "NAME,TYPE,DEVICE", "connection", "show"],
    ) {
        for line in out.lines() {
            let p: Vec<&str> = line.split(':').collect();
            if p.len() >= 3 {
                snap.conns.push((p[0].into(), p[1].into(), p[2].into()));
            }
        }
    }
    if let Some(out) = run("ss", &["-tanup"]) {
        snap.ss_ok = true;
        for line in out.lines().skip(1) {
            let p: Vec<&str> = line.split_whitespace().collect();
            if p.len() < 5 {
                continue;
            }
            // Con protocols mixtos la primera columna es «Netid»; sin ella
            // el estado viene el primero. Se detecta para no leer torcido.
            let netid = matches!(p[0], "tcp" | "udp" | "udplite" | "raw" | "unix");
            let (proto, state, addr, proc) = if netid {
                (
                    p[0],
                    p[1],
                    p.get(4).copied().unwrap_or(""),
                    p.get(6..).map(|s| s.join(" ")).unwrap_or_default(),
                )
            } else {
                (
                    "",
                    p[0],
                    p.get(3).copied().unwrap_or(""),
                    p.get(5..).map(|s| s.join(" ")).unwrap_or_default(),
                )
            };
            snap.socks.push(Sock {
                proto: proto.into(),
                state: state.into(),
                addr: addr.into(),
                proc,
            });
        }
    }
    snap
}

// ── Arranque ─────────────────────────────────────────────────────────────

/// Análisis real de arranque: va mandando líneas de progreso y, de paso,
/// alimenta las cachés de herramientas, paquetes y red para que la primera
/// visita a cada pestaña sea inmediata.
pub fn spawn_boot(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let step_ms = 260;

        // 1) Configuración.
        let cfg_path = crate::config::config_path();
        let cfg_txt = if cfg_path.is_file() {
            "configuración leída".to_string()
        } else {
            "configuración por defecto".to_string()
        };
        let _ = tx.send(Msg::Boot(cfg_txt));
        std::thread::sleep(std::time::Duration::from_millis(step_ms));

        // 2) Herramientas: calienta la caché de `resources`.
        let total = crate::resources::total();
        let have = crate::resources::flags().iter().filter(|f| **f).count();
        let _ = tx.send(Msg::Boot(format!(
            "herramientas · {have}/{total} presentes"
        )));
        std::thread::sleep(std::time::Duration::from_millis(step_ms));

        // 3) Paquetes apt: alimenta la pestaña Paquetes.
        let pkgs = build_pkgs();
        let ups = pkgs.upgradable.len();
        let _ = tx.send(Msg::Pkgs(pkgs));
        let _ = tx.send(Msg::Boot(format!("paquetes · {ups} actualizables")));
        std::thread::sleep(std::time::Duration::from_millis(step_ms));

        // 4) Red: alimenta la pestaña Red.
        let net = build_net();
        let ifs = net.devices.len();
        let _ = tx.send(Msg::Net(net));
        let _ = tx.send(Msg::Boot(format!("red · {ifs} dispositivos")));
        std::thread::sleep(std::time::Duration::from_millis(step_ms));

        // 5) Sistema.
        let _ = tx.send(Msg::Boot(format!("sistema · {}", distro_name().trim())));
        std::thread::sleep(std::time::Duration::from_millis(step_ms));

        let _ = tx.send(Msg::Boot("listo ✦".into()));
        let _ = tx.send(Msg::BootEnd);
    });
}

// ── Audio · brillo · batería ─────────────────────────────────────────────

/// Evita lanzar varios `pkexec` de brillo a la vez (el polkit pediría clave).
static BRIGHT_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn read_trim(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

fn backlight_paths() -> Option<(String, String)> {
    let dir = std::path::Path::new("/sys/class/backlight");
    let entry = std::fs::read_dir(dir).ok()?.flatten().next()?;
    let base = entry.path();
    let cur = read_trim(&base.join("brightness").to_string_lossy())?;
    let max = read_trim(&base.join("max_brightness").to_string_lossy())?;
    Some((cur, max))
}

fn battery_snap() -> Option<BatSnap> {
    let dir = std::path::Path::new("/sys/class/power_supply");
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if !p.join("capacity").exists() {
            continue;
        }
        let pct: f32 = read_trim(&p.join("capacity").to_string_lossy())?
            .parse()
            .unwrap_or(0.0);
        let status = read_trim(&p.join("status").to_string_lossy()).unwrap_or_default();
        let charging = matches!(status.as_str(), "Charging" | "Full");
        // Estimación: carga restante (µAh) / consumo (µA) → horas.
        let secs = match (
            read_trim(&p.join("charge_now").to_string_lossy()),
            read_trim(&p.join("current_now").to_string_lossy()),
        ) {
            (Some(now), Some(cur)) if !charging => {
                let (now, cur): (f64, f64) =
                    (now.parse().unwrap_or(0.0), cur.parse().unwrap_or(0.0));
                if cur > 1.0 && now > 1.0 {
                    Some((now / cur * 3600.0) as u64)
                } else {
                    None
                }
            }
            _ => None,
        };
        return Some(BatSnap {
            pct,
            charging,
            secs,
        });
    }
    None
}

fn av_snap() -> AvSnap {
    let mut av = AvSnap::default();
    if let Some(out) = run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]) {
        if let Some(rest) = out.strip_prefix("Volume: ") {
            let mut it = rest.split_whitespace();
            av.volume = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
            av.muted = rest.contains("[MUTED]");
        }
    }
    if let Some((cur, max)) = backlight_paths() {
        let (c, m): (f64, f64) = (cur.parse().unwrap_or(0.0), max.parse().unwrap_or(1.0));
        if m > 0.0 {
            av.brightness = Some((c / m).clamp(0.0, 1.0) as f32);
        }
    }
    av.has_brightnessctl = which("brightnessctl").is_some();
    av.battery = battery_snap();
    av
}

/// Hilo de encuesta: volumen, brillo y batería cada 2 s.
pub fn spawn_av(tx: Sender<Msg>) {
    std::thread::spawn(move || loop {
        let _ = tx.send(Msg::Av(av_snap()));
        std::thread::sleep(std::time::Duration::from_secs(2));
    });
}

/// Cambia el volumen (+/- porcentaje).
pub fn set_volume(delta_pct: i32) {
    let d = if delta_pct >= 0 {
        format!("{delta_pct}%+")
    } else {
        format!("{}%-", -delta_pct)
    };
    std::thread::spawn(move || {
        let _ = run(
            "wpctl",
            &["set-volume", "@DEFAULT_AUDIO_SINK@", &d, "LIMIT", "1.5"],
        );
    });
}

/// Alterna el silencio.
pub fn toggle_mute() {
    std::thread::spawn(move || {
        let _ = run("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]);
    });
}

/// Cambia el brillo. Con `brightnessctl` si está instalado **y funciona**;
/// si falla (sin permisos, sin binario) se cae a sysfs con pkexec.
pub fn set_brightness(delta_pct: i32) {
    if which("brightnessctl").is_some() {
        let d = if delta_pct >= 0 {
            format!("{delta_pct}%+")
        } else {
            format!("{}%-", -delta_pct)
        };
        std::thread::spawn(move || {
            let (ok, _) = run_full("brightnessctl", &["set", &d]);
            if !ok {
                // Está instalado pero no puede escribir: sysfs elevado.
                sysfs_set_pct(step_abs(delta_pct as f64));
            }
        });
        return;
    }
    sysfs_set_pct(step_abs(delta_pct as f64));
}

/// Brillo actual en porcentaje, leyendo sysfs (o None si la pantalla no
/// es regulable, como en un monitor externo).
pub fn brightness_pct() -> Option<f32> {
    for e in std::fs::read_dir("/sys/class/backlight").ok()?.flatten() {
        let base = e.path();
        let Some(cur) = read_trim(&base.join("brightness").to_string_lossy()) else {
            continue;
        };
        let Some(max) = read_trim(&base.join("max_brightness").to_string_lossy()) else {
            continue;
        };
        let (Ok(cur), Ok(max)) = (cur.trim().parse::<f64>(), max.trim().parse::<f64>()) else {
            continue;
        };
        if max > 0.0 {
            return Some(((cur / max) * 100.0).clamp(0.0, 100.0) as f32);
        }
    }
    None
}

/// El valor absoluto que corresponde a un paso relativo.
fn step_abs(delta_pct: f64) -> f64 {
    (brightness_pct().unwrap_or(50.0) as f64 + delta_pct).clamp(0.0, 100.0)
}

/// Escribe el brillo en sysfs con privilegios (una operación a la vez).
fn sysfs_set_pct(pct: f64) {
    if BRIGHT_BUSY
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
        )
        .is_err()
    {
        return;
    }
    std::thread::spawn(move || {
        if let Ok(dir) = std::fs::read_dir("/sys/class/backlight") {
            for e in dir.flatten() {
                let base = e.path();
                let max: f64 = read_trim(&base.join("max_brightness").to_string_lossy())
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0.0);
                if max <= 0.0 {
                    continue;
                }
                let new = (max * pct / 100.0).round().clamp(0.0, max);
                let path = base.join("brightness");
                let script = format!("echo {} > {}", new as i64, path.display());
                let _ = run_privileged("/bin/sh", &["-c", &script]);
            }
        }
        BRIGHT_BUSY.store(false, std::sync::atomic::Ordering::Relaxed);
    });
}

// ── Clima (wttr.in) ──────────────────────────────────────────────────────

/// Primer número entero de una cadena ("+18°C" → 18, "↑7km/h" → 7).
fn first_int(s: &str) -> Option<i32> {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() || (c == '-' && out.is_empty()) || (c == '+' && out.is_empty()) {
            out.push(c);
        } else if !out.is_empty() {
            break;
        }
    }
    out.parse().ok()
}

/// Icono a partir de la descripción (ya en español).
fn weather_icon(desc: &str) -> &'static str {
    let d = desc.to_lowercase();
    if d.contains("tormenta") || d.contains("trueno") {
        "⛈"
    } else if d.contains("nieve") || d.contains("nevad") || d.contains("granizo") {
        "❄"
    } else if d.contains("lluvia")
        || d.contains("chubasco")
        || d.contains("llovizna")
        || d.contains("lluvios")
    {
        "🌦"
    } else if d.contains("niebla") || d.contains("bruma") {
        "🌫"
    } else if d.contains("nublad")
        || d.contains("intervalos")
        || d.contains("cubierto")
        || d.contains("nubes")
    {
        if d.contains("parcial") || d.contains("parsamen") {
            "⛅"
        } else {
            "☁"
        }
    } else if d.contains("despejad") || d.contains("solead") || d.contains("sol") {
        "☀"
    } else {
        "🌡"
    }
}

fn fetch_weather(place: &str) -> Option<WeatherSnap> {
    // Formato compacto de wttr.in, localizado con lang=es.
    // El lugar va en la ruta y hay que codificarlo (acentos, espacios).
    let url = if place.trim().is_empty() {
        "wttr.in/".to_string()
    } else {
        format!("wttr.in/{}", url_path(place.trim()))
    };
    let out = run(
        "curl",
        &[
            "-s",
            "-m",
            "6",
            "--get",
            "--data-urlencode",
            "format=%t|%f|%C|%h|%w|%l",
            "--data-urlencode",
            "lang=es",
            &url,
        ],
    )?;
    let parts: Vec<&str> = out.trim().split('|').collect();
    if parts.len() < 5 {
        return None;
    }
    let temp = first_int(parts[0])?;
    let feels = first_int(parts[1]).unwrap_or(temp);
    let desc = parts[2].trim().to_string();
    let humidity = first_int(parts[3]).unwrap_or(0).max(0) as u32;
    let wind = first_int(parts[4]).unwrap_or(0).max(0) as f32;
    let place = parts
        .get(5)
        .map(|p| p.split(',').next().unwrap_or(p).trim().to_string())
        .unwrap_or_default();
    Some(WeatherSnap {
        ok: true,
        temp,
        feels,
        humidity,
        wind,
        icon: weather_icon(&desc).to_string(),
        desc,
        place,
        at: chrono::Local::now().format("%H:%M").to_string(),
    })
}

/// Lugar compartido entre el hilo de clima y los cambios de Ajustes.
pub type Place = std::sync::Arc<std::sync::Mutex<String>>;

/// Crea el lugar compartido a partir de la configuración.
pub fn place_cell(query: &str) -> Place {
    std::sync::Arc::new(std::sync::Mutex::new(query.to_string()))
}

/// Hilo de clima: consulta al arrancar y cada 10 minutos.
pub fn spawn_weather(tx: Sender<Msg>, place: Place) {
    std::thread::spawn(move || loop {
        let q = place.lock().map(|g| g.clone()).unwrap_or_default();
        match fetch_weather(&q) {
            Some(w) => {
                let _ = tx.send(Msg::Weather(w));
                std::thread::sleep(std::time::Duration::from_secs(600));
            }
            None => {
                // Sin red: reintentamos en un rato sin insistir.
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
        }
    });
}

/// Consulta inmediata (al cambiar la ubicación en Ajustes).
pub fn fetch_weather_now(tx: Sender<Msg>, place: Place) {
    std::thread::spawn(move || {
        let q = place.lock().map(|g| g.clone()).unwrap_or_default();
        if let Some(w) = fetch_weather(&q) {
            let _ = tx.send(Msg::Weather(w));
        }
    });
}

/// Codifica un lugar para llevarlo a la ruta de wttr.in (acentos, espacios).
/// La coma se deja tal cual: wttr.in la entiende como «Ciudad,País».
fn url_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b',' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ── Acciones del usuario ─────────────────────────────────────────────────

/// Ejecuta una acción administrativa con pkexec/sudo y devuelve el resultado.
pub fn run_privileged(prog: &str, args: &[&str]) -> (bool, String) {
    if which("pkexec").is_some() {
        let mut a = vec![prog];
        a.extend_from_slice(args);
        return run_full("pkexec", &a);
    }
    let mut a = vec!["-n", prog];
    a.extend_from_slice(args);
    run_full("sudo", &a)
}

// ── Visor interno: salida línea a línea ──────────────────────────────────

/// Ejecuta `prog args` en un hilo y manda **cada línea** de su stdout y
/// stderr por `tx`. Es lo que alimenta al visor interno: el resultado se
/// ve dentro de Nébula, sin otra terminal.
///
/// No emite `Msg::OutEnd`: eso lo hace el que encadena (`spawn_stream`).
pub fn stream_run(
    tx: &Sender<Msg>,
    id: u64,
    prog: &str,
    args: &[&str],
    cwd: Option<&Path>,
) -> bool {
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    match cmd.spawn() {
        Ok(child) => pump_all(child, tx.clone(), id),
        Err(e) => {
            let _ = tx.send(Msg::Out {
                id,
                line: format!("no se pudo ejecutar {prog}: {e}"),
            });
            false
        }
    }
}

/// Igual que `stream_run` con `sh -c cmd` (para comandos con `&&`,
/// tuberías o comillas que no encajan en un argv).
pub fn shell_stream_run(tx: &Sender<Msg>, id: u64, cwd: Option<&Path>, cmd: &str) -> bool {
    stream_run(tx, id, "sh", &["-c", cmd], cwd)
}

/// Ejecuta y avisa al terminar con `Msg::OutEnd` (lo habitual).
pub fn spawn_stream(tx: Sender<Msg>, id: u64, prog: &str, args: &[&str], cwd: Option<&Path>) {
    let ok = stream_run(&tx, id, prog, args, cwd);
    let _ = tx.send(Msg::OutEnd { id, ok });
}

/// Igual que `spawn_stream` pero con `sh -c cmd`.
pub fn spawn_shell_stream(tx: Sender<Msg>, id: u64, cwd: Option<&Path>, cmd: &str) {
    let ok = shell_stream_run(&tx, id, cwd, cmd);
    let _ = tx.send(Msg::OutEnd { id, ok });
}

/// Igual que `spawn_priv_stream` con privilegios pero **sin** `OutEnd`:
/// sirve para encadenar pasos en un mismo visor.
pub fn priv_stream_run(tx: &Sender<Msg>, id: u64, prog: &str, args: &[&str]) -> bool {
    if which("pkexec").is_some() {
        let mut a = vec![prog];
        a.extend_from_slice(args);
        stream_run(tx, id, "pkexec", &a, None)
    } else {
        let mut a = vec!["-n", prog];
        a.extend_from_slice(args);
        stream_run(tx, id, "sudo", &a, None)
    }
}

/// Ejecuta con privilegios (pkexec/sudo) y avisa al terminar: sólo la
/// contraseña pasa por el sistema; la salida, al visor.
pub fn spawn_priv_stream(tx: Sender<Msg>, id: u64, prog: &str, args: &[&str]) {
    let ok = priv_stream_run(&tx, id, prog, args);
    let _ = tx.send(Msg::OutEnd { id, ok });
}

/// Lee un flujo entero enviando línea a línea hasta el EOF.
fn pump<R: std::io::Read + Send + 'static>(tx: Sender<Msg>, id: u64, reader: R) {
    use std::io::BufRead;
    let mut buf = std::io::BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        match buf.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&line)
                    .trim_end_matches(['\n', '\r'])
                    .to_string();
                if tx.send(Msg::Out { id, line: text }).is_err() {
                    break;
                }
            }
        }
    }
}

/// Drena stdout y stderr en dos hilos y espera al hijo. Devuelve si salió
/// bien (el `OutEnd` lo manda el que lanza la ejecución).
fn pump_all(mut child: std::process::Child, tx: Sender<Msg>, id: u64) -> bool {
    let mut workers = Vec::new();
    if let Some(stream) = child.stdout.take() {
        let tx = tx.clone();
        workers.push(std::thread::spawn(move || pump(tx, id, stream)));
    }
    if let Some(stream) = child.stderr.take() {
        let tx = tx.clone();
        workers.push(std::thread::spawn(move || pump(tx, id, stream)));
    }
    let ok = child.wait().map(|s| s.success()).unwrap_or(false);
    for w in workers {
        let _ = w.join();
    }
    ok
}

// ── Reproductores: arranque y control sin salir de Nébula ────────────────

/// ¿cliamp está respondiendo? (su daemon vive en un socket; sin él todo
/// comando devuelve «not running»).
pub fn cliamp_running() -> bool {
    let Some(bin) = which("cliamp") else {
        return false;
    };
    let (ok, out) = run_full(&bin.to_string_lossy(), &["status"]);
    ok && !out.contains("not running") && !out.to_lowercase().contains("no socket")
}

/// Arranca el daemon de cliamp si hace falta. Devuelve si queda vivo.
///
/// cliamp tarda bastante en abrir su socket (primero resuelve sus orígenes
/// remotos, que pueden tardar medio minuto), así que se le da margen: el
/// aviso «cliamp…» está en la barra mientras espera.
pub fn ensure_cliamp() -> bool {
    if cliamp_running() {
        return true;
    }
    let Some(bin) = which("cliamp") else {
        return false;
    };
    spawn_detached(&bin.to_string_lossy(), &["--daemon"], None);
    for _ in 0..140 {
        std::thread::sleep(Duration::from_millis(250));
        if cliamp_running() {
            return true;
        }
    }
    false
}

/// ¿cmus está respondiendo?
pub fn cmus_running() -> bool {
    which("cmus-remote").is_some() && run_full("cmus-remote", &["-Q"]).0
}

/// Arranca cmus en una pty invisible si no corre. Devuelve si queda vivo.
pub fn ensure_cmus() -> bool {
    if cmus_running() {
        return true;
    }
    if which("cmus").is_none() || !spawn_in_pty("cmus", &[]) {
        return false;
    }
    for _ in 0..25 {
        std::thread::sleep(Duration::from_millis(200));
        if cmus_running() {
            return true;
        }
    }
    false
}

/// Volumen de cmus en ±`delta`: cmus-remote sólo acepta valores absolutos,
/// así que se consulta el actual y se devuelve el nuevo.
pub fn cmus_volume(delta: i32) -> (bool, String) {
    let out = run("cmus-remote", &["-Q"]).unwrap_or_default();
    let cur = out
        .lines()
        .find_map(|l| l.strip_prefix("vol_left "))
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(50);
    let next = (cur + delta).clamp(0, 100);
    run_full("cmus-remote", &["-v", &next.to_string()])
}

// ── Fase 7: terminales, emisoras y reinicio ───────────────────────────────

/// Detiene cmus (la instancia que esté) y **espera** a que libere su
/// socket: sin esa espera la siguiente instancia se queja de que ya hay
/// una escuchando y no arranca.
pub fn stop_cmus() -> bool {
    if which("cmus-remote").is_none() {
        return false;
    }
    let (sent, _) = run_full("cmus-remote", &["-C", "quit"]);
    for _ in 0..40 {
        if !cmus_running() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    sent
}

/// Pid del daemon de cliamp, del fichero que él mismo escribe.
fn cliamp_daemon_pid() -> Option<i32> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".config/cliamp/cliamp.sock.pid");
    let pid: i32 = read_trim(&path.to_string_lossy())?.trim().parse().ok()?;
    // Por si el pid hubiera sido reutilizado por otro proceso.
    let comm = read_trim(&format!("/proc/{pid}/comm"))?;
    (comm.trim() == "cliamp").then_some(pid)
}

/// Para el daemon de cliamp con SIGTERM a **su** pid (nunca `pkill -f`,
/// que se llevaría por delante lo que no debe).
///
/// El TUI necesita el socket libre para arrancar; al abrirlo después,
/// cliamp retoma su estado con `resume.json`.
pub fn stop_cliamp() -> bool {
    let Some(pid) = cliamp_daemon_pid() else {
        return !cliamp_running();
    };
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }
    for _ in 0..50 {
        if !cliamp_running() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Una operación de la API v2 de cliamp (`cliamp remote call … --wait`).
/// Devuelve (¿ok?, salida cruda con el JSON del trabajo).
pub fn cliamp_v2(op: &str, params: &serde_json::Value) -> (bool, String) {
    let Some(bin) = which("cliamp") else {
        return (false, "cliamp no está instalado".into());
    };
    let json = params.to_string();
    run_full(
        &bin.to_string_lossy(),
        &["remote", "call", op, "--params", &json, "--wait"],
    )
}

/// Fila del menú nativo de emisoras y géneros.
#[derive(Clone, Debug)]
pub struct Station {
    /// Proveedor que sabe cargarla (`radio`, `cliamp`…).
    pub provider: String,
    /// Id que entiende `provider.load`.
    pub id: String,
    /// Nombre visible.
    pub name: String,
    /// Detalle: país de la emisora, o nº de pistas del género.
    pub detail: String,
}

/// Trae emisoras (catálogo de `radio`) y géneros (listas de `cliamp`).
pub fn spawn_stations(tx: Sender<Msg>, token: u64) {
    std::thread::spawn(move || {
        if !cliamp_running() && !ensure_cliamp() {
            let _ = tx.send(Msg::Stations {
                token,
                items: Vec::new(),
                error: "cliamp no está instalado ni arranca".into(),
            });
            return;
        }
        let mut items = Vec::new();
        let mut error = String::new();
        let lists = [
            (
                "provider.catalog",
                serde_json::json!({ "provider": "radio", "limit": 80 }),
            ),
            (
                "provider.playlists",
                serde_json::json!({ "provider": "cliamp", "limit": 80 }),
            ),
        ];
        for (op, params) in lists {
            match station_list(op, params) {
                Ok(list) => items.extend(list),
                Err(e) if error.is_empty() => error = e,
                Err(_) => {}
            }
        }
        let _ = tx.send(Msg::Stations {
            token,
            items,
            error,
        });
    });
}

/// Juega una operación de listado de cliamp y la convierte en filas.
fn station_list(op: &str, params: serde_json::Value) -> Result<Vec<Station>, String> {
    let (ok, out) = cliamp_v2(op, &params);
    if !ok {
        return Err(summarize("cliamp", false, &out));
    }
    let v: serde_json::Value =
        serde_json::from_str(&out).map_err(|_| "respuesta ilegible de cliamp".to_string())?;
    let rows = v
        .pointer("/job/result/playlists")
        .and_then(|p| p.as_array())
        .ok_or_else(|| "cliamp no devolvió ninguna lista".to_string())?;
    let mut list = Vec::new();
    for r in rows {
        let id = r.get("id").and_then(|x| x.as_str()).unwrap_or("");
        let raw = r.get("name").and_then(|x| x.as_str()).unwrap_or("");
        if id.is_empty() || raw.is_empty() {
            continue;
        }
        let provider = r
            .get("provider")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        // Las emisoras vienen como «Nombre [bitrate] · País».
        let (name, detail) = match raw.split_once(" · ") {
            Some((a, b)) if provider == "radio" => (a.to_string(), b.to_string()),
            _ => {
                let detail = match r.get("track_count").and_then(|x| x.as_u64()) {
                    Some(n) => format!("{n} pistas"),
                    None => String::new(),
                };
                (raw.to_string(), detail)
            }
        };
        list.push(Station {
            provider,
            id: id.to_string(),
            name,
            detail,
        });
    }
    Ok(list)
}

/// Carga una emisora o género en cliamp y avisa del resultado.
pub fn spawn_station_play(tx: Sender<Msg>, provider: String, id: String, name: String) {
    std::thread::spawn(move || {
        if !cliamp_running() && !ensure_cliamp() {
            let _ = tx.send(Msg::Note("cliamp no está instalado ni arranca".into()));
            return;
        }
        let params = serde_json::json!({ "provider": provider, "playlist": id });
        let (ok, out) = cliamp_v2("provider.load", &params);
        let note = if ok {
            format!("♪ {name}")
        } else {
            summarize("cliamp", false, &out)
        };
        let _ = tx.send(Msg::Note(note));
    });
}

/// Carga un directorio en la biblioteca de cmus (arrancándolo si falta).
pub fn cmus_add_library(dir: &str) -> (bool, String) {
    if !ensure_cmus() {
        return (false, "cmus no está instalado ni arranca".into());
    }
    let (ok, out) = run_full("cmus-remote", &["-l", dir]);
    (
        ok,
        if ok {
            format!("biblioteca · añadido {}", ui_short(dir))
        } else {
            summarize("cmus", false, &out)
        },
    )
}

/// Nombre corto de una ruta para los avisos (`/home/u/Música` → `Música`).
fn ui_short(p: &str) -> &str {
    p.trim_end_matches('/').rsplit('/').next().unwrap_or(p)
}

/// ¿Está en blanco la biblioteca guardada de cmus? Es `lib.pl`, el archivo
/// que cmus carga al arrancar: si no existe o viene vacío, su árbol arranca
/// sin nada y conviene sembrarlo con la carpeta de música.
pub fn cmus_lib_empty() -> bool {
    let Ok(home) = std::env::var("HOME") else {
        return false; // sin HOME no se arriesga nada
    };
    match std::fs::read_to_string(format!("{home}/.config/cmus/lib.pl")) {
        Ok(s) => s.trim().is_empty(),
        Err(_) => true, // no existe: cmus arranca con el árbol en blanco
    }
}

// ── Sesión y TTY ──────────────────────────────────────────────────────────

/// Dónde vive Nébula: sesión gráfica o consola de texto, y qué gestor de
/// display la sostiene. Se calcula una sola vez: no cambia en caliente.
pub struct Session {
    /// Tipo de sesión: `wayland`, `x11` o `tty`.
    pub kind: String,
    /// Gestor de display activo (`gdm3`, `lightdm`…). Vacío si no hay.
    pub dm: String,
    /// Objetivo por defecto de systemd (`graphical.target`…).
    pub target: String,
}

/// Información de la sesión (cacheada).
pub fn session() -> &'static Session {
    static S: std::sync::OnceLock<Session> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        let raw = std::env::var("XDG_SESSION_TYPE")
            .unwrap_or_default()
            .to_lowercase();
        let kind = match raw.as_str() {
            "wayland" | "x11" => raw,
            _ if std::env::var("WAYLAND_DISPLAY").is_ok() => "wayland".into(),
            _ if std::env::var("DISPLAY").is_ok() => "x11".into(),
            _ => "tty".into(),
        };
        Session {
            kind,
            dm: display_manager(),
            target: default_target(),
        }
    })
}

/// Gestor de display: el que esté activo en systemd, con el fichero
/// clásico de Debian/Ubuntu como respaldo.
fn display_manager() -> String {
    for cand in ["gdm3", "gdm", "lightdm", "sddm"] {
        let (ok, out) = run_full("systemctl", &["is-active", cand]);
        if ok && out.trim() == "active" {
            return cand.into();
        }
    }
    read_trim("/etc/X11/default-display-manager")
        .map(|p| {
            std::path::Path::new(p.trim())
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Objetivo por defecto de systemd (`graphical.target`, `multi-user.target`…).
fn default_target() -> String {
    run("systemctl", &["get-default"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Terminal donde corre Nébula (`/dev/tty1`, `/dev/pts/3`…).
pub fn tty_name() -> String {
    std::fs::read_link("/proc/self/fd/0")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

// ── Búsquedas de Entretenimiento ─────────────────────────────────────────

/// Busca vídeos con yt-dlp (YouTube) sin salir de Nébula.
pub fn spawn_video_search(tx: Sender<Msg>, token: u64, query: String) {
    std::thread::spawn(move || {
        let spec = format!("ytsearch14:{query}");
        let out = run(
            "yt-dlp",
            &[
                "--flat-playlist",
                "--no-warnings",
                "--print",
                "%(title)s\t%(duration)s\t%(id)s",
                &spec,
            ],
        );
        let (hits, error) = match out {
            Some(o) if !o.trim().is_empty() => (parse_video(&o), String::new()),
            Some(_) => (Vec::new(), "sin resultados para esa búsqueda".into()),
            None => (Vec::new(), "yt-dlp no está instalado".into()),
        };
        let _ = tx.send(Msg::Search { token, hits, error });
    });
}

/// Salida de `yt-dlp --print` → filas del visor de resultados.
fn parse_video(out: &str) -> Vec<Hit> {
    out.lines()
        .filter_map(|l| {
            let mut parts = l.splitn(3, '\t');
            let title = parts.next()?.trim().to_string();
            if title.is_empty() {
                return None;
            }
            let dur = parts.next().unwrap_or("").trim();
            let id = parts.next()?.trim().to_string();
            let sub = match dur.parse::<u64>() {
                Ok(s) if s > 0 => format!("{}:{:02}", s / 60, s % 60),
                _ => "—".into(),
            };
            Some(Hit { title, sub, id })
        })
        .collect()
}

pub fn kill_process(pid: i32) -> (bool, String) {
    unsafe {
        if libc::kill(pid as libc::pid_t, libc::SIGTERM) == 0 {
            (true, format!("señal TERM enviada a {pid}"))
        } else {
            (false, format!("no se pudo señalizar a {pid}"))
        }
    }
}

// ── Bases de datos ───────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct DbResult {
    pub name: String,
    pub ok: bool,
    pub tables: Vec<String>,
    pub error: String,
    pub elapsed_ms: u128,
}

pub fn spawn_db_test(
    tx: Sender<Msg>,
    name: String,
    host: String,
    port: u16,
    user: String,
    password: String,
    dbname: String,
) {
    std::thread::spawn(move || {
        use std::time::Instant;
        let start = Instant::now();
        let mut res = DbResult {
            name: name.clone(),
            ..Default::default()
        };
        let cs = format!(
            "host='{host}' port={port} user='{user}' password='{password}' dbname='{dbname}' connect_timeout=4"
        );
        match postgres::Client::connect(&cs, postgres::NoTls) {
            Ok(mut client) => {
                let q = "SELECT table_schema || '.' || table_name \
                         FROM information_schema.tables \
                         WHERE table_schema NOT IN ('pg_catalog','information_schema') \
                         ORDER BY 1 LIMIT 60";
                match client.query(q, &[]) {
                    Ok(rows) => {
                        res.ok = true;
                        res.tables = rows.iter().map(|r| r.get::<_, String>(0)).collect();
                    }
                    Err(e) => res.error = e.to_string(),
                }
            }
            Err(e) => res.error = e.to_string(),
        }
        res.elapsed_ms = start.elapsed().as_millis();
        let _ = tx.send(Msg::Db(res));
    });
}
