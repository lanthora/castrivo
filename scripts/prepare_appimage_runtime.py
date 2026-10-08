#!/usr/bin/env python3
"""Pin the AppImage runtime and retain its sources, relinking recipe and notices."""
import hashlib
from pathlib import Path
import urllib.request

COMMIT = '8f39b89e2ac31e1640b3d3f7e9a5108e6ce805fa'
RUNTIME_SHA256 = '156f4bdbde9c52d01814600013e0a273f0118dc2de98975f3c8c63427ec79074'


def prepare(work):
    sources = work / 'sources'
    notices = work / 'licenses/appimage-runtime'
    notices.mkdir(parents=True, exist_ok=True)
    runtime = work / 'runtime-x86_64'
    url = 'https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64'
    def fetch(url, destination, digest=None):
        data = urllib.request.urlopen(url, timeout=180).read()
        checksum = hashlib.sha256(data).hexdigest()
        if digest and checksum != digest:
            raise RuntimeError(f'Checksum mismatch: {url}')
        destination.write_bytes(data)
        return checksum
    fetch(url, runtime, RUNTIME_SHA256)
    archives = [
        ('type2-runtime.tar.gz', f'https://codeload.github.com/AppImage/type2-runtime/tar.gz/{COMMIT}', None),
        ('fuse-3.15.0.tar.xz', 'https://github.com/libfuse/libfuse/releases/download/fuse-3.15.0/fuse-3.15.0.tar.xz', '70589cfd5e1cff7ccd6ac91c86c01be340b227285c5e200baa284e401eea2ca0'),
        ('squashfuse-0.5.2.tar.gz', 'https://codeload.github.com/vasi/squashfuse/tar.gz/0.5.2', 'db0238c5981dabbd80ee09ae15387f390091668ca060a7bc38047912491443d3'),
    ]
    checksums = []
    for name, source_url, digest in archives:
        checksums.append(fetch(source_url, sources / name, digest) + '  ' + name)
    licenses = {
        'type2-runtime-MIT.txt': f'AppImage/type2-runtime/{COMMIT}/LICENSE',
        'AppRun-MIT.txt': 'AppImage/AppImageKit/13/LICENSE',
        'libfuse-LGPL2.txt': 'libfuse/libfuse/fuse-3.15.0/LGPL2.txt',
        'squashfuse-BSD.txt': 'vasi/squashfuse/0.5.2/LICENSE',
        'zstd-BSD.txt': 'facebook/zstd/v1.5.6/LICENSE',
        'mimalloc-MIT.txt': 'microsoft/mimalloc/v2.1.7/LICENSE',
        'zlib.txt': 'madler/zlib/v1.3.1/LICENSE',
    }
    for name, reference in licenses.items():
        fetch('https://raw.githubusercontent.com/' + reference, notices / name)
    fetch('https://git.musl-libc.org/cgit/musl/plain/COPYRIGHT?h=v1.2.5', notices / 'musl-MIT.txt')
    (notices / 'SOURCES-SHA256SUMS').write_text('\n'.join(checksums) + '\n')
    (notices / 'BUILD.txt').write_text(
        f'Runtime commit: {COMMIT}\nRuntime SHA256: {RUNTIME_SHA256}\n'
        'The source archive includes the upstream Docker build recipe and libfuse patch.\n'
        'Original libfuse 3.15.0 and squashfuse 0.5.2 sources are supplied alongside it.\n'
        'Rebuild/relink using BUILD.md and scripts/docker/build-with-docker.sh in type2-runtime.\n')
    return {'package': 'AppImage type2 runtime', 'commit': COMMIT, 'sha256': RUNTIME_SHA256,
            'sourceArchives': [x[0] for x in archives]}
