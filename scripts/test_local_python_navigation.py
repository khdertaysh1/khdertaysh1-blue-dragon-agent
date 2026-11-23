"""Tiny synthetic fixtures only; no agent, imports of inspected code, or network."""
import contextlib
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import local_python_navigation as nav


class NavigationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def put(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8')
        return path

    def test_nested_symbols_import_aliases_and_calls(self):
        self.put('a.py', 'from .helpers import build as make\nclass Box:\n    async def run(self):\n        def inner():\n            return make()\n        return inner()\n')
        result = nav.navigate(self.root, ['a.py'])
        records = result['files'][0]['records']
        self.assertEqual([r['qualified_name'] for r in records if 'qualified_name' in r], ['Box', 'Box.run', 'Box.run.inner'])
        self.assertEqual(records[0]['bindings'][0]['alias'], 'make')
        self.assertEqual(records[0]['relative_level'], 1)
        calls = [r for r in records if r['kind'] == 'call_reference']
        self.assertEqual([r['name'] for r in calls], ['make', 'inner'])
        self.assertTrue(all(r['resolved'] is False for r in calls))

    def test_multifile_error_does_not_hide_valid_file(self):
        self.put('bad.py', 'def broken(:\n')
        self.put('good.py', 'def good():\n    pass\n')
        result = nav.navigate(self.root, ['bad.py', 'good.py'])
        self.assertFalse(result['ok'])
        self.assertEqual(result['coverage']['parsed_files'], 1)
        self.assertEqual(result['files'][0]['diagnostic']['code'], 'syntax_error')
        self.assertIn('sha256', result['files'][0])
        self.assertEqual(result['files'][1]['records'][0]['name'], 'good')

    def test_never_executes_top_level_or_follows_import(self):
        self.put('a.py', 'raise RuntimeError("must not run")\nimport missing_package\n')
        with patch('socket.socket', side_effect=AssertionError('network')):
            self.assertTrue(nav.navigate(self.root, ['a.py'])['ok'])

    def test_query_and_zero_matches_have_explicit_coverage(self):
        self.put('a.py', 'def alpha():\n    return 1\ndef beta():\n    return alpha()\n')
        result = nav.navigate(self.root, ['a.py'], 'ALPHA')
        self.assertEqual(result['coverage']['records_returned'], 2)
        empty = nav.navigate(self.root, ['a.py'], 'not_here')
        self.assertTrue(empty['coverage_complete'])
        self.assertEqual(empty['coverage']['records_returned'], 0)

    def test_unicode_locations_hash_and_encoding_cookie(self):
        path = self.put('a.py', 'def مرحبا():\n    return str("سلام")\n')
        result = nav.navigate(self.root, ['a.py'])
        self.assertEqual(result['files'][0]['sha256'], nav.hashlib.sha256(path.read_bytes()).hexdigest())
        self.assertEqual(result['files'][0]['records'][0]['name'], 'مرحبا')
        (self.root / 'latin.py').write_bytes(b'# coding: latin-1\nname = "caf\xe9"\n')
        self.assertTrue(nav.navigate(self.root, ['latin.py'])['ok'])

    def test_dotted_and_dynamic_calls(self):
        self.put('a.py', 'obj.method()\nfactory()()\n')
        names = [r['name'] for r in nav.navigate(self.root, ['a.py'])['files'][0]['records']]
        self.assertIn('obj.method', names)
        self.assertIn('<dynamic>', names)

    def test_path_errors_are_reported(self):
        for name in ('../escape.py', '/tmp/escape.py', '.git/a.py', 'a.js', 'missing.py'):
            result = nav.navigate(self.root, [name])
            self.assertFalse(result['coverage_complete'])
            self.assertEqual(result['files'][0]['status'], 'error')

    def test_duplicate_and_count_rejected_before_read(self):
        with patch.object(nav, 'read_source', side_effect=AssertionError('read started')):
            for paths in ([], ['a.py', 'a.py'], [f'{i}.py' for i in range(33)]):
                with self.assertRaises(ValueError):
                    nav.navigate(self.root, paths)

    def test_symlinks_and_hardlinks_rejected(self):
        self.put('a.py', 'pass\n')
        (self.root / 'linked.py').symlink_to(self.root / 'a.py')
        self.assertFalse(nav.navigate(self.root, ['linked.py'])['ok'])
        os.link(self.root / 'a.py', self.root / 'hard.py')
        self.assertFalse(nav.navigate(self.root, ['hard.py'])['ok'])

    def test_file_total_and_record_limits(self):
        self.put('a.py', 'x=1\n')
        self.put('b.py', 'y=2\n')
        with patch.object(nav, 'FILE_BYTES', 3):
            self.assertFalse(nav.navigate(self.root, ['a.py'])['ok'])
        with patch.object(nav, 'TOTAL_BYTES', 4):
            result = nav.navigate(self.root, ['a.py', 'b.py'])
            self.assertEqual(result['coverage']['parsed_files'], 1)
            self.assertFalse(result['ok'])
        self.put('calls.py', 'one()\ntwo()\n')
        with patch.object(nav, 'MAX_RECORDS', 1):
            result = nav.navigate(self.root, ['calls.py'])
            self.assertFalse(result['ok'])
            self.assertEqual(result['files'][0]['matching_records'], 2)
            self.assertEqual(result['coverage']['records_returned'], 1)

    def test_explicit_files_only_and_no_writes(self):
        self.put('a.py', 'pass\n')
        self.put('not_selected.py', 'this is broken !!!')
        before = {p.name: p.read_bytes() for p in self.root.iterdir()}
        self.assertTrue(nav.navigate(self.root, ['a.py'])['ok'])
        self.assertEqual(before, {p.name: p.read_bytes() for p in self.root.iterdir()})

    def test_readable_diagnostics_escape_controls(self):
        self.put('bad.py', 'def :\n')
        text = nav.readable(nav.navigate(self.root, ['bad.py', 'bad\x1b.py']))
        self.assertIn('INCOMPLETE', text)
        self.assertIn('syntax_error', text)
        self.assertNotIn('\x1b', text)

    def test_cli_exit_status_and_json(self):
        self.put('a.py', 'pass\n')
        for names, expected in [(['a.py'], 0), (['bad.py'], 1), (['a.py', 'a.py'], 2)]:
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                status = nav.main(['--root', str(self.root), *names])
            self.assertEqual(status, expected)
            self.assertEqual(json.loads(output.getvalue())['schema_version'], 1)


if __name__ == '__main__':
    unittest.main(verbosity=2)
