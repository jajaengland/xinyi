"""Failure-path checks for the QEMU harness; no emulator or kernel build needed."""
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import smoke


class StartupEvidenceTests(unittest.TestCase):
    def run_failure(self, expected, guest=None):
        fixtures = smoke.ROOT / 'target'
        fixtures.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='xinyi-smoke-harness-', dir=fixtures) as temporary:
            root = Path(temporary)
            kernel = root / 'dummy-kernel'
            kernel.write_bytes(b'not booted')
            arguments = ['smoke.py', '--qemu', str(root / 'missing-qemu'), '--kernel', str(kernel)]
            with patch.object(smoke, 'ROOT', root), patch.object(sys, 'argv', arguments):
                if guest:
                    with patch.object(smoke, 'Guest', guest), self.assertRaises(expected):
                        smoke.main()
                else:
                    with self.assertRaises(expected):
                        smoke.main()
            output = root / 'target' / 'smoke'
            self.assertIn('[FAILED: shell]', (output / 'transcript.txt').read_text())
            self.assertTrue((output / 'shell-qemu.stderr').is_file())
            if guest:
                self.assertEqual((output / 'shell-debug.log').read_text(), 'startup diagnostics')
            # TemporaryDirectory cleanup also checks the Windows stderr handle is released.

    def test_missing_executable_keeps_original_error_and_closes_log(self):
        self.run_failure(FileNotFoundError)

    def test_startup_failure_retains_existing_debug_output(self):
        class FailedGuest:
            def __init__(self, _qemu, _kernel, directory):
                (directory / 'debug.log').write_text('startup diagnostics')
                (directory / 'qemu.stderr').write_text('startup error')
                raise RuntimeError('fixture startup failure')
        self.run_failure(RuntimeError, FailedGuest)


if __name__ == '__main__':
    unittest.main()
