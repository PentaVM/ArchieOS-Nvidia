"""Exercise real operate task construction; external session/model calls are stubs.

These tests start no WSL services, browsers or models and prove no IRIS accuracy.
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest

REPO = Path(__file__).resolve().parents[2]
OPERATE = REPO / 'distro/wsl-operator/rootfs/usr/bin/archie-operate'


def bash_path(path):
    text = str(path).replace('\\', '/')
    if len(text) > 1 and text[1] == ':':
        return '/' + text[0].lower() + text[2:]
    return text


class OperateTask(unittest.TestCase):
    def run_operate(self, goal, *arguments):
        bash = os.environ.get('ARCHIE_TEST_BASH')
        if not bash and os.name == 'nt':
            # Windows' system bash.exe starts WSL. This test must stay native.
            git_bash = Path(os.environ.get('ProgramFiles', 'C:/Program Files')) / 'Git/bin/bash.exe'
            bash = str(git_bash) if git_bash.is_file() else None
        elif not bash:
            bash = shutil.which('bash')
        if not bash:
            self.skipTest('bash is required')
        with tempfile.TemporaryDirectory(prefix='archie-operate-task-') as temp:
            root = Path(temp)
            run = root / 'run'
            run.mkdir()
            env = dict(os.environ, HOME=bash_path(root), ARCHIE_RUN=bash_path(run),
                       OPERATE_SCRIPT=bash_path(OPERATE),
                       ARCHIE_TEST_PYTHON=str(Path(sys.executable)).replace('\\', '/'),
                       NEBIUS_TOKEN_FACTORY_KEY='noncredential-test-fixture')
            # Session startup and model inference have real external side effects.
            # Replace those boundaries; the actual wrapper writes its actual task TOML.
            harness = r'''
jq() {
  [ "$#" -eq 5 ] && [ "$1" = -Rn ] && [ "$2" = --arg ] && [ "$3" = v ] || return 99
  "$ARCHIE_TEST_PYTHON" -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$4"
}
archie-session() { printf '%s\n' "$*" >> "$ARCHIE_RUN/session-calls"; }
archie-arena-live() {
  printf '%s\n' "$@" > "$ARCHIE_RUN/arena-arguments"
  mkdir -p "$HOME/archie-runs/operate/test-run"
}
source "$OPERATE_SCRIPT" "$@"
'''
            result = subprocess.run([bash, '-c', harness, 'operate-test', goal, *arguments],
                                    env=env, capture_output=True, text=True, timeout=15)
            task_file = run / 'task.toml'
            task = tomllib.loads(task_file.read_text()) if task_file.exists() else None
            calls = (run / 'session-calls').read_text() if (run / 'session-calls').exists() else ''
            arena = (run / 'arena-arguments').read_text().splitlines() if (run / 'arena-arguments').exists() else []
            return result, task, calls, arena

    def test_firefox_preserves_one_goal_and_requests_gui_clicks(self):
        goal = 'Open Firefox and play the latest MrBeast video; say "done".\nNo extra instructions.'
        result, task, calls, arena = self.run_operate(goal, '--app', 'firefox', '--seconds', '120')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(task['prompt'], goal)
        self.assertEqual(task['app_id'], 'firefox')
        self.assertEqual(task['reset'], 'archie-session open about:blank')
        self.assertEqual(task['judge'], 'gui')
        self.assertEqual(task['tiers'], [120])
        self.assertEqual(calls, 'up\n')
        self.assertIn('/usr/share/archie/operator/apps.toml', arena)
        self.assertIn('cannot type arbitrary text', result.stderr)

    def test_url_gui_enables_clicks_without_rewriting_requested_url(self):
        result, task, _, _ = self.run_operate('Follow the visible link', '--url', 'https://example.org/')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(task['judge'], 'gui')
        self.assertEqual(task['reset'], "archie-session open 'https://example.org/'")

    def test_car_soccer_keeps_controller_judge(self):
        result, task, _, _ = self.run_operate('Drive toward the ball', '--app', 'car-soccer')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(task['judge'], 'none')

    def test_firefox_rejects_ambiguous_url_before_starting_session(self):
        result, task, calls, _ = self.run_operate('Open the browser', '--app', 'firefox', '--url', 'https://example.org/')
        self.assertEqual(result.returncode, 64)
        self.assertIsNone(task)
        self.assertEqual(calls, '')


if __name__ == '__main__':
    unittest.main()
