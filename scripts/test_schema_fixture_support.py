import json
import subprocess
import unittest
from schema_fixture_support import fixture_env, require_progress


class FixtureSupportTests(unittest.TestCase):
    def test_windows_dependencies_preserved_without_user_configuration(self):
        source = {'Path':'tools', 'SystemRoot':'C:\\Windows', 'windir':'C:\\Windows',
                  'TEMP':'temp', 'COMSPEC':'cmd', 'PATHEXT':'.EXE',
                  'HOME':'private', 'USERPROFILE':'private', 'BLUE_DRAGON_HOME':'private',
                  'BLUE_DRAGON_INPUT_BUDGET':'999', 'OPENAI_API_KEY':'secret',
                  'HTTP_PROXY':'private', 'ALL_PROXY':'private', 'MODEL':'private'}
        env = fixture_env('fixture', source)
        for key in ('Path', 'SystemRoot', 'windir', 'TEMP', 'COMSPEC', 'PATHEXT'):
            self.assertEqual(env[key], source[key])
        self.assertEqual(env['HOME'], 'fixture')
        self.assertEqual(env['USERPROFILE'], 'fixture')
        self.assertEqual(env['BLUE_DRAGON_INPUT_BUDGET'], '8192')
        self.assertFalse(any(v in ('secret','private','999') for v in env.values()))

    def test_empty_requests_preserve_failure_reason_not_index_error(self):
        run = subprocess.CompletedProcess([], 23, 'synthetic stdout', 'synthetic startup failure')
        with self.assertRaises(AssertionError) as caught:
            require_progress(run, [], 3, [])
        record = json.loads(str(caught.exception))
        self.assertEqual(record['exit_code'], 23)
        self.assertEqual(record['provider_requests_received'], 0)
        self.assertEqual(record['stderr_tail'], run.stderr)
        with self.assertRaises(AssertionError):
            require_progress(run, [{}, {}, {}], 3, ['session'], [], 3)
        require_progress(run, [{}, {}, {}], 3, ['session'], [{}, {}, {}], 3)


if __name__ == '__main__':
    unittest.main()
