#!/usr/bin/env python3
"""Recorre todos los módulos de Nébula en un pty y avisa si algo revienta.

Uso: python3 scripts/smoke.py [--release]
Los raws son opcionales (NEBULA_RAW=1) y van a /tmp/nebula-smoke/ (reciclable).
"""
import fcntl, os, pty, select, struct, sys, termios, time, signal

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
profile = "release" if "--release" in sys.argv else "debug"
BIN = os.path.join(ROOT, "target", profile, "nebula")
OUT = "/tmp/nebula-smoke"
RAW = os.environ.get("NEBULA_RAW") == "1"
os.makedirs(OUT, exist_ok=True)
COLS, ROWS = 120, 40

pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.execvp(BIN, [BIN])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

buf = bytearray()
mark = 0
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


steps = [
    ("espera", b"", 3.0),
    ("sistema", b"2", 1.5),
    ("red", b"3", 1.2),
    ("paquetes", b"4", 1.5),
    ("servicios", b"5", 1.2),
    ("contenedores", b"6", 1.2),
    ("proyectos", b"7", 1.2),
    ("git", b"8", 1.2),
    ("desarrollo", b"9", 1.2),
    ("entretenimiento", b"0", 1.5),
    ("notas", b"\t", 1.2),
    ("ajustes", b"\t", 1.2),
    ("mover ajustes", b"\x1b[B\x1b[B ", 1.0),
    ("ayuda", b"?", 1.0),
    ("cerrar ayuda", b"\x1b", 0.5),
    ("lanzador", b"\x0b", 1.0),
    ("filtrar lanzador", b"fire", 0.8),
    ("cerrar lanzador", b"\x1b", 0.6),
    ("volver inicio", b"1", 1.0),
]

for name, keys, secs in steps:
    if keys:
        try:
            os.write(fd, keys)
        except OSError:
            print(f"[FAIL] se cortó antes de {name}")
            fails += 1
            break
    if not pump(secs):
        print(f"[FAIL] el proceso murió en «{name}»")
        fails += 1
        break
    if not alive():
        print(f"[FAIL] Nébula terminó inesperadamente en «{name}»")
        fails += 1
        break
    if RAW:
        open(f"{OUT}/{mark:06d}_{name}.raw", "wb").write(bytes(buf))
    mark = len(buf)
    print(f"[ok] {name}")
else:
    os.write(fd, b"q")
    pump(1.0)
    if alive():
        print("[warn] no salió con q; mato el proceso")
        os.kill(pid, signal.SIGKILL)
        fails += 1
    else:
        print("[ok] salida limpia con q")

if RAW:
    open(f"{OUT}/smoke.raw", "wb").write(bytes(buf))
sys.exit(1 if fails else 0)
