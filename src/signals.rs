//! Salida limpia ante señales: Nébula no deja la TTY rota.
//!
//! Con el terminal en crudo (`ISIG` desactivado) Ctrl+C llega como tecla y
//! Nébula ya sale con ella. Pero `kill`, un cierre de sesión o el apagado
//! del gestor mandan `SIGTERM`/`SIGHUP`/`SIGQUIT`/`SIGINT` **con la pantalla
//! alternativa puesta y el modo crudo activo**: sin más, el programa moriría
//! dejando la consola con el cursor invisible y el teclado mudo.
//!
//! Las cuatro señales se interceptan aquí y tienen dos velocidades:
//!
//! * **1.ª señal** — sólo levanta una bandera; el bucle principal la ve en
//!   menos de 16 ms, restaura el terminal por la vía normal y sale con 0.
//! * **2.ª señal** — se acabó la paciencia: termios original, secuencias de
//!   salida y `_exit(128+señal)` al instante, sin pasar por nada más.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::OnceLock;

/// Señal esperando a que el bucle la recoja (0 = ninguna).
static PENDING: AtomicI32 = AtomicI32::new(0);
/// Ya se atendió una señal: lo que llegue después fuerza la salida ya.
static EXITING: AtomicBool = AtomicBool::new(false);
/// El termios original, guardado antes de que ratatui lo ponga en crudo.
static ORIG: OnceLock<libc::termios> = OnceLock::new();

/// Las cuatro señales que pueden llegarle a Nébula.
const SIGNALS: [libc::c_int; 4] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT];

/// Guarda el termios original e instala los manejadores.
///
/// Se llama **antes** de `ratatui::init()`, que es quien pone el terminal
/// en crudo y entra en la pantalla alternativa.
pub fn install() {
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut t) == 0 {
            let _ = ORIG.set(t);
        }
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as usize;
        // Sin SA_RESTART: poll() recibe EINTR en cuanto llega la señal y el
        // bucle comprueba la bandera nada más volver (≤ 16 ms).
        sa.sa_flags = 0;
        libc::sigemptyset(&mut sa.sa_mask);
        for sig in SIGNALS {
            libc::sigaction(sig, &sa, std::ptr::null_mut());
        }
    }
}

/// ¿Hay una señal pendiente de atender? Al recogerla se marca el estado
/// como «saliendo»: lo que llegue después ya no espera al bucle.
pub fn pending() -> bool {
    if PENDING.swap(0, Ordering::SeqCst) != 0 {
        EXITING.store(true, Ordering::SeqCst);
        true
    } else {
        false
    }
}

/// Vuelve a dejar las señales con su comportamiento de siempre. Se llama
/// al terminar, cuando el terminal ya está restaurado y sólo queda correr,
/// p. ej., la orden que va a la TTY (pkexec y compañía): allí un `kill`
/// legítimo tiene que volver a poder con Nébula.
pub fn disarm() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        libc::sigemptyset(&mut sa.sa_mask);
        for sig in SIGNALS {
            libc::sigaction(sig, &sa, std::ptr::null_mut());
        }
    }
}

/// Manejador: la primera señal sólo deja la bandera; si ya había otra
/// pendiente (o ya se estaba saliendo), se fuerza la salida.
extern "C" fn on_signal(sig: libc::c_int) {
    let prev = PENDING.swap(sig, Ordering::SeqCst);
    if prev != 0 || EXITING.load(Ordering::SeqCst) {
        emergency(sig);
    }
}

/// Último recurso: restaurar y marcharse sin dejar la TTY hecha un desastre.
/// Todo lo de aquí dentro es async-signal-safe: ni allocations ni bloqueos.
fn emergency(sig: libc::c_int) {
    unsafe {
        if let Some(t) = ORIG.get() {
            // El termios vale por terminal, así que stdin basta; si acaso
            // no es la TTY, se prueba con /dev/tty.
            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, t) != 0 {
                let fd = libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC);
                if fd >= 0 {
                    libc::tcsetattr(fd, libc::TCSANOW, t);
                    libc::close(fd);
                }
            }
        }
        // Ratón fuera, pegado de portapapeles fuera, pantalla alternativa
        // abajo y cursor de vuelta: exactamente lo que hace restore().
        const OUT: &[u8] =
            b"\x1b[?2004l\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l\x1b[?1049l\x1b[?25h";
        libc::write(libc::STDOUT_FILENO, OUT.as_ptr().cast(), OUT.len());
        libc::_exit(128 + sig);
    }
}
