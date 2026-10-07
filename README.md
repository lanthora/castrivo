# Castrivo

Turn your desktop into a video casting receiver.

Castrivo is a planned Rust desktop application that lets compatible video apps send video to a computer as if it were a TV. Choose a video on your phone, select Castrivo from the app's casting menu, and watch it on your desktop.

## Project status

This repository currently contains the project README only. No application code, build configuration, or working receiver has been implemented. The immediate goal is a minimal feasibility prototype. First-release features remain longer-term goals, not requirements for that prototype.

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
- Use a cross-platform window and rendering design from the start. Validation may begin on an available development platform, but the architecture must not depend on macOS-specific behavior. Cross-platform packaging is deferred.

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
| Device discovery | SSDP discovery responses and device advertisements on the selected interface |
| Receiver protocol | Prefer a reusable DLNA MediaRenderer library for descriptions, SOAP, and GENA; library selection pending |
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

A future compatibility matrix should record the video app, app version, sender operating system, desktop platform, media type, and observed discovery, playback, and control results.

## Desktop platform targets

The intended desktop platforms are macOS, Windows, and Linux. Include both X11 and Wayland in Linux validation. Minimum operating-system versions will follow the selected dependency requirements rather than custom legacy-platform support. Use the same implementation across platforms and announce support separately as each is verified. None is supported yet. Discovery, video rendering, audio output, and packaging must be validated separately on each platform before claiming support.

## Implementation milestones

1. **Receiver protocol foundation:** evaluate reusable MediaRenderer libraries and define a standards-based DLNA/UPnP contract and a compatibility test matrix spanning mainstream video apps and phone operating systems; no single target app or phone is an implementation prerequisite.
2. **Playback foundation:** adapt the libmpv2 SDL2 OpenGL example into the retained cross-platform playback foundation. Verify video, audio, resizing, and shutdown with representative media; retain this path for the receiver.
3. **Minimal casting prototype:** connect discovery and standard receiver service actions to that playback foundation. Record real-app discovery, video, audio, and stop results.
4. **Receiver completeness:** expand service actions, subscriptions, playback events, controls, and session transitions for standards compliance and observed interoperability requirements.
5. **Desktop experience:** add receiver configuration, playback controls, fullscreen, and actionable errors on the existing playback foundation. Keep the first release limited to shortcuts and terminal configuration; consider widgets only for later demonstrated needs.
6. **Compatibility and distribution:** publish measured app compatibility and validate platform packages and native dependency licensing.

## Remaining implementation decisions

The architecture and behavior above are accepted. Remaining choices are implementation details or outcomes that require evidence:

- **Receiver library:** identify and review a reusable DLNA MediaRenderer implementation; document any missing services or events. If none is suitable, justify the smallest necessary custom implementation.
- **Dependency versions and native artifacts:** select compatible stable SDL2, libmpv2, and libmpv versions, existing native build tools or artifacts, and per-platform package layouts. Dynamic linking and bundled release dependencies are already decided.
- **Network defaults:** determine the automatic interface-selection rule, default HTTP port, and stable device identity mechanism. Keep configuration to command-line options with defaults initially.
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

There are no build or run commands yet. A Cargo workspace and development instructions will be added when implementation begins.

## License

The project license has not been selected. Distribution must account for the licenses of the playback engine and its bundled dependencies.
