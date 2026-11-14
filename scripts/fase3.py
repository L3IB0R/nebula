#!/usr/bin/env python3
"""Comprobaciones de la Fase 3 en un pty.

Cubre: los 5 estilos de reloj, las 7 categorías de Ajustes, los widgets
configurables, la sección Recursos (instalación) y el botón ⟨/⟩ de la lateral.

El estilo actual se lee de la propia pantalla (no se asume ninguno) y se
restaura al final, así el test sirve con cualquier config.

Uso: python3 scripts/fase3.py [--release]
Los raws son opcionales (NEBULA_RAW=1) y van a /tmp/nebula-fase3/ (reciclable).
"""
import fcntl, os, pty, re, select, signal, struct, sys, termios, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import ansi  # noqa: E402

profile = "release" if "--release" in sys.argv else "debug"
BIN = os.path.join(ROOT, "target", profile, "nebula")
OUT = "/tmp/nebula-fase3"
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

# Etiquetas de la fila «estilo», en el mismo orden que CLOCK_STYLES.
ESTILOS = ["digital", "una línea", "tablero", "analógico", "mínimo"]


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


def sidebar(t, width):
    return "\n".join(l[:width] for l in t.split("\n")[1:14])


def reloj_zone(t, n=6):
    """Primeras n líneas sin las de los widgets (CPU/RAM), para mirar el reloj."""
    return "\n".join(
        l for l in t.split("\n")[:n] if "CPU" not in l and "RAM" not in l
    )


def set_cat(n):
    """Lleva Ajustes a la categoría n sin depender de dónde esté (7 categorías)."""
    for _ in range(8):
        send(b"\x1b[D")
    for _ in range(n):
        send(b"\x1b[C")


def estilo_actual(t):
    """Etiqueta de la fila «estilo» tal como la pinta Ajustes."""
    for l in t.split("\n"):
        if "estilo" in l and "◀▶" in l:
            m = re.search(r"estilo\s+(.+?)\s+◀▶", l)
            if m:
                return m.group(1).strip()
    return None


def set_estilo(etiqueta):
    """Deja la etiqueta indicada en la fila «estilo» (ciclando con espacio)."""
    ajustes()
    set_cat(2)
    for _ in range(6):  # la fila 0 (estilo) seleccionada
        send(b"\x1b[A")
    for _ in range(6):
        cur = estilo_actual(snap("tmp_estilo"))
        if cur == etiqueta:
            return True
        if cur is None:
            return False
        send(b" ", 0.5)
    return estilo_actual(snap("tmp_estilo")) == etiqueta


def ajustes():
    send(b"\x1b[12~", 1.2)  # F2


def inicio():
    send(b"1", 1.3)


# ── Arranque ──────────────────────────────────────────────────────────────
send(b"", 3.5)
t = snap("01_inicio")
check("arranca con la hora visible", re.search(r"\d\d:\d\d", head(t, 14)) is not None)
check(
    "el botón ⟨/⟩ está en el borde de la lateral",
    any(len(l) > 22 and l[22] == "⟨" for l in t.split("\n")),
)

# ── Ajustes: siete categorías ─────────────────────────────────────────────
ajustes()
t = snap("02_ajustes")
check(
    "ajustes abre con las 7 categorías",
    all(
        c in t
        for c in ["Temas", "Apariencia", "Reloj", "Widgets", "Clima", "Sistema", "Recursos"]
    ),
)
check("contador de recursos en la categoría", bool(re.search(r"Recursos\s+\d+/\d+", t)))

# ── Reloj: los 5 estilos ──────────────────────────────────────────────────
# Primero se guarda el estilo que trae la config para poder devolverlo.
set_cat(2)
inicial = estilo_actual(snap("03_reloj"))
check("la fila estilo muestra su etiqueta", inicial in ESTILOS)
if inicial is None:
    inicial = "digital"

checks_estilo = {
    "digital": lambda h: re.search(r"█{6,}", h) is not None and "┌" not in h,
    "una línea": lambda h: "✦" in h and "─" in h,
    "tablero": lambda h: any(l.count("┌") >= 4 for l in h.split("\n")),
    "analógico": lambda h: "◉" in h and "◆" in h,
    "mínimo": lambda h: re.search(r"\d\d:\d\d", h) is not None and "█" not in h,
}
# digital/minimal se miran con más líneas (el reloj digital es alto).
ventanas = {"digital": 14, "una línea": 4, "tablero": 16, "analógico": 14, "mínimo": 6}

prev = None
for etiqueta in ESTILOS:
    if not check(f"se puede elegir el estilo «{etiqueta}»", set_estilo(etiqueta)):
        break
    inicio()
    t = snap(f"04_estilo_{etiqueta.replace(' ', '_')}")
    h = reloj_zone(t, ventanas[etiqueta])
    ok = alive() and checks_estilo[etiqueta](h)
    # Además: cada estilo debe pintar algo distinto al anterior.
    if ok and prev is not None:
        ok = h != prev
    check(f"estilo «{etiqueta}» se pinta en Inicio", ok)
    if not ok:
        break
    prev = h

# Se devuelve el estilo que traía la config (ApplyConfig no persiste en disco,
# pero así la sesión de prueba no lo deja cambiado).
if set_estilo(inicial):
    check(f"se restaura el estilo «{inicial}»", True)
else:
    check("se restaura el estilo inicial", False)

# ── Widgets: mostrar/ocultar ──────────────────────────────────────────────
ajustes()
set_cat(3)
t = snap("06_widgets")
check("lista de widgets con mostrar/ocultar", "◉" in t and "○" in t)
check("formas de widget con selector", "◀▶" in t)

send(b" ", 0.8)  # oculta el widget seleccionado (clima)
t = snap("07_widget_oculto")
check("espacio oculta el widget", "oculto" in t)

inicio()
t = snap("08_sin_clima")
check("el widget oculto desaparece de Inicio", "☀" not in t)

ajustes()
send(b" ", 0.8)  # lo vuelve a mostrar
inicio()
t = snap("09_con_clima")
check("el widget vuelve a Inicio", "☀" in t)

# ── Sistema ───────────────────────────────────────────────────────────────
ajustes()
set_cat(5)
t = snap("10_sistema")
check("categoría Sistema con la lateral", "lateral" in t and "nombre" in t)

# ── Recursos ──────────────────────────────────────────────────────────────
set_cat(6)
t = snap("11_recursos")
check("catálogo de recursos con estado", ("✓" in t or "✗" in t) and "apt" in t)
check("secciones agrupadas", "programación" in t and "sistema" in t)

send(b" ", 1.0)  # instalar la sección seleccionada
t = snap("12_instalar_seccion")
check(
    "instalar sección pide confirmación (o ya está todo)",
    "Confirmar" in t or "instalado" in t,
)
send(b"\x1b", 0.7)  # cancelamos: la prueba no instala nada

send(b"a", 1.0)  # instalar todo
t = snap("13_instalar_todo")
check(
    "«instalar todo» pide confirmación (o no hay nada que instalar)",
    "Confirmar" in t or "detecta nada" in t,
)
send(b"\x1b", 0.7)

# ── Botón ⟨/⟩ con el ratón ────────────────────────────────────────────────
inicio()
send(b"\x1b[<0;23;21M", 0.2)  # pulsa el botón (columna 22, centro)
send(b"\x1b[<0;23;21m", 1.2)
t = snap("14_lateral_oculta")
check("el botón cierra la lateral", "Proyectos" not in sidebar(t, 5))
check("la lateral colapsada muestra iconos", "⌂" in sidebar(t, 5))
check("aparece el aviso de la lateral", "barra lateral oculta" in t)

send(b"\x1b[<0;5;21M", 0.2)  # ahora el botón está en la columna 4
send(b"\x1b[<0;5;21m", 1.2)
t = snap("15_lateral_abierta")
check("el botón vuelve a abrir la lateral", "Proyectos" in sidebar(t, 22))
check("la lateral restaurada muestra nombres", "Inicio" in sidebar(t, 22))

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
