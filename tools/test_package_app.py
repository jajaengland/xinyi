"""Host checks for the offline package builder (python -m unittest tools.test_package_app)."""
import tempfile
import unittest
from pathlib import Path
import struct
import zlib
from tools.package_app import build,MAX_BYTES

class PackageBuilderTests(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory();self.addCleanup(self.temporary.cleanup)
        self.folder=Path(self.temporary.name)/'DEMO';self.folder.mkdir()
        (self.folder/'DEMO.BIN').write_bytes(b'code')
        (self.folder/'DEMO.APP').write_text('XINYI-APP 1\nexec=DEMO.BIN\ntitle=Demo\ndescription=Builder check\nicon=application\naccess=read-only\nhandles=\nversion=1\n')
    def test_layout_checksum_and_nested_empty_assets(self):
        (self.folder/'DATA').mkdir();(self.folder/'DATA'/'EMPTY.TXT').touch();(self.folder/'EMPTY').mkdir()
        b=build(self.folder);self.assertEqual(b[:16],b'XINYIPK1DEMO\0\0\0\0')
        self.assertEqual(struct.unpack_from('<H',b,16)[0],5)
        self.assertEqual(struct.unpack_from('<I',b,20)[0],zlib.crc32(b[:20]+b[24:]))
        self.assertIn(b'DATA/EMPTY.TXT',b)
    def test_long_names_and_oversized_payload(self):
        bad=self.folder/'LONGFILENAME.TXT';bad.touch()
        with self.assertRaises(ValueError):build(self.folder)
        bad.unlink();(self.folder/'BIG.DAT').write_bytes(bytes(MAX_BYTES))
        with self.assertRaises(ValueError):build(self.folder)
    def test_ambiguous_executable_and_bad_manifest(self):
        exe=self.folder/'DEMO.ELF';exe.write_bytes(b'elf')
        with self.assertRaises(ValueError):build(self.folder)
        exe.unlink();(self.folder/'DEMO.APP').write_text('not a manifest')
        with self.assertRaises(ValueError):build(self.folder)
    def test_parent_depth(self):
        parent=self.folder
        for _ in range(7):parent=parent/'D';parent.mkdir()
        with self.assertRaises(ValueError):build(self.folder)
    def test_external_symlink_refused(self):
        source=Path(self.temporary.name)/'OUT.TXT';source.write_text('outside')
        try:(self.folder/'LINK.TXT').symlink_to(source)
        except OSError:self.skipTest('Host does not permit creating symlinks')
        with self.assertRaises(ValueError):build(self.folder)

if __name__=='__main__':unittest.main()
