#!/usr/bin/env python3
"""Update an extracted native SDK's build paths before linking against it."""
import json
from pathlib import Path
import subprocess
import sys


def main():
    prefix = Path(sys.argv[1]).resolve()
    manifest = prefix / 'BUILD-INFO.json'
    info = json.loads(manifest.read_text())
    old = info['prefix']
    for directory, pattern in [('lib/pkgconfig', '*.pc'), ('lib/cmake', '*.cmake'),
                               ('lib', '*.la'), ('bin', '*-config')]:
        for path in (prefix / directory).rglob(pattern):
            if path.is_file():
                path.write_text(path.read_text().replace(old, str(prefix)))
    for path in sorted((prefix / 'lib').glob('*.dylib')):
        if path.is_symlink():
            continue
        lines = subprocess.check_output(['otool', '-L', str(path)], text=True).splitlines()[1:]
        for index, line in enumerate(lines):
            dep = line.strip().split(' (compatibility', 1)[0]
            if dep.startswith(old + '/'):
                new = str(prefix) + dep[len(old):]
                cmd = ['install_name_tool', '-id', new, str(path)] if index == 0 else [
                    'install_name_tool', '-change', dep, new, str(path)]
                subprocess.run(cmd, check=True)
        subprocess.run(['codesign', '--force', '--sign', '-', str(path)], check=True)
    info['prefix'] = str(prefix)
    manifest.write_text(json.dumps(info, indent=2) + '\n')
    print(f'PKG_CONFIG_PATH={prefix / "lib/pkgconfig"}')


if __name__ == '__main__':
    main()
