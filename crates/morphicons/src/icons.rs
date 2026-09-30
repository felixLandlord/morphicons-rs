//! A small built-in icon set on the 24 grid, drawn in the common
//! stroke-icon style (2px strokes, round caps). Enough for toggles and demos;
//! bring real sets (Lucide, Tabler, …) with [`Icon::from_d`] or
//! [`Icon::from_nodes`].
//!
//! Each function returns a cheap clone of an icon parsed once on first use.

use std::sync::LazyLock;

use crate::Icon;

macro_rules! icons {
    ($($(#[$meta:meta])* $name:ident => $d:literal;)*) => {
        $(
            $(#[$meta])*
            pub fn $name() -> Icon {
                static ICON: LazyLock<Icon> =
                    LazyLock::new(|| Icon::from_d($d).expect("built-in icon is valid"));
                ICON.clone()
            }
        )*

        /// Every built-in icon, by name.
        pub fn all() -> Vec<(&'static str, Icon)> {
            vec![$((stringify!($name), $name()),)*]
        }
    };
}

icons! {
    /// Three horizontal bars (hamburger).
    menu => "M4 6h16M4 12h16M4 18h16";
    /// A diagonal cross (close).
    x => "M18 6 6 18M6 6l12 12";
    plus => "M5 12h14M12 5v14";
    minus => "M5 12h14";
    check => "M20 6 9 17l-5-5";
    arrow_right => "M5 12h14M12 5l7 7-7 7";
    arrow_left => "M19 12H5M12 19l-7-7 7-7";
    arrow_up => "M12 19V5M5 12l7-7 7 7";
    arrow_down => "M12 5v14M19 12l-7 7-7-7";
    chevron_right => "m9 18 6-6-6-6";
    chevron_left => "m15 18-6-6 6-6";
    chevron_up => "m18 15-6-6-6 6";
    chevron_down => "m6 9 6 6 6-6";
    play => "M6 3 20 12 6 21Z";
    pause => "M6 4h4v16H6ZM14 4h4v16h-4Z";
    circle => "M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0Z";
    square => "M3 3h18v18H3Z";
    search => "M3 11a8 8 0 1 0 16 0a8 8 0 1 0-16 0ZM21 21l-4.3-4.3";
}
