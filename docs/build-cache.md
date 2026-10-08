# Release build caches

The release workflow runs on main to warm caches shared with future version tags.
Only version tags publish a GitHub Release. A manual run on main also warms caches.

- Rust dependencies are keyed by compiler, Cargo inputs, platform and installed native packages.
- cargo-packager 0.11.8 is installed in a separate cache per runner and target.
- The macOS 11 SDK caches its completed prefix and source archive rather than intermediate builds. Its key includes pinned source versions, checksums, build recipe and patch. Restored libraries are relocated and signed; packaging still verifies library architecture and minimum system versions.
- Windows dependency sources and notices are reused only with identical installed package versions, collector, native library discovery code, actual DLL dependency closure.
- Linux source downloads are retained across package updates. apt still resolves sources for the installed package versions. Partial downloads are never treated as complete files; pinned AppImage dependencies are checksum checked.

Source bundle filenames use the current Cargo version, even when cached content is reused.
Normal compilation, checks, packaging and installer smoke tests continue to run.

Changing a cache key's revision (for example `v1` to `v2`) forces regeneration.
GitHub may evict caches, so every cache has a cold-build fallback. Changes to
compilers or native dependencies intentionally cause misses. This does not promise
an identical build duration on every runner.
