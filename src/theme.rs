//! Persistent user-selectable display themes. No document or print data here.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Light,
    DarkBlue,
    Parchment,
    Sage,
    WarmGraphite,
    AmberTerminal,
}

impl Theme {
    pub const ALL: [Theme; 6] = [
        Theme::Light,
        Theme::DarkBlue,
        Theme::Parchment,
        Theme::Sage,
        Theme::WarmGraphite,
        Theme::AmberTerminal,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::DarkBlue => "dark-blue",
            Self::Parchment => "parchment",
            Self::Sage => "sage",
            Self::WarmGraphite => "warm-graphite",
            Self::AmberTerminal => "amber-terminal",
        }
    }

    pub fn from_slug(value: &str) -> Self {
        match value.trim() {
            "dark-blue" => Self::DarkBlue,
            "parchment" => Self::Parchment,
            "sage" => Self::Sage,
            "warm-graphite" => Self::WarmGraphite,
            "amber-terminal" => Self::AmberTerminal,
            _ => Self::Light,
        }
    }

    pub fn label_ru(self) -> &'static str {
        match self {
            Self::Light => "Светлая",
            Self::DarkBlue => "Тёмно-синяя",
            Self::Parchment => "Пергамент",
            Self::Sage => "Шалфей",
            Self::WarmGraphite => "Тёплый графит",
            Self::AmberTerminal => "Янтарный терминал",
        }
    }

    pub fn label_en(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::DarkBlue => "Dark blue",
            Self::Parchment => "Parchment",
            Self::Sage => "Sage",
            Self::WarmGraphite => "Warm graphite",
            Self::AmberTerminal => "Amber terminal",
        }
    }
}
