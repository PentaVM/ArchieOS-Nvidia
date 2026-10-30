"""Owner launcher lifecycle regressions; no claim of live provider coverage."""
import unittest
from pathlib import Path
import subprocess
import sys
import tempfile
from globe_demo import maintain_navigation


class NavigationLifetime(unittest.TestCase):
    def setUp(self):
        self.current = dict(id='owned', expires_at_ms=1000)
        self.calls = []

    def status(self, session_id):
        self.calls.append(('status', session_id))
        return dict(id=session_id, state='expired')

    def renew(self):
        self.calls.append(('renew',))
        return dict(id='renewed', expires_at_ms=2000)

    def test_never_resets_budget_before_expiry(self):
        self.assertIs(maintain_navigation(self.current, self.status, self.renew, 999), self.current)
        self.assertEqual(self.calls, [])

    def test_expired_owned_session_renews(self):
        result = maintain_navigation(self.current, self.status, self.renew, 1000)
        self.assertEqual(result['id'], 'renewed')
        self.assertEqual(self.calls, [('status', 'owned'), ('renew',)])

    def test_explicit_stop_foreign_session_and_exhaustion_never_renew(self):
        for status in [dict(id='owned', state='stopped'), dict(id='other', state='expired'),
                       dict(id='owned', state='budget_exhausted'), dict(id='owned', state='ready')]:
            self.calls.clear()
            self.assertIsNone(maintain_navigation(self.current, lambda _: status, self.renew, 1001))
            self.assertEqual(self.calls, [])

    def test_disabled_renewal_stays_disabled(self):
        self.assertIsNone(maintain_navigation(None, self.status, self.renew, 1001))
        self.assertEqual(self.calls, [])

    def test_control_error_does_not_start_an_unverified_session(self):
        def failed(_):
            raise RuntimeError('owner control unavailable')
        with self.assertRaises(RuntimeError):
            maintain_navigation(self.current, failed, self.renew, 1001)
        self.assertEqual(self.calls, [])


class LauncherPrerequisites(unittest.TestCase):
    def test_missing_shell_bridge_is_reported_before_starting_services(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ['archie', 'archie-worldd', 'archie-displayd', 'native']:
                binary = root / name
                binary.write_text('launcher prerequisite fixture', encoding='utf-8')
                binary.chmod(0o700)
            result = subprocess.run([
                sys.executable, str(Path(__file__).with_name('globe_demo.py')),
                '--bin-dir', str(root), '--run-dir', str(root / 'run'),
                '--native-bin', str(root / 'native'),
            ], capture_output=True, text=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('archie-shell-bridge', result.stderr)
            self.assertFalse((root / 'run').exists())


if __name__ == '__main__':
    unittest.main()
