import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from verify_live_provider_capacity import RELEVANT_SOURCE, check_read, verify


class LiveReadOracleTests(unittest.TestCase):
    def setUp(self):
        self.source = RELEVANT_SOURCE + b'// padding\n' * 40
        self.good = self.window(0, len(RELEVANT_SOURCE))

    def window(self, start, end):
        content = self.source[start:end]
        return {'ok': True, 'byte_offset': start, 'end_byte_offset': end,
                'content': content.decode(), 'window_sha256': hashlib.sha256(content).hexdigest()}

    def test_explicit_success_and_native_observation(self):
        self.assertTrue(check_read(self.good, self.source))
        native = dict(self.good)
        del native['ok']
        self.assertFalse(check_read(native, self.source))
        self.assertTrue(check_read(native, self.source, observation_ok=True))
        self.assertFalse(check_read(native, self.source, observation_ok=False))
        for state in ('completed', 'succeeded', 'success'):
            self.assertTrue(check_read(dict(native, execution_status=state), self.source))

    def test_non_success_status_overrides_matching_bytes_and_success_flags(self):
        for state in ('failed', 'denied', 'not_executed', 'cancelled', 'unknown',
                      'stale', 'pending', 'running', 'invented', '', None, [], True):
            with self.subTest(state=state):
                self.assertFalse(check_read(dict(self.good, execution_status=state),
                                            self.source, observation_ok=True))

    def test_empty_and_irrelevant_or_partial_windows(self):
        for start, end in ((0, 0), (10, 10), (0, 20), (1, len(RELEVANT_SOURCE)),
                           (len(RELEVANT_SOURCE), len(self.source))):
            with self.subTest(start=start, end=end):
                self.assertFalse(check_read(self.window(start, end), self.source))
        self.assertFalse(check_read(dict(self.good, byte_offset=False), self.source))
        self.assertFalse(check_read(dict(self.good, end_byte_offset=len(self.source)+1), self.source))

    def test_conflicting_or_forged_evidence(self):
        for changes in ({'ok': False}, {'ok': 1}, {'denied': True}, {'error': 'failure'},
                        {'cached_result': True}, {'fresh_evidence': False},
                        {'window_sha256': '0'*64}, {'content': 'other'}):
            self.assertFalse(check_read(dict(self.good, **changes), self.source, observation_ok=True))
        self.assertFalse(check_read(self.good, self.source, observation_ok=False))
        self.assertFalse(check_read(self.good, b'changed fixture'))
        full = {'ok': True, 'content': self.source.decode(), 'sha256': hashlib.sha256(self.source).hexdigest()}
        self.assertTrue(check_read(full, self.source))
        self.assertFalse(check_read(dict(full, execution_status='failed'), self.source))

    def test_saved_session_cannot_count_failed_or_empty_read_as_fresh(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'fixture').mkdir()
            (root/'fixture/app.js').write_bytes(self.source)
            (root/'state/sessions').mkdir(parents=True)
            trial = {'fixture_unchanged': True, 'rounds': [{'round': 0, 'native_observations_total': 1,
                      'finished': True, 'last_assistant_text': 'vulnerableRead protectedRead owner'}]}
            (root/'result.json').write_text(json.dumps(trial))
            session = {'messages': [{'role': 'user', 'content': 'NEVER MODIFY FILES'},
                       {'role': 'tool', 'content': json.dumps(self.good)}],
                       'observations': [{'ok': True, 'tool': 'read_file_window', 'message_index': 1, 'id': 'E1'}]}
            path = root/'state/sessions/session.json'
            path.write_text(json.dumps(session))
            self.assertTrue(verify(root)['rounds'][0]['fresh_source_recheck_verified'])
            for state in ('failed', 'denied', 'not_executed', 'cancelled'):
                modified = copy.deepcopy(session)
                modified['messages'][1]['content'] = json.dumps(dict(self.good, execution_status=state))
                path.write_text(json.dumps(modified))
                with self.assertRaises(AssertionError):
                    verify(root)
            modified = copy.deepcopy(session)
            modified['messages'][1]['content'] = json.dumps(self.window(0, 0))
            path.write_text(json.dumps(modified))
            with self.assertRaises(AssertionError):
                verify(root)
            modified = copy.deepcopy(session)
            modified['observations'][0]['ok'] = False
            path.write_text(json.dumps(modified))
            with self.assertRaises(AssertionError):
                verify(root)


if __name__ == '__main__':
    unittest.main()
