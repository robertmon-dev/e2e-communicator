use ratatui::style::Color;

// ------------------------------------------------------------------------------------------
// Main palette
// ------------------------------------------------------------------------------------------

pub const RED: Color = Color::Rgb(0xf7, 0x76, 0x8e);
pub const ORANGE: Color = Color::Rgb(0xff, 0x9e, 0x64);
pub const YELLOW: Color = Color::Rgb(0xe0, 0xaf, 0x68);
pub const BEIGE: Color = Color::Rgb(0xcf, 0xc9, 0xc2);

pub const GREEN: Color = Color::Rgb(0x9e, 0xce, 0x6a);
pub const TEAL: Color = Color::Rgb(0x73, 0xda, 0xca);
pub const CYAN_PALE: Color = Color::Rgb(0xb4, 0xf9, 0xf8);
pub const CYAN: Color = Color::Rgb(0x2a, 0xc3, 0xde);
pub const CYAN_DIM: Color = Color::Rgb(0x41, 0xa6, 0xb5);

pub const BLUE_SKY: Color = Color::Rgb(0x7d, 0xcf, 0xff);
pub const BLUE: Color = Color::Rgb(0x7a, 0xa2, 0xf7);
pub const PURPLE: Color = Color::Rgb(0xbb, 0x9a, 0xf7);
pub const LAVENDER: Color = Color::Rgb(0xc0, 0xca, 0xf5);

pub const GREY_BLUE: Color = Color::Rgb(0xa9, 0xb1, 0xd6);
pub const GREY_MUTED: Color = Color::Rgb(0x9a, 0xa5, 0xce);
pub const SLATE: Color = Color::Rgb(0x56, 0x5f, 0x89);
pub const STORM: Color = Color::Rgb(0x41, 0x48, 0x68);

pub const BG_NIGHT: Color = Color::Rgb(0x1a, 0x1b, 0x26);
pub const BG_STORM: Color = Color::Rgb(0x24, 0x28, 0x3b);

// ------------------------------------------------------------------------------------------
// Backgrounds
// ------------------------------------------------------------------------------------------

pub const BG_APP: Color = BG_NIGHT;
pub const BG_SURFACE: Color = BG_STORM;
pub const BG_SIDEBAR: Color = STORM;
pub const BG_INPUT: Color = Color::Rgb(0x16, 0x17, 0x20);

// ------------------------------------------------------------------------------------------
// Interactions
// ------------------------------------------------------------------------------------------

pub const PRIMARY: Color = BLUE;
pub const HIGHLIGHT: Color = CYAN;
pub const HIGHLIGHT_DIM: Color = CYAN_DIM;
pub const DANGER: Color = RED;
pub const SUCCESS: Color = GREEN;
pub const WARNING: Color = ORANGE;
pub const INFO: Color = YELLOW;

// ------------------------------------------------------------------------------------------
// Text
// ------------------------------------------------------------------------------------------

pub const TEXT_MAIN: Color = LAVENDER;
pub const TEXT_MUTED: Color = SLATE;
pub const TEXT_DIM: Color = GREY_MUTED;
pub const TEXT_ON_ACCENT: Color = Color::Rgb(0x15, 0x16, 0x1e);
pub const TEXT_LINK: Color = CYAN_PALE;
pub const BORDER: Color = STORM;

// ------------------------------------------------------------------------------------------
// Syntax tokens, for the raw frame view
// ------------------------------------------------------------------------------------------

pub const TOKEN_KEY: Color = TEAL;
pub const TOKEN_STRING: Color = GREEN;
pub const TOKEN_NUMBER: Color = ORANGE;
pub const TOKEN_PUNCTUATION: Color = Color::Rgb(0x9a, 0xbd, 0xf5);
pub const TOKEN_OPERATOR: Color = Color::Rgb(0x89, 0xdd, 0xff);
pub const TOKEN_COMMENT: Color = SLATE;

pub fn border_style(focused: bool) -> ratatui::style::Style {
    ratatui::style::Style::default().fg(if focused { HIGHLIGHT } else { BORDER })
}
