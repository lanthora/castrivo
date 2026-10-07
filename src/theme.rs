//! Cross-platform system appearance detection, kept off the playback thread.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    Light,
    #[default]
    Dark,
}
pub struct Palette {
    pub background: &'static str,
    pub foreground: &'static str,
    pub secondary: &'static str,
    pub accent: &'static str,
    pub hover: &'static str,
    pub track: &'static str,
    pub gradient: &'static str,
}
impl Theme {
    // ASS colors are BGR, not RGB.
    pub fn palette(self) -> Palette {
        match self {
            Self::Dark => Palette {
                background: "1C1511",
                foreground: "F4EDE8",
                secondary: "BEAFA5",
                accent: "BFD485",
                hover: "403830",
                track: "FFFFFF",
                gradient: "000000",
            },
            Self::Light => Palette {
                background: "F8F6F3",
                foreground: "30251B",
                secondary: "796B5C",
                accent: "766D16",
                hover: "E5DED5",
                track: "30251B",
                gradient: "F8F6F3",
            },
        }
    }
    fn detected(mode: Result<dark_light::Mode, dark_light::Error>) -> Option<Self> {
        match mode {
            Ok(dark_light::Mode::Light) => Some(Self::Light),
            Ok(dark_light::Mode::Dark) => Some(Self::Dark),
            _ => None,
        }
    }
}
pub struct SystemTheme {
    dark: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}
impl SystemTheme {
    pub fn new() -> Self {
        let dark = Arc::new(AtomicBool::new(true));
        let stop = Arc::new(AtomicBool::new(false));
        let current = Arc::clone(&dark);
        let stopping = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                if let Some(theme) = Theme::detected(dark_light::detect()) {
                    let value = theme == Theme::Dark;
                    if current.swap(value, Ordering::Relaxed) != value {
                        tracing::info!(?theme, "System appearance changed");
                    }
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
        Self { dark, stop }
    }
    pub fn current(&self) -> Theme {
        if self.dark.load(Ordering::Relaxed) {
            Theme::Dark
        } else {
            Theme::Light
        }
    }
}
impl Drop for SystemTheme {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_system_preferences_map_to_themes() {
        assert_eq!(
            Theme::detected(Ok(dark_light::Mode::Light)),
            Some(Theme::Light)
        );
        assert_eq!(
            Theme::detected(Ok(dark_light::Mode::Dark)),
            Some(Theme::Dark)
        );
        assert_eq!(Theme::detected(Ok(dark_light::Mode::Unspecified)), None);
    }
}
