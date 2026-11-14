#!/usr/bin/env bash
# ── Nébula · instalador ────────────────────────────────────────────────────
#   ./install.sh            instala en ~/.local/bin (sin sudo)
#   ./install.sh --system   instala en /usr/local/bin (pide sudo)
#   ./install.sh --dev      sólo compila en debug
#   ./install.sh --uninstall  quita la instalación de ~/.local/bin
#   En todos los modos (salvo --uninstall) se crea la carpeta
#   ~/Música/Nébula para las descargas, siguiendo XDG_MUSIC_DIR.
set -euo pipefail

BIN=nebula
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODE="user"
DO_BUILD=1

for arg in "$@"; do
  case "$arg" in
    --system)   MODE="system" ;;
    --dev)      MODE="debug" ;;
    --uninstall) MODE="uninstall" ;;
    -h|--help)
      sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) echo "opción desconocida: $arg (prueba --help)"; exit 1 ;;
  esac
done

CARGO="$HOME/.cargo/bin/cargo"
[ -x "$CARGO" ] || CARGO="$(command -v cargo || true)"
if [ -z "$CARGO" ]; then
  echo "✗ no encuentra cargo. Instala Rust con:"
  echo "    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  exit 1
fi

if [ "$MODE" = "uninstall" ]; then
  rm -f "$HOME/.local/bin/$BIN" /usr/local/bin/$BIN 2>/dev/null || true
  rm -f "$HOME/.local/share/applications/nebula.desktop" 2>/dev/null || true
  echo "✓ Nébula desinstalada (tu ~/.config/nebula se queda intacta)"
  exit 0
fi

# Carpeta de trabajo de las descargas: la usa el descargador de música y
# se crea en todos los modos, respetando el XDG de la usuaria si existe.
[ -f "$HOME/.config/user-dirs.dirs" ] && . "$HOME/.config/user-dirs.dirs"
MUSIC="${XDG_MUSIC_DIR:-$HOME/Música}"
mkdir -p "$MUSIC/Nébula"
echo "♪ carpeta musical: $MUSIC/Nébula"

cd "$HERE"

if [ "$MODE" = "debug" ]; then
  echo "› compilando en debug…"
  "$CARGO" build
  echo "✓ listo: $HERE/target/debug/$BIN   (ejecútalo con ./target/debug/$BIN)"
  exit 0
fi

echo "› compilando en release (esto tarda un poco la primera vez)…"
"$CARGO" build --release
SRC="$HERE/target/release/$BIN"
[ -x "$SRC" ] || { echo "✗ la compilación no produjo $SRC"; exit 1; }

if [ "$MODE" = "system" ]; then
  DEST="/usr/local/bin/$BIN"
  echo "› instalando en $DEST (puede pedirte la contraseña)…"
  sudo install -m 755 "$SRC" "$DEST"
else
  DEST="$HOME/.local/bin/$BIN"
  mkdir -p "$HOME/.local/bin"
  install -m 755 "$SRC" "$DEST"
  case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) echo "· aviso: ~/.local/bin no está en tu PATH; añade a ~/.bashrc:"
       echo '    export PATH="$HOME/.local/bin:$PATH"' ;;
  esac
fi

# Entrada de escritorio (abre Nébula en una terminal)
if [ "$MODE" != "system" ]; then
  APPS="$HOME/.local/share/applications"
  mkdir -p "$APPS"
  cat > "$APPS/nebula.desktop" <<EOF
[Desktop Entry]
Name=Nébula
Comment=Centro de mando TUI con fondo, reloj y música
Exec=$DEST
Terminal=true
Type=Application
Categories=System;Utility;
Keywords=TUI;monitor;launcher;terminal;
EOF
fi

echo
echo "✓ Nébula instalada en $DEST"
echo "  · ejecuta:  $BIN"
echo "  · temas y fondo:  $BIN  →  Ajustes (F2)"
echo "  · desinstala:  $0 --uninstall"
