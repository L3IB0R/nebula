#!/usr/bin/env python3
"""Comprobaciones de la Fase 5 en un pty: todo se ejecuta DENTRO de Nébula.

Cubre: el visor interno de salida (git status/diff/log, systemctl), el menú
de acciones y el commit de dos campos de git, el panel de instalación de apt,
la búsqueda de anime sin terminal externa y el arranque de cliamp/cmus desde
la pestaña Música. En ningún momento debe abrirse otra terminal.

Uso: python3 scripts/fase5.py [--release]
Los raws son opcionales (NEBULA_RAW=1) y van a /tmp/nebula-fase5/ (reciclable).
"""
import fcntl, os, pty, re, select, signal, struct, subprocess, sys, termios, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import ansi  # noqa: E402

profile = "release" if "--release" in sys.argv else "debug"
BIN = os.path.join(ROOT, "target", profile, "nebula")
OUT = "/tmp/nebula-fase5"
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


def head(t, n):
    return "\n".join(t.split("\n")[:n])


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


def viewer_open(t):
    return "$" in t and ("terminado" in t or "en marcha" in t)


# ── Arranque ──────────────────────────────────────────────────────────────
send(b"", 1.2)
send(b"\x1b", 1.2)  # Esc: salta el splash
t = snap("01_inicio")
check("arranca y salta el splash", "N É B U L A" not in t and re.search(r"\d\d:\d\d", head(t, 14)))
term0 = terminals()

# ── Git: estatus en el visor interno ──────────────────────────────────────
goto(7)
t = snap("02_git")
check("la pestaña Git abre", "Repos" in t and "Estado" in t)

send(b"\r", 1.5)  # Intro → git status dentro de Nébula
t = snap("03_git_status")
check("Intro abre el visor con git status", viewer_open(t) and "git status" in t)
check("la salida del estatus se ve dentro", "master" in t or "??" in t or "commits" in t)
check("el visor muestra su pie de ayuda", "esc cerrar" in t)
check("el estatus terminó sin errores", "terminado" in t and "errores" not in t)

send(b"\x1b", 0.6)  # Esc: cierra el visor
t = snap("04_git_cerrado")
check("Esc cierra el visor", not viewer_open(t))

send(b"d", 1.5)  # git diff
t = snap("05_git_diff")
check("la tecla d abre git diff aquí dentro", viewer_open(t) and "git diff" in t)
send(b"\x1b", 0.5)

send(b"m", 0.7)  # Menú de acciones
t = snap("06_git_acciones")
check("la tecla m abre el menú de acciones", "Acciones" in t)
check("lista las acciones de git", all(s in t for s in ["estatus", "diferencias", "bitácora", "subir"]))
send(b"\r", 1.5)  # Intro en «estatus»
t = snap("07_git_accion_estatus")
check("ejecuta la acción dentro del visor", viewer_open(t) and "git status" in t)
send(b"\x1b", 0.5)

send(b"m", 0.6)
send(b"\x1b[B", 0.2)
send(b"\x1b[B", 0.2)
send(b"\r", 1.5)  # bitácora (git log)
t = snap("08_git_log")
check("git log se ve dentro (con su error si no hay commits)",
      viewer_open(t) and ("git log" in t or "oneline" in t))
send(b"\x1b", 0.5)

# ── Git: commit de dos campos ─────────────────────────────────────────────
send(b"c", 0.7)
t = snap("09_git_commit")
check("la tecla c abre el commit", "Nuevo commit" in t)
check("el commit separa título y descripción", "título" in t and "descripción" in t)
check("recuerda el atajo de campos", "tab campo" in t)
send(b"\t", 0.3)  # Tab → título
send(b"prueba fase cinco", 0.5)
t = snap("10_git_commit_texto")
check("Tab cambia de campo y el texto entra", "prueba fase cinco" in t)
send(b"\x1b", 0.5)

# ── Paquetes: panel de instalación apt ────────────────────────────────────
goto(3)
t = snap("11_paquetes")
check("la pestaña Paquetes abre", "Paquetes" in head(t, 4))

send(b"i", 0.7)
t = snap("12_instalar")
check("la tecla i abre el panel de instalación", "Instalar con apt" in t)
send(b"cmus timg", 0.4)
t = snap("13_instalar_texto")
check("se escriben los paquetes aquí dentro", "cmus timg" in t)
check("el panel avisa de que la salida es interna", "apt install -y" in t)
send(b"\x1b", 0.5)
t = snap("14_instalar_cerrado")
check("Esc cancela la instalación", "Instalar con apt" not in t)

send(b"f", 0.7)
t = snap("15_flatpak")
check("la tecla f pide confirmación (sin terminal)", "Confirmar" in t and "flatpak" in t)
send(b"n", 0.4)
t = snap("16_flatpak_no")
check("n cancela y no se abre nada", "Confirmar" not in t)

# ── Servicios: estado en el visor ─────────────────────────────────────────
goto(4)
t = snap("17_servicios")
check("la pestaña Servicios abre", "Servicios" in head(t, 4))
if re.search(r"\b(apache|ssh|NetworkManager|bluetooth|cups|cron|getty)\b", t):
    send(b"\r", 1.5)
    t = snap("18_servicio_estado")
    check("el estado del servicio se ve dentro", viewer_open(t) and "systemctl" in t)
    send(b"\x1b", 0.5)
else:
    print("[warn] sin servicios que seleccionar; se omite")

# ── Entretenimiento: buscar anime sin terminal ────────────────────────────
goto(9)
t = snap("19_entretenimiento")
check("la pestaña Entretenimiento abre", "Entretenimiento" in head(t, 4))

send(b"\r", 0.8)  # Intro en «buscar anime»
t = snap("20_buscar_anime")
check("abre la búsqueda dentro de Nébula", "buscar anime" in t and "intro buscar" in t)
send(b"naruto", 0.4)
send(b"\r", 0.5)
ok = wait_for(lambda s: "resultados" in s or "sin resultados" in s or "no se pudo" in s, 8.0)
t = snap("21_resultados")
check("los resultados aparecen en la lista", ok and "resultados" in t)
check("lista títulos con su detalle", "NARUTO" in t.upper() and "epi" in t)
send(b"\x1b", 0.4)  # → consulta
send(b"\x1b", 0.4)  # → cierra
t = snap("22_buscar_cerrado")
check("Esc vuelve a la pestaña", "intro buscar" not in t)

# ── Música: cliamp y cmus embebidos en Entretenimiento ────────────────────
# Estado limpio: si quedó algo sonando de pruebas previas, el motor elegido
# para el espacio no sería el esperado.
subprocess.run(["pkill", "-x", "cmus"], capture_output=True)
subprocess.run(["pkill", "-x", "cliamp"], capture_output=True)
time.sleep(1.0)
goto(9)
t = snap("23_musica")
check("el reproductor vive en Entretenimiento", "Ahora suena" in t or "Nada sonando" in t)
check("el lateral muestra el grupo cliamp", "cliamp · online" in t)

send(b"m", 0.7)
t = snap("24_menu_musica")
check("el menú ofrece arrancar cmus", "arrancar cmus" in t)
check("y los dos instaladores", "instalar cmus" in t and "instalar cliamp" in t)
send(b"\x1b", 0.5)

# cliamp primero: el daemon arranca solo y el aviso llega aquí dentro.
send(b" ", 1.0)
ok = wait_for(lambda s: "reproduciendo" in s or "en pausa" in s, 45.0)
t = snap("25_cliamp")
check("el espacio arranca cliamp y avisa aquí dentro", ok)
subprocess.run(["timeout", "4", "cliamp", "stop"], capture_output=True)

# cmus en su pty invisible, controlado desde Nébula.
send(b"a", 2.5)
ok = wait_for(lambda s: "corriendo en segundo plano" in s, 6.0)
t = snap("26_cmus")
check("la tecla a arranca cmus sin terminal", ok)
cmus_ok = subprocess.run(
    ["timeout", "4", "cmus-remote", "-Q"], capture_output=True
).returncode == 0
check("cmus-remote obedece a cmus (pty)", cmus_ok)

# ── Sin fugas de terminal ─────────────────────────────────────────────────
t = snap("27_final")
term1 = terminals()
check("no se ha abierto ninguna terminal externa", term1 <= term0)
check("sigue viva tras todo el recorrido", alive())

# ── Limpieza ──────────────────────────────────────────────────────────────
subprocess.run(["pkill", "-x", "cmus"], capture_output=True)
send(b"q", 1.2)
if alive():
    print("[warn] no salió con q; mato el proceso")
    os.kill(pid, signal.SIGKILL)
    fails += 1
else:
    print("[ok] salida limpia con q")

print(f"\n{'TODO CORRECTO' if fails == 0 else f'{fails} FALLO(S)'}")
sys.exit(1 if fails else 0)
