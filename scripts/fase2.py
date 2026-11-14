#!/usr/bin/env python3
"""Comprobaciones de la Fase 2 en un pty: zen, lateral, volumen y clima."""
import fcntl, os, pty, select, struct, sys, termios, time, signal

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import ansi  # noqa: E402

BIN = os.path.join(ROOT, "target", "debug", "nebula")
OUT = "/tmp/nebula-fase2"
RAW = os.environ.get("NEBULA_RAW") == "1"
os.makedirs(OUT, exist_ok=True)
COLS, ROWS = 120, 40

pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.execvp(BIN, [BIN])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

buf = bytearray()


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


def snap(name):
    if RAW:
        open(f"{OUT}/{name}.raw", "wb").write(bytes(buf))
    sc = ansi.Screen(COLS, ROWS)
    sc.feed(bytes(buf).decode("utf-8", "replace"))
    return sc.text()


def send(k):
    os.write(fd, k)


def check(label, cond):
    print(("[ok] " if cond else "[FAIL] ") + label)
    return 0 if cond else 1


fails = 0
send(b"")
pump(3.5)
t1 = snap("01_inicio")
fails += check("clima con temperatura", "°C" in t1)
fails += check("chip de volumen en el pie", "vol " in t1)
fails += check("chip de batería en el pie", "bat " in t1)

send(b"\x1b[23~")  # F11 zen
pump(1.4)
t2 = snap("02_zen")
fails += check("zen oculta la barra superior", "NÉBULA" not in t2.split("\n")[0])
fails += check("zen oculta el pie", "q salir" not in t2)

send(b"\x1b[23~")
pump(1.4)
t3 = snap("03_normal")
fails += check("salida de zen restaura el cromo", "NÉBULA" in t3 and "q salir" in t3)

send(b"\x02")  # ctrl+b
pump(1.0)
t4 = snap("04_colapsada")
side = "\n".join(l[:5] for l in t4.split("\n")[1:14])
fails += check("lateral colapsada muestra iconos", "⌂" in side and "◉" in side)
fails += check("lateral colapsada no muestra nombres", "Proyectos" not in t4.split("\n")[3])

send(b"\x02")
pump(1.0)
t5 = snap("05_abierta")
fails += check("lateral reabierta con nombres", "Proyectos" in t5)

before = None
import re
m = re.search(r"vol (\d+)%", t5)
if m:
    before = int(m.group(1))
send(b"\x1b[1;5A")  # ctrl+up (la encuesta de audio corre cada 2 s)
pump(2.6)
t6 = snap("06_volumen")
m2 = re.search(r"vol (\d+)%", t6)
fails += check(
    f"volumen sube ({before} → {m2.group(1) if m2 else '?'})",
    m2 and before is not None and int(m2.group(1)) > before,
)

# ctrl+espacio = silencio (atajo configurable [keys].mute)
send(b"\x00")
pump(2.6)
t7 = snap("07_silencio")
fails += check("ctrl+espacio silencia", "vol ✕" in t7)
send(b"\x00")
pump(2.6)
t8 = snap("08_sonido")
fails += check("ctrl+espacio devuelve el sonido", re.search(r"vol \d+%", t8) is not None)

send(b"q")
pump(1.0)
try:
    os.kill(pid, signal.SIGKILL)
except ProcessLookupError:
    pass

print(f"\n{'TODO CORRECTO' if not fails else str(fails) + ' FALLOS'}")
sys.exit(1 if fails else 0)