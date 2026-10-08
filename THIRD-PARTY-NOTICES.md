# Third-party software

Castrivo is licensed under GPL-3.0-or-later. Its dependencies retain their own
copyrights and licenses; the project license does not replace those licenses.

Each release includes a source archive with Cargo.lock, vendored Rust crates,
their original license files, the application source, and build instructions.
The matching platform dependency-source archive supplies native dependency
sources, distribution build recipes/patches, version metadata and license
notices. Refer to those notices for the applicable license of each component.

- macOS: SDL2, libmpv, FFmpeg, dav1d, FreeType, FriBidi, HarfBuzz, libass,
  libplacebo, Lua, fast_float and Vulkan headers. Their versions, source
  checksums and the mpv patch are recorded in packaging/macos-native-sources.json
  and the native SDK's BUILD-INFO.json. Other required libraries/frameworks are
  supplied by macOS.
- Windows: MSYS2 provides SDL2/libmpv and their runtime DLL dependencies. The
  dependency-source archive contains exact-version source tarballs, PKGBUILD
  recipes and installed package license notices.
- Linux: Ubuntu provides SDL2/libmpv and their native dependencies. The
  dependency-source archive contains exact-version Debian/Ubuntu source
  packages (including distribution patches and build recipes) and copyright
  notices. The AppImage packaging runtime also retains its upstream license.

To rebuild, start with docs/building.md and .github/workflows/release.yml.
The macOS SDK contains original source archives plus the included mpv patch;
Rust dependencies can be built offline with the source archive's vendored crates.

No warranty is provided. See LICENSE and the individual dependency licenses.
