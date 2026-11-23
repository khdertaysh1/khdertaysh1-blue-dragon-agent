"""Synthetic-only tests for the independent manual reviewer; never run the agent."""
import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import local_workspace as review


class ManualReviewTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def write(self, name, text):
        target = self.root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding='utf-8')
        return target

    def cli(self, *args):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            code = review.main(['--root', str(self.root), *args])
        return code, json.loads(output.getvalue())

    def test_utf8_windows_reconstruct_large_file(self):
        text = 'سطر 🙂 abc\n' * 20000
        self.write('large.txt', text)
        offset, result = 0, bytearray()
        while True:
            part = review.window(self.root, 'large.txt', offset, 65536)
            data = part['text'].encode()
            self.assertEqual(review.digest(data), part['window_sha256'])
            result.extend(data)
            offset = part['next_offset']
            if part['complete']:
                break
        self.assertEqual(result.decode(), text)

    def test_reject_offset_inside_codepoint(self):
        self.write('a', 'مرحبا')
        with self.assertRaises(ValueError):
            review.window(self.root, 'a', 1, 4)

    def test_read_bounds_and_eof(self):
        self.write('a', 'abc')
        for offset, count in [(-1, 4), (0, 1), (4, 4), (0, 65537)]:
            with self.assertRaises(ValueError):
                review.window(self.root, 'a', offset, count)
        self.assertTrue(review.window(self.root, 'a', 3, 4)['complete'])

    def test_binary_and_invalid_utf8_rejected(self):
        for content in (b'a\0b', b'\xff'):
            (self.root / 'a').write_bytes(content)
            with self.assertRaises(ValueError):
                review.window(self.root, 'a', 0, 4)

    def test_path_traversal_and_git_metadata_rejected(self):
        for raw in ('../a', '/etc/passwd', 'a/../b', '.git/config', 'a\\b', 'C:a', 'a\n'):
            with self.assertRaises(ValueError):
                review.safe_path(self.root, raw, missing=True)

    @unittest.skipUnless(hasattr(os, 'symlink'), 'symlinks unavailable')
    def test_symlink_rejected(self):
        self.write('a', 'hello')
        (self.root / 'link').symlink_to(self.root / 'a')
        with self.assertRaises(ValueError):
            review.window(self.root, 'link', 0, 4)

    @unittest.skipUnless(hasattr(os, 'link'), 'hardlinks unavailable')
    def test_hardlink_rejected(self):
        self.write('a', 'hello')
        os.link(self.root / 'a', self.root / 'b')
        with self.assertRaises(ValueError):
            review.read_bytes(self.root, 'a')

    @unittest.skipUnless(hasattr(os, 'mkfifo'), 'FIFO unavailable')
    def test_fifo_rejected_without_open(self):
        os.mkfifo(self.root / 'pipe')
        with self.assertRaises(ValueError):
            review.window(self.root, 'pipe', 0, 4)

    def test_inventory_order_and_exclusions(self):
        self.write('b', 'b')
        self.write('a', 'a')
        self.write('.git/config', 'not code')
        result = review.inventory(self.root)
        self.assertEqual([x['path'] for x in result['files']], ['a', 'b'])
        self.assertTrue(result['skipped'])

    def test_inventory_limit(self):
        for i in range(201):
            self.write(str(i), '')
        self.assertFalse(review.inventory(self.root)['complete'])

    def test_search_literal_vs_regex(self):
        self.write('code.txt', 'a.b\naxb\nمرحبا\n')
        self.assertEqual(len(review.search(self.root, 'a.b')['matches']), 1)
        self.assertEqual(len(review.search(self.root, 'a.b', True)['matches']), 2)
        self.assertEqual(review.search(self.root, 'مرحبا')['matches'][0]['line'], 3)

    def test_invalid_regex(self):
        self.write('a', 'abc')
        with self.assertRaises(ValueError):
            review.search(self.root, '[', True)

    def test_regex_timeout(self):
        self.write('a', 'a' * 10000 + '!')
        with self.assertRaisesRegex(ValueError, 'timed out'):
            review.search(self.root, '(a+)+$', True)

    def test_search_reports_incomplete_files_and_matches(self):
        self.write('large', 'x' * (review.FILE_CAP + 1))
        self.write('small', 'hit\n' * 201)
        result = review.search(self.root, 'hit')
        self.assertFalse(result['complete'])
        self.assertEqual(len(result['matches']), 200)
        self.assertTrue(result['skipped'])

    def manifest(self, changes):
        self.write('proposal.json', json.dumps({'changes': changes}))
        return 'proposal.json'

    def test_multifile_preview_create_update_delete_no_writes(self):
        self.write('update', 'old\n')
        self.write('delete', 'bye\n')
        changes = [dict(path='update', expected_sha256=review.digest(b'old\n'), content='new\n'),
                   dict(path='delete', expected_sha256=review.digest(b'bye\n'), content=None),
                   dict(path='create', expected_sha256=None, content='hello\n')]
        manifest = self.manifest(changes)
        before = {p.name: p.read_bytes() for p in self.root.iterdir()}
        result = review.preview(self.root, manifest)
        self.assertFalse(result['applied'])
        self.assertEqual([x['operation'] for x in result['changes']], ['update', 'delete', 'create'])
        self.assertEqual(before, {p.name: p.read_bytes() for p in self.root.iterdir()})

    def test_preview_conflict_and_duplicate(self):
        self.write('a', 'before')
        change = dict(path='a', expected_sha256='wrong', content='after')
        with self.assertRaisesRegex(ValueError, 'Conflict'):
            review.preview(self.root, self.manifest([change]))
        change['expected_sha256'] = review.digest(b'before')
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            review.preview(self.root, self.manifest([change, change]))
        self.assertEqual((self.root / 'a').read_text(), 'before')

    def test_preview_rejects_unknown_fields_and_traversal(self):
        for change in [dict(path='../a', expected_sha256=None, content='x'),
                       dict(path='a', expected_sha256=None, content='x', command='ignored?')]:
            with self.assertRaises(ValueError):
                review.preview(self.root, self.manifest([change]))

    def test_sessions_separate_sources_roles_and_skip_corrupt(self):
        snapshot = {'id': 'synthetic', 'messages': [{'role': r, 'content': 'needle ' + r}
                                                  for r in ('user', 'assistant', 'tool')]}
        self.write('chosen/a.json', json.dumps(snapshot))
        self.write('other/a.json', json.dumps(snapshot))
        self.write('chosen/broken.json', '{')
        result = review.sessions(self.root, 'chosen', 'needle', 'tool')
        self.assertEqual(len(result['matches']), 1)
        self.assertEqual(result['matches'][0]['source'], 'chosen/a.json')
        self.assertEqual(result['matches'][0]['role'], 'tool')
        self.assertFalse(result['complete'])

    def test_junit_counts_and_no_execution_claim(self):
        self.write('results.xml', '<testsuite><testcase/><testcase><failure/></testcase>'
                   '<testcase><error/></testcase><testcase><skipped/></testcase></testsuite>')
        result = review.test_report(self.root, 'results.xml')
        self.assertEqual([result[k] for k in ('tests', 'passed', 'failed', 'errors', 'skipped')], [4, 1, 1, 1, 1])
        self.assertFalse(result['executed_by_this_command'])
        self.assertEqual(result['freshness'], 'unverified')

    def test_xml_entities_and_wrong_format_rejected(self):
        for text in ('<!DOCTYPE a [<!ENTITY b "x">]><testsuite/>', '<notjunit/>'):
            self.write('r.xml', text)
            with self.assertRaises(ValueError):
                review.test_report(self.root, 'r.xml')

    def test_utf16_xml_is_rejected(self):
        (self.root / 'r.xml').write_bytes('<testsuite/>'.encode('utf-16'))
        with self.assertRaises(ValueError):
            review.test_report(self.root, 'r.xml')

    def test_preview_approval_digest_changes_with_content(self):
        change = dict(path='new', expected_sha256=None, content='first')
        first = review.preview(self.root, self.manifest([change]))['approval_digest']
        change['content'] = 'second'
        second = review.preview(self.root, self.manifest([change]))['approval_digest']
        self.assertNotEqual(first, second)

    def git(self, *args):
        return subprocess.run(['git', '-c', 'core.hooksPath=' + os.devnull, '-C', str(self.root), *args],
                              check=True, capture_output=True)

    def init_git(self):
        self.git('init')
        self.git('config', 'user.name', 'Synthetic Test')
        self.git('config', 'user.email', 'synthetic@example.invalid')
        self.write('a', 'original\n')
        self.git('add', 'a')
        self.git('commit', '-m', 'synthetic fixture')

    def test_git_staged_unstaged_and_path(self):
        self.init_git()
        self.write('a', 'staged\n')
        self.git('add', 'a')
        self.write('a', 'working\n')
        staged = review.git_review(self.root, True, 'a')['diff']['text']
        working = review.git_review(self.root, False, 'a')['diff']['text']
        self.assertIn('+staged', staged)
        self.assertNotIn('+working', staged)
        self.assertIn('+working', working)

    def test_git_filter_config_refused_before_process(self):
        self.init_git()
        self.git('config', 'filter.test.clean', 'must-not-execute')
        with patch.object(review.subprocess, 'Popen', side_effect=AssertionError('spawned')):
            with self.assertRaisesRegex(ValueError, 'allowlist'):
                review.git_review(self.root)

    def test_git_include_config_refused(self):
        self.init_git()
        self.git('config', 'include.path', '/nonexistent')
        with self.assertRaisesRegex(ValueError, 'allowlist'):
            review.git_review(self.root)

    def test_cli_json_contract_and_error(self):
        self.write('a', '\x1b[2Jمرحبا')
        code, result = self.cli('read', 'a')
        self.assertEqual(code, 0)
        self.assertEqual(result['schema_version'], 1)
        self.assertIn('مرحبا', result['result']['text'])
        code, result = self.cli('read', '../a')
        self.assertEqual(code, 1)
        self.assertFalse(result['ok'])

    def test_manual_operations_do_not_connect_to_provider(self):
        self.write('a', 'synthetic')
        with patch('socket.socket', side_effect=AssertionError('network used')):
            self.assertEqual(self.cli('read', 'a')[0], 0)
            self.assertEqual(self.cli('files')[0], 0)

    def test_no_agent_or_network_imports(self):
        import ast
        tree = ast.parse(Path(review.__file__).read_text())
        names = {node.names[0].name for node in ast.walk(tree) if isinstance(node, ast.Import)}
        self.assertFalse(names & {'requests', 'urllib', 'socket', 'agent'})


if __name__ == '__main__':
    unittest.main(verbosity=2)
