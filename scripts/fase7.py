#!/usr/bin/env python3
"""Comprobaciones de la Fase 7 en un pty: terminales, radio y sistema.

Cubre: el terminal embebido de Entretenimiento (`t`), su salida con `Ctrl+G`,
el cambio de aplicación con `Ctrl+O`, la detención con `Ctrl+T`, las teclas
`g` (emisoras y géneros) y `d` (descargador por URL), el hint nuevo del pie,
la categoría **Terminal** de Ajustes con sus filas (sesión, detener, brillo,
modo terminal y reinicio) y que los diálogos destructivos se cancelan con
`Esc`. En ningún momento se apaga la sesión ni se reinicia la máquina.

Uso: python3 scripts/fase7.py [--release]
Los raws son opcionales (NEBULA_RAW=1) y van a /tmp/nebula-fase7/ (reciclable).
"""
import fcntl, os, pty, re, select, signal, struct, subprocess, sys, termios, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import ansi  # noqa: E402

profile = "release" if "--release" in sys.argv else "debug"
BIN = os.path.join(ROOT, "target", profile, "nebula")
OUT = "/tmp/nebula-fase7"
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
    """Bombea hasta que la pantalla cumple `pred` (o se acaba el tiempo)."""
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


def content(t, start=24):
    return "\n".join(l[start:] for l in lines(t))


def hint_row(t):
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


def count(name):
    out = subprocess.run(["pgrep", "-c", "-x", name], capture_output=True, text=True)
    try:
        return int(out.stdout.strip() or "0")
    except ValueError:
        return 0


def goto(n):
    """Va a la pestaña n con BackTab desde Inicio (n = 11 es la última)."""
    send(b"1", 1.0)
    for _ in range((12 - n) % 12):
        send(b"\x1b[Z", 0.25)


def panel_title(t):
    """Título del panel del terminal (▸ etiqueta · estado)."""
    for l in lines(t):
        if "▸" in l and "·" in l and ("en marcha" in l or "finalizado" in l or "sin abrir" in l):
            return l.strip()
    return ""


def daemon_up():
    return subprocess.run(
        ["cliamp", "status"], capture_output=True, text=True, timeout=20
    ).returncode == 0 and "not running" not in (
        subprocess.run(["cliamp", "status"], capture_output=True, text=True, timeout=20).stdout
    )


# ── Estado previo (para restaurarlo al final) ─────────────────────────────
cmus_before, cliamp_before = count("cmus"), count("cliamp")
term0 = terminals()

# ── Arranque ──────────────────────────────────────────────────────────────
send(b"", 1.2)
send(b"\x1b", 1.2)  # Esc: salta el splash
t = snap("01_inicio")
check("arranca y salta el splash",
      "N É B U L A" not in t and re.search(r"\d\d:\d\d", head(t, 14)))

# ── Entretenimiento: el pie enseña las teclas nuevas ──────────────────────
goto(9)
t = snap("10_pie")
check("el pie de Entretenimiento anuncia t, g y d",
      all(s in hint_row(t) for s in ["terminal", "emisoras", "descargar"]))

# ── Terminal embebido: t ──────────────────────────────────────────────────
send(b"t", 3.0)
t = snap("11_terminal")
title = panel_title(t)
check("t abre el panel del terminal", "▸" in title and "·" in title)
first = title
check("el panel muestra el proceso en marcha", "en marcha" in title)
check("el pie del panel explica cómo salir", "ctrl+g" in t and "ctrl+t" in t)
check("no se ha abierto ninguna terminal externa", terminals() <= term0)

# ── Ctrl+O cambia de aplicación ───────────────────────────────────────────
send(b"\x0f", 4.0)  # Ctrl+O
t = snap("12_cambio")
second = panel_title(t)
check("Ctrl+O cambia al otro reproductor", second != first and "▸" in second)

# ── q es de la aplicación, no de Nébula ───────────────────────────────────
send(b"q", 2.5)
t = snap("13_q_app")
check("q dentro del terminal no cierra Nébula", alive())
check("q termina el proceso y el panel lo nota", "finalizado" in panel_title(t))

# ── Ctrl+T detiene lo que esté en pantalla ────────────────────────────────
send(b"t", 3.0)  # reabre el mismo terminal (proceso nuevo)
send(b"\x14", 2.0)  # Ctrl+T
t = snap("14_detenido")
check("Ctrl+T detiene el proceso del panel", "detenido" in t)

# ── Ctrl+G sale del panel ─────────────────────────────────────────────────
send(b"\x07", 0.8)  # Ctrl+G
t = snap("15_salida")
check("Ctrl+G vuelve al módulo",
      "play/pausa" in hint_row(t) and "ctrl+t" not in t)

# ── Menú nativo de emisoras y géneros (g) ─────────────────────────────────
# El daemon tiene que estar: si algo lo paró, se levanta aquí.
subprocess.Popen(["cliamp", "--daemon"], stdout=subprocess.DEVNULL,
                 stderr=subprocess.DEVNULL, start_new_session=True)
end = time.time() + 45
while time.time() < end and not daemon_up():
    time.sleep(1.0)

send(b"g", 1.0)
t = snap("20_emisoras")
check("g abre el menú de emisoras y géneros", "emisoras y géneros" in t)
ok = wait_for(lambda s: "géneros" in s and ("emisoras" in s), 45.0)
t = snap("21_listas")
check("trae emisoras y géneros de cliamp", ok)
check("el pie del menú explica sus teclas", "intro reproducir" in t)

send(b"/", 0.4)  # filtro
send(b"e", 0.4)
t = snap("22_filtro")
check("el filtro estrecha la lista", "filtrar" in t)
send(b"\x1b", 0.5)
send(b"\x1b", 0.5)
t = snap("23_cierra")
check("Esc cierra el menú", "intro reproducir" not in t)

# ── Descargador (d) ───────────────────────────────────────────────────────
send(b"d", 0.7)
t = snap("30_descarga")
check("d abre el descargador", "descargar música" in t)
check("ofrece el formato y cómo cambiarlo",
      "formato" in t and "mp3" in t and "←/→" in t)

send(b"\r", 0.5)  # Enter sin URL
t = snap("31_sin_url")
check("Enter sin URL avisa del fallo", "pega la URL" in t)

send(b"\x1b[C", 0.4)  # → cambia el formato
t = snap("32_formato")
check("→ cambia el formato de audio", "m4a" in t)
send(b"\t", 0.4)  # Tab: cargar en cmus
t = snap("33_cmus")
check("Tab alterna la carga en cmus", "cargar al terminar" in t)
send(b"\x1b", 0.5)
t = snap("34_cierra")
check("Esc cierra el descargador", "intro descargar" not in t)

# ── Ajustes → categoría Terminal ──────────────────────────────────────────
send(b"\x1b[12~", 1.2)  # F2 → Ajustes
for _ in range(7):
    send(b"\x1b[C", 0.2)  # →
t = snap("40_terminal")
check("la octava categoría es Terminal", "Terminal" in t)
check("lista los dos terminales",
      all(s in content(t) for s in ["terminal de cmus", "terminal de cliamp"]))
check("lista detener, brillo y modo terminal",
      all(s in content(t) for s in ["detener cmus", "detener cliamp", "brillo", "modo terminal"]))
check("muestra la sesión en la que vivimos", "objetivo" in content(t))
check("el pie de Terminal explica sus teclas", "brillo" in hint_row(t))

# ── Detener desde Ajustes ─────────────────────────────────────────────────
for _ in range(3):
    send(b"\x1b[B", 0.3)  # ↓ hasta «detener cmus»
send(b"\r", 1.5)
t = snap("41_detener")
check("detener cmus responde con un aviso",
      any(s in status_row(t) for s in ["detenido", "no estaba corriendo"]))

# ── Modo terminal: sólo el diálogo, nunca la orden ────────────────────────
for _ in range(3):
    send(b"\x1b[B", 0.3)  # ↓ hasta «modo terminal»
send(b"\r", 0.8)
t = snap("42_modo_terminal")
check("el modo terminal pide confirmación",
      "Modo terminal" in t and "TTY" in t)
send(b"\x1b", 0.5)  # Esc: se cancela
t = snap("43_cancela")
check("Esc cancela sin salir de Nébula", alive() and "Modo terminal" not in t)

send(b"\x1b[B", 0.3)  # ↓ reiniciar
send(b"\r", 0.8)
t = snap("44_reinicio")
check("reiniciar también pide confirmación", "Reiniciar el equipo" in t)
send(b"\x1b", 0.5)
check("y se cancela igual", "Reiniciar el equipo" not in screen())

# ── Brillo con las teclas del portátil ────────────────────────────────────
send(b"+", 0.6)
send(b"+", 0.6)
send(b"-", 0.6)
t = snap("45_brillo")
check("las teclas de brillo no rompen nada", alive())

# ── La consola de 16 colores no cambia el comportamiento ──────────────────
check("sigue viva tras todo el recorrido", alive())
term1 = terminals()
check("no se ha abierto ninguna terminal externa", term1 <= term0)

# ── La consola de 16 colores: el tema viaja como índice ───────────────────
# Segunda instancia con TERM=linux (lo que hay en la TTY de un servidor):
# su salida cruda no debe llevar ni un solo color RGB.
p2, f2 = pty.fork()
if p2 == 0:
    os.environ["TERM"] = "linux"
    os.execvp(BIN, [BIN])
fcntl.ioctl(f2, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
raw2 = bytearray()
for _ in range(3):  # arranque + Esc + pintado
    end = time.time() + 2.5
    while time.time() < end:
        r, _, _ = select.select([f2], [], [], 0.1)
        if r:
            try:
                d = os.read(f2, 65536)
            except OSError:
                d = b""
            if not d:
                break
            raw2.extend(d)
    try:
        os.write(f2, b"\x1b")
    except OSError:
        break
try:
    os.kill(p2, signal.SIGKILL)
    os.waitpid(p2, 0)
except (ProcessLookupError, ChildProcessError):
    pass
blob = bytes(raw2)
check("con TERM=linux no viaja ningún color RGB",
      re.search(rb"\x1b\[(?:38|48);2;", blob) is None)
check("en cambio sí viajan los 16 colores por índice",
      re.search(rb"\x1b\[(?:38|48);5;", blob) is not None)

# ── Limpieza y restauración del estado previo ─────────────────────────────
subprocess.run(["pkill", "-x", "cmus"], capture_output=True)
subprocess.run(["pkill", "-x", "cliamp"], capture_output=True)
time.sleep(1.0)
if cmus_before:
    subprocess.Popen(["cmus"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                     start_new_session=True)
if cliamp_before:
    subprocess.Popen(["cliamp", "--daemon"], stdout=subprocess.DEVNULL,
                     stderr=subprocess.DEVNULL, start_new_session=True)

send(b"q", 1.2)
if alive():
    print("[warn] no salió con q; mato el proceso")
    os.kill(pid, signal.SIGKILL)
    fails += 1
else:
    print("[ok] salida limpia con q")

print(f"\n{'TODO CORRECTO' if fails == 0 else f'{fails} FALLO(S)'}")
sys.exit(1 if fails else 0)
