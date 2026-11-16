# Changelog

Todo lo notable de Nébula, formato basado en [Keep a Changelog](https://keepachangelog.com/es/1.1.0/),
y el proyecto sigue [SemVer](https://semver.org/lang/es/).

## [Sin publicar]

### Cambiado
- **Anime**: «buscar anime» ya no busca dentro de la TUI — abre `ani-cli`
  en la consola embebida y le deja el flujo entero: él pide la búsqueda,
  él lista los animes y los capítulos y **sólo reproduce el que tú elijas**
  (adiós a que arrancara un capítulo sin escogerlo). Fuera la ficha de
  AniList, la fase de episodio y las teclas `e`/`D`.
- La búsqueda de vídeo (yt-dlp) sigue igual, dentro de la TUI.

## [0.7.0] - 2026-11-14

Versión "terminales embebidas": Nébula crece como centro de mando y limpia su salida.

### Añadido
- **Señales del sistema** (`src/signals.rs`): módulo dedicado para capturar y reaccionar a señales Unix.
- **Consola embebida**: `Alt+T` abre una **tercera PTY** con `Job::Shell` — una terminal real
  dentro de Nébula, sin emulador externo.
- **Radio**: reproductor de emisoras añadido al módulo de entretenimiento.
- **Siembra de cmus**: Nébula arranca y sincroniza `cmus` si está instalado.
- **`install.sh`**: instalador que crea `~/Música/Nébula` y copia el binario a `~/.local/bin`.
- **`scripts/mant.py`**: suite de mantenimiento (13 comprobaciones) para regresiones rápidas.

### Mejorado
- **Salida limpia**: Nébula restaura la terminal al salir sin dejar restos de estado.
- Documentación ampliada: README, ayuda en pantalla (`HELP`), `GITHUB.md`.

### Técnico
- `.gitignore` del proyecto; CI con GitHub Actions (`fmt` + `clippy -D warnings` + `build`).
- Suite completa verde: smoke (20) + fase 7 (38) + mantenimiento (13/13).

## [0.6.0] y anteriores

Las fases 1 a 6 (Inicio, Sistema, Proyectos, Git, Desarrollo, Red, Paquetes,
Servicios, Contenedores, Notas, Ajustes, temas y fondo de pantalla real)
quedan descritas en la historia del repositorio.

[0.7.0]: https://github.com/L3IB0R/nebula/releases/tag/v0.7.0
