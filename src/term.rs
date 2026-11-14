//! Terminales embebidos: cada aplicación vive en su propia pty.
//!
//! Nébula no abre otra ventana: **cmus** y **cliamp** corren aquí dentro,
//! cada uno con su pseudo-terminal, su pantalla vt100 y su teclado. La pty
//! sigue viva aunque el panel esté cerrado (`Ctrl+G` sólo la oculta), así
//! que entrar y salir no interrumpe nada.
//!
//! Teclas de Nébula dentro del terminal:
//!   `Ctrl+G` salir del panel · `Ctrl+Tab` / `Ctrl+O` cambiar de app
//!   `Ctrl+T` detener el proceso actual
//!
//! El resto (q, Esc, dígitos, `?`…) va entero a la aplicación: sus atajos
//! son suyos.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;
use tui_term::vt100;

/// Una aplicación corriendo en su propia pseudo-terminal.
pub struct Pty {
    /// Maestro: por aquí se escribe lo que teclea la usuaria.
    master: File,
    /// Salida del hijo, en canal (la lee un hilo aparte).
    rx: Receiver<Vec<u8>>,
    /// Pantalla emulada (vt100): aquí se dibuja todo lo que sale.
    parser: vt100::Parser,
    /// Pid del hijo (para detenerlo).
    pid: libc::pid_t,
    /// ¿Sigue vivo el hijo? Lo pone el hilo lector al recogerlo.
    alive: Arc<AtomicBool>,
    cols: u16,
    rows: u16,
    /// Nombre visible («cmus», «cliamp»).
    pub label: &'static str,
}

impl Pty {
    /// Lanza `prog args` en una pty de `cols`×`rows`. `None` si no arranca.
    pub fn open(
        prog: &str,
        args: &[&str],
        cols: u16,
        rows: u16,
        label: &'static str,
    ) -> Option<Pty> {
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
            return None;
        }
        let ws = libc::winsize {
            ws_row: rows.max(2),
            ws_col: cols.max(2),
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe {
            libc::ioctl(master, libc::TIOCSWINSZ, &ws as *const libc::winsize);
        }

        let owner = unsafe { File::from_raw_fd(slave) };
        let (Ok(s0), Ok(s1), Ok(s2)) = (owner.try_clone(), owner.try_clone(), owner.try_clone())
        else {
            unsafe {
                libc::close(master);
            }
            return None;
        };
        let tty_fd = owner.as_raw_fd();

        let mut cmd = std::process::Command::new(prog);
        cmd.args(args)
            .stdin(std::process::Stdio::from(s0))
            .stdout(std::process::Stdio::from(s1))
            .stderr(std::process::Stdio::from(s2));
        unsafe {
            cmd.pre_exec(move || {
                // Sesión propia: el hijo no recibe las señales de Nébula.
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                // Su terminal de control es este pty (así cmus cree tener TTY).
                libc::ioctl(tty_fd, libc::TIOCSCTTY, 0);
                Ok(())
            });
        }

        let mut child = cmd.spawn().ok()?;
        let pid = child.id() as libc::pid_t;
        drop(owner); // el hijo ya tiene sus copias: sin esto el pty da EOF

        let alive = Arc::new(AtomicBool::new(true));
        let alive_r = alive.clone();
        let (tx, rx) = channel::<Vec<u8>>();
        // El maestro vive en dos copias: una lee (hilo), otra escribe (Nébula).
        let mut reader = unsafe { File::from_raw_fd(master) };
        let writer = reader.try_clone().ok()?;
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
            // Al llegar al EOF recogemos al hijo: sin zombis.
            let _ = child.wait();
            alive_r.store(false, Ordering::SeqCst);
        });

        Some(Pty {
            master: writer,
            rx,
            parser: vt100::Parser::new(rows.max(2), cols.max(2), 0),
            pid,
            alive,
            cols,
            rows,
            label,
        })
    }

    /// Pasa al parser todo lo que el hijo ha escrito desde la última vez.
    pub fn feed(&mut self) {
        while let Ok(bytes) = self.rx.try_recv() {
            self.parser.process(&bytes);
        }
    }

    /// Lo que teclea la usuaria, hacia el hijo.
    pub fn write(&mut self, bytes: &[u8]) {
        if self.master.write_all(bytes).is_err() {
            return;
        }
        let _ = self.master.flush();
    }

    /// Redimensiona la pty (TIOCSWINSZ → SIGWINCH) y la pantalla emulada.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.max(2);
        let rows = rows.max(2);
        if (cols, rows) == (self.cols, self.rows) {
            return;
        }
        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe {
            libc::ioctl(
                self.master.as_raw_fd(),
                libc::TIOCSWINSZ,
                &ws as *const libc::winsize,
            );
        }
        self.parser.screen_mut().set_size(rows, cols);
        self.cols = cols;
        self.rows = rows;
    }

    /// ¿Sigue vivo el proceso?
    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Detiene el proceso: SIGHUP → SIGTERM → SIGKILL (con calma).
    pub fn kill(&mut self) {
        if !self.alive() {
            return;
        }
        let pid = self.pid;
        unsafe {
            libc::kill(pid, libc::SIGHUP);
        }
        let alive = self.alive.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(700));
            if !alive.load(Ordering::SeqCst) {
                return;
            }
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
            std::thread::sleep(Duration::from_millis(1500));
            if alive.load(Ordering::SeqCst) {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        });
    }

    /// La pantalla emulada, para pintarla con `PseudoTerminal`.
    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// ¿El hijo pidió ratón? Sólo en ese caso se le reenvían los eventos.
    pub fn wants_mouse(&self) -> bool {
        use vt100::MouseProtocolMode;
        self.parser.screen().mouse_protocol_mode() != MouseProtocolMode::None
    }
}

// ── Del teclado de Nébula a las secuencias de un terminal xterm ───────────

/// Traduce una tecla de crossterm a los bytes que espera un TUI normal.
///
/// Devuelve `None` para las teclas que un terminal no entiende.
pub fn key_bytes(k: KeyEvent) -> Option<Vec<u8>> {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let alt = k.modifiers.contains(KeyModifiers::ALT);
    let shift = k.modifiers.contains(KeyModifiers::SHIFT);
    // Código de modificador xterm: 1 + may(1) + alt(2) + ctrl(4).
    let m = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);

    let mut out: Vec<u8> = Vec::new();
    // Alt se envía como prefijo ESC (así lo hace xterm).
    if alt {
        out.push(0x1b);
    }

    // Secuencia con modificador: CSI 1;mod LETRA · o CSI n;mod ~
    let arrow = |out: &mut Vec<u8>, fin: u8, m: u8| {
        if m > 1 {
            out.extend_from_slice(format!("\x1b[1;{m}{}", fin as char).as_bytes());
        } else {
            out.extend_from_slice(&[0x1b, b'[', fin]);
        }
    };
    let tilde = |out: &mut Vec<u8>, n: u8, m: u8| {
        if m > 1 {
            out.extend_from_slice(format!("\x1b[{n};{m}~").as_bytes());
        } else {
            out.extend_from_slice(format!("\x1b[{n}~").as_bytes());
        }
    };

    match k.code {
        KeyCode::Char(c) => {
            if ctrl {
                // Ctrl+letra = 0x01..0x1A; con los demás, su código de control.
                let code = match c {
                    'a'..='z' => c as u8 - b'a' + 1,
                    'A'..='Z' => c as u8 - b'A' + 1,
                    ' ' | '@' | '2' => 0x00,
                    '[' | '3' | '1' => 0x1b,
                    '\\' | '4' => 0x1c,
                    ']' | '5' => 0x1d,
                    '^' | '6' | '-' | '7' => 0x1e,
                    '_' | '/' | '8' | '?' => 0x1f,
                    '9' => 0x7f,
                    _ => return None,
                };
                out.push(code);
            } else {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
        KeyCode::Enter => out.push(b'\r'),
        KeyCode::Tab => out.push(b'\t'),
        KeyCode::BackTab => out.extend_from_slice(b"\x1b[Z"),
        KeyCode::Backspace => out.push(0x7f),
        KeyCode::Esc => out.push(0x1b),
        KeyCode::Up => arrow(&mut out, b'A', m),
        KeyCode::Down => arrow(&mut out, b'B', m),
        KeyCode::Right => arrow(&mut out, b'C', m),
        KeyCode::Left => arrow(&mut out, b'D', m),
        KeyCode::Home => arrow(&mut out, b'H', m),
        KeyCode::End => arrow(&mut out, b'F', m),
        KeyCode::Insert => tilde(&mut out, 2, m),
        KeyCode::Delete => tilde(&mut out, 3, m),
        KeyCode::PageUp => tilde(&mut out, 5, m),
        KeyCode::PageDown => tilde(&mut out, 6, m),
        KeyCode::F(n) => match n {
            1..=4 => {
                if m > 1 {
                    out.extend_from_slice(format!("\x1b[1;{m}{}", b'P' + (n - 1)).as_bytes());
                } else {
                    out.extend_from_slice(&[0x1b, b'O', b'P' + (n - 1)]);
                }
            }
            5 => tilde(&mut out, 15, m),
            6 => tilde(&mut out, 17, m),
            7 => tilde(&mut out, 18, m),
            8 => tilde(&mut out, 19, m),
            9 => tilde(&mut out, 20, m),
            10 => tilde(&mut out, 21, m),
            11 => tilde(&mut out, 23, m),
            12 => tilde(&mut out, 24, m),
            _ => return None,
        },
        _ => return None,
    }
    Some(out)
}

/// Evento de ratón como secuencia SGR (sólo si el hijo la pidió).
///
/// `col`/`row` son coordenadas **relativas al área del terminal**.
pub fn mouse_bytes(ev: &MouseEvent, col: u16, row: u16) -> Option<Vec<u8>> {
    let x = col.saturating_add(1);
    let y = row.saturating_add(1);
    let (btn, press) = match ev.kind {
        MouseEventKind::Down(MouseButton::Left) => (0u8, true),
        MouseEventKind::Down(MouseButton::Middle) => (1, true),
        MouseEventKind::Down(MouseButton::Right) => (2, true),
        MouseEventKind::Up(MouseButton::Left) => (0, false),
        MouseEventKind::Up(MouseButton::Middle) => (1, false),
        MouseEventKind::Up(MouseButton::Right) => (2, false),
        MouseEventKind::Drag(MouseButton::Left) => (32, true),
        MouseEventKind::Drag(MouseButton::Middle) => (33, true),
        MouseEventKind::Drag(MouseButton::Right) => (34, true),
        MouseEventKind::ScrollUp => (64, true),
        MouseEventKind::ScrollDown => (65, true),
        _ => return None,
    };
    let mut out = format!("\x1b[<{btn};{x};{y}").into_bytes();
    out.push(if press { b'M' } else { b'm' });
    Some(out)
}

impl Drop for Pty {
    /// Nébula se va, o el panel sustituye su proceso: el hijo no queda
    /// huérfano. Al cerrarse el maestro la pty ya envía SIGHUP; aquí se
    /// remata con SIGTERM si aún respira (y nada si ya terminó).
    fn drop(&mut self) {
        if self.alive.load(Ordering::SeqCst) {
            unsafe {
                libc::kill(self.pid, libc::SIGTERM);
            }
        }
    }
}
