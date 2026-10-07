//! Lightweight presentation through mpv ASS overlays; no widget framework.
use crate::player::Snapshot;

pub fn overlay(state: &Snapshot, name: &str) -> String {
    if !state.error && state.state == "PLAYING" {
        return String::new();
    }
    if !state.error && state.state == "PAUSED_PLAYBACK" {
        return String::new();
    }
    if !state.error && state.state == "TRANSITIONING" {
        return r"{\an5\pos(480,270)\fnArial\fs18\bord0\shad1\1c&HF4EDE8&}Buffering...".into();
    }
    let (heading, detail) = if state.error {
        (
            "Unable to play this video",
            "Try casting again from your phone.",
        )
    } else if state.state == "STOPPED" {
        (
            "Ready for the next video",
            "Resume playback or choose another video on your phone.",
        )
    } else {
        (name, "Open a video on your phone and choose this device.")
    };
    // A single reference canvas scales with the render target, including HiDPI.
    // ASS colors use BGR order. Names are escaped and clipped to the text region.
    format!(
        r"{{\an7\pos(0,0)\bord0\shad0\1c&H1C1511&\p1}}m 0 0 l 960 0 l 960 540 l 0 540{{\p0}}
{{\an7\pos(436,152)\bord0\shad0\1c&HBFD485&\p1}}m 8 0 l 80 0 b 85 0 88 3 88 8 l 88 50 b 88 55 85 58 80 58 l 8 58 b 3 58 0 55 0 50 l 0 8 b 0 3 3 0 8 0 m 42 58 l 46 58 l 46 68 l 61 68 l 61 71 l 27 71 l 27 68 l 42 68{{\p0}}
{{\an7\pos(439,155)\bord0\shad0\1c&H1C1511&\p1}}m 5 0 l 77 0 b 80 0 82 2 82 5 l 82 47 b 82 50 80 52 77 52 l 5 52 b 2 52 0 50 0 47 l 0 5 b 0 2 2 0 5 0{{\p0}}
{{\an5\pos(480,264)\fnArial\fs28\b1\bord0\shad0\1c&HF4EDE8&\clip(120,240,840,290)}}{heading}
{{\an5\pos(480,306)\fnArial\fs14\bord0\shad0\1c&HBEAFA5&}}{detail}",
        heading = ass_text(heading)
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
