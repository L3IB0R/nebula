#!/usr/bin/env python3
"""Comprobaciones de la Fase 6 en un pty: reorganización y usabilidad.

Cubre: el lateral por secciones (cabeceras de grupo y salto de foco), el pie
de atajos estándar en las doce pestañas, la tira de git con «a» (casillas por
ruta) y «k» (clonar), el panel de operaciones apt siempre visible, los filtros
de Sistema y Red, el reproductor embebido en Entretenimiento y el chip ♪ de la
barra de estado (un clic alterna play/pausa). En ningún momento debe abrirse
una terminal externa.

Uso: python3 scripts/fase6.py [--release]
Los raws son opcionales (NEBULA_RAW=1) y van a /tmp/nebula-fase6/ (reciclable).
"""
import fcntl, os, pty, re, select, signal, struct, subprocess, sys, termios, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import ansi  # noqa: E402

profile = "release" if "--release" in sys.argv else "debug"
BIN = os.path.join(ROOT, "target", profile, "nebula")
OUT = "/tmp/nebula-fase6"
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
    """Bombea hasta que la pantalla cumpla `pred` (o se acabe el tiempo)."""
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


def sidebar(t, width=22):
    return "\n".join(l[:width] for l in lines(t)[1:19])


def content(t, start=24):
    """Columnas del área de contenido (la lateral ocupa las 22 primeras)."""
    return "\n".join(l[start:] for l in lines(t))


def hint_row(t):
    """La fila del pie de atajos: última fila del contenido (índice 38)."""
    ls = lines(t)
    return ls[38][24:] if len(ls) > 38 else ""


def status_row(t):
    ls = lines(t)
    return ls[39] if len(ls) > 39 else ""


def terminals():
    """Nº de emuladores de terminal vivos (para detectar fugas)."""
    out = subprocess.run(
        ["pgrep", "-c", "-f", "ptyxis|x-terminal-emulator|xdg-terminal-exec"],
        capture_output=True, text=True,
    )
    try:
        return int(out.stdout.strip() or "0")
    except ValueError:
        return 0


def goto(n):
    """Va a la pestaña n con BackTab desde Inicio (n = 11 es la última)."""
    send(b"1", 1.0)
    for _ in range((12 - n) % 12):
        send(b"\x1b[Z", 0.25)


def click_chip():
    """Clic en el chip ♪ de la barra de estado (coords SGR, 1-based)."""
    x0 = len("Entretenimiento") + 6  # ancho fijo del chip: 4 celdas
    col, row = x0 + 1, ROWS          # segunda celda del chip, última fila
    send(f"\x1b[<0;{col};{row}M".encode(), 0.3)
    send(f"\x1b[<0;{col};{row}m".encode(), 0.4)


# ── Arranque ──────────────────────────────────────────────────────────────
send(b"", 1.2)
send(b"\x1b", 1.2)  # Esc: salta el splash
t = snap("01_inicio")
check("arranca y salta el splash", "N É B U L A" not in t and re.search(r"\d\d:\d\d", head(t, 14)))
term0 = terminals()

# ── Lateral por secciones ─────────────────────────────────────────────────
t = snap("02_lateral")
sb = sidebar(t)
check("la lateral agrupa en secciones",
      all(h in sb for h in ["GENERAL", "SISTEMA", "CÓDIGO", "OCIO", "CONFIGURACIÓN"]))

send(b"\x05", 0.6)  # Ctrl+E
t = snap("03_foco")
check("Ctrl+E marca el foco en Inicio", "▸⌂ Inicio" in sidebar(t))

send(b"\x1b[B", 0.35)  # ↓
t = snap("04_foco_sistema")
check("↓ salta la cabecera y aterriza en Sistema", "▸◉ Sistema" in sidebar(t))

send(b"\x1b[B", 0.35)  # ↓
t = snap("05_foco_red")
check("sigue bajando dentro de la sección", "▸⇄ Red" in sidebar(t))

send(b"\x1b", 0.4)  # Esc
t = snap("06_foco_sale")
check("Esc suelta el foco lateral", "▸" not in sidebar(t))

# ── Chip ♪ siempre presente ───────────────────────────────────────────────
t = snap("07_chip")
check("el chip ♪ está en la barra de estado", "♪" in status_row(t))

# ── Pie de atajos en las doce pestañas ────────────────────────────────────
HINTS = [
    (0, "Inicio", "secciones"),
    (1, "Sistema", "matar proceso"),
    (2, "Red", "buscar puerto/nombre"),
    (3, "Paquetes", "operaciones apt"),
    (4, "Servicios", "detener"),
    (5, "Contenedores", "instalar docker"),
    (6, "Proyectos", "abrir en editor"),
    (7, "Git", "clonar"),
    (8, "Desarrollo", "ejecutar script"),
    (9, "Entretenimiento", "play/pausa"),
    (10, "Notas", "editar"),
    (11, "Ajustes", "categoría"),
]
for n, name, marker in HINTS:
    goto(n)
    t = snap(f"10_pie_{n:02d}_{name.lower()}")
    check(f"pie de atajos en {name}", marker in hint_row(t))

# ── Git: la tira de acciones, a la vista siempre ──────────────────────────
goto(7)
t = snap("20_git_tira")
strip = "\n".join(lines(t)[1:3])
check("la tira de git lista todo lo esencial",
      all(s in strip for s in ["estatus", "añadir", "sin stage", "commit", "ramas", "stash"]))
check("la segunda línea cierra con log, fetch, diff y push",
      all(s in strip for s in ["log", "fetch", "diff", "pull", "push", "clonar"]))

send(b"a", 0.8)  # añadir por ruta
t = snap("21_git_add")
check("a abre el añadido archivo por archivo", "Añadir al área de preparación" in t)
check("avisa de que jamás usa «git add .»", "nunca «git add .»" in t)
check("lista los archivos con casillas", "☑" in t)
send(b"\x1b", 0.5)

send(b"u", 0.8)  # quitar del stage
t = snap("22_git_unstage")
check("u avisa de que no hay nada preparado",
      "no hay nada en el área de preparación" in t)

send(b"k", 0.7)  # clonar por URL
t = snap("23_git_clonar")
check("k abre el clonado con su destino", "Clonar repositorio" in t and "destino" in t)
send(b"\x1b", 0.5)

# ── Paquetes: panel apt siempre visible ───────────────────────────────────
goto(3)
t = snap("30_paquetes")
check("el panel de operaciones apt está siempre visible", "Operaciones apt" in content(t))
check("lista las operaciones de apt",
      any(s in content(t) for s in ["actualizar lista", "full-upgrade", "limpiar"]))

send(b"m", 0.6)
t = snap("31_paquetes_m")
check("m enfoca el panel apt", "eligiendo" in content(t))
send(b"\x1b", 0.5)
t = snap("32_paquetes_esc")
check("Esc suelta el panel apt", "eligiendo" not in content(t))

# ── Sistema: micro-barras y filtros ───────────────────────────────────────
goto(1)
t = snap("40_sistema")
check("el filtro de procesos arranca en todos", "filtro: todos" in content(t))
check("cada proceso lleva su micro-barra",
      re.search(r"(?<![━─])[━─]{4}(?![━─])", content(t)) is not None)

send(b"f", 0.5)
t = snap("41_sistema_filtro")
check("f cambia el filtro a cpu >1%", "filtro: cpu >1%" in content(t))

send(b"/", 0.5)
t = snap("42_sistema_buscar")
check("/ abre la búsqueda por nombre", "buscar «" in content(t))
send(b"\x1b", 0.4)

# ── Red: filtros ──────────────────────────────────────────────────────────
goto(2)
t = snap("50_red")
check("el filtro de red arranca en todas", "todas" in content(t))

send(b"f", 0.5)
t = snap("51_red_filtro")
check("f pasa el filtro a escuchando", "escuchando" in content(t))

send(b"/", 0.5)
t = snap("52_red_buscar")
check("/ abre la búsqueda de puerto o nombre", "Buscar" in content(t))
send(b"\x1b", 0.4)

# ── Entretenimiento: reproductor embebido y chip clicable ─────────────────
# Estado limpio: el motor elegido para el espacio depende de lo que corra.
subprocess.run(["pkill", "-x", "cmus"], capture_output=True)
subprocess.run(["pkill", "-x", "cliamp"], capture_output=True)
time.sleep(1.0)

goto(9)
t = snap("60_entretenimiento")
check("el reproductor vive en Entretenimiento",
      "Ahora suena" in content(t) or "Nada sonando" in content(t))
check("el grupo de música está a la vista", "cliamp · online" in content(t))

send(b" ", 1.0)
ok = wait_for(lambda s: "♪ ▶" in status_row(s), 45.0)
t = snap("61_sonando")
check("el espacio arranca cliamp desde aquí", ok)

click_chip()
ok = wait_for(lambda s: "♪ ‖" in status_row(s), 10.0)
t = snap("62_pausa")
check("un clic en el chip ♪ pausa la música", ok)

click_chip()
ok = wait_for(lambda s: "♪ ▶" in status_row(s), 10.0)
t = snap("63_reanuda")
check("otro clic la vuelve a poner en marcha", ok)

subprocess.run(["timeout", "5", "cliamp", "stop"], capture_output=True)

# ── Sin fugas de terminal ─────────────────────────────────────────────────
t = snap("70_final")
term1 = terminals()
check("no se ha abierto ninguna terminal externa", term1 <= term0)
check("sigue viva tras todo el recorrido", alive())

# ── Limpieza ──────────────────────────────────────────────────────────────
subprocess.run(["pkill", "-x", "cmus"], capture_output=True)
subprocess.run(["pkill", "-x", "cliamp"], capture_output=True)
send(b"q", 1.2)
if alive():
    print("[warn] no salió con q; mato el proceso")
    os.kill(pid, signal.SIGKILL)
    fails += 1
else:
    print("[ok] salida limpia con q")

print(f"\n{'TODO CORRECTO' if fails == 0 else f'{fails} FALLO(S)'}")
sys.exit(1 if fails else 0)
