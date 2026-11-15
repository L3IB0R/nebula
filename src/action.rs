//! Acciones que los módulos piden al corazón de Nébula.

use crate::config::Config;

/// Trabajo potencialmente destructivo que requiere confirmación.
#[derive(Clone, Debug)]
pub enum Job {
    /// `apt <subcomando> [args]` con privilegios
    Apt(Vec<String>),
    /// Instala varios paquetes apt de golpe (un solo pkexec) y, después,
    /// unos instaladores sueltos que no pasan por apt (p. ej. cliamp).
    /// `cmds` es (nombre visible, comando).
    Install {
        pkgs: Vec<String>,
        cmds: Vec<(String, String)>,
    },
    /// systemctl <acción> <servicio>
    Service(String, String),
    /// docker <acción> <recurso>
    Docker(String, String),
    /// Matar proceso
    Kill(i32),
    /// Comando con privilegios arbitrario
    Privileged(Vec<String>),
    /// Comando normal en segundo plano
    Plain(Vec<String>),
    /// Ejecuta `cmd` con `sh -c` en `cwd`, espera y avisa con el resultado.
    RunIn { cwd: String, cmd: String },
    /// Shell: `cmd` vacío = terminal interactivo en `cwd`.
    Shell { cwd: String, cmd: String },
    /// Ejecuta `cmd` en la consola embebida de Nébula (pty con teclado):
    /// para lo interactivo —`fzf`, los menús de `ani-cli`— que el visor
    /// interno no puede atender.
    Console(String),
    /// Ejecuta `cmd` en `cwd` y pinta la salida **dentro** de Nébula
    /// (viser interno): nada se abre en otra terminal.
    View {
        title: String,
        cwd: String,
        cmd: String,
    },
    /// Comando a cliamp con el reproductor asegurado (arranca su daemon
    /// si hace falta) y el resultado como aviso.
    Cliamp(Vec<String>),
    /// Sale de Nébula y deja el comando en la TTY: así `pkexec` pregunta
    /// en una terminal de verdad (y un cambio de sesión no arrastra a
    /// Nébula con él). p. ej. parar la interfaz gráfica o reiniciar.
    Tty(Vec<String>),
    /// Control de cmus: lo arranca en segundo plano si hace falta y
    /// ejecuta `cmus-remote <args>` (vacío = sólo arrancarlo).
    Cmus(Vec<String>),
}

#[derive(Debug)]
pub enum Action {
    /// La módulo se lo comió: no hacer nada global.
    Keep,
    Quit,
    Tab(usize),
    Next,
    Prev,
    Launcher,
    Help,
    Refresh,
    /// Ejecuta ya (sin confirmación).
    Run(Job),
    /// Pide confirmación con un mensaje.
    Confirm(String, Job),
    /// Aviso temporal en la barra de estado.
    Toast(String),
    /// Probar la conexión a `cfg.databases[idx]`.
    TestDb(usize),
    /// Prueba de latencia.
    Ping,
    /// Aplicar una configuración en memoria.
    ApplyConfig(Box<Config>),
    /// Aplicar y guardar en disco.
    SaveConfig(Box<Config>),
    /// Volver a construir el fondo.
    ReloadWallpaper,
    /// Consultar el clima ahora (tras cambiar la ubicación).
    RefreshWeather,
    /// Buscar en Entretenimiento sin salir de Nébula: `anime` = AniList
    /// (ani-cli para reproducir), lo contrario = yt-dlp. `token` casa la
    /// respuesta con la consulta que la lanzó.
    Search {
        token: u64,
        anime: bool,
        query: String,
    },
    /// Abrir el terminal embebido: `0` = cmus, `1` = cliamp.
    Term(usize),
    /// Detener la instancia de cmus que corre en segundo plano.
    StopCmus,
    /// Detener el daemon de cliamp.
    StopCliamp,
    /// Cargar la carpeta de música en la biblioteca de cmus.
    LoadLib,
    /// Menú nativo de emisoras y géneros de cliamp.
    Stations,
    /// Cargar una emisora o género en cliamp.
    Station {
        provider: String,
        id: String,
        name: String,
    },
    /// Descargador de música de YouTube (por URL).
    Download,
}
