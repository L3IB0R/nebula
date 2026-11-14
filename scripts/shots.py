#!/usr/bin/env python3
"""Capturas automáticas para la web de Nébula.

Arranca el TUI en un pty, recorre momentos interesantes y guarda un PNG por
momento en web/shots/. Uso:

    python3 scripts/shots.py [--release]
"""
import fcntl, os, pty, re, select, struct, subprocess, sys, termios, time, signal

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import ansi  # noqa: E402

from PIL import Image, ImageDraw, ImageFont  # noqa: E402

profile = "release" if "--release" in sys.argv else "debug"
BIN = os.path.join(ROOT, "target", profile, "nebula")
DEST = os.path.join(ROOT, "web", "shots")
os.makedirs(DEST, exist_ok=True)
COLS, ROWS = 120, 40

FONT_PATH = "/usr/share/fonts/CascadiaCode/CascadiaMono.ttf"
FONT_SIZE = 16

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


def send(k):
    os.write(fd, k)


def screen():
    sc = ansi.Screen(COLS, ROWS)
    sc.feed(bytes(buf).decode("utf-8", "replace"))
    return sc


def render(name, expect=None):
    sc = screen()
    font = ImageFont.truetype(FONT_PATH, FONT_SIZE)
    cw = font.getlength("M")
    ch = round(cw * 1.95)  # celda ~2:1 como en el terminal
    img = Image.new("RGB", (round(cw * COLS), ch * ROWS), (8, 8, 12))
    d = ImageDraw.Draw(img)

    # 1) Fondos primero (por tramos iguales): así ningún glifo se pisa.
    for y, row in enumerate(sc.grid):
        x = 0
        while x < COLS:
            bg = row[x][2]
            x0 = x
            while x < COLS and row[x][2] == bg:
                x += 1
            if bg:
                d.rectangle([x0 * cw, y * ch, x * cw, (y + 1) * ch], fill=tuple(bg))

    # 2) Texto después.
    for y, row in enumerate(sc.grid):
        for x, (c, fg, bg, _bold) in enumerate(row):
            if c:
                d.text(
                    (x * cw, y * ch),
                    c,
                    font=font,
                    fill=tuple(fg) if fg else (230, 230, 230),
                )
    path = os.path.join(DEST, name)
    img.save(path, "PNG", optimize=True)
    # Volcado de texto para poder revisar la captura sin mirar el PNG.
    os.makedirs("/tmp/nebula-shots", exist_ok=True)
    open(f"/tmp/nebula-shots/{name}.txt", "w").write(sc.text())
    print(f"[ok] {name}  ({img.width}x{img.height})")
    if expect is not None:
        text = sc.text()
        if not expect(text):
            print(f"[FAIL] {name}: no se ve lo esperado en la captura")
            return False
    return True


def _head(t, n):
    return "\n".join(t.split("\n")[:n])


def _flip(t):
    return any(l.count("┌") >= 4 for l in t.split("\n"))


def estilo_actual(t):
    """Etiqueta de la fila «estilo» tal como la pinta Ajustes."""
    for l in t.split("\n"):
        if "estilo" in l and "◀▶" in l:
            m = re.search(r"estilo\s+(.+?)\s+◀▶", l)
            if m:
                return m.group(1).strip()
    return None


def ir_a_cat(n):
    """Categoría n de Ajustes sin depender de dónde se esté (8 en total)."""
    for _ in range(8):
        send(b"\x1b[D")
        pump(0.15)
    for _ in range(n):
        send(b"\x1b[C")
        pump(0.15)


def cliamp_arriba():
    """Levanta el daemon de cliamp si hace falta y espera su socket."""
    try:
        out = subprocess.run(["cliamp", "status"], capture_output=True,
                             text=True, timeout=20).stdout
        if "State:" in out:
            return True
    except Exception:
        pass
    subprocess.Popen(["cliamp", "--daemon"], stdout=subprocess.DEVNULL,
                     stderr=subprocess.DEVNULL, start_new_session=True)
    end = time.time() + 45
    while time.time() < end:
        try:
            out = subprocess.run(["cliamp", "status"], capture_output=True,
                                 text=True, timeout=20).stdout
            if "State:" in out:
                return True
        except Exception:
            pass
        time.sleep(1.0)
    return False


def poner_estilo(etiqueta):
    """Deja la etiqueta indicada en la fila «estilo» (ciclando con espacio)."""
    for _ in range(6):
        send(b"\x1b[A")
        pump(0.15)
    for _ in range(6):
        pump(0.4)
        cur = estilo_actual(screen().text())
        if cur == etiqueta:
            return True
        if cur is None:
            return False
        send(b" ")
        pump(0.4)
    return False


# (nombre, teclas — cada trozo se escribe por separado —, espera, comprobación)
shots = [
    # Fase 4: el arranque se captura antes de saltarlo.
    ("00-splash.png", [b""], 0.8, lambda t: "N É B U L A" in t),
    ("01-inicio.png", [b"\x1b"], 4.0,
     lambda t: re.search(r"\d\d:\d\d", _head(t, 14)) is not None),
    ("02-sistema.png", [b"2"], 2.0, lambda t: "Memoria" in t and "Procesos" in t),
    ("03-entretenimiento.png", [b"0"], 2.0,
     lambda t: "Entretenimiento" in t and ("Ahora suena" in t or "Nada sonando" in t)),
    ("04-ajustes.png", [b"1", b"\x1b[12~"], 2.0,
     lambda t: "Temas" in t and "Recursos" in t),
    ("05-lanzador.png", [b"1", b"\x0b"], 1.5, None),
    ("06-zen.png", [b"\x1b", b"\x1b[23~"], 1.5,
     lambda t: "NÉBULA" not in t.split("\n")[0]),
    ("07-lateral-colapsada.png", [b"\x1b[23~", b"\x02"], 1.5,
     lambda t: "⌂" in "\n".join(l[:5] for l in t.split("\n")[1:19])),
    ("08-ayuda.png", [b"\x02", b"?"], 1.5, None),
]

fails = 0
for name, keys, secs, expect in shots:
    for chunk in keys:
        if chunk:
            send(chunk)
            pump(0.4)
    if not pump(secs):
        print(f"[FAIL] el proceso murió en {name}")
        fails += 1
        break
    if not render(name, expect):
        fails += 1

# El tablero se elige leyendo la etiqueta actual (la config puede traer
# cualquier estilo); Recursos se alcanza con las 8 categorías.
if not fails:
    send(b"\x1b")  # cierra la ayuda
    pump(0.5)
    send(b"\x1b[12~")  # Ajustes
    pump(1.2)
    ir_a_cat(2)
    if not poner_estilo("tablero"):
        print("[FAIL] no se pudo poner el estilo tablero")
        fails += 1
    send(b"1")
    pump(2.0)
    if not render("09-reloj-tablero.png", _flip):
        fails += 1

    send(b"\x1b[12~")
    pump(1.2)
    ir_a_cat(6)
    if not render("10-recursos.png",
                  lambda t: ("✓" in t or "✗" in t) and "apt" in t):
        fails += 1

    # Fase 4: foco lateral, Clima, Entretenimiento y el menú de git.
    send(b"1")
    pump(1.0)
    send(b"\x05")  # Ctrl+E
    pump(0.8)
    if not render("11-foco-lateral.png", lambda t: "▸⌂ Inicio" in t):
        fails += 1
    send(b"\x1b")  # Esc: sale del foco
    pump(0.4)

    send(b"\x1b[12~")  # Ajustes → Clima
    pump(1.2)
    ir_a_cat(4)
    if not render("12-clima.png",
                  lambda t: all(s in t for s in ["lugar", "país", "ciudad", "consultar"])):
        fails += 1

    send(b"1")  # Inicio → BackTab ×3: pestaña Entretenimiento
    pump(1.0)
    for _ in range(3):
        send(b"\x1b[Z")
        pump(0.3)
    pump(1.5)
    if not render("13-entretenimiento.png",
                  lambda t: "Entretenimiento" in t and "herramientas" in t):
        fails += 1

    send(b"1")  # Inicio → BackTab ×5: pestaña Git
    pump(1.0)
    for _ in range(5):
        send(b"\x1b[Z")
        pump(0.25)
    pump(1.5)
    send(b"c")  # menú de commit
    pump(0.8)
    if not render("14-menu-git.png", lambda t: "Nuevo commit" in t):
        fails += 1

    send(b"\x1b")
    pump(0.4)

    # Fase 5: el visor interno (git status sin salir de Nébula).
    send(b"\r")
    pump(2.0)
    if not render("15-visor-git.png",
                  lambda t: "$" in t and "git status" in t and "terminado" in t):
        fails += 1
    send(b"\x1b")
    pump(0.5)

    # Fase 5: búsqueda de anime dentro de la propia TUI (AniList).
    send(b"1")  # Inicio → BackTab ×3: Entretenimiento
    pump(1.0)
    for _ in range(3):
        send(b"\x1b[Z")
        pump(0.3)
    pump(1.0)
    send(b"\r")  # «buscar anime»
    pump(0.8)
    send(b"naruto")
    pump(0.4)
    send(b"\r")
    pump(5.0)
    if not render("16-buscar-anime.png",
                  lambda t: "resultados" in t and "epi" in t):
        fails += 1
    send(b"\x1b")
    pump(0.4)
    send(b"\x1b")
    pump(0.4)

    # Fase 6: la tira de git y el panel apt, siempre a la vista.
    send(b"1")  # Inicio → BackTab ×5: pestaña Git
    pump(1.0)
    for _ in range(5):
        send(b"\x1b[Z")
        pump(0.25)
    pump(1.5)
    if not render("17-git-tira.png",
                  lambda t: "añadir" in t and "clonar" in t and "atajos" in t):
        fails += 1

    send(b"4")  # Paquetes directo
    pump(1.8)
    if not render("18-paquetes-apt.png",
                  lambda t: "Operaciones apt" in t and "flatpak" in t):
        fails += 1

    # Fase 7: primero el menú nativo de emisoras y géneros (API v2),
    # mientras el daemon de cliamp es el dueño del socket. La tecla `g`
    # sólo la intercepta el panel Entretenimiento: hay que llegar antes.
    send(b"1")  # Inicio → BackTab ×3: Entretenimiento
    pump(1.0)
    for _ in range(3):
        send(b"\x1b[Z")
        pump(0.3)
    pump(1.0)
    if not cliamp_arriba():
        print("[warn] cliamp no levantó su daemon: se omite la captura de radio")
        fails += 1
    else:
        send(b"g")
        pump(3.0)
        end = time.time() + 45
        while time.time() < end and "géneros" not in screen().text():
            pump(0.5)
        if not render("20-emisoras.png",
                      lambda t: "emisoras y géneros" in t and "intro reproducir" in t):
            fails += 1
        send(b"\x1b")
        pump(0.5)

    # Fase 7: el terminal embebido (cmus o cliamp) dentro de un panel.
    # Al salir de Nébula el proceso embebido se va con ella.
    send(b"1")
    pump(1.0)
    for _ in range(3):  # BackTab ×3: Entretenimiento
        send(b"\x1b[Z")
        pump(0.3)
    pump(1.0)
    send(b"t")
    pump(7.0)
    if not render("19-terminal-cliamp.png",
                  lambda t: "en marcha" in t and "ctrl+g" in t):
        fails += 1
    send(b"\x07")  # Ctrl+G: se sale del panel sin tocar el proceso
    pump(0.8)

    send(b"q")
    pump(1.0)

try:
    os.kill(pid, signal.SIGKILL)
except ProcessLookupError:
    pass
print("capturas listas en web/shots/" if not fails else f"{fails} captura(s) con problemas")
sys.exit(1 if fails else 0)
