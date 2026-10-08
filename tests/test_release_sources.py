"""Release source downloads only cache completely transferred files."""
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    'collect_release_sources', Path(__file__).resolve().parents[1] / 'scripts/collect_release_sources.py')
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class DownloadTests(unittest.TestCase):
    def test_completed_download_is_reused(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / 'source.tar.gz'
            with patch.object(MODULE.urllib.request, 'urlopen', return_value=io.BytesIO(b'source')) as fetch:
                first = MODULE.download('https://example.org/source', archive)
                second = MODULE.download('https://example.org/source', archive)
                self.assertEqual(first, second)
                fetch.assert_called_once()
                self.assertFalse(archive.with_suffix('.gz.part').exists())

    def test_failed_download_does_not_become_cache_hit(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / 'source.tar.gz'
            with patch.object(MODULE.urllib.request, 'urlopen', return_value=io.BytesIO(b'source')):
                with patch.object(MODULE.shutil, 'copyfileobj', side_effect=OSError('connection lost')):
                    with self.assertRaises(OSError):
                        MODULE.download('https://example.org/source', archive)
            self.assertFalse(archive.exists())
            with patch.object(MODULE.urllib.request, 'urlopen', return_value=io.BytesIO(b'complete')):
                MODULE.download('https://example.org/source', archive)
            self.assertEqual(archive.read_bytes(), b'complete')
