/// ANSI color/style helpers.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Always,
    Never,
    Auto,
}

impl ColorMode {
    pub fn should_color(self) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => std::env::var("NO_COLOR").is_err(),
        }
    }
}

impl std::str::FromStr for ColorMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            "auto" => Ok(Self::Auto),
            _ => Err(format!("invalid color mode: {s}")),
        }
    }
}

pub struct Style {
    enabled: bool,
}

impl Style {
    pub fn new(mode: ColorMode) -> Self {
        Self {
            enabled: mode.should_color(),
        }
    }

    fn esc(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn dim(&self, text: &str) -> String {
        self.esc("2", text)
    }

    pub fn fg(&self, color: Color, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{}m{text}\x1b[0m", color.code())
        } else {
            text.to_string()
        }
    }

    pub fn bold_fg(&self, color: Color, text: &str) -> String {
        if self.enabled {
            format!("\x1b[1;{}m{text}\x1b[0m", color.code())
        } else {
            text.to_string()
        }
    }
}

#[derive(Clone, Copy)]
pub enum Color {
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
}

impl Color {
    fn code(self) -> u8 {
        match self {
            Self::Red => 31,
            Self::Green => 32,
            Self::Yellow => 33,
            Self::Blue => 34,
            Self::Magenta => 35,
            Self::Cyan => 36,
        }
    }
}
