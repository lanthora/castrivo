# Castrivo

Turn your desktop into a video casting receiver.

Castrivo is a Rust desktop application in early development that lets compatible video apps send video to a computer as if it were a TV. Choose a video on your phone, select Castrivo from the app's casting menu, and watch it on your desktop.

## Project status

A runnable Rust receiver now implements SSDP discovery, UPnP service descriptions, SOAP playback controls, GENA subscriptions, and in-process video playback through SDL2 and libmpv2. It is an early implementation, not a released or certified DLNA device.

Automated controller tests on the development host have exercised discovery, playback from a public HTTPS sample and a local HTTP video with audio, pause/resume, seek, volume/mute, media replacement, stop/replay, SOAP faults, subscription renewal, state events, and unsubscribe. These tests establish protocol-to-player operation. On 2026-10-07, the user reported successful casting from the Bilibili phone app on the development setup. App version, phone operating system, media category, and individual control/audio checks were not recorded, so this result must not be generalized to every Bilibili version or media item. iQIYI, Windows, Linux, and clean-machine package execution remain unverified. The Apple Silicon macOS package has passed dependency/signature checks and receiver startup on the development host.

## Language

English is the project language. All user-facing text, documentation, code comments, identifiers, and development messages should be written in English.

## First-release scope

The first release aims to be a general-purpose video casting receiver for mainstream phone video apps on the same local network. Apps such as iQIYI and Bilibili should be able to discover Castrivo in their casting menus and send supported videos for playback. These are representative compatibility examples, not an exclusive app list or a restriction to a particular phone model. This is intended behavior, not a claim of compatibility already achieved.

- Advertise Castrivo as a discoverable video receiver.
- Accept a media URL and available metadata from a casting sender.
- Play the video in a desktop window with fullscreen support.
- Support play, pause, stop, seek, and volume control where the sender and media support them.
- Report playback state and position to the sender.
- Expose the receiver name, connection status, and useful playback errors through the window title and terminal diagnostics in English.
- Handle one active playback session at a time.

The initial protocol target is a DLNA/UPnP AV MediaRenderer. Implement a standards-based receiver rather than designing around one selected app. Validate this protocol against multiple mainstream apps; DLNA/UPnP alone has not yet been proven sufficient for the compatibility goal. A generic receiver may not appear in apps that only discover approved devices or use proprietary casting protocols.

Screen mirroring, desktop sharing, remote internet casting, media libraries, and multi-receiver synchronization are outside the first-release scope. AirPlay and Google Cast support are possible later investigations, not first-release commitments.

## Intended experience

1. Launch Castrivo on a desktop computer.
2. Use the default receiver name and automatically selected network interface, or override them through command-line options.
3. Open a compatible video app on a phone connected to the same network.
4. Tap the app's TV or casting button and select Castrivo.
5. Watch the video on the computer and control playback from the phone or supported desktop shortcuts.

The phone sends playback instructions and a media address. Castrivo retrieves and plays that media directly; it does not need to capture the phone's screen.

## Minimal feasibility prototype

Minimize application code by reusing suitable libraries and their examples. Prefer existing abstractions over custom FFI, protocol stacks, media parsing, and configuration systems. Use the simplest implementation that can verify the complete casting path while preserving the intended production architecture. Do not build temporary playback paths that are already expected to be removed.

- Run a Rust receiver from the terminal with default settings and command-line overrides for the receiver name, network interface, and HTTP port. Do not add configuration files or a settings page initially.
- Start with standard DLNA/UPnP receiver discovery, services, actions, and event subscriptions. Prefer a reusable MediaRenderer library after reviewing its service coverage, event support, maintenance, and license; a media server or casting controller library is not a substitute. Custom protocol implementation is the fallback if no suitable receiver library exists.
- Use SDL2 through its Rust bindings for the application-owned window, OpenGL context, and event loop. Use the libmpv2 OpenGL example as the starting point for the retained playback implementation.
- Receive a media URL from a real phone app and play it in-process through `libmpv2` and `libmpv`, using its recommended Render API for video output.
- Use libmpv2 to control playback, receive events, and access the Render API; do not write custom FFI. Include play, pause, stop, seek, volume, state queries, and event subscriptions in the receiver foundation where supported by the media.
- Keep one active playback session. New casting requests replace the current media; process commands in order. Treat libmpv playback events and properties as the source of playback state, with only minimal receiver-specific state.
- Do not actively detect sender disconnection; playback continues if the phone disconnects. Closing the playback window stops playback and exits Castrivo.
- Delegate format handling, decoding, live playback, and seekability detection to libmpv. Pass through necessary headers and cookies when supplied through the supported sender protocol and playback API; do not build custom media parsing or credential retrieval.
- Log discovery, commands, playback results, and errors with sensitive URLs and credentials redacted.
- Use a cross-platform window and rendering design from the start. Validation may begin on an available development platform, but the architecture must not depend on macOS-specific behavior. Native packaging uses cargo-packager; platform support follows actual validation.

Do not introduce Tauri, `egui`, `eframe`, or a custom desktop control panel for this prototype. Window creation, an event loop, and an OpenGL context are required; a GUI widget framework is not. The application owns the video window and handles redraws, resizing, and shutdown. libmpv renders video into the supplied rendering target and handles decoding and audio output. Do not launch an external `mpv` application or use native window-handle embedding as a temporary playback path.

A successful casting test requires discovery in a phone app, delivery of a playable media URL, and video with audio on the desktop. Validate across multiple mainstream apps, including iQIYI and Bilibili as representative examples, and record app versions, phone operating systems, media types, stop behavior, controls, and state reporting. One successful app or a known compatible UPnP controller is not sufficient evidence of general compatibility.

Implementation does not require choosing a single target app or phone. Start from the standard receiver contract and maintain a compatibility matrix. If mainstream-app testing exposes discovery or playback gaps, determine whether they require standards fixes or an additional protocol before changing the common receiver architecture. Do not add app-specific branches to the initial implementation.

## Rust implementation plan

Application logic, networking, and protocol handling will be implemented in Rust. The application will own a cross-platform playback window and OpenGL context. An in-process libmpv adapter will handle media playback and Render API integration.

| Area | Planned approach |
| --- | --- |
| Window and event loop | SDL2 through Rust bindings |
| Graphics context | SDL2 OpenGL context, integrated with the libmpv Render API |
| Desktop controls | Video window, fullscreen, and a small set of keyboard shortcuts; no GUI widget framework in the first release |
| Async networking | Tokio for network I/O, timers, and background tasks |
| Device discovery | Small socket2/Tokio SSDP layer with multicast advertisements and targeted/all-device search responses |
| Receiver protocol | Axum HTTP, library-based XML parsing, declarative service descriptions, SOAP handlers, and GENA subscriptions; receiver-library reuse audit recorded below |
| Playback services | AVTransport, RenderingControl, and ConnectionManager |
| Media playback | libmpv2 wrapping in-process libmpv Client API and Render API; thin application adapter |
| Session management | Ordered commands, latest casting request replaces media; libmpv is the playback-state source |
| Diagnostics | Structured local logs with sensitive URLs and credentials redacted |

The proposed components are documented in the official [Tokio documentation](https://tokio.rs/), [libmpv2 documentation and SDL2 OpenGL example](https://docs.rs/crate/libmpv2/latest), and [libmpv API and rendering examples](https://github.com/mpv-player/mpv-examples/tree/master/libmpv).

Choose stable dependency versions during implementation and lock them; validate upgrades separately. Users should not need to install another player application. libmpv and its native dependencies remain required libraries; use dynamic linking and include the required native libraries in release packages, reusing existing build tools rather than creating a custom dependency build system. Development may use locally installed libraries until distribution is implemented.

## Architecture

```text
Video app on phone
    | SSDP discovery / UPnP playback commands
    v
Rust receiver service
    | validated commands
    v
Minimal receiver state + ordered commands
    | playback commands / events
    v
Thin libmpv2 / libmpv adapter
    | client API: media loading, controls, audio, events
    | Render API: video frames
    v
SDL2 window + OpenGL context
    |
    v
Video display on desktop
```

Protocol code will be independent of the desktop UI and playback engine. A playback adapter will translate commands and events so that protocol behavior can be tested without opening a video window.

## Compatibility boundaries

Video-app compatibility is a release criterion to measure, not a guarantee implied by the receiver name.

- The sender must support the receiver's implemented casting protocol.
- The supplied media address must be reachable from the desktop.
- Playback depends on the stream format, codecs, server requirements, and available playback engine support.
- Signed URLs, required request headers, cookies, and expiring credentials may affect playback. Sender-specific behavior must be verified individually.
- DRM-protected streams and proprietary device authorization may require integrations that a generic receiver cannot provide. DRM bypass is outside the project scope.
- Seeking and duration reporting depend on whether the media is live or seekable.
- Network isolation and firewall rules can prevent discovery or control even on the same Wi-Fi network.

Compatibility records must include the video app, app version, sender operating system, desktop platform, media type, and observed discovery, playback, and control results.

| Date | Sender | Result | Coverage limits |
| --- | --- | --- | --- |
| 2026-10-07 | Bilibili phone app | User reported successful casting on the development setup | App version, phone OS, media type, and individual controls/audio checks not recorded |
| 2026-10-07 | Automated UPnP controller | SSDP, public HTTPS/local HTTP playback, transport controls, volume/mute, replacement, faults, and GENA passed | Does not establish compatibility with other phone apps |

## Known remaining work

- **Release delivery:** native dependencies are installed on the development host; local package scripts collect native dependencies; clean-machine verification, public release signing, and dependency/license review remain pending.
- **Compatibility and platform coverage:** verify iQIYI and additional app/OS/media combinations, and build/run on Windows and Linux X11/Wayland. Bilibili's reported success is one observed setup.
- **Failure-path robustness:** network stalls, expired URLs, unsupported media, interrupted connections, natural end-of-file/replay, and prolonged use need targeted tests. Playback API calls are currently synchronous on the window thread, which can affect responsiveness; a timed-out SOAP request does not cancel an already queued player command.
- **Network changes:** the interface and HTTP listener are selected at startup. Switching networks, changing addresses, and sleep/wake recovery do not yet trigger automatic rebinding or rediscovery.
- **Media interoperability:** sender-specific headers/cookies are not extracted. Live streams and seeking limitations require verification; next-item queuing and other casting protocols remain outside the current implementation.
- **Diagnostics and protocol review:** playback failures currently omit details to avoid leaking credentials. Add actionable sanitized errors and review the implemented service/action/event subset before claiming standards completeness.

These are implementation gaps or validation risks, not failures reported by the Bilibili test.

## Desktop platform targets

The intended desktop platforms are macOS, Windows, and Linux. Include both X11 and Wayland in Linux validation. Minimum operating-system versions will follow the selected dependency requirements rather than custom legacy-platform support. Use the same implementation across platforms and announce support separately as each is verified. None is supported yet. Discovery, video rendering, audio output, and packaging must be validated separately on each platform before claiming support.

## Implementation milestones

1. **Receiver protocol foundation:** evaluate reusable MediaRenderer libraries and define a standards-based DLNA/UPnP contract and a compatibility test matrix spanning mainstream video apps and phone operating systems; no single target app or phone is an implementation prerequisite.
2. **Playback foundation:** adapt the libmpv2 SDL2 OpenGL example into the retained cross-platform playback foundation. Verify video, audio, resizing, and shutdown with representative media; retain this path for the receiver.
3. **Minimal casting prototype:** connect discovery and standard receiver service actions to that playback foundation. Record real-app discovery, video, audio, and stop results.
4. **Receiver completeness:** expand service actions, subscriptions, playback events, controls, and session transitions for standards compliance and observed interoperability requirements.
5. **Desktop experience:** add receiver configuration, playback controls, fullscreen, and actionable errors on the existing playback foundation. Keep the first release limited to shortcuts and terminal configuration; consider widgets only for later demonstrated needs.
6. **Compatibility and distribution:** publish measured app compatibility and validate platform packages and native dependency licensing.

## Implementation decisions and remaining validation

The architecture and behavior above are accepted. Remaining details are decided during implementation and recorded here; no separate approval is required for routine choices:

- **Receiver reuse review:** `dlna-dmr` 0.1.3 lacks GENA subscription handling, leaves ConnectionManager as a placeholder, and has discovery advertisement/search inconsistencies. `tokio-ssdp` 0.1.0 lacks `ssdp:all` handling and has multicast/shutdown issues; the inspected `cotton-ssdp` implementation uses Unix/Linux-specific networking APIs. None was adopted as the common cross-platform receiver. The current implementation uses existing Axum, XML, socket, and Tokio libraries with a small SSDP layer, declarative service schemas, and protocol-to-player handlers. Reconsider a receiver library if a suitable implementation becomes available.
- **Dependency versions and native artifacts:** Rust dependencies are locked in Cargo.lock, using libmpv2 6 and SDL2 bindings 0.38. Development links native libraries through pkg-config; scripts/build.py collects native dependencies and delegates platform layouts to cargo-packager. Dynamic linking and bundled release dependencies are already decided.
- **Network defaults:** HTTP defaults to port 5200. Prefer the routed private IPv4 interface, falling back to an available private IPv4 interface; --ip overrides selection, especially with VPNs or multiple LANs. Derive a stable UUID from the hostname, with --uuid available for collision avoidance. No configuration file is used.
- **Compatibility evidence:** record discovery, playback, controls, and errors for representative iQIYI and Bilibili versions on Android and iOS, then expand coverage. Additional protocols require evidence from these tests; broad compatibility is not yet verified.
- **Platform validation:** record dependency-derived minimum system versions and arrange verification on available macOS, Windows, X11, and Wayland hosts.
- **Licensing:** select the project license after dependency review; prefer permissively licensed dependencies and verify the full native-library license chain before distribution.

## Validation plan

- Unit-test command validation and session-state transitions.
- Exercise device descriptions, SOAP responses, subscription renewal, and playback-state events with a protocol controller.
- Verify discovery and casting with selected real video apps.
- Test play, pause, stop, seek, volume, session replacement, and sender disconnection.
- Test unreachable URLs, expired streams, unsupported media, and interrupted networks.
- Smoke-test video, audio, fullscreen, and shutdown on each supported desktop platform.

## Development

Install Rust and the SDL2/libmpv development libraries with pkg-config metadata. Castrivo links the libraries in-process and does not launch the mpv executable. Package managers may install that executable alongside the development library.

On macOS with Homebrew:

```sh
brew install pkg-config sdl2 mpv
cargo run
```

On Debian/Ubuntu (not yet runtime-verified):

```sh
sudo apt install pkg-config libsdl2-dev libmpv-dev build-essential
cargo run
```

On Windows, provide SDL2 and libmpv libraries compatible with the Rust target and their pkg-config metadata. Windows build and runtime validation is pending. Set `PKG_CONFIG_PATH` if native libraries are outside the package manager's standard search paths.

Common options:

```sh
cargo run -- --name Castrivo --ip 192.168.2.40 --port 5200
cargo run -- --media https://example.com/video.mp4
cargo run -- --help
```

Replace the example IP with the computer's LAN address. Keep the phone on the same LAN and select **Castrivo** from its app's casting menu. Discovery uses IPv4 SSDP multicast on UDP 1900; control uses the configured HTTP port. If automatic selection chooses a VPN or another network, use `--ip`. Receiver startup prints the selected address when info logging is enabled (`RUST_LOG=castrivo=info`).

Shortcuts: `F` toggles fullscreen, `Space` pauses/resumes, arrow keys seek five seconds, and `Escape` exits. Closing the window also exits the receiver. A new casting request replaces the previous media; losing the phone connection does not stop playback.

Checks:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

With Castrivo already running, exercise actual network discovery and casting without a phone:

```sh
python3 scripts/smoke_cast.py --receiver http://192.168.2.40:5200
```

The default smoke test uses the public jellyfish sample from the libmpv2 example repository. Its accessibility depends on the network. To test an HTTP media URL with a local audio/video fixture, first create a diagnostic file (FFmpeg is a test tool only, not a runtime dependency):

```sh
mkdir -p artifacts
ffmpeg -f lavfi -i testsrc2=size=640x360:rate=30 -f lavfi -i sine=frequency=440:sample_rate=48000 -t 30 -c:v libx264 -pix_fmt yuv420p -c:a aac -movflags +faststart artifacts/smoke.mp4
python3 scripts/smoke_cast.py --receiver http://192.168.2.40:5200 --local-media artifacts/smoke.mp4
```

The script serves the fixture over HTTP, subscribes to playback events, and sends standard UPnP controls. Use the LAN address rather than localhost for multicast discovery. Tests change the running player's media, volume, and playback state.

Current boundaries: one transport instance, one active media item, HTTP/HTTPS casting URLs, IPv4 discovery, and HTTP event callbacks with numeric local IPv4 addresses. The implementation does not yet extract sender-specific headers or cookies, queue the next item, implement AirPlay/Google Cast, or implement sender-specific authentication. GUI settings and app-specific protocol branches are absent. Bilibili casting has one user-reported successful test; additional mainstream-app compatibility and detailed playback/control checks need actual device verification.

## Build and package

Use Python 3.9+ and the Rust/native development dependencies described above:

```sh
python3 scripts/build.py
python3 scripts/build.py --check --package
```

The first command builds the release executable. The second runs formatting, Clippy, and Rust tests, then bundles native libraries and creates installers. Install the pinned packaging tool first:

```sh
cargo install cargo-packager --version 0.11.8 --locked
```

On macOS, also install `dylibbundler` (`brew install dylibbundler`). Packaging produces `Castrivo.app` and a `.dmg`, rewrites native library paths, checks that linked dependencies are bundled or system libraries, verifies the ad-hoc signature, and runs the bundled executable's version command. SDL3 is included when SDL2 uses the compatibility layer. The minimum macOS version follows the actual bundled libraries. These local packages are ad-hoc signed with library-loading and JIT permissions for bundled libmpv/LuaJIT; Apple signing and notarization are not configured. The development host's current Apple Silicon dependency set requires macOS 26.0; this is a measured local bundle requirement, not a project-wide platform baseline.

On Windows, use matching native SDL2/libmpv libraries and pkg-config metadata, with MinGW `objdump` on PATH and NSIS installed. The script collects imported DLLs recursively; add `--native-dir PATH` for additional runtime DLL directories. The default output is a current-user NSIS `.exe` installer. On Linux, the default is an AppImage; cargo-packager uses linuxdeploy to collect dependencies and may download packaging tools. Windows and Linux package execution still require validation on those platforms.

Build each installer on its native OS and architecture. Output goes to `dist/<rust-host-target>/`, alongside `BUILD-INFO.json` and installer `SHA256SUMS`. Use `--format` to select a host-supported format, `--out` to select another output directory, or `--skip-build` to package an existing release binary whose freshness you have verified. `--target` supports ordinary Rust cross-compilation, but installer generation requires the native host target. No player application needs to be installed by the recipient; bundled libraries still require clean-machine runtime and license verification before public distribution.

Run the packaging helper tests with:

```sh
python3 -m unittest discover -s tests -v
```

## License

The project license will be selected after dependency review. libmpv2 is LGPL-2.1; the license of libmpv depends on its build, and native dependencies carry their own terms. Generated bundles are for local testing until the license review is complete. Distribution must account for the complete license chain.
