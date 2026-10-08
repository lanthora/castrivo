#!/usr/bin/env python3
"""Archive native dependency sources and licenses for the matching release."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def output(*cmd):
    return subprocess.check_output(list(map(str, cmd)), text=True).strip()


def download(url, path):
    print('Download:', url, flush=True)
    with urllib.request.urlopen(url, timeout=180) as response, path.open('wb') as stream:
        shutil.copyfileobj(response, stream)
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--upgrade-native', action='store_true',
                        help='Update Linux runtime packages on an ephemeral CI builder before collecting matching sources')
    args = parser.parse_args()
    work = ROOT / 'artifacts/release-sources'
    sources = work / 'sources'
    notices = work / 'licenses'
    sources.mkdir(parents=True, exist_ok=True)
    notices.mkdir(exist_ok=True)
    entries = []
    if sys.platform == 'win32':
        sys.path.insert(0, str(ROOT / 'scripts'))
        from build import windows_libraries
        dlls = work / 'dlls'
        dlls.mkdir(exist_ok=True)
        prefix = Path(output('pkg-config', '--variable=prefix', 'mpv'))
        files = windows_libraries(args.binary.resolve(), dlls, [prefix / 'bin'])
        packages = set()
        for file in files:
            original = prefix / 'bin' / file.name
            unix_path = output('cygpath', '-u', original)
            packages.add(output('pacman', '-Qqo', unix_path))
        database = Path(output('cygpath', '-w', '/var/lib/pacman/local'))
        for package in sorted(packages):
            version = output('pacman', '-Q', package).split()[1]
            desc = (database / f'{package}-{version}' / 'desc').read_text()
            values = dict(re.findall(r'%([^%]+)%\n(.*?)\n\n', desc, re.S))
            base = values.get('BASE', package)
            archive = f'{base}-{version.split(":")[-1]}.src.tar.zst'
            url = 'https://mirror.msys2.org/mingw/sources/' + archive
            digest = download(url, sources / archive)
            license_dir = prefix / 'share/licenses' / base.removeprefix('mingw-w64-')
            for candidate in [license_dir, prefix / 'share/licenses' / package,
                              prefix / 'share/licenses' / base]:
                if candidate.is_dir():
                    shutil.copytree(candidate, notices / package, dirs_exist_ok=True)
            entries.append({'package': package, 'version': version, 'source': url,
                            'sha256': digest, 'licenses': values.get('LICENSE', '')})
        label = 'windows-x64'
    elif sys.platform.startswith('linux'):
        from prepare_appimage_runtime import prepare
        entries.append(prepare(work))
        packages = set()
        for path in re.findall(r'(?:=>\s+)?(/\S+)', output('ldd', args.binary.resolve())):
            library = Path(path)
            candidates = [library, library.resolve()]
            if str(library).startswith('/lib/'):
                candidates.append(Path('/usr' + str(library)))
            for candidate in candidates:
                result = subprocess.run(['dpkg-query', '-S', str(candidate)],
                                        text=True, capture_output=True)
                owners = re.findall(r'^([a-z0-9][a-z0-9+.-]*(?::[a-z0-9]+)?): /',
                                    result.stdout, re.M)
                if result.returncode == 0 and owners:
                    packages.update(owners)
                    break
            else:
                raise RuntimeError(f'Cannot identify native library owner: {library}')
        if args.upgrade_native:
            subprocess.run(['sudo', 'apt-get', 'install', '--only-upgrade', '-y',
                            *sorted(packages)], check=True)
        source_versions = set()
        for package in sorted(packages):
            source, version = output('dpkg-query', '-W', '-f=${source:Package} ${source:Version}', package).split()
            source_versions.add((source, version))
            copyright_file = Path('/usr/share/doc') / package.split(':')[0] / 'copyright'
            if copyright_file.exists():
                destination = notices / package.replace(':', '-')
                destination.mkdir(exist_ok=True)
                shutil.copy2(copyright_file, destination / 'copyright')
            entries.append({'package': package, 'sourcePackage': source, 'sourceVersion': version})
        source_arguments = [f'{source}={version}' for source, version in sorted(source_versions)]
        subprocess.run(['apt-get', 'source', '--download-only', *source_arguments],
                       cwd=sources, check=True)
        shutil.copytree('/usr/share/common-licenses', notices / 'common-licenses', dirs_exist_ok=True)
        label = 'linux-x64'
    else:
        parser.error('Use the native SDK archive for macOS')
    manifest = {'platform': label, 'packages': entries}
    (work / 'DEPENDENCIES.json').write_text(json.dumps(manifest, indent=2) + '\n')
    (notices / 'DEPENDENCIES.json').write_text(json.dumps(manifest, indent=2) + '\n')
    args.out.mkdir(parents=True, exist_ok=True)
    with tarfile.open(args.out / f'Castrivo_0.1.0_{label}_dependency-sources.tar.gz', 'w:gz') as bundle:
        bundle.add(sources, arcname='sources')
        bundle.add(notices, arcname='licenses')
        bundle.add(work / 'DEPENDENCIES.json', arcname='DEPENDENCIES.json')
    print('License directory:', notices, flush=True)


if __name__ == '__main__':
    main()
