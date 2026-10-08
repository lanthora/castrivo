#!/usr/bin/env python3
"""Normalize release filenames and create a Finder-independent macOS DMG."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--platform', required=True, choices=['macos-arm64', 'windows-x64', 'linux-x64'])
    parser.add_argument('--packages', type=Path, required=True)
    parser.add_argument('--out', type=Path, default=Path('dist/release'))
    args = parser.parse_args()
    info = json.loads((args.packages / 'BUILD-INFO.json').read_text())
    version = info['version']
    args.out.mkdir(parents=True, exist_ok=True)
    base = f'Castrivo_{version}_{args.platform}'
    if args.platform == 'macos-arm64':
        with tempfile.TemporaryDirectory(prefix='castrivo-dmg-') as directory:
            stage = Path(directory)
            subprocess.run(['ditto', str(args.packages / 'Castrivo.app'), str(stage / 'Castrivo.app')], check=True)
            (stage / 'Applications').symlink_to('/Applications')
            subprocess.run(['hdiutil', 'create', '-volname', 'Castrivo', '-srcfolder', str(stage),
                            '-ov', '-format', 'UDZO', str(args.out / (base + '.dmg'))], check=True)
    else:
        suffix = '.exe' if args.platform == 'windows-x64' else '.AppImage'
        packages = list(args.packages.glob('*' + suffix))
        if len(packages) != 1:
            raise RuntimeError(f'Expected one installer, found {packages}')
        shutil.copy2(packages[0], args.out / (base + ('_setup.exe' if suffix == '.exe' else suffix)))
    shutil.copy2(args.packages / 'BUILD-INFO.json', args.out / (base + '_BUILD-INFO.json'))


if __name__ == '__main__':
    main()
