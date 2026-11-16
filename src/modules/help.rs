//! Ayuda: mapa de atajos.

use crate::action::Action;
use crate::app::Ctx;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

#[derive(Default)]
pub struct Help {
    pub page: usize,
}

const PAGES: &[(&str, &[(&str, &str)])] = &[
    (
        "General",
        &[
            ("Ctrl+K / espacio", "abrir el lanzador"),
            ("Tab · Shift+Tab", "siguiente / anterior módulo"),
            (
                "secciones",
                "el lateral agrupa: general · sistema · código · ocio · configuración",
            ),
            ("1 … 9, 0", "ir al módulo directo"),
            ("F2", "ajustes"),
            ("?", "esta ayuda"),
            ("Esc", "cerrar lo que tengas abierto"),
            ("q", "salir de Nébula"),
            ("Ctrl+B", "ocultar / mostrar la barra lateral"),
            (
                "Ctrl+E",
                "enfocar la lateral · ↑/↓ saltan las cabeceras de sección",
            ),
            ("F11", "modo zen: sólo fondo y contenido"),
            ("Ctrl+↑ / Ctrl+↓", "volumen +5% / −5%"),
            ("Ctrl+Mayús+↑ / ↓", "brillo +5% / −5%"),
            ("Ctrl+espacio", "silenciar / activar sonido"),
            ("pie de atajos", "las teclas de cada módulo, siempre abajo"),
            ("chip ♪", "clic = play/pausa desde cualquier pestaña"),
            ("ratón", "clic en la lateral · clic en el chip · rueda"),
        ],
    ),
    (
        "Módulos",
        &[
            ("Inicio", "reloj, saludo y widgets · enter abre el lanzador"),
            ("Sistema", "f filtro · / buscar nombre · k matar · m menú"),
            ("Red", "f filtro · / buscar puerto · p ping · m menú"),
            (
                "Paquetes",
                "m panel apt (siempre visible) · i instalar · f flatpak",
            ),
            (
                "Servicios",
                "intro ver estado · / filtrar · s/x/r gestionar · m menú",
            ),
            ("Contenedores", "espacio cambiar vista · s/x/r/d · m menú"),
            ("Proyectos", "/ filtrar · enter editor · g ver git · m menú"),
            (
                "Git",
                "tira arriba: intro estatus · a añadir · u sin stage · c commit",
            ),
            (
                "",
                "   y en la segunda línea: l log · f fetch · p pull · P push · k clonar",
            ),
            (
                "Desarrollo",
                "←/→ sección · enter ejecutar script · m menú · t probar BD",
            ),
            (
                "Entretenimiento",
                "espacio play/pausa · intro buscar vídeo · m menú música",
            ),
            (
                "t / g / d",
                "terminal · emisoras y géneros · descargar música",
            ),
            (
                "",
                "   el terminal es de cmus o de cliamp, con sus propias teclas",
            ),
            (
                "Música",
                "vive dentro de Entretenimiento, con su chip ♪ arriba",
            ),
            (
                "Notas",
                "i editar · ↑/↓ mover · Ctrl+S guardar · tab módulo",
            ),
            ("Ajustes", "←/→ sección · espacio cambiar · w guardar"),
        ],
    ),
    (
        "Terminal",
        &[
            ("t", "abrir el terminal embebido (cmus o cliamp)"),
            ("alt+t", "abrir la consola embebida (shell con login)"),
            ("g", "emisoras y géneros de cliamp, sin salir de aquí"),
            ("d", "descargar música de YouTube por URL"),
            ("ctrl+g", "salir del panel · el proceso sigue vivo"),
            (
                "ctrl+tab / ctrl+o",
                "cambiar de aplicación (cmus ↔ cliamp ↔ consola)",
            ),
            ("ctrl+t", "detener el proceso que está en pantalla"),
            ("sus teclas", "cada aplicación conserva todas las suyas"),
            (
                "señales",
                "SIGTERM/SIGHUP cierran Nébula limpio, sin dejar la TTY rota",
            ),
            ("detener", "Ajustes → Terminal · Entretenimiento → ■"),
            (
                "brillo",
                "Ajustes → Terminal, con + y − (teclas del portátil)",
            ),
            ("reiniciar", "Ajustes → Terminal, con confirmación"),
        ],
    ),
    (
        "Seguridad",
        &[
            ("confirmación", "toda acción destructiva pide un sí/no"),
            ("privilegios", "apt, servicios y docker pasan por pkexec"),
            (
                "visor",
                "git, apt y scripts se ejecutan y se leen aquí dentro",
            ),
            ("git", "las rutas se añaden una a una, nunca «git add .»"),
            ("config", "~/.config/nebula/config.toml"),
            ("notas", "~/.config/nebula/notas.md"),
            ("respaldo", "la config se regenera si se borra"),
        ],
    ),
];

impl Help {
    pub fn render(&self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let t = ctx.theme;
        let block = ui::panel_on(t, ctx.wp, area, " ?  Ayuda de Nébula ", 0.95);
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.height < 6 {
            return;
        }
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(inner);

        let titles: Vec<Line> = PAGES
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                Line::from(Span::styled(
                    format!(" {} ", name),
                    Style::default()
                        .fg(if i == self.page { t.accent } else { t.fg_faint })
                        .add_modifier(if i == self.page {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ))
            })
            .collect();
        f.render_widget(ratatui::widgets::Paragraph::new(titles), rows[0]);

        let (_, entries) = PAGES[self.page.min(PAGES.len() - 1)];
        let lines: Vec<Line> = entries
            .iter()
            .map(|(k, v)| {
                Line::from(vec![
                    Span::styled(
                        format!("  {:<22}", k),
                        Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(v.to_string(), Style::default().fg(t.fg_dim)),
                ])
            })
            .collect();
        f.render_widget(ratatui::widgets::Paragraph::new(lines), rows[1]);

        ui::hint_bar(
            f,
            t,
            rows[2],
            &[
                ("←/→", "cambiar página"),
                ("↑/↓", "desplazar"),
                ("esc", "cerrar"),
            ],
        );
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Left => {
                self.page = (self.page + PAGES.len() - 1) % PAGES.len();
                Some(Action::Keep)
            }
            KeyCode::Right => {
                self.page = (self.page + 1) % PAGES.len();
                Some(Action::Keep)
            }
            _ => None,
        }
    }
}
