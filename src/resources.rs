//! Catálogo de recursos opcionales, agrupados en secciones.
//!
//! Alimenta la pestaña **Recursos** de Ajustes: cada ítem sabe si ya está
//! instalado (binario en el PATH o paquete dpkg) y cómo instalarlo.
//!
//! La consulta se cachea unos segundos: `dpkg-query` es un proceso aparte y
//! no tiene sentido lanzarlo en cada fotograma.

use crate::data::{pkg_installed, which};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Un recurso instalable.
pub struct Item {
    /// Binario a comprobar con `which`. Vacío = sólo se mira el paquete.
    pub bin: &'static str,
    /// Paquete apt. Vacío = no se puede instalar con apt.
    pub apt: &'static str,
    /// Nombre visible.
    pub label: &'static str,
    /// Instalador alternativo (vacío = apt). Se ejecuta con `sh -c`.
    pub cmd: &'static str,
}

/// Grupo de recursos con un título.
pub struct Section {
    pub icon: &'static str,
    pub name: &'static str,
    pub items: &'static [Item],
}

const fn it(bin: &'static str, apt: &'static str, label: &'static str) -> Item {
    Item {
        bin,
        apt,
        label,
        cmd: "",
    }
}

pub const SECTIONS: &[Section] = &[
    Section {
        icon: "❖",
        name: "programación",
        items: &[
            it("git", "git", "git"),
            it("curl", "curl", "curl"),
            it("", "build-essential", "compilador C/C++"),
            it("python3", "python3", "python 3"),
            it("node", "nodejs", "node.js"),
        ],
    },
    Section {
        icon: "♪",
        name: "música",
        items: &[
            it("cmus-remote", "cmus", "cmus (offline)"),
            Item {
                bin: "cliamp",
                apt: "",
                label: "cliamp (online)",
                cmd: "curl -fsSL https://cliamp.dev/install.sh | sh",
            },
            it("mpd", "mpd", "mpd"),
            it("playerctl", "playerctl", "playerctl"),
            it("yt-dlp", "yt-dlp", "yt-dlp (descargas)"),
            it("ffmpeg", "ffmpeg", "ffmpeg"),
        ],
    },
    Section {
        icon: "⚙",
        name: "sistema",
        items: &[
            it("brightnessctl", "brightnessctl", "brightnessctl"),
            it("zoxide", "zoxide", "zoxide"),
            it("tmux", "tmux", "tmux"),
            it("htop", "htop", "htop"),
            it("tree", "tree", "tree"),
        ],
    },
    Section {
        icon: "▣",
        name: "contenedores",
        items: &[
            it("docker", "docker.io", "docker"),
            it("podman", "podman", "podman"),
            it("flatpak", "flatpak", "flatpak"),
        ],
    },
    Section {
        icon: "⇄",
        name: "red",
        items: &[
            it("nmcli", "network-manager", "network-manager"),
            it("ss", "iproute2", "iproute2 (ss)"),
            it("ssh", "openssh-client", "openssh-client"),
        ],
    },
];

// ── Caché de detección ────────────────────────────────────────────────────

static CACHE: Mutex<Option<(Instant, Vec<bool>)>> = Mutex::new(None);
const TTL: Duration = Duration::from_secs(3);

/// Estado de todos los ítems, en orden plano (sección por sección).
pub fn flags() -> Vec<bool> {
    if let Ok(g) = CACHE.lock() {
        if let Some((at, v)) = g.as_ref() {
            if at.elapsed() < TTL {
                return v.clone();
            }
        }
    }
    let v: Vec<bool> = SECTIONS
        .iter()
        .flat_map(|s| s.items.iter())
        .map(|i| {
            if !i.bin.is_empty() && which(i.bin).is_some() {
                true
            } else {
                !i.apt.is_empty() && pkg_installed(i.apt)
            }
        })
        .collect();
    if let Ok(mut g) = CACHE.lock() {
        *g = Some((Instant::now(), v.clone()));
    }
    v
}

/// Fuerza una nueva consulta (tras instalar algo).
pub fn refresh() {
    if let Ok(mut g) = CACHE.lock() {
        *g = None;
    }
}

// ── Consultas ─────────────────────────────────────────────────────────────

/// Índice plano de un ítem.
pub fn flat_index(section: usize, item: usize) -> Option<usize> {
    let mut i = 0;
    for (si, s) in SECTIONS.iter().enumerate() {
        for ii in 0..s.items.len() {
            if si == section && ii == item {
                return Some(i);
            }
            i += 1;
        }
    }
    None
}

/// ¿Está instalado el ítem (sección, item)?
pub fn is_installed(section: usize, item: usize) -> bool {
    flat_index(section, item)
        .and_then(|i| flags().get(i).copied())
        .unwrap_or(false)
}

/// Nº total de recursos del catálogo.
pub fn total() -> usize {
    SECTIONS.iter().map(|s| s.items.len()).sum()
}

/// Nº de recursos ya instalados.
pub fn installed_count() -> usize {
    flags().iter().filter(|f| **f).count()
}

/// Paquetes apt que faltan (para «instalar todo»).
pub fn missing_pkgs() -> Vec<String> {
    let f = flags();
    let mut i = 0;
    let mut out = Vec::new();
    for s in SECTIONS {
        for item in s.items {
            let ok = f.get(i).copied().unwrap_or(false);
            i += 1;
            if !ok && !item.apt.is_empty() {
                out.push(item.apt.to_string());
            }
        }
    }
    out
}

/// Instaladores que no son apt (p. ej. cliamp).
pub fn missing_cmds() -> Vec<(String, String)> {
    let f = flags();
    let mut i = 0;
    let mut out = Vec::new();
    for s in SECTIONS {
        for item in s.items {
            let ok = f.get(i).copied().unwrap_or(false);
            i += 1;
            if !ok && item.apt.is_empty() && !item.cmd.is_empty() {
                out.push((item.label.to_string(), item.cmd.to_string()));
            }
        }
    }
    out
}

/// Paquetes apt que faltan dentro de una sección concreta.
pub fn missing_in(section: usize) -> Vec<String> {
    SECTIONS
        .get(section)
        .map(|s| {
            s.items
                .iter()
                .enumerate()
                .filter(|(ii, item)| !is_installed(section, *ii) && !item.apt.is_empty())
                .map(|(_, item)| item.apt.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Instaladores no-apt que faltan dentro de una sección (label, cmd).
pub fn missing_cmds_in(section: usize) -> Vec<(String, String)> {
    let Some(s) = SECTIONS.get(section) else {
        return Vec::new();
    };
    s.items
        .iter()
        .enumerate()
        .filter(|(ii, item)| {
            !is_installed(section, *ii) && item.apt.is_empty() && !item.cmd.is_empty()
        })
        .map(|(_, item)| (item.label.to_string(), item.cmd.to_string()))
        .collect()
}
