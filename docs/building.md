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

Use `--out PATH` for another output directory. Local macOS packages are ad-hoc signed, without Apple notarization. The current Apple Silicon library bundle requires macOS 26 or later. Clean-machine testing and dependency license review remain pending; these packages are for local testing.

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
