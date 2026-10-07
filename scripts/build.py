#!/usr/bin/env python3
"""Build Castrivo and optionally create a native installer with bundled libraries."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PACKAGER_VERSION = "0.11.8"
FORMATS = {"darwin": ["app", "dmg"], "win32": ["nsis"], "linux": ["appimage"]}


def run(*command, capture=False, env=None):
    print("+ " + " ".join(map(str, command)), flush=True)
    result = subprocess.run(list(map(str, command)), cwd=ROOT, env=env,
                            check=True, text=True, stdout=subprocess.PIPE if capture else None)
    return result.stdout.strip() if capture else None


def require(tool, instructions):
    if not shutil.which(tool):
        raise RuntimeError(f"Missing {tool}. {instructions}")


def dylib_dependencies(path):
    return [line.strip().split(" (compatibility", 1)[0]
            for line in run("otool", "-L", path, capture=True).splitlines()[1:]]


def system_dylib(name):
    return name.startswith(("/usr/lib/", "/System/Library/"))


def macos_libraries(binary, stage):
    require("dylibbundler", "Install it with: brew install dylibbundler")
    libraries = stage / "libraries"
    libraries.mkdir()
    extra = []
    # Homebrew's SDL2 compatibility layer dlopens SDL3; otool cannot see that edge.
    sdl_dir = Path(run("pkg-config", "--variable=libdir", "sdl2", capture=True))
    sdl = sdl_dir / "libSDL2.dylib"
    if sdl.is_file() and b"SDL2COMPAT_" in sdl.read_bytes():
        sdl3_dir = Path(run("pkg-config", "--variable=libdir", "sdl3", capture=True))
        source = sdl3_dir / "libSDL3.dylib"
        if not source.is_file():
            raise RuntimeError("SDL2-compat requires SDL3 development libraries")
        destination = libraries / "libSDL3.dylib"
        shutil.copy2(source, destination)
        run("install_name_tool", "-id", "@executable_path/../Frameworks/libSDL3.dylib", destination)
        extra = ["-x", destination]
    run("dylibbundler", "-b", "-x", binary, *extra, "-d", libraries,
        "-p", "@executable_path/../Frameworks/")
    files = sorted(libraries.glob("*.dylib"))
    if not files:
        raise RuntimeError("No native libraries were bundled")
    versions = []
    for path in [binary, *files]:
        load_commands = run("otool", "-l", path, capture=True)
        # Bundled links use explicit Frameworks paths. Remove inherited search
        # paths, which dylibbundler can rewrite into duplicate LC_RPATH entries.
        for rpath in dict.fromkeys(re.findall(r"cmd LC_RPATH\s+cmdsize \d+\s+path (.*?) \(offset", load_commands)):
            run("install_name_tool", "-delete_rpath", rpath, path)
        # Use requirements from the executable and every library, not an invented OS baseline.
        versions.extend(re.findall(r"\bminos (\d+(?:\.\d+)+)", load_commands))
        versions.extend(re.findall(r"cmd LC_VERSION_MIN_MACOSX\s+cmdsize \d+\s+version (\d+(?:\.\d+)+)", load_commands))
    settings = {"frameworks": list(map(str, files)), "signingIdentity": "-",
                "entitlements": str(ROOT / "packaging/entitlements.plist")}
    if versions:
        settings["minimumSystemVersion"] = max(versions, key=lambda v: tuple(map(int, v.split('.'))))
    return files, settings


def dll_imports(path):
    return re.findall(r"DLL Name:\s*(\S+)", run("objdump", "-p", path, capture=True))


def windows_libraries(binary, stage, directories):
    require("objdump", "Use the matching MSYS2/MinGW toolchain on PATH")
    # Do not copy an entire toolchain. Collect the imported DLL closure only.
    for package in ["mpv", "sdl2"]:
        prefix = run("pkg-config", "--variable=prefix", package, capture=True)
        if prefix:
            directories.append(Path(prefix) / "bin")
    candidates = {}
    for directory in directories:
        if not directory.is_dir():
            raise RuntimeError(f"DLL directory does not exist: {directory}")
        for path in directory.glob("*.dll"):
            candidates.setdefault(path.name.lower(), path)
    system = Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32"
    pending = [binary]
    collected = {}
    sdl = candidates.get("sdl2.dll")
    if sdl and b"SDL2COMPAT_" in sdl.read_bytes():
        sdl3 = candidates.get("sdl3.dll")
        if not sdl3:
            raise RuntimeError("SDL2-compat requires SDL3.dll in a native directory")
        collected["sdl3.dll"] = sdl3
        pending.append(sdl3)
    while pending:
        for name in dll_imports(pending.pop()):
            key = name.lower()
            if key in collected or key.startswith(("api-ms-win-", "ext-ms-win-")):
                continue
            dependency = candidates.get(key)
            if dependency is None:
                if (system / name).is_file():
                    continue
                raise RuntimeError(f"Missing runtime DLL: {name}. Supply --native-dir with matching libraries")
            collected[key] = dependency
            pending.append(dependency)
    if not collected:
        raise RuntimeError("No native DLLs found; check the target and import tool")
    files = []
    for path in collected.values():
        destination = stage / path.name
        shutil.copy2(path, destination)
        files.append(destination)
    return files


def verify_macos(app):
    frameworks = app / "Contents/Frameworks"
    executable = app / "Contents/MacOS/castrivo"
    for path in [executable, *frameworks.glob("*.dylib")]:
        for dependency in dylib_dependencies(path):
            if system_dylib(dependency):
                continue
            if dependency.startswith("@executable_path/../Frameworks/"):
                resolved = frameworks / dependency.removeprefix("@executable_path/../Frameworks/")
            elif dependency.startswith("@loader_path/"):
                resolved = path.parent / dependency.removeprefix("@loader_path/")
            else:
                raise RuntimeError(f"Unbundled dependency in {path.name}: {dependency}")
            if not resolved.is_file():
                raise RuntimeError(f"Missing bundled dependency: {resolved}")
    run("codesign", "--verify", "--deep", "--strict", app)
    # No player, SDK, or package-manager environment is needed for this CLI check.
    env = {k: v for k, v in os.environ.items() if not k.startswith(("DYLD_", "PKG_CONFIG"))}
    env["PATH"] = "/usr/bin:/bin"
    run(executable, "--version", env=env)


def package(binary, metadata, args, target):
    require("cargo-packager", f"Install it with: cargo install cargo-packager --version {PACKAGER_VERSION} --locked")
    reported = run("cargo-packager", "--version", capture=True)
    if reported.split()[-1] != PACKAGER_VERSION:
        raise RuntimeError(f"Expected cargo-packager {PACKAGER_VERSION}, got {reported}")
    formats = args.format or FORMATS.get(sys.platform)
    if not formats or any(value not in FORMATS.get(sys.platform, []) for value in formats):
        raise RuntimeError("Choose package formats supported on this host")
    out = args.out.resolve() if args.out else ROOT / "dist" / target
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="castrivo-package-", dir=binary.parent) as directory:
        stage = Path(directory)
        staged_binary = stage / binary.name
        shutil.copy2(binary, staged_binary)
        config = json.loads((ROOT / "packaging/packager.json").read_text())
        config.update(version=metadata["version"], targetTriple=target, outDir=str(out), binariesDir=str(stage), formats=formats)
        resources = [{"src": str(ROOT / "README.md"), "target": "README.md"}]
        files = []
        if sys.platform == "darwin":
            files, config["macos"] = macos_libraries(staged_binary, stage)
        elif sys.platform == "win32":
            files = windows_libraries(staged_binary, stage, args.native_dir.copy())
            resources += [{"src": str(path), "target": path.name} for path in files]
        else:
            # linuxdeploy, used by cargo-packager, gathers transitive ELF dependencies.
            libraries = ["libmpv.so.*", "libSDL2-2.0.so.*"]
            sdl_dir = Path(run("pkg-config", "--variable=libdir", "sdl2", capture=True))
            if any(b"SDL2COMPAT_" in path.read_bytes() for path in sdl_dir.glob("libSDL2*.so") if path.is_file()):
                libraries.append("libSDL3.so.*")
            config["appimage"] = {"libs": libraries}
        info = {"name": "Castrivo", "version": metadata["version"], "target": target,
                "packager": reported, "nativeLibraries": [p.name for p in files],
                "licenseReview": "Pending; these are local test packages, not approved public releases."}
        if sys.platform == "darwin":
            info["minimumSystemVersion"] = config["macos"].get("minimumSystemVersion")
        manifest = stage / "BUILD-INFO.json"
        manifest.write_text(json.dumps(info, indent=2) + "\n")
        config["resources"] = resources + [{"src": str(manifest), "target": manifest.name}]
        config["icons"] = [str(ROOT / "packaging/icon.png")]
        config_file = stage / "packager.json"
        config_file.write_text(json.dumps(config, indent=2) + "\n")
        # Only local ad-hoc signing. Never pick up notarization/upload credentials implicitly.
        env = {k: v for k, v in os.environ.items() if not k.startswith(("APPLE_", "CARGO_PACKAGER_SIGN_"))}
        if sys.platform == "darwin" and (out / "Castrivo.app").exists():
            # Fresh inodes avoid macOS retaining signatures from a previous build.
            shutil.rmtree(out / "Castrivo.app")
        run("cargo-packager", "--config", config_file, env=env)
        if sys.platform == "darwin":
            verify_macos(out / "Castrivo.app")
        shutil.copy2(manifest, out / manifest.name)
    suffixes = {"dmg": ".dmg", "nsis": ".exe", "appimage": ".AppImage"}
    for fmt in formats:
        if fmt == "app":
            continue
        outputs = list(out.glob("*" + suffixes[fmt]))
        if not outputs:
            raise RuntimeError(f"Packager did not produce a {fmt} file")
    sums = []
    for path in sorted(out.iterdir()):
        if path.is_file() and path.suffix in suffixes.values():
            with path.open("rb") as stream:
                digest = hashlib.sha256()
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(chunk)
                sums.append(f"{digest.hexdigest()}  {path.name}")
    (out / "SHA256SUMS").write_text("\n".join(sums) + "\n")
    print(f"Packages: {out}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", action="store_true", help="Bundle native libraries and create host-platform installers")
    parser.add_argument("--check", action="store_true", help="Run formatting, Clippy, and tests before the release build")
    parser.add_argument("--target", help="Rust target triple; installer generation requires the native host target")
    parser.add_argument("--format", action="append", choices=["app", "dmg", "nsis", "appimage"], help="Package format; repeat to select multiple")
    parser.add_argument("--native-dir", type=Path, action="append", default=[], help="Additional Windows runtime DLL directory")
    parser.add_argument("--out", type=Path, help="Package output directory (default: dist/HOST_TARGET)")
    parser.add_argument("--skip-build", action="store_true", help="Package an existing release binary; caller is responsible for freshness")
    args = parser.parse_args()
    require("cargo", "Install Rust with rustup")
    require("pkg-config", "Install pkg-config and SDL2/libmpv development libraries")
    if (args.format or args.native_dir or args.out or args.skip_build) and not args.package:
        parser.error("--format, --native-dir, --out, and --skip-build require --package")
    host = next(line.removeprefix("host: ") for line in run("rustc", "-vV", capture=True).splitlines() if line.startswith("host: "))
    target = args.target or host
    if args.package and target != host:
        parser.error("Build each installer on its native OS/architecture; cross-packaging is not supported")
    metadata = json.loads(run("cargo", "metadata", "--no-deps", "--format-version", "1", capture=True))
    project = next(p for p in metadata["packages"] if p["name"] == "castrivo")
    target_args = ["--target", target] if args.target else []
    if args.check:
        run("cargo", "fmt", "--check")
        run("cargo", "clippy", "--locked", "--all-targets", *target_args, "--", "-D", "warnings")
        run("cargo", "test", "--locked", *target_args)
    if not args.skip_build:
        run("cargo", "build", "--release", "--locked", *target_args)
    binary = Path(metadata["target_directory"]) / (target if args.target else "") / "release" / ("castrivo.exe" if "windows" in target else "castrivo")
    if not binary.is_file():
        raise RuntimeError(f"Release binary not found: {binary}")
    print(f"Release binary: {binary}")
    if args.package:
        package(binary, project, args, target)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Build failed: {error}", file=sys.stderr)
        sys.exit(1)
