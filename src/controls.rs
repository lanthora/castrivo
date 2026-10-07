//! Playback controls drawn on the same reference canvas as the status overlay.
use crate::player::Snapshot;
use std::fmt::Write;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    TogglePause,
    Seek(f64),
    Volume(u16),
    ToggleMute,
    Fullscreen,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Seek,
    Volume,
    Pause,
    Mute,
    Fullscreen,
}
pub struct Controls {
    pointer: Option<(f64, f64)>,
    drag: Option<Target>,
    last_motion: Instant,
    pub pinned: bool,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            pointer: None,
            drag: None,
            last_motion: Instant::now(),
            pinned: false,
        }
    }
}
impl Controls {
    pub fn visible(&self, state: &Snapshot) -> bool {
        active(state)
            && (self.pinned
                || self.drag.is_some()
                || state.state == "PAUSED_PLAYBACK"
                || self.pointer.is_some_and(|(_, y)| y >= 430.)
                || self.last_motion.elapsed() < Duration::from_secs(2))
    }
    pub fn motion(&mut self, x: i32, y: i32, size: (u32, u32), state: &Snapshot) -> Option<Action> {
        self.pointer = Some((
            f64::from(x) * 960. / f64::from(size.0.max(1)),
            f64::from(y) * 540. / f64::from(size.1.max(1)),
        ));
        self.last_motion = Instant::now();
        self.drag.and_then(|target| self.action(target, state))
    }
    pub fn down(&mut self, state: &Snapshot) -> Option<Action> {
        if !self.visible(state) {
            return None;
        }
        let target = self.hit()?;
        let action = self.action(target, state)?;
        if matches!(target, Target::Seek | Target::Volume) {
            self.drag = Some(target);
        }
        Some(action)
    }
    pub fn up(&mut self) {
        self.drag = None;
    }
    pub fn leave(&mut self) {
        self.pointer = None;
        self.drag = None;
    }
    fn hit(&self) -> Option<Target> {
        let (x, y) = self.pointer?;
        if (435. ..=463.).contains(&y) && (28. ..=932.).contains(&x) {
            return Some(Target::Seek);
        }
        if !(478. ..=524.).contains(&y) {
            return None;
        }
        match x {
            28.0..=72.0 => Some(Target::Pause),
            708.0..=748.0 => Some(Target::Mute),
            764.0..=856.0 => Some(Target::Volume),
            888.0..=932.0 => Some(Target::Fullscreen),
            _ => None,
        }
    }
    fn action(&self, target: Target, state: &Snapshot) -> Option<Action> {
        let x = self.pointer?.0;
        Some(match target {
            Target::Seek if state.seekable && state.duration > 0. => {
                Action::Seek(((x - 28.) / 904.).clamp(0., 1.) * state.duration)
            }
            Target::Seek => return None,
            Target::Volume => {
                Action::Volume((((x - 764.) / 92.).clamp(0., 1.) * 100.).round() as u16)
            }
            Target::Pause => Action::TogglePause,
            Target::Mute => Action::ToggleMute,
            Target::Fullscreen => Action::Fullscreen,
        })
    }
    pub fn overlay(&self, state: &Snapshot, fullscreen: bool) -> String {
        if !self.visible(state) {
            return String::new();
        }
        let mut out = String::new();
        // A stack of translucent strips provides a soft gradient without textures.
        for y in (400..540).step_by(4) {
            let alpha = 255 - ((y - 400) as f64 / 140. * 190.) as u8;
            rect(&mut out, 0., y as f64, 960., 4., "000000", alpha);
        }
        let hovered = self.hit();
        let seek_hover = hovered == Some(Target::Seek) || self.drag == Some(Target::Seek);
        let thickness = if seek_hover { 4. } else { 2. };
        let fraction = if self.drag == Some(Target::Seek) {
            ((self.pointer.unwrap().0 - 28.) / 904.).clamp(0., 1.)
        } else if state.duration > 0. {
            (state.position / state.duration).clamp(0., 1.)
        } else {
            0.
        };
        rect(
            &mut out,
            28.,
            449. - thickness / 2.,
            904.,
            thickness,
            "FFFFFF",
            160,
        );
        rect(
            &mut out,
            28.,
            449. - thickness / 2.,
            904. * fraction,
            thickness,
            "BFD485",
            0,
        );
        if seek_hover && state.seekable {
            circle(&mut out, 28. + 904. * fraction, 449., 6., "BFD485");
            text(
                &mut out,
                (28. + 904. * fraction).clamp(60., 900.),
                422.,
                &clock(fraction * state.duration),
                13,
            );
        }
        for (target, x) in [
            (Target::Pause, 50.),
            (Target::Mute, 728.),
            (Target::Fullscreen, 910.),
        ] {
            if hovered == Some(target) {
                circle(&mut out, x, 501., 21., "403830");
            }
        }
        if state.state == "PAUSED_PLAYBACK" {
            path(&mut out, 43., 490., "m 0 0 l 0 22 l 18 11", "F4EDE8");
        } else {
            rect(&mut out, 42., 490., 5., 22., "F4EDE8", 0);
            rect(&mut out, 53., 490., 5., 22., "F4EDE8", 0);
        }
        text_left(
            &mut out,
            88.,
            501.,
            &format!("{}  /  {}", clock(state.position), clock(state.duration)),
            14,
        );
        path(
            &mut out,
            718.,
            491.,
            "m 0 6 l 5 6 l 12 0 l 12 20 l 5 14 l 0 14",
            "F4EDE8",
        );
        if state.mute {
            path(
                &mut out,
                733.,
                495.,
                "m 0 0 l 2 0 l 10 12 l 8 12 m 8 0 l 10 0 l 2 12 l 0 12",
                "BFD485",
            );
        } else {
            rect(&mut out, 734., 495., 2., 12., "F4EDE8", 0);
        }
        rect(&mut out, 764., 500., 92., 2., "FFFFFF", 160);
        let volume = if state.mute {
            0.
        } else {
            f64::from(state.volume) / 100.
        };
        rect(&mut out, 764., 500., 92. * volume, 2., "BFD485", 0);
        circle(&mut out, 764. + 92. * volume, 501., 4., "F4EDE8");
        let shape = if fullscreen {
            "m 0 6 l 6 6 l 6 0 l 8 0 l 8 8 l 0 8 m 14 0 l 16 0 l 16 6 l 22 6 l 22 8 l 14 8 m 0 14 l 8 14 l 8 22 l 6 22 l 6 16 l 0 16 m 14 14 l 22 14 l 22 16 l 16 16 l 16 22 l 14 22"
        } else {
            "m 0 0 l 8 0 l 8 2 l 2 2 l 2 8 l 0 8 m 14 0 l 22 0 l 22 8 l 20 8 l 20 2 l 14 2 m 0 14 l 2 14 l 2 20 l 8 20 l 8 22 l 0 22 m 20 14 l 22 14 l 22 22 l 14 22 l 14 20 l 20 20"
        };
        path(&mut out, 899., 490., shape, "F4EDE8");
        out
    }
}
fn active(state: &Snapshot) -> bool {
    !state.error && matches!(state.state, "PLAYING" | "PAUSED_PLAYBACK" | "TRANSITIONING")
}
fn clock(seconds: f64) -> String {
    let s = seconds.max(0.) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}
fn path(out: &mut String, x: f64, y: f64, shape: &str, color: &str) {
    let _ = writeln!(
        out,
        r"{{\an7\pos({x},{y})\bord0\shad0\1c&H{color}&\p1}}{shape}{{\p0}}"
    );
}
fn rect(out: &mut String, x: f64, y: f64, w: f64, h: f64, color: &str, alpha: u8) {
    let _ = writeln!(
        out,
        r"{{\an7\pos({x},{y})\bord0\shad0\1c&H{color}&\alpha&H{alpha:02X}&\p1}}m 0 0 l {w} 0 l {w} {h} l 0 {h}{{\p0}}"
    );
}
fn circle(out: &mut String, x: f64, y: f64, r: f64, color: &str) {
    let k = r * 0.552285;
    path(
        out,
        x - r,
        y - r,
        &format!(
            "m {r} 0 b {} 0 {} {} {} {r} b {} {} {} {} {r} {} b {} {} 0 {} 0 {r} b 0 {} {} 0 {r} 0",
            r + k,
            2. * r,
            r - k,
            2. * r,
            2. * r,
            r + k,
            r + k,
            2. * r,
            2. * r,
            r - k,
            2. * r,
            r + k,
            r - k,
            r - k
        ),
        color,
    );
}
fn text(out: &mut String, x: f64, y: f64, value: &str, size: u8) {
    let _ = writeln!(
        out,
        r"{{\an5\pos({x},{y})\fnArial\fs{size}\bord0\shad0\1c&HF4EDE8&}}{value}"
    );
}
fn text_left(out: &mut String, x: f64, y: f64, value: &str, size: u8) {
    let _ = writeln!(
        out,
        r"{{\an4\pos({x},{y})\fnArial\fs{size}\bord0\shad0\1c&HF4EDE8&}}{value}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn playing() -> Snapshot {
        Snapshot {
            state: "PLAYING",
            duration: 120.,
            position: 30.,
            seekable: true,
            ..Snapshot::default()
        }
    }
    #[test]
    fn dragging_clamps_and_scales_logical_coordinates() {
        let mut c = Controls::default();
        let s = playing();
        c.motion(960, 449, (1920, 540), &s);
        assert_eq!(c.down(&s), Some(Action::Seek(60.)));
        assert_eq!(
            c.motion(3000, 449, (1920, 540), &s),
            Some(Action::Seek(120.))
        );
        c.up();
        assert_eq!(c.motion(100, 100, (960, 540), &s), None);
    }
    #[test]
    fn unavailable_seek_and_focus_loss_do_not_keep_dragging() {
        let mut c = Controls::default();
        let mut s = playing();
        s.seekable = false;
        c.motion(480, 449, (960, 540), &s);
        assert_eq!(c.down(&s), None);
        c.leave();
        assert_eq!(c.motion(800, 500, (960, 540), &s), None);
        assert_eq!(c.down(&s), Some(Action::Volume(39)));
    }
    #[test]
    fn visibility_respects_pause_pin_hover_and_timeout() {
        let mut c = Controls::default();
        let mut s = playing();
        c.last_motion = Instant::now() - Duration::from_secs(3);
        assert!(!c.visible(&s));
        s.state = "PAUSED_PLAYBACK";
        assert!(c.visible(&s));
        s.state = "PLAYING";
        c.pinned = true;
        assert!(c.visible(&s));
        c.pinned = false;
        c.pointer = Some((400., 500.));
        assert!(c.visible(&s));
        s.state = "NO_MEDIA_PRESENT";
        assert!(!c.visible(&s));
    }
}
