//! The screen's colours: `light`, a light page, as a reference manager's
//! looks, painted whatever the terminal's own colours; or `dark`, in the
//! terminal's own background.

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
    pub folder: Color,
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
                page: Color::Rgb(255, 255, 255),
                side: Color::Rgb(245, 246, 248),
                text: Color::Rgb(51, 51, 51),
                dim: Color::Rgb(128, 131, 137),
                title: Color::Rgb(31, 57, 104),
                venue: Color::Rgb(46, 125, 50),
                accent: Color::Rgb(58, 106, 178),
                on_accent: Color::Rgb(255, 255, 255),
                row: Color::Rgb(232, 239, 250),
                rule: Color::Rgb(229, 231, 235),
                folder: Color::Rgb(240, 192, 64),
                star: Color::Rgb(232, 170, 30),
                badge: Color::Rgb(243, 244, 246),
                on_badge: Color::Rgb(110, 114, 120),
                field: Color::Rgb(255, 255, 255),
                add: Color::Rgb(56, 142, 60),
            },
            Name::Dark => Theme {
                page: Color::Reset,
                side: Color::Reset,
                text: Color::Reset,
                dim: Color::DarkGray,
                title: Color::LightBlue,
                venue: Color::Green,
                accent: Color::Yellow,
                on_accent: Color::Black,
                row: Color::Indexed(236),
                rule: Color::Indexed(238),
                folder: Color::Yellow,
                star: Color::Yellow,
                badge: Color::Gray,
                on_badge: Color::Black,
                field: Color::Indexed(236),
                add: Color::Green,
            },
        }
    }
}
