use anyhow::{Context, Result};
use libmpv2::{
    Mpv,
    render::{OpenGLInitParams, RenderParam, RenderParamApiType},
};
use sdl2::{
    event::Event,
    keyboard::Keycode,
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
) -> Result<()> {
    let sdl = sdl2::init().map_err(anyhow::Error::msg)?;
    let video = sdl.video().map_err(anyhow::Error::msg)?;
    let attr = video.gl_attr();
    attr.set_context_profile(GLProfile::Core);
    attr.set_context_version(3, 3);
    attr.set_context_flags().forward_compatible().set();
    let mut window = video
        .window(name, 960, 540)
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
        init.set_property("vo", "libmpv")?;
        init.set_property("idle", "yes")?;
        init.set_property("ytdl", "no")?;
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
    while !*shutdown.borrow() {
        while let Ok(request) = commands.try_recv() {
            let result = apply(&mpv, &mut state, request.command)
                .map_err(|_| "Playback command failed".to_string());
            if result.is_err() {
                tracing::warn!("Playback command failed");
            }
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
                    let _ = mpv.command("seek", &["-5", "relative"]);
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Right),
                    ..
                } => {
                    let _ = mpv.command("seek", &["5", "relative"]);
                }
                _ => {}
            }
        }
        while let Some(event) = mpv.wait_event(0.) {
            match event {
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
                Err(_) => {
                    state.error = true;
                    state.state = "STOPPED";
                    tracing::warn!(
                        "Player reported an error; details omitted to protect media credentials"
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
            window.set_title(&format!(
                "{name} — {}{}",
                state.state,
                if state.error {
                    " — playback error"
                } else {
                    ""
                }
            ))?;
            last_update = std::time::Instant::now();
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
