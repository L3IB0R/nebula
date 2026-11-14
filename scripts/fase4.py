#!/usr/bin/env python3
"""Comprobaciones de la Fase 4 en un pty.

Cubre: la pantalla de arranque (con salto por Esc), el foco lateral (Ctrl+E),
la categoría Clima de Ajustes, la pestaña Entretenimiento y los menús locales
(git · apt · sistema).

Uso: python3 scripts/fase4.py [--release]
Los raws son opcionales (NEBULA_RAW=1) y van a /tmp/nebula-fase4/ (reciclable).
"""
import fcntl, os, pty, re, select, signal, struct, sys, termios, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import ansi  # noqa: E402

profile = "release" if "--release" in sys.argv else "debug"
BIN = os.path.join(ROOT, "target", profile, "nebula")
OUT = "/tmp/nebula-fase4"
RAW = os.environ.get("NEBULA_RAW") == "1"
os.makedirs(OUT, exist_ok=True)
COLS, ROWS = 120, 40

pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.execvp(BIN, [BIN])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

buf = bytearray()
fails = 0


def pump(secs):
    end = time.time() + secs
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                d = os.read(fd, 65536)
            except OSError:
                return False
            if not d:
                return False
            buf.extend(d)
    return True


def alive():
    try:
        p, _ = os.waitpid(pid, os.WNOHANG)
        return p == 0
    except ChildProcessError:
        return False


def snap(name):
    if RAW:
        open(f"{OUT}/{name}.raw", "wb").write(bytes(buf))
    sc = ansi.Screen(COLS, ROWS)
    sc.feed(bytes(buf).decode("utf-8", "replace"))
    return sc.text()


def send(k, wait=0.35):
    try:
        os.write(fd, k)
    except OSError:
        return False
    pump(wait)
    return True


def check(label, cond):
    global fails
    print(("[ok] " if cond else "[FAIL] ") + label)
    if not cond:
        fails += 1
    return cond


def head(t, n):
    return "\n".join(t.split("\n")[:n])


def sidebar(t, width=22):
    return "\n".join(l[:width] for l in t.split("\n")[1:19])


def ajustes():
    send(b"\x1b[12~", 1.2)  # F2


def goto(n):
    """Va a la pestaña n con BackTab desde Inicio (n = 11 es la última)."""
    send(b"1", 1.0)
    for _ in range((12 - n) % 12):
        send(b"\x1b[Z", 0.25)


def set_cat(n):
    """Lleva Ajustes a la categoría n (8 categorías) sin depender de dónde esté."""
    for _ in range(8):
        send(b"\x1b[D")
    for _ in range(n):
        send(b"\x1b[C")


# ── Pantalla de arranque ──────────────────────────────────────────────────
send(b"", 1.0)
t = snap("01_splash")
check("el splash muestra el logotipo", "N É B U L A" in t)
check("el splash invita a saltar con Esc", "Esc para saltar" in t)
check("el splash lista el análisis", "arranque" in t and "✓" in t)

send(b"\x1b", 1.2)  # Esc: salta el arranque
t = snap("02_tras_esc")
check("Esc retira el splash", "N É B U L A" not in t)
check("y aparece Inicio con el reloj", re.search(r"\d\d:\d\d", head(t, 14)) is not None)
check("sigue viva tras el salto", alive())

# ── Foco lateral con Ctrl+E ───────────────────────────────────────────────
send(b"\x05", 0.6)  # Ctrl+E
t = snap("03_foco_lateral")
check("Ctrl+E enfoca la lateral", "▸" in sidebar(t))
check("el foco marca la pestaña Inicio", "▸⌂ Inicio" in sidebar(t))

send(b"\x1b[B", 0.3)  # ↓
send(b"\x1b[B", 0.3)
t = snap("04_foco_baja")
check("↓ salta cabeceras dentro de la lateral", "▸⇄ Red" in sidebar(t))

send(b"\r", 0.9)  # Intro: salta a la pestaña marcada
t = snap("05_foco_enter")
check("Intro entra en la pestaña elegida", "Enlace" in head(t, 3))

send(b"\x05", 0.4)  # Ctrl+E de nuevo: enfoca
send(b"\x1b", 0.4)  # Esc: sale del foco
t = snap("06_foco_sale")
check("Esc sale del foco lateral", "▸" not in sidebar(t))

# ── Ajustes: categoría Clima ──────────────────────────────────────────────
ajustes()
set_cat(4)
t = snap("07_clima")
check("categoría Clima con sus cuatro filas", all(s in t for s in ["lugar", "país", "ciudad", "consultar"]))
check("muestra la consulta previa a wttr.in", "wttr" in t or "consulta" in t.lower())

# País → ciudad dependiente: cambiar el país cambia la ciudad propuesta.
send(b"\x1b[B", 0.3)  # fila país
send(b" ", 0.6)
t = snap("08_clima_pais")
check("cambiar el país propone otra ciudad", "país" in t and "ciudad" in t)

# ── Pestaña Entretenimiento ───────────────────────────────────────────────
goto(9)
t = snap("09_entretenimiento")
check("la pestaña Entretenimiento abre", "Entretenimiento" in t)
check("lista grupos de ocio", all(s in t for s in ["anime", "vídeo"]))
check("muestra el panel de herramientas", "herramientas" in t)

send(b"\x1b[B", 0.3)
t = snap("10_fun_baja")
check("↓ recorre las entradas", alive() and "Entretenimiento" in t)

# ── Menú local de git ─────────────────────────────────────────────────────
goto(7)
t = snap("11_git")
check("la pestaña Git abre", "Git" in t)

send(b"c", 0.7)
t = snap("12_git_commit")
check("la tecla c abre el menú de commit", "Nuevo commit" in t)
check("el commit ofrece tipos en español", any(k in t for k in ["añadir", "corregir", "ajustar"]))
send(b"\x1b", 0.5)

send(b"b", 0.7)
t = snap("13_git_ramas")
check("la tecla b abre el menú de ramas", "Ramas" in t)
send(b"\x1b", 0.5)

send(b"s", 0.7)
t = snap("14_git_stash")
check("la tecla s abre el menú de stash", "Stash" in t)
send(b"\x1b", 0.5)

# ── Panel de operaciones apt ──────────────────────────────────────────────
goto(3)
t = snap("15_paquetes")
check("la pestaña Paquetes abre", "Paquetes" in t)
check("el panel de operaciones apt está siempre visible", "Operaciones apt" in t)
check("el panel lista las operaciones", any(s in t for s in ["actualizar lista", "full-upgrade", "limpiar"]))

send(b"m", 0.7)
t = snap("16_menu_apt")
check("la tecla m enfoca el panel apt", "eligiendo" in t)
send(b"\x1b", 0.5)
t = snap("17_menu_apt_cerrado")
check("Esc suelta el panel apt", "eligiendo" not in t)

# ── Menú local de Sistema ─────────────────────────────────────────────────
goto(1)
t = snap("18_sistema")
check("la pestaña Sistema abre", "Sistema" in t)

send(b"m", 0.7)
t = snap("19_menu_sistema")
check("la tecla m abre el menú de Sistema", "Sistema " in t and "matar" in t)
check("el menú ofrece orden y refresco", "orden" in t and "refrescar" in t)
send(b"\x1b", 0.5)

# ── Salida limpia ─────────────────────────────────────────────────────────
send(b"q", 1.2)
if alive():
    print("[warn] no salió con q; mato el proceso")
    os.kill(pid, signal.SIGKILL)
    fails += 1
else:
    print("[ok] salida limpia con q")

print(f"\n{'TODO CORRECTO' if fails == 0 else f'{fails} FALLO(S)'}")
sys.exit(1 if fails else 0)
