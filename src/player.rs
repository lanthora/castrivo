use anyhow::{Context, Result};
use libmpv2::{
    Mpv,
    render::{OpenGLInitParams, RenderParam, RenderParamApiType},
};
use sdl2::{
    event::Event,
    keyboard::Keycode,
    mouse::MouseButton,
    video::{FullscreenType, GLProfile},
};
use std::{ffi::c_void, time::Duration};
use tokio::sync::{mpsc, oneshot, watch};

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub uri: String,
    pub metadata: String,
    pub state: &'static str,
    pub position: f64,
    pub duration: f64,
    pub volume: u16,
    pub mute: bool,
    pub seekable: bool,
    pub error: bool,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            uri: String::new(),
            metadata: String::new(),
            state: "NO_MEDIA_PRESENT",
            position: 0.,
            duration: 0.,
            volume: 100,
            mute: false,
            seekable: false,
            error: false,
        }
    }
}
#[derive(Debug)]
pub enum Command {
    Load { uri: String, metadata: String },
    Play,
    Pause,
    Stop,
    Seek(f64),
    Volume(u16),
    Mute(bool),
}
pub struct Request {
    pub command: Command,
    pub reply: oneshot::Sender<Result<(), String>>,
}
#[derive(Debug)]
enum Wake {
    Render,
    Events,
}
fn proc_address(video: &sdl2::VideoSubsystem, name: &str) -> *mut c_void {
    video.gl_get_proc_address(name) as *mut c_void
}

pub fn run(
    mut commands: mpsc::Receiver<Request>,
    snapshot: watch::Sender<Snapshot>,
    shutdown: watch::Receiver<bool>,
    name: &str,
    media: Option<String>,
    native_log: Option<&str>,
) -> Result<()> {
    let sdl = sdl2::init().map_err(anyhow::Error::msg)?;
    let video = sdl.video().map_err(anyhow::Error::msg)?;
    let attr = video.gl_attr();
    attr.set_context_profile(GLProfile::Core);
    attr.set_context_version(3, 3);
    attr.set_context_flags().forward_compatible().set();
    let mut window = video
        .window("Castrivo", 960, 540)
        .opengl()
        .resizable()
        .allow_highdpi()
        .position_centered()
        .build()?;
    let _context = window.gl_create_context().map_err(anyhow::Error::msg)?;
    video.gl_set_swap_interval(1).map_err(anyhow::Error::msg)?;
    let events = sdl.event().map_err(anyhow::Error::msg)?;
    events
        .register_custom_event::<Wake>()
        .map_err(anyhow::Error::msg)?;
    let mut pump = sdl.event_pump().map_err(anyhow::Error::msg)?;
    let mut mpv = Mpv::with_initializer(|init| {
        if let Some(path) = native_log {
            init.set_property("log-file", path)?;
        }
        init.set_property("vo", "libmpv")?;
        init.set_property("idle", "yes")?;
        init.set_property("ytdl", "no")?;
        init.set_property("osc", "no")?;
        init.set_property("osd-level", 0i64)?;
        init.set_property("input-default-bindings", "yes")?;
        init.set_property("input-cursor", "yes")?;
        Ok(())
    })
    .context("Could not initialize libmpv")?;
    let sender = events.event_sender();
    mpv.set_wakeup_callback(move || {
        let _ = sender.push_custom_event(Wake::Events);
    });
    mpv.enable_all_events()?;
    mpv.disable_deprecated_events()?;
    let mut render = mpv.create_render_context(vec![
        RenderParam::ApiType(RenderParamApiType::OpenGl),
        RenderParam::InitParams(OpenGLInitParams {
            get_proc_address: proc_address,
            ctx: video,
        }),
    ])?;
    let sender = events.event_sender();
    render.set_update_callback(move || {
        let _ = sender.push_custom_event(Wake::Render);
    });
    tracing::info!(
        mpv_version = mpv.get_property::<String>("mpv-version").unwrap_or_default(),
        sdl_version = %sdl2::version::version(),
        "Player initialized with custom controls"
    );
    // Keep the render output available for waiting/error overlays without media.
    mpv.set_property("force-window", "yes")?;
    let mut state = Snapshot::default();
    if let Some(uri) = media {
        apply(
            &mpv,
            &mut state,
            Command::Load {
                uri,
                metadata: String::new(),
            },
        )?;
        apply(&mpv, &mut state, Command::Play)?;
    }
    let mut redraw = true;
    let mut last_update = std::time::Instant::now();
    let mut displayed_status = None;
    let system_theme = crate::theme::SystemTheme::new();
    let mut controls = crate::controls::Controls::default();
    while !*shutdown.borrow() {
        while let Ok(request) = commands.try_recv() {
            let operation = match &request.command {
                Command::Load { .. } => "load",
                Command::Play => "play",
                Command::Pause => "pause",
                Command::Stop => "stop",
                Command::Seek(_) => "seek",
                Command::Volume(_) => "volume",
                Command::Mute(_) => "mute",
            };
            tracing::info!(operation, "Applying playback command");
            let result = apply(&mpv, &mut state, request.command).map_err(|error| {
                tracing::warn!(operation, %error, "Playback command failed");
                "Playback command failed".to_string()
            });
            refresh(&mpv, &mut state);
            snapshot.send_replace(state.clone());
            let _ = request.reply.send(result);
            redraw = true;
        }
        for event in pump.poll_iter() {
            if let Some(wake) = event.as_user_event_type::<Wake>() {
                match wake {
                    Wake::Render => {
                        render.update()?;
                        redraw = true;
                    }
                    Wake::Events => {}
                }
            }
            if let Event::KeyDown {
                keycode: Some(key),
                repeat: false,
                ..
            } = &event
                && matches!(
                    *key,
                    Keycode::Escape
                        | Keycode::F
                        | Keycode::Space
                        | Keycode::Left
                        | Keycode::Right
                        | Keycode::Tab
                )
            {
                tracing::info!(?key, "Playback shortcut input");
            }
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => return Ok(()),
                Event::Window {
                    win_event:
                        sdl2::event::WindowEvent::Resized(..) | sdl2::event::WindowEvent::Exposed,
                    ..
                } => redraw = true,
                Event::KeyDown {
                    keycode: Some(Keycode::F),
                    repeat: false,
                    ..
                } => {
                    let mode = if window.fullscreen_state() == FullscreenType::Off {
                        FullscreenType::Desktop
                    } else {
                        FullscreenType::Off
                    };
                    window.set_fullscreen(mode).map_err(anyhow::Error::msg)?;
                    mpv.set_property("fullscreen", mode != FullscreenType::Off)?;
                    redraw = true;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Tab),
                    repeat: false,
                    ..
                } => {
                    controls.pinned = !controls.pinned;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Space),
                    repeat: false,
                    ..
                } => {
                    let _ = mpv.command("cycle", &["pause"]);
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Left),
                    ..
                } => {
                    if state.seekable {
                        let _ = mpv.command("seek", &["-5", "relative"]);
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Right),
                    ..
                } => {
                    if state.seekable {
                        let _ = mpv.command("seek", &["5", "relative"]);
                    }
                }
                Event::MouseMotion { x, y, .. } => {
                    if let Some(action) = controls.motion(x, y, window.size(), &state) {
                        control_action(&mpv, &mut state, action)?;
                    }
                }
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    controls.motion(x, y, window.size(), &state);
                    if let Some(action) = controls.down(&state) {
                        tracing::info!(?action, "Playback control activated");
                        control_action(&mpv, &mut state, action)?;
                    }
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    ..
                } => controls.up(),
                Event::Window {
                    win_event: sdl2::event::WindowEvent::Leave,
                    ..
                } => controls.leave(),
                Event::Window {
                    win_event: sdl2::event::WindowEvent::FocusLost,
                    ..
                } => {
                    controls.leave();
                    release_input(&mpv)?;
                }
                _ => {}
            }
        }
        while let Some(event) = mpv.wait_event(0.) {
            match event {
                Ok(libmpv2::events::Event::Shutdown) => return Ok(()),
                Ok(libmpv2::events::Event::EndFile(reason)) => {
                    state.state = if state.uri.is_empty() {
                        "NO_MEDIA_PRESENT"
                    } else {
                        "STOPPED"
                    };
                    state.error = reason == libmpv2::mpv_end_file_reason::Error;
                    if state.error {
                        tracing::warn!("Media playback failed; URL and credentials omitted");
                    }
                }
                Err(error) => {
                    state.error = true;
                    state.state = "STOPPED";
                    tracing::warn!(
                        %error, "Player reported an error"
                    );
                }
                _ => {}
            }
        }
        if last_update.elapsed() >= Duration::from_millis(200) {
            refresh(&mpv, &mut state);
            if snapshot.borrow().state != state.state {
                tracing::info!(
                    state = state.state,
                    video_width = mpv.get_property::<i64>("video-params/w").unwrap_or(0),
                    video_height = mpv.get_property::<i64>("video-params/h").unwrap_or(0),
                    audio_sample_rate = mpv
                        .get_property::<i64>("audio-out-params/samplerate")
                        .unwrap_or(0),
                    "Playback state changed"
                );
            }
            if *snapshot.borrow() != state {
                snapshot.send_replace(state.clone());
            }
            // Control requests target mpv; SDL owns the actual window.
            let fullscreen = mpv.get_property::<bool>("fullscreen").unwrap_or(false);
            if fullscreen != (window.fullscreen_state() != FullscreenType::Off) {
                window
                    .set_fullscreen(if fullscreen {
                        FullscreenType::Desktop
                    } else {
                        FullscreenType::Off
                    })
                    .map_err(anyhow::Error::msg)?;
                redraw = true;
            }
            last_update = std::time::Instant::now();
        }
        let theme = system_theme.current();
        let status = format!(
            "{}{}",
            crate::ui::overlay(&state, name, theme),
            controls.overlay(
                &state,
                window.fullscreen_state() != FullscreenType::Off,
                theme
            )
        );
        if displayed_status.as_ref() != Some(&status) {
            mpv.command("osd-overlay", &["100", "ass-events", &status, "960", "540"])?;
            displayed_status = Some(status);
            redraw = true;
        }
        if redraw {
            let (width, height) = window.drawable_size();
            if width > 0 && height > 0 {
                render.render::<sdl2::VideoSubsystem>(0, width as i32, height as i32, true)?;
                window.gl_swap_window();
            }
            redraw = false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
fn apply(mpv: &Mpv, state: &mut Snapshot, command: Command) -> Result<()> {
    match command {
        Command::Load { uri, metadata } => {
            if uri.is_empty() {
                mpv.command("stop", &[])?;
                *state = Snapshot {
                    volume: state.volume,
                    mute: state.mute,
                    ..Snapshot::default()
                };
                return Ok(());
            }
            mpv.set_property("pause", true)?;
            mpv.command("loadfile", &[&uri, "replace"])?;
            *state = Snapshot {
                uri,
                metadata,
                state: "TRANSITIONING",
                volume: state.volume,
                mute: state.mute,
                ..Snapshot::default()
            };
        }
        Command::Play => {
            if mpv.get_property::<bool>("idle-active").unwrap_or(false) && !state.uri.is_empty() {
                mpv.command("loadfile", &[&state.uri, "replace"])?;
            }
            mpv.set_property("pause", false)?;
        }
        Command::Pause => {
            mpv.set_property("pause", true)?;
        }
        Command::Stop => {
            mpv.command("stop", &[])?;
            state.state = "STOPPED";
            state.position = 0.;
        }
        Command::Seek(seconds) => {
            mpv.command("seek", &[&seconds.to_string(), "absolute"])?;
        }
        Command::Volume(volume) => {
            mpv.set_property("volume", f64::from(volume))?;
        }
        Command::Mute(mute) => {
            mpv.set_property("mute", mute)?;
        }
    }
    Ok(())
}
fn refresh(mpv: &Mpv, state: &mut Snapshot) {
    if mpv.get_property::<bool>("eof-reached").unwrap_or(false) {
        state.state = "STOPPED";
    } else if !mpv.get_property::<bool>("idle-active").unwrap_or(true) {
        state.state = if mpv
            .get_property::<bool>("paused-for-cache")
            .unwrap_or(false)
        {
            "TRANSITIONING"
        } else if mpv.get_property::<bool>("pause").unwrap_or(false) {
            "PAUSED_PLAYBACK"
        } else {
            "PLAYING"
        };
    }
    state.position = mpv.get_property("time-pos").unwrap_or(0.);
    state.duration = mpv.get_property("duration").unwrap_or(0.);
    state.seekable = mpv.get_property("seekable").unwrap_or(false);
    state.volume = mpv
        .get_property::<f64>("volume")
        .unwrap_or(100.)
        .clamp(0., 100.) as u16;
    state.mute = mpv.get_property("mute").unwrap_or(false);
}

fn control_action(mpv: &Mpv, state: &mut Snapshot, action: crate::controls::Action) -> Result<()> {
    use crate::controls::Action;
    match action {
        Action::TogglePause => {
            mpv.command("cycle", &["pause"])?;
        }
        Action::Seek(position) => apply(mpv, state, Command::Seek(position))?,
        Action::Volume(volume) => {
            apply(mpv, state, Command::Volume(volume))?;
            apply(mpv, state, Command::Mute(false))?;
        }
        Action::ToggleMute => apply(mpv, state, Command::Mute(!state.mute))?,
        Action::Fullscreen => {
            mpv.command("cycle", &["fullscreen"])?;
        }
    }
    refresh(mpv, state);
    Ok(())
}
fn release_input(mpv: &Mpv) -> Result<()> {
    // mpv 0.41 dereferences a null argument when keyup's optional name is omitted.
    // An explicit empty string safely requests release of all held inputs.
    tracing::info!(command = "keyup", "Window lost focus; releasing held input");
    mpv.command("keyup", &[""])?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{ass_text, overlay};

    #[test]
    fn custom_controls_update_real_mpv_properties() {
        use crate::controls::Action;
        let mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "null")?;
            init.set_property("idle", "yes")?;
            Ok(())
        })
        .unwrap();
        let mut state = Snapshot::default();
        control_action(&mpv, &mut state, Action::ToggleMute).unwrap();
        assert!(state.mute);
        control_action(&mpv, &mut state, Action::Volume(35)).unwrap();
        assert_eq!(state.volume, 35);
        assert!(!state.mute);
        mpv.set_property("pause", false).unwrap();
        control_action(&mpv, &mut state, Action::TogglePause).unwrap();
        assert!(mpv.get_property::<bool>("pause").unwrap());
        control_action(&mpv, &mut state, Action::Fullscreen).unwrap();
        assert!(mpv.get_property::<bool>("fullscreen").unwrap());
    }

    #[test]
    fn focus_loss_releases_all_input_with_real_libmpv() {
        let mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "null")?;
            init.set_property("idle", "yes")?;
            Ok(())
        })
        .unwrap();
        for button in ["MBTN_LEFT", "MBTN_MID", "MBTN_RIGHT"] {
            mpv.command("keydown", &[button]).unwrap();
            release_input(&mpv).unwrap();
        }
        release_input(&mpv).unwrap();
        // A native crash would terminate this test process before this round-trip.
        mpv.set_property("pause", true).unwrap();
        assert!(mpv.get_property::<bool>("pause").unwrap());
    }

    #[test]
    fn status_shows_receiver_identity_but_never_media_credentials() {
        let mut state = Snapshot {
            uri: "https://example.test/video?token=private".into(),
            metadata: "private media metadata".into(),
            ..Snapshot::default()
        };
        let waiting = overlay(&state, "Living room", crate::theme::Theme::Dark);
        assert!(waiting.contains("Living room"));
        assert!(!waiting.contains("private"));
        state.state = "PLAYING";
        assert!(overlay(&state, "Living room", crate::theme::Theme::Dark).is_empty());
        state.error = true;
        let message = overlay(&state, "Living room", crate::theme::Theme::Dark);
        assert!(!message.is_empty());
        assert!(!message.contains("private"));
    }

    #[test]
    fn device_names_cannot_inject_ass_formatting() {
        let text = ass_text("Room {\\pos(0,0)}\\N\r\nReceiver");
        assert!(!text.contains("{\\pos"));
        assert!(!text.contains("}\\N"));
        assert!(!text.contains('\r'));
        assert!(text.ends_with("\\NReceiver"));
    }
}
