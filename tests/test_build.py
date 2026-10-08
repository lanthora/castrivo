"""Packaging dependency checks that do not require another operating system."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('build', Path(__file__).resolve().parents[1] / 'scripts/build.py')
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)


class PackagingTests(unittest.TestCase):
    def test_macos_versions_compare_with_optional_patch(self):
        self.assertEqual(build.version_tuple('11.0'), build.version_tuple('11.0.0'))
        self.assertGreater(build.version_tuple('26.0'), build.version_tuple('11.0'))

    def test_macos_packaging_rejects_dependencies_above_target(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def fake_run(*command, **kwargs):
                if command[:2] == ('pkg-config', '--variable=libdir'):
                    return str(root)
                if command[0] == 'dylibbundler':
                    (root / 'libraries/libmpv.dylib').write_bytes(b'library')
                if command[:2] == ('otool', '-l'):
                    return 'cmd LC_BUILD_VERSION\n minos 26.0\n sdk 26.5'
                return ''
            with patch.object(build, 'require'), patch.object(build, 'run', side_effect=fake_run):
                with self.assertRaisesRegex(RuntimeError, 'exceeding target 11.0'):
                    build.macos_libraries(root / 'castrivo', root, '11.0')

    def test_macos_dependency_parser_preserves_spaces(self):
        with patch.object(build, 'run', return_value='app:\n\t/opt/Video Libraries/libmpv.dylib (compatibility version 2.0.0)\n\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0)'):
            self.assertEqual(build.dylib_dependencies(Path('app')), ['/opt/Video Libraries/libmpv.dylib', '/usr/lib/libSystem.B.dylib'])

    def test_macos_verification_rejects_external_dependencies(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(build, 'dylib_dependencies', return_value=['/opt/homebrew/lib/libmpv.dylib']):
                with self.assertRaisesRegex(RuntimeError, 'Unbundled dependency'):
                    build.verify_macos(Path(directory) / 'Castrivo.app')

    def test_windows_collects_transitive_dlls_only(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            native = root / 'bin'; native.mkdir()
            stage = root / 'stage'; stage.mkdir()
            for name in ['mpv.dll', 'avcodec.dll', 'unused.dll']:
                (native / name).write_bytes(b'test')
            binary = stage / 'castrivo.exe'; binary.write_bytes(b'exe')
            imports = {'castrivo.exe': ['MPV.dll', 'api-ms-win-core-test.dll'], 'mpv.dll': ['avcodec.dll'], 'avcodec.dll': ['mpv.dll']}
            with patch.object(build, 'require'), patch.object(build, 'run', return_value=str(root)), patch.object(build, 'dll_imports', side_effect=lambda path: imports[path.name.lower()]):
                files = build.windows_libraries(binary, stage, [native])
            self.assertEqual({p.name for p in files}, {'mpv.dll', 'avcodec.dll'})
            self.assertFalse((stage / 'unused.dll').exists())

    def test_windows_missing_dll_is_not_silently_omitted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); (root / 'bin').mkdir()
            with patch.object(build, 'require'), patch.object(build, 'run', return_value=str(root)), patch.object(build, 'dll_imports', return_value=['missing-castrivo-test.dll']):
                with self.assertRaisesRegex(RuntimeError, 'Missing runtime DLL'):
                    build.windows_libraries(root / 'castrivo.exe', root, [])

    def test_windows_sdl_compat_requires_dynamically_loaded_sdl3(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); native = root / 'bin'; native.mkdir()
            (native / 'SDL2.dll').write_bytes(b'SDL2COMPAT_test')
            with patch.object(build, 'require'), patch.object(build, 'run', return_value=str(root)):
                with self.assertRaisesRegex(RuntimeError, 'SDL3.dll'):
                    build.windows_libraries(root / 'castrivo.exe', root, [])


if __name__ == '__main__':
    unittest.main()
