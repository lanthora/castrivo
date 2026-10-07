//! Lightweight presentation through mpv ASS overlays; no widget framework.
use crate::player::Snapshot;

pub fn overlay(state: &Snapshot, name: &str, theme: crate::theme::Theme) -> String {
    if !state.error && state.state == "PLAYING" {
        return String::new();
    }
    if !state.error && state.state == "PAUSED_PLAYBACK" {
        return String::new();
    }
    if !state.error && state.state == "TRANSITIONING" {
        return r"{\an5\pos(480,270)\fnArial\fs18\bord0\shad1\1c&HF4EDE8&}Buffering...".into();
    }
    if !state.error {
        return waiting(name, theme);
    }
    let palette = theme.palette();
    let heading = "Unable to play this video";
    let detail = "Try casting again from your phone.";
    // A single reference canvas scales with the render target, including HiDPI.
    // ASS colors use BGR order. Names are escaped and clipped to the text region.
    format!(
        r"{{\an7\pos(0,0)\bord0\shad0\1c&H{background}&\p1}}m 0 0 l 960 0 l 960 540 l 0 540{{\p0}}
{{\an7\pos(436,152)\bord0\shad0\1c&H{accent}&\p1}}m 8 0 l 80 0 b 85 0 88 3 88 8 l 88 50 b 88 55 85 58 80 58 l 8 58 b 3 58 0 55 0 50 l 0 8 b 0 3 3 0 8 0 m 42 58 l 46 58 l 46 68 l 61 68 l 61 71 l 27 71 l 27 68 l 42 68{{\p0}}
{{\an7\pos(439,155)\bord0\shad0\1c&H{background}&\p1}}m 5 0 l 77 0 b 80 0 82 2 82 5 l 82 47 b 82 50 80 52 77 52 l 5 52 b 2 52 0 50 0 47 l 0 5 b 0 2 2 0 5 0{{\p0}}
{{\an5\pos(480,264)\fnArial\fs28\b1\bord0\shad0\1c&H{foreground}&\clip(120,240,840,290)}}{heading}
{{\an5\pos(480,306)\fnArial\fs14\bord0\shad0\1c&H{secondary}&}}{detail}",
        heading = ass_text(heading),
        background = palette.background,
        accent = palette.accent,
        foreground = palette.foreground,
        secondary = palette.secondary
    )
}

fn waiting(name: &str, theme: crate::theme::Theme) -> String {
    let palette = theme.palette();
    let surface = match theme {
        crate::theme::Theme::Light => "FFFFFF",
        crate::theme::Theme::Dark => "261F19",
    };
    // One card groups receiver identity and the actions needed to start casting.
    // Keep the device name once, matching the phone's device picker.
    format!(
        r"{{\an7\pos(0,0)\bord0\shad0\1c&H{background}&\p1}}m 0 0 l 960 0 l 960 540 l 0 540{{\p0}}
{{\an7\pos(96,102)\bord1\shad0\3c&H{hover}&\1c&H{surface}&\p1}}m 20 0 l 748 0 b 760 0 768 8 768 20 l 768 316 b 768 328 760 336 748 336 l 20 336 b 8 336 0 328 0 316 l 0 20 b 0 8 8 0 20 0{{\p0}}
{{\an7\pos(140,144)\bord0\shad0\1c&H{accent}&\p1}}m 4 0 b 9 0 9 8 4 8 b -1 8 -1 0 4 0{{\p0}}
{{\an4\pos(160,148)\fnArial\fs13\bord0\shad0\1c&H{secondary}&}}Ready to receive
{{\an7\pos(170,207)\bord0\shad0\1c&H{accent}&\p1}}m 8 0 l 80 0 b 85 0 88 3 88 8 l 88 50 b 88 55 85 58 80 58 l 8 58 b 3 58 0 55 0 50 l 0 8 b 0 3 3 0 8 0 m 42 58 l 46 58 l 46 68 l 61 68 l 61 71 l 27 71 l 27 68 l 42 68{{\p0}}
{{\an7\pos(173,210)\bord0\shad0\1c&H{surface}&\p1}}m 5 0 l 77 0 b 80 0 82 2 82 5 l 82 47 b 82 50 80 52 77 52 l 5 52 b 2 52 0 50 0 47 l 0 5 b 0 2 2 0 5 0{{\p0}}
{{\an4\pos(140,320)\fnArial\fs28\b1\bord0\shad0\1c&H{foreground}&\clip(140,295,440,347)}}{name}
{{\an7\pos(140,355)\fnArial\fs13\bord0\shad0\1c&H{secondary}&}}Look for this name in your\Nphone's casting menu.
{{\an7\pos(466,185)\bord0\shad0\1c&H{hover}&\alpha&H80&\p1}}m 0 0 l 1 0 l 1 211 l 0 211{{\p0}}
{{\an4\pos(506,205)\fnArial\fs20\b1\bord0\shad0\1c&H{foreground}&}}Start from your phone
{{\an5\pos(518,266)\fnArial\fs13\bord0\shad0\1c&H{accent}&}}01
{{\an4\pos(550,266)\fnArial\fs15\bord0\shad0\1c&H{foreground}&}}Connect to the same network
{{\an5\pos(518,320)\fnArial\fs13\bord0\shad0\1c&H{accent}&}}02
{{\an4\pos(550,320)\fnArial\fs15\bord0\shad0\1c&H{foreground}&}}Open a video and tap Cast
{{\an5\pos(518,374)\fnArial\fs13\bord0\shad0\1c&H{accent}&}}03
{{\an4\pos(550,374)\fnArial\fs15\bord0\shad0\1c&H{foreground}&}}Choose this device",
        name = ass_text(name),
        background = palette.background,
        foreground = palette.foreground,
        secondary = palette.secondary,
        accent = palette.accent,
        hover = palette.hover,
    )
}

pub(crate) fn ass_text(text: &str) -> String {
    // Break ASS escape/tag syntax in arbitrary device names, preserving line breaks.
    text.replace('\\', "\\\u{feff}")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('\r', "")
        .replace('\n', "\\N")
}
