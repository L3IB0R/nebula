#!/usr/bin/env python3
"""Comprobaciones del mantenimiento de v0.7.0 en un pty.

Cubre: la siembra de la biblioteca de cmus (se vacía `lib.pl` antes para
forzarlo, y se restaura al final), la consola embebida con `Alt+T` (tercer
pty con `$SHELL -l`), el ciclo de las tres aplicaciones con `Ctrl+O` y la
salida limpia ante `SIGTERM` (código 0 con la pantalla alternativa
restaurada). En ningún momento se apaga nada más allá de lo que se restaura.

Uso: python3 scripts/mant.py [--release]
Los raws son opcionales (NEBULA_RAW=1) y van a /tmp/nebula-mant/.
"""
import fcntl, os, pty, re, select, signal, struct, subprocess, sys, termios, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import ansi  # noqa: E402

profile = "release" if "--release" in sys.argv else "debug"
BIN = os.path.join(ROOT, "target", profile, "nebula")
OUT = "/tmp/nebula-mant"
RAW = os.environ.get("NEBULA_RAW") == "1"
os.makedirs(OUT, exist_ok=True)
COLS, ROWS = 120, 40

LIB = os.path.expanduser("~/.config/cmus/lib.pl")
lib_backup = None
if os.path.exists(LIB):
    with open(LIB, "rb") as fh:
        lib_backup = fh.read()

cmus_before = cliamp_before = 0
for name in ("cmus", "cliamp"):
    out = subprocess.run(["pgrep", "-c", "-x", name], capture_output=True, text=True)
    try:
        n = int(out.stdout.strip() or "0")
    except ValueError:
        n = 0
    if name == "cmus":
        cmus_before = n
    else:
        cliamp_before = n
# Estado limpio y determinista: con los dos reproductores parados, la tecla
# `t` de Entretenimiento abre cmus (que es el que tiene que sembrarse).
for name in ("cmus", "cliamp"):
    subprocess.run(["pkill", "-x", name], capture_output=True)
if cmus_before or cliamp_before:
    time.sleep(1.0)

# Biblioteca en blanco a propósito: así el arranque de cmus tiene que sembrarla.
os.makedirs(os.path.dirname(LIB), exist_ok=True)
with open(LIB, "wb") as fh:
    fh.write(b"")

pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.execvp(BIN, [BIN])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

buf = bytearray()
fails = 0
status = None  # código de salida de Nébula, en cuanto se reape


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
    global status
    if status is not None:
        return False
    try:
        p, st = os.waitpid(pid, os.WNOHANG)
    except ChildProcessError:
        return False
    if p == 0:
        return True
    status = st
    return False


def screen():
    sc = ansi.Screen(COLS, ROWS)
    sc.feed(bytes(buf).decode("utf-8", "replace"))
    return sc.text()


def snap(name):
    if RAW:
        open(f"{OUT}/{name}.raw", "wb").write(bytes(buf))
    return screen()


def send(k, wait=0.35):
    try:
        os.write(fd, k)
    except OSError:
        return False
    pump(wait)
    return True


def wait_for(pred, secs=4.0):
    end = time.time() + secs
    while time.time() < end:
        if pred(screen()):
            return True
        pump(0.25)
    return pred(screen())


def check(label, cond):
    global fails
    print(("[ok] " if cond else "[FAIL] ") + label)
    if not cond:
        fails += 1
    return cond


def lines(t):
    return t.split("\n")


def head(t, n):
    return "\n".join(lines(t)[:n])


def panel_title(t):
    """Título del panel del terminal (▸ etiqueta · estado)."""
    for l in lines(t):
        if "▸" in l and "·" in l and ("en marcha" in l or "finalizado" in l or "sin abrir" in l):
            return l.strip()
    return ""


def goto(n):
    """Va a la pestaña n con BackTab desde Inicio (n = 11 es la última)."""
    send(b"1", 1.0)
    for _ in range((12 - n) % 12):
        send(b"\x1b[Z", 0.25)


def terminals():
    out = subprocess.run(
        ["pgrep", "-c", "-f", "ptyxis|x-terminal-emulator|xdg-terminal-exec"],
        capture_output=True, text=True,
    )
    try:
        return int(out.stdout.strip() or "0")
    except ValueError:
        return 0


term0 = terminals()

# ── Arranque ──────────────────────────────────────────────────────────────
send(b"", 1.2)
send(b"\x1b", 1.2)  # Esc: salta el splash
t = snap("01_inicio")
check("arranca y salta el splash",
      "N É B U L A" not in t and re.search(r"\d\d:\d\d", head(t, 14)))

# ── Siembra de la biblioteca: lib.pl estaba vacío ─────────────────────────
def tree_letters(s):
    """Líneas con palabras de verdad en la zona del árbol de cmus.
    Calibrado: árbol vacío = 0 (sólo la columna «x»), cargado = 15."""
    return sum(1 for l in s.split("\n")[4:18] if re.search(r"[A-Za-z]{5,}", l[8:110]))


goto(9)  # Entretenimiento
send(b"t", 0.8)
t = snap("02_cmus")
title = panel_title(t)
check("t abre el panel con cmus", "▸" in title and "cmus" in title)
# El toast vive 5 s: hay que mirarlo enseguida, no tras un sondeo largo.
check("con la biblioteca en blanco llega el aviso «biblioteca cargada»",
      wait_for(lambda s: "biblioteca cargada" in s, 6.5))
check("el árbol de cmus se llena con los archivos de ~/Música",
      wait_for(lambda s: tree_letters(s) >= 3, 6.0))

# ── Alt+T: la consola embebida (tercer pty) ───────────────────────────────
send(b"\x1bt", 2.0)
t = snap("03_consola")
title = panel_title(t)
check("Alt+T abre la consola en marcha", "▸" in title and "consola" in title
      and "en marcha" in title)
check("el pie del panel anuncia alt+t", "alt+t" in t)
check("la consola no es una terminal externa", terminals() <= term0)

# ── Ctrl+O sigue el ciclo por las tres aplicaciones ───────────────────────
send(b"\x0f", 3.0)  # Ctrl+O: de la consola vuelve a cmus
t = snap("04_ciclo")
title2 = panel_title(t)
check("Ctrl+O lleva de la consola a la siguiente aplicación",
      "▸" in title2 and title2 != title and "cmus" in title2)

# ── SIGTERM: salida limpia con el terminal restaurado ─────────────────────
send(b"\x07", 0.6)  # Ctrl+G: fuera del panel, a la app entera
if alive():
    os.kill(pid, signal.SIGTERM)
end = time.time() + 5.0
while alive() and time.time() < end:
    pump(0.2)
pump(0.6)
check("SIGTERM termina Nébula", not alive())
if status is not None:
    check("sale con código 0",
          os.WIFEXITED(status) and os.WEXITSTATUS(status) == 0)
else:
    check("sale con código 0 (nunca se reapeó)", False)
blob = bytes(buf)
check("restaura la pantalla alternativa", b"\x1b[?1049l" in blob)
check("vuelve a mostrar el cursor", b"\x1b[?25h" in blob)

# ── Limpieza y restauración del estado previo ─────────────────────────────
subprocess.run(["pkill", "-x", "cmus"], capture_output=True)
time.sleep(1.5)
# Prueba en disco: cmus sólo puede haber guardado la biblioteca completa
# si la siembra la cargó (al morir por SIGHUP escribe su lib.pl).
try:
    saved = os.path.getsize(LIB)
except OSError:
    saved = 0
check("cmus guardó la biblioteca al morir (lib.pl con contenido)", saved > 10000)
if lib_backup is None:
    try:
        os.remove(LIB)
    except OSError:
        pass
else:
    with open(LIB, "wb") as fh:
        fh.write(lib_backup)
if cmus_before:
    subprocess.Popen(["cmus"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                     start_new_session=True)
if cliamp_before:
    subprocess.Popen(["cliamp", "--daemon"], stdout=subprocess.DEVNULL,
                     stderr=subprocess.DEVNULL, start_new_session=True)

print(f"\n{'TODO CORRECTO' if fails == 0 else f'{fails} FALLO(S)'}")
sys.exit(1 if fails else 0)
