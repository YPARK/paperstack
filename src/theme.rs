//! The screen's colours: `light`, a page of warm paper with dark blue ink
//! and collections marked in rust, painted whatever the terminal's own
//! colours; or `dark`, in the terminal's own background.

use ratatui::style::Color;

#[derive(Clone, Copy, Debug, Default, PartialEq, clap::ValueEnum)]
pub enum Name {
    #[default]
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    /// The page, behind the papers and the details.
    pub page: Color,
    /// The sidebar's and the foot's ground.
    pub side: Color,
    pub text: Color,
    /// What matters less: counts, years, keys, hints.
    pub dim: Color,
    pub title: Color,
    pub venue: Color,
    /// What is picked and in focus, and what may be acted on.
    pub accent: Color,
    pub on_accent: Color,
    /// The paper or shelf picked, while the other side has the focus.
    pub row: Color,
    /// Lines between papers, and edges.
    pub rule: Color,
    /// A collection's mark on the sidebar.
    pub collection: Color,
    pub star: Color,
    pub badge: Color,
    pub on_badge: Color,
    /// The search box.
    pub field: Color,
    pub add: Color,
}

impl Theme {
    pub fn of(name: Name) -> Theme {
        match name {
            Name::Light => Theme {
                page: Color::Rgb(251, 250, 247),
                side: Color::Rgb(241, 238, 232),
                text: Color::Rgb(43, 43, 43),
                dim: Color::Rgb(138, 133, 126),
                title: Color::Rgb(28, 30, 36),
                venue: Color::Rgb(29, 78, 137),
                accent: Color::Rgb(29, 78, 137),
                on_accent: Color::Rgb(255, 255, 255),
                row: Color::Rgb(231, 236, 244),
                rule: Color::Rgb(230, 226, 218),
                collection: Color::Rgb(168, 84, 43),
                star: Color::Rgb(201, 138, 18),
                badge: Color::Rgb(236, 232, 223),
                on_badge: Color::Rgb(107, 102, 94),
                field: Color::Rgb(255, 255, 255),
                add: Color::Rgb(29, 78, 137),
            },
            Name::Dark => Theme {
                page: Color::Reset,
                side: Color::Reset,
                text: Color::Reset,
                dim: Color::DarkGray,
                title: Color::Reset,
                venue: Color::LightBlue,
                accent: Color::Blue,
                on_accent: Color::White,
                row: Color::Indexed(236),
                rule: Color::Indexed(238),
                collection: Color::Indexed(173),
                star: Color::Yellow,
                badge: Color::Gray,
                on_badge: Color::Black,
                field: Color::Indexed(236),
                add: Color::Blue,
            },
        }
    }
}
