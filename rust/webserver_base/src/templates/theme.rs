//! Light and dark, declared once and used consistently.

use serde::{Deserialize, Serialize};

/// The colour a browser tints its chrome with, and the schemes the site
/// supports.
///
/// The two are one type because they must agree: declaring `color-scheme: light
/// dark` while offering a single theme colour gives a dark page a light address
/// bar, and declaring `color-scheme: light` on a site with dark styles renders
/// its form controls and scrollbars wrong. Naming the scheme in the constructor
/// makes the contradiction unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeColor {
    /// A light-only site.
    Light(String),
    /// A dark-only site.
    Dark(String),
    /// A site that follows the reader's preference.
    LightDark { light: String, dark: String },
}

/// One `<meta name="theme-color">`, with the media query it applies under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeColorTag {
    pub content: String,
    /// `None` on a single-scheme site, where an unconditional tag is correct.
    pub media: Option<String>,
}

impl ThemeColor {
    /// A light-only site.
    #[must_use]
    pub fn light(color: impl Into<String>) -> Self {
        Self::Light(color.into())
    }

    /// A dark-only site.
    #[must_use]
    pub fn dark(color: impl Into<String>) -> Self {
        Self::Dark(color.into())
    }

    /// A site that follows `prefers-color-scheme`.
    ///
    /// Both colours should be the page's *background* per scheme, so the
    /// browser chrome reads as continuous with the page rather than as an
    /// accent stripe above it.
    #[must_use]
    pub fn light_dark(light: impl Into<String>, dark: impl Into<String>) -> Self {
        Self::LightDark {
            light: light.into(),
            dark: dark.into(),
        }
    }

    /// The `<meta name="color-scheme">` value.
    #[must_use]
    pub const fn color_scheme(&self) -> &'static str {
        match self {
            Self::Light(_) => "light",
            Self::Dark(_) => "dark",
            Self::LightDark { .. } => "light dark",
        }
    }

    /// One representative colour, for contexts that accept only a single value
    /// — the web app manifest, which has no media-query equivalent. The light
    /// value wins, matching how a browser renders a splash screen by default.
    #[must_use]
    pub fn primary(&self) -> &str {
        match self {
            Self::Light(color) | Self::Dark(color) => color,
            Self::LightDark { light, .. } => light,
        }
    }

    /// The `<meta name="theme-color">` tags to emit, in order.
    #[must_use]
    pub fn tags(&self) -> Vec<ThemeColorTag> {
        match self {
            Self::Light(color) | Self::Dark(color) => vec![ThemeColorTag {
                content: color.clone(),
                media: None,
            }],
            Self::LightDark { light, dark } => vec![
                ThemeColorTag {
                    content: light.clone(),
                    media: Some(String::from("(prefers-color-scheme: light)")),
                },
                ThemeColorTag {
                    content: dark.clone(),
                    media: Some(String::from("(prefers-color-scheme: dark)")),
                },
            ],
        }
    }
}

/// Which theme to stamp when the reader has expressed no preference and the
/// operating system reports none either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fallback {
    Light,
    Dark,
}

impl Fallback {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// The storage key a reader's explicit theme choice is saved under.
///
/// Fixed rather than configurable: it is a contract between this script and
/// every project's own theme toggle, and two spellings of it is simply a bug.
pub const THEME_STORAGE_KEY: &str = "theme";

/// The pre-paint theme script.
///
/// Runs before the first paint so the page never flashes the wrong theme, which
/// is why it is inline and blocking rather than a module at the end of `body`.
///
/// It stamps `data-theme` on `<html>` in exactly two of three cases:
///
/// 1. the reader chose a theme — stamp it;
/// 2. the reader chose nothing but the OS states a preference — stamp
///    **nothing**, so the stylesheet's media queries stay live and follow the
///    OS if it changes mid-session;
/// 3. neither states anything — stamp [`Fallback`].
///
/// What `[data-theme="dark"]` *means* is entirely the project's CSS. This type
/// owns only the handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeScript {
    fallback: Fallback,
}

impl ThemeScript {
    /// A script that falls back to `fallback` when nothing states a preference.
    #[must_use]
    pub const fn new(fallback: Fallback) -> Self {
        Self { fallback }
    }

    /// The JavaScript to inline.
    ///
    /// The `try` wraps only the storage read: browsers with site data blocked
    /// throw there, which is an environment, not a bug. Everything after it is
    /// left unguarded so a genuine mistake in this crate fails loudly.
    #[must_use]
    pub fn source(&self) -> String {
        format!(
            "(function(){{var t;try{{t=localStorage.getItem(\"{key}\")}}catch(e){{}}\
             if(t===\"light\"||t===\"dark\"){{document.documentElement.dataset.theme=t;return}}\
             if(!matchMedia(\"(prefers-color-scheme: light)\").matches&&\
             !matchMedia(\"(prefers-color-scheme: dark)\").matches)\
             {{document.documentElement.dataset.theme=\"{fallback}\"}}}})()",
            key = THEME_STORAGE_KEY,
            fallback = self.fallback.as_str(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Fallback, ThemeColor, ThemeColorTag, ThemeScript};

    #[test]
    fn a_single_scheme_site_emits_one_unconditional_tag() {
        let expected: Vec<ThemeColorTag> = vec![ThemeColorTag {
            content: String::from("#fafafa"),
            media: None,
        }];
        let actual: Vec<ThemeColorTag> = ThemeColor::light("#fafafa").tags();
        assert_eq!(expected, actual);

        let expected_scheme: &str = "light";
        let actual_scheme: &str = ThemeColor::light("#fafafa").color_scheme();
        assert_eq!(expected_scheme, actual_scheme);
    }

    #[test]
    fn a_dual_scheme_site_emits_one_media_scoped_tag_per_scheme() {
        let theme: ThemeColor = ThemeColor::light_dark("#fafafa", "#121212");

        let expected_scheme: &str = "light dark";
        let actual_scheme: &str = theme.color_scheme();
        assert_eq!(expected_scheme, actual_scheme);

        let tags: Vec<ThemeColorTag> = theme.tags();

        let expected_len: usize = 2;
        let actual_len: usize = tags.len();
        assert_eq!(expected_len, actual_len);

        assert_eq!(
            Some(String::from("(prefers-color-scheme: light)")),
            tags[0].media
        );
        assert_eq!(
            Some(String::from("(prefers-color-scheme: dark)")),
            tags[1].media
        );
    }

    #[test]
    fn the_theme_script_stamps_nothing_when_the_os_states_a_preference() {
        let source: String = ThemeScript::new(Fallback::Dark).source();

        // The OS-following branch is the one that must NOT assign, or a theme
        // change mid-session would be ignored until reload.
        assert!(source.contains("prefers-color-scheme: light"));
        assert!(source.contains("prefers-color-scheme: dark"));
        assert!(source.contains("!matchMedia"));
    }

    #[test]
    fn the_theme_script_guards_only_the_storage_read() {
        let source: String = ThemeScript::new(Fallback::Light).source();

        let expected: usize = 1;
        let actual: usize = source.matches("try{").count();
        assert_eq!(expected, actual);
        assert!(source.contains("try{t=localStorage.getItem(\"theme\")}catch(e){}"));
    }

    #[test]
    fn the_fallback_is_what_gets_stamped_when_nothing_states_a_preference() {
        assert!(
            ThemeScript::new(Fallback::Dark)
                .source()
                .contains(r#"theme="dark""#)
        );
        assert!(
            ThemeScript::new(Fallback::Light)
                .source()
                .contains(r#"theme="light""#)
        );
    }
}
