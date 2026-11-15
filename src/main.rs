//! Nébula — centro de mando TUI para Debian/Ubuntu.

mod action;
mod anim;
mod app;
mod config;
mod data;
mod geo;
mod menus;
mod modules;
mod output;
mod resources;
mod signals;
mod term;
mod ui;
mod wallpaper;

use app::App;
use config::Config;
use ratatui::DefaultTerminal;
use std::time::Duration;

/// Qué hacer tras interpretar la línea de comandos.
enum Cli {
    /// Seguir y arrancar la interfaz.
    Run,
    /// Terminar con este código de salida.
    Exit(i32),
}

const HELP: &str = "Nébula — centro de mando TUI para Debian/Ubuntu

Uso: nebula [OPCIÓN]

Opciones:
  -t, --theme NOMBRE   arrancar con un tema (queda guardado en la config)
      --themes         listar los temas disponibles
  -c, --config         mostrar la ruta del archivo de configuración
  -h, --help           esta ayuda
  -V, --version        versión de Nébula

Dentro del programa:
  ? ayuda · ctrl+k lanzador · ctrl+b barra lateral · ctrl+e foco lateral
  f11 modo zen · ctrl+↑/↓ volumen · ctrl+mayús+↑/↓ brillo · ctrl+espacio silencio
  t terminal de cmus/cliamp · g emisoras y géneros · d descargar música
  q salir · (arranque: Esc salta la pantalla de bienvenida)

Terminal embebido (t):
  ctrl+g salir del panel · ctrl+tab / ctrl+o cambiar de aplicación
  alt+t consola embebida (shell con login) · ctrl+t detener el proceso
  el resto de teclas son de la aplicación · señales (sigterm, sighup…)
  cierran Nébula limpio, con el terminal restaurado

Configuración: ~/.config/nebula/config.toml";

/// Interpreta los argumentos; puede mutar la config (por --theme).
fn parse_args(cfg: &mut Config) -> Cli {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return Cli::Run;
    }
    {
        match args[0].as_str() {
            "-h" | "--help" => {
                println!("{HELP}");
                Cli::Exit(0)
            }
            "-V" | "--version" => {
                println!("nebula {}", env!("CARGO_PKG_VERSION"));
                Cli::Exit(0)
            }
            "--themes" => {
                for t in config::presets() {
                    println!("{}", t.name);
                }
                Cli::Exit(0)
            }
            "-c" | "--config" => {
                println!("{}", config::config_path().display());
                Cli::Exit(0)
            }
            "-t" | "--theme" => {
                let Some(name) = args.get(1) else {
                    eprintln!("falta el nombre del tema · prueba con: nebula --themes");
                    return Cli::Exit(2);
                };
                let found = config::presets()
                    .into_iter()
                    .find(|t| t.name.eq_ignore_ascii_case(name));
                match found {
                    Some(t) => {
                        cfg.general.theme = t.name.clone();
                        if let Err(e) = cfg.save() {
                            eprintln!("no se pudo guardar la config: {e}");
                        }
                        Cli::Run
                    }
                    None => {
                        eprintln!("tema desconocido: {name}");
                        eprintln!(
                            "disponibles: {}",
                            config::presets()
                                .iter()
                                .map(|t| t.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        Cli::Exit(1)
                    }
                }
            }
            other => {
                eprintln!("opción desconocida: {other}");
                eprintln!("prueba con: nebula --help");
                Cli::Exit(2)
            }
        }
    }
}

fn main() -> std::io::Result<()> {
    let mut cfg = Config::load();
    match parse_args(&mut cfg) {
        Cli::Run => {}
        Cli::Exit(code) => std::process::exit(code),
    }
    // Señales: hay que guardar el termios original ANTES de que ratatui
    // ponga el terminal en crudo, y colgar los manejadores de salida.
    signals::install();
    // Nébula es una TUI: si no hay una terminal a la que agarrarse (un tubo,
    // un servicio, un agente sin tty) no se puede poner en crudo ni leer el
    // teclado. `try_init` lo resuelve con un mensaje en vez de un pánico.
    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            signals::disarm();
            eprintln!("nébula necesita una terminal interactiva (tty): {e}");
            eprintln!("ábrelo en un emulador de terminal o en una consola de texto (ctrl+alt+f3).");
            std::process::exit(1);
        }
    };
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::event::EnableMouseCapture,
        crossterm::terminal::SetTitle("Nébula")
    );
    let res = run(&mut terminal, cfg);
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    ratatui::restore();
    // Terminal devuelto a la normalidad: las señales vuelven a su
    // comportamiento de siempre para lo que quede (ver más abajo).
    signals::disarm();
    // Modo terminal: Nébula ya devolvió la TTY, así que el comando que
    // viene a continuación (pkexec, systemctl…) puede preguntar su
    // contraseña en una terminal de verdad, tanto en la sesión gráfica
    // como en una consola de servidor.
    if let Ok(Some(args)) = &res {
        if let Some((prog, rest)) = args.split_first() {
            let st = std::process::Command::new(prog).args(rest).status();
            if st.is_err() {
                eprintln!("no se pudo ejecutar: {prog}");
            }
        }
    }
    res.map(|_| ())
}

fn run(
    terminal: &mut DefaultTerminal,
    cfg: config::Config,
) -> std::io::Result<Option<Vec<String>>> {
    let mut app = App::new(cfg);
    while app.running {
        app.drain();

        // poll() interrumpe EINTR con la señal; por eso la bandera se
        // comprueba justo después, antes de que el error cuela. 1.ª señal =
        // salida limpia (restore + código 0); si llega otra mientras tanto,
        // el manejador se encarga él solo.
        let ev = crossterm::event::poll(Duration::from_millis(16));
        if signals::pending() {
            app.running = false;
            // Ninguna orden a medias (pkexec y demás) sale por una señal.
            app.pending = None;
            break;
        }
        match ev {
            Ok(true) => match crossterm::event::read()? {
                crossterm::event::Event::Key(k) => app.on_key(k),
                crossterm::event::Event::Mouse(m) => app.on_mouse(m),
                crossterm::event::Event::Resize(w, h) => app.on_resize(w, h),
                _ => {}
            },
            Ok(false) => {}
            // Si poll falló por la señal, aquí ya no se llega: la bandera
            // se comprobó antes. Un error real corta como siempre.
            Err(e) => return Err(e),
        }

        app.tick();
        terminal.draw(|f| app.render(f))?;
    }
    Ok(app.pending.take())
}
