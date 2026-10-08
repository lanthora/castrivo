#!/usr/bin/env python3
"""Build an isolated ARM64/macOS 11 player SDK and archive it for reuse."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work', type=Path, default=ROOT / 'artifacts/macos-native')
    parser.add_argument('--verify-only', action='store_true', help='Verify and archive an already built SDK without compiling')
    args = parser.parse_args()
    if platform.system() != 'Darwin' or platform.machine() != 'arm64':
        parser.error('Requires an Apple Silicon macOS host')
    work = args.work.resolve()
    prefix = work / 'prefix'
    sources = json.loads((ROOT / 'packaging/macos-native-sources.json').read_text())
    for directory in ['sources', 'src', 'build', 'prefix']:
        (work / directory).mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.update(MACOSX_DEPLOYMENT_TARGET='11.0', CC='/usr/bin/clang',
               CXX='/usr/bin/clang++', CFLAGS='-O2 -mmacosx-version-min=11.0',
               CXXFLAGS='-O2 -mmacosx-version-min=11.0',
               LDFLAGS='-mmacosx-version-min=11.0',
               PKG_CONFIG_PATH=str(prefix / 'lib/pkgconfig'),
               PKG_CONFIG_LIBDIR=str(prefix / 'lib/pkgconfig'),
               CMAKE_PREFIX_PATH=str(prefix),
               PATH=str(work / 'tools/bin') + ':/usr/bin:/bin:/usr/sbin:/sbin')
    env['PKG_CONFIG'] = shutil.which('pkg-config') or '/opt/homebrew/bin/pkg-config'
    env['CFLAGS'] += ' -I' + str(prefix / 'include')
    env['CXXFLAGS'] += ' -I' + str(prefix / 'include')
    env.pop('DYLD_LIBRARY_PATH', None)
    env.pop('DYLD_FALLBACK_LIBRARY_PATH', None)
    jobs = str(min(os.cpu_count() or 4, 12))

    def run(*cmd, cwd=work):
        print('+', *map(str, cmd), flush=True)
        subprocess.run(list(map(str, cmd)), cwd=cwd, env=env, check=True)

    if not (work / 'tools/bin/meson').exists():
        run('/usr/bin/python3', '-m', 'venv', work / 'tools')
    run(work / 'tools/bin/pip', 'install', 'meson==1.7.2', 'ninja==1.11.1.4',
        'Jinja2==3.1.6', 'MarkupSafe==3.0.3')
    dirs = {}
    for name, entry in sources.items():
        archive = work / 'sources' / entry['archive']
        if not archive.exists():
            with urllib.request.urlopen(entry['url'], timeout=120) as response:
                archive.write_bytes(response.read())
        if hashlib.sha256(archive.read_bytes()).hexdigest() != entry['sha256']:
            raise RuntimeError(f'Checksum mismatch: {archive}')
        with tarfile.open(archive) as bundle:
            top = bundle.getnames()[0].split('/')[0]
            dirs[name] = work / 'src' / top
            if not dirs[name].exists():
                # Reject paths and links escaping the extraction directory.
                root = (work / 'src').resolve()
                for member in bundle.getmembers():
                    target = (root / member.name).resolve()
                    if root not in target.parents:
                        raise RuntimeError(f'Unsafe archive member: {member.name}')
                    if member.issym() or member.islnk():
                        link = ((target.parent if member.issym() else root) / member.linkname).resolve()
                        if root not in link.parents:
                            raise RuntimeError(f'Unsafe archive link: {member.name}')
                bundle.extractall(root)
    patch = ROOT / 'packaging/mpv-0.40-libmpv-opengl.patch'
    marker = dirs['mpv'] / '.castrivo-libmpv-opengl-patch'
    patch_digest = hashlib.sha256(patch.read_bytes()).hexdigest()
    if not marker.exists():
        run('patch', '-p1', '-i', patch, cwd=dirs['mpv'])
        marker.write_text(patch_digest)
    elif marker.read_text() != patch_digest:
        raise RuntimeError('Native source patch changed; use a fresh --work directory')

    def meson(name, *options, shared=False):
        build = work / 'build' / name
        reconfigure = ['--reconfigure', '--clearcache'] if (build / 'build.ninja').exists() else []
        run('meson', 'setup', *reconfigure, build, dirs[name], '--prefix', prefix,
            '--libdir=lib', '--buildtype=release', '--auto-features=disabled',
            '-Ddefault_library=' + ('shared' if shared else 'static'),
            '-Db_staticpic=true', '-Dprefer_static=' + ('false' if shared else 'true'),
            '--wrap-mode=nofallback', *options)
        run('meson', 'compile', '-C', build, '-j', jobs)
        run('meson', 'install', '-C', build)

    if not args.verify_only:
        # Native SDL2 avoids the additional SDL3 dependency from SDL2-compat.
        sdl_build = work / 'build/sdl2'
        cmake = shutil.which('cmake')
        if not cmake:
            raise RuntimeError('Install CMake before building')
        run(cmake, '-S', dirs['sdl2'], '-B', sdl_build, '-G', 'Ninja',
            '-DCMAKE_BUILD_TYPE=Release', '-DCMAKE_INSTALL_PREFIX=' + str(prefix),
            '-DCMAKE_OSX_DEPLOYMENT_TARGET=11.0', '-DCMAKE_OSX_ARCHITECTURES=arm64',
            '-DSDL_SHARED=ON', '-DSDL_STATIC=OFF', '-DSDL_TEST=OFF', '-DSDL_TESTS=OFF')
        run(cmake, '--build', sdl_build, '--parallel', jobs)
        run(cmake, '--install', sdl_build)
        meson('freetype', '-Dzlib=system', '-Dmmap=enabled')
        meson('fribidi', '-Ddocs=false', '-Dtests=false', '-Dbin=false')
        meson('harfbuzz', '-Dfreetype=enabled', '-Dcoretext=enabled',
              '-Dtests=disabled', '-Dutilities=disabled')
        meson('libass', '-Dcoretext=enabled')
        for name in ['fast_float', 'vulkan_headers']:
            shutil.copytree(dirs[name] / 'include', prefix / 'include', dirs_exist_ok=True)
        meson('libplacebo', '-Ddemos=false',
              '-Dc_args=-O2 -mmacosx-version-min=11.0 -I' + str(prefix / 'include'),
              '-Dcpp_args=-O2 -mmacosx-version-min=11.0 -I' + str(prefix / 'include'))
        # libplacebo's static pkg-config metadata omits its C++ runtime.
        pc = prefix / 'lib/pkgconfig/libplacebo.pc'
        contents = pc.read_text()
        if '-lc++' not in contents:
            pc.write_text(contents.replace(' -lplacebo -lm', ' -lplacebo -lm -lc++'))
        run('make', '-C', dirs['lua'] / 'src', '-j' + jobs, 'liblua.a',
            'CC=/usr/bin/clang', 'MYCFLAGS=-O2 -mmacosx-version-min=11.0 -DLUA_USE_POSIX -DLUA_USE_DLOPEN')
        shutil.copy2(dirs['lua'] / 'src/liblua.a', prefix / 'lib/liblua5.2.a')
        lua_headers = prefix / 'include/lua5.2'
        lua_headers.mkdir(exist_ok=True)
        for name in ['lua.h', 'luaconf.h', 'lualib.h', 'lauxlib.h']:
            shutil.copy2(dirs['lua'] / 'src' / name, lua_headers / name)
        (prefix / 'lib/pkgconfig/lua5.2.pc').write_text(
            f'prefix={prefix}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include/lua5.2\n'
            'Name: Lua\nDescription: Lua scripting engine\nVersion: 5.2.4\n'
            'Libs: -L${libdir} -llua5.2 -lm\nCflags: -I${includedir}\n')
        meson('dav1d', '-Denable_tools=false', '-Denable_tests=false')
        ffbuild = work / 'build/ffmpeg'
        ffbuild.mkdir(exist_ok=True)
        run(dirs['ffmpeg'] / 'configure', '--prefix=' + str(prefix),
                '--arch=arm64', '--target-os=darwin', '--cc=/usr/bin/clang',
                '--enable-static', '--disable-shared', '--enable-pic',
                '--disable-autodetect', '--disable-programs', '--disable-doc',
                '--enable-securetransport', '--enable-videotoolbox', '--enable-audiotoolbox',
                '--enable-zlib', '--enable-libdav1d', '--pkg-config=' + env['PKG_CONFIG'],
                '--pkg-config-flags=--static', '--disable-encoders', '--disable-muxers',
                '--extra-cflags=-mmacosx-version-min=11.0',
                '--extra-ldflags=-mmacosx-version-min=11.0', cwd=ffbuild)
        run('make', '-j' + jobs, cwd=ffbuild)
        run('make', 'install', cwd=ffbuild)
        meson('mpv', '-Dcplayer=false', '-Dlibmpv=true', '-Dlua=lua5.2',
              '-Dbuild-date=false', '-Dcoreaudio=enabled', '-Dplain-gl=enabled',
              '-Dcocoa=disabled', '-Dswift-build=disabled',
              '-Db_lundef=true', '-Dgl=enabled', '-Dgl-cocoa=enabled',
              '-Dvideotoolbox-gl=enabled', '-Diconv=enabled', '-Dzlib=enabled', shared=True)
    for name in ['libmpv.dylib', 'libSDL2.dylib']:
        if not (prefix / 'lib' / name).is_file():
            raise RuntimeError(f'Missing required SDK library: {name}')
    report = []
    for library in sorted((prefix / 'lib').glob('*.dylib')):
        if library.is_symlink():
            continue
        # Build-time SDK links resolve without DYLD_* or a Homebrew search path.
        # The app packager rewrites these IDs to Contents/Frameworks later.
        run('install_name_tool', '-id', library, library)
        run('codesign', '--force', '--sign', '-', library)
        commands = subprocess.check_output(['otool', '-l', str(library)], text=True)
        versions = re.findall(r'\bminos (\d+(?:\.\d+)+)', commands)
        if not versions or any(tuple(map(int, v.split('.'))) > (11, 0, 0) for v in versions):
            raise RuntimeError(f'{library.name} exceeds macOS 11: {versions}')
        arch = subprocess.check_output(['lipo', '-archs', str(library)], text=True).strip()
        if arch != 'arm64':
            raise RuntimeError(f'Unexpected architecture: {library.name}: {arch}')
        deps = subprocess.check_output(['otool', '-L', str(library)], text=True).splitlines()[2:]
        for line in deps:
            dep = line.strip().split(' (compatibility', 1)[0]
            if not dep.startswith(('/usr/lib/', '/System/Library/', str(prefix / 'lib') + '/')):
                raise RuntimeError(f'External dependency: {library.name}: {dep}')
        report.append({'file': library.name, 'minimumSystemVersion': versions[0], 'architecture': arch})
    # Include the exact source archives, license texts, and recipe with the SDK.
    licenses = prefix / 'share/castrivo-native'
    licenses.mkdir(parents=True, exist_ok=True)
    shutil.copy2(__file__, licenses / Path(__file__).name)
    shutil.copy2(ROOT / 'scripts/relocate_macos_native.py', licenses)
    shutil.copy2(ROOT / 'packaging/macos-native-sources.json', licenses)
    shutil.copy2(patch, licenses)
    for name, directory in dirs.items():
        destination = licenses / name
        destination.mkdir(exist_ok=True)
        for path in directory.rglob('*'):
            if path.is_file() and path.name.upper().startswith(('COPYING', 'LICENSE', 'COPYRIGHT', 'FTL')):
                relative = path.relative_to(directory)
                (destination / relative).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, destination / relative)
    shutil.copy2(dirs['lua'] / 'README', licenses / 'lua/README')
    info = {'target': 'aarch64-apple-darwin', 'deploymentTarget': '11.0', 'prefix': str(prefix),
            'libraries': report, 'sources': sources,
            'patches': {patch.name: patch_digest},
            'notes': 'Built for macOS 11; runtime on macOS 11 not yet tested. FFmpeg encoders/muxers and optional mpv backends are disabled. Lua is statically linked for application option compatibility.'}
    (prefix / 'BUILD-INFO.json').write_text(json.dumps(info, indent=2) + '\n')
    out = ROOT / 'dist/macos-native'
    out.mkdir(parents=True, exist_ok=True)
    archive = out / 'castrivo-native-macos-arm64-11.0.tar.gz'
    with tarfile.open(archive, 'w:gz') as bundle:
        bundle.add(prefix, arcname='prefix')
        bundle.add(work / 'sources', arcname='sources')
    (out / 'SHA256SUMS').write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '  ' + archive.name + '\n')
    print(f'Native SDK: {archive}\nPKG_CONFIG_PATH={prefix / "lib/pkgconfig"}', flush=True)


if __name__ == '__main__':
    main()
