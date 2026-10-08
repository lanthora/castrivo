# Building from source

Install Rust, Python 3.9 or later, and the SDL2/libmpv development libraries with pkg-config metadata.

## macOS

```sh
brew install pkg-config sdl2 mpv
cargo run
```

## Linux

On Debian or Ubuntu:

```sh
sudo apt install pkg-config libsdl2-dev libmpv-dev build-essential
cargo run
```

Linux runtime and packaging have not yet been verified.

## Windows

Provide SDL2 and libmpv libraries compatible with the Rust target, along with their pkg-config metadata. Set `PKG_CONFIG_PATH` if needed. Windows builds and runtime have not yet been verified.

## Options

```sh
cargo run -- --name "Living room"
cargo run -- --ip 192.168.1.10
cargo run -- --help
```

Use the computer's local network address with `--ip` if automatic selection chooses the wrong network, such as a VPN. The default control port is 5200; override it with `--port`.

## Packaging

```sh
cargo install cargo-packager --version 0.11.8 --locked
python3 scripts/build.py --check --package
```

On macOS, also install `dylibbundler`:

```sh
brew install dylibbundler
```

Build packages on their native operating system and architecture. Output goes to `dist/<rust-host-target>/`. macOS produces an application and DMG, Windows an NSIS installer, and Linux an AppImage. Windows packaging needs NSIS and MinGW `objdump` on PATH; use `--native-dir PATH` for additional runtime DLL directories.

For an application-only macOS build:

```sh
python3 scripts/build.py --package --format app
```

Use `--out PATH` for another output directory. Local macOS packages are ad-hoc signed, without Apple notarization. The original Homebrew-based Apple Silicon library bundle requires macOS 26 or later; use the isolated SDK below to build for older systems. Clean-machine testing and dependency license review remain pending; these packages are for local testing.

### macOS 11 ARM64 dependency SDK

To avoid inheriting Homebrew runtime libraries' minimum system versions, build
the isolated native SDK on an Apple Silicon Mac (Xcode command-line tools,
CMake, pkg-config, Rust, cargo-packager and dylibbundler are required):

```sh
python3 scripts/build_macos_native.py
export MACOSX_DEPLOYMENT_TARGET=11.0
export PKG_CONFIG_PATH="$PWD/artifacts/macos-native/prefix/lib/pkgconfig"
export PKG_CONFIG_LIBDIR="$PKG_CONFIG_PATH"
export CARGO_TARGET_DIR="$PWD/target/macos11"
python3 scripts/build.py --check --package --macos-max-version 11.0
```

The SDK archive and checksum are in `dist/macos-native/`. It includes headers,
static dependency archives, SDL2/libmpv dynamic libraries, original source
archives, licenses, the build recipe, and `BUILD-INFO.json`. Versions and source
SHA-256 digests are pinned in `packaging/macos-native-sources.json`. Re-running
the build reuses downloaded sources and incremental build directories.

When extracting the SDK at a different path, relocate its library install names
and pkg-config files before building the application:

```sh
tar -xzf /path/to/castrivo-native-macos-arm64-11.0.tar.gz -C /path/to/sdk
python3 scripts/relocate_macos_native.py /path/to/sdk/prefix
export MACOSX_DEPLOYMENT_TARGET=11.0
export PKG_CONFIG_PATH=/path/to/sdk/prefix/lib/pkgconfig
export PKG_CONFIG_LIBDIR="$PKG_CONFIG_PATH"
export CARGO_TARGET_DIR="$PWD/target/macos11"
python3 scripts/build.py --check --package --macos-max-version 11.0
```

This SDK uses original SDL2 (not SDL2-compat/SDL3) and libmpv 0.40. It retains
FFmpeg's built-in decoders, dav1d for AV1, HTTPS via Apple's Secure Transport, CoreAudio,
VideoToolbox/OpenGL, and CoreText-backed subtitles. FFmpeg encoders/muxers,
mpv CLI, Vulkan, optional external codec libraries and optional
player backends are disabled. OpenGL rendering uses mpv's existing renderer;
libplacebo is built without its optional GPU backends. The bundled mpv patch
allows macOS OpenGL/VideoToolbox in a library-only build without mpv's Cocoa
application or Swift runtime; Castrivo owns the SDL window and GL context.
Lua is statically linked so Castrivo can retain its existing OSC/ytdl options.
This is a deployment target, not proof of macOS 11 runtime
compatibility; old-system playback testing and distribution license review are
still required.

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
python3 -m unittest discover -s tests -v
```

To exercise a running receiver, replace the address with its local network address:

```sh
python3 scripts/smoke_cast.py --receiver http://192.168.1.10:5200
```

This test replaces the running video's media and changes playback state and volume.

## Logs

Startup prints the log directory. On macOS, logs are in `~/Library/Application Support/dev.castrivo.Castrivo/logs`. Up to seven daily log files are retained. `RUST_LOG=castrivo=debug` enables extra terminal diagnostics.

Use `--mpv-log /absolute/path/mpv.log` only when detailed playback diagnostics are needed. That file may contain media URLs or credentials; review it before sharing. Normal application logs omit media URLs and metadata.

## License

A project license has not yet been selected. Review the complete native dependency license chain before distributing packages.
