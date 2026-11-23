import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import local_developer_toolkit as kit


class ToolkitTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def put(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def test_absolute_and_relative_module_candidates(self):
        self.put('pkg/__init__.py', '')
        self.put('pkg/a.py', 'import pkg.b\nfrom . import b\nfrom .b import VALUE\n')
        self.put('pkg/b.py', 'VALUE=1\n')
        r = kit.import_map(self.root, ['pkg/__init__.py', 'pkg/a.py', 'pkg/b.py'])
        self.assertTrue(r['ok'])
        b_edges = [e for e in r['imports'] if e['module'] == 'pkg.b']
        self.assertEqual(len(b_edges), 3)
        self.assertTrue(all(e['candidates'] == ['pkg/b.py'] and not e['runtime_resolution_verified'] for e in b_edges))

    def test_ambiguous_modules_never_choose_silently(self):
        self.put('pkg.py', '')
        self.put('pkg/__init__.py', '')
        self.put('a.py', 'import pkg\n')
        r = kit.import_map(self.root, ['a.py', 'pkg.py', 'pkg/__init__.py'])
        self.assertEqual(r['imports'][0]['status'], 'ambiguous')
        self.assertEqual(len(r['imports'][0]['candidates']), 2)

    def test_unresolved_and_out_of_package_diagnostics(self):
        self.put('a.py', 'import not_selected\nfrom .. import other\n')
        r = kit.import_map(self.root, ['a.py'])
        self.assertTrue(all(e['status'] == 'unresolved' for e in r['imports']))
        self.assertTrue(r['unresolved_is_not_missing_dependency'])
        self.assertTrue(any('beyond' in (e['note'] or '') for e in r['imports']))

    def test_syntax_error_does_not_hide_other_files(self):
        self.put('bad.py', 'def :')
        self.put('good.py', 'import missing\n')
        r = kit.import_map(self.root, ['bad.py', 'good.py'])
        self.assertFalse(r['ok'])
        self.assertEqual(r['coverage']['parsed'], 1)
        self.assertEqual(len(r['imports']), 1)

    def test_edge_limit_is_explicit(self):
        self.put('a.py', 'import a,b,c\n')
        with patch.object(kit, 'MAX_EDGES', 1):
            r = kit.import_map(self.root, ['a.py'])
        self.assertFalse(r['ok'])
        self.assertEqual(r['coverage']['edges_omitted'], 2)

    def test_import_root_and_file_budget_rejections(self):
        self.put('bad-name.py', '')
        self.assertFalse(kit.import_map(self.root, ['bad-name.py'])['ok'])
        with self.assertRaises(ValueError):
            kit.import_map(self.root, ['a.py'] * 2)

    def test_no_execution_or_import_following(self):
        self.put('a.py', 'raise Exception("never execute")\nimport nonexistent\n')
        with patch('socket.socket', side_effect=AssertionError('network')):
            self.assertTrue(kit.import_map(self.root, ['a.py'])['ok'])

    def test_json_compare_types_arrays_and_pointer_escaping(self):
        self.put('a.json', '{"a/b":true,"list":[1,2],"gone":null}')
        self.put('b.json', '{"a/b":1,"list":[1,3,4],"new":false}')
        r = kit.json_compare(self.root, 'a.json', 'b.json')
        self.assertFalse(r['equal'])
        changes = {c['pointer']: c for c in r['changes']}
        self.assertEqual(changes['/a~1b']['before_type'], 'bool')
        self.assertEqual(changes['/gone']['change'], 'removed')
        self.assertEqual(changes['/list/2']['change'], 'added')
        self.assertNotIn('before_value', changes['/list/1'])

    def test_json_formatting_only_is_equal(self):
        self.put('a.json', '{"a":1,"b":2}')
        self.put('b.json', '{ "b": 2, "a": 1 }')
        self.assertTrue(kit.json_compare(self.root, 'a.json', 'b.json')['equal'])

    def test_duplicate_keys_nonfinite_and_malformed_rejected(self):
        for text in ('{"a":1,"a":2}', 'NaN', 'Infinity', '1e999', '{'):
            with self.assertRaises(ValueError):
                kit.strict_json(text)

    def test_depth_node_and_diff_limits(self):
        with self.assertRaises(ValueError):
            kit.strict_json('[' * 66 + '0' + ']' * 66)
        with patch.object(kit, 'MAX_NODES', 2):
            with self.assertRaises(ValueError):
                kit.strict_json('[1,2]')
        self.put('a.json', '[1,2]')
        self.put('b.json', '[3,4]')
        with patch.object(kit, 'MAX_DIFFS', 1):
            r = kit.json_compare(self.root, 'a.json', 'b.json')
        self.assertFalse(r['coverage_complete'])
        self.assertEqual(r['changes_omitted'], 1)

    def test_source_paths_rejected(self):
        self.put('a.json', '{}')
        with self.assertRaises(ValueError):
            kit.json_compare(self.root, '../outside', 'a.json')

    def test_deterministic_selftest_walkthrough(self):
        with patch('socket.socket', side_effect=AssertionError('network')):
            first, second = kit.self_test(), kit.self_test()
        self.assertEqual(first, second)
        self.assertTrue(first['ok'])
        self.assertEqual(len(first['checks']), 6)

    def test_help_discloses_all_commands(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(kit.main(['--help']), 0)
        for command in ('python', 'review', 'imports', 'json-diff', 'self-test'):
            self.assertIn(command, out.getvalue())

    def test_existing_tools_are_delegated_without_rewrite(self):
        with patch.object(kit.navigation, 'main', return_value=7) as call:
            self.assertEqual(kit.main(['python', '--help']), 7)
            call.assert_called_once_with(['--help'])
        with patch.object(kit.review, 'main', return_value=8) as call:
            self.assertEqual(kit.main(['review', '--help']), 8)
            call.assert_called_once_with(['--help'])

    def test_cli_json_error_and_success(self):
        self.put('a.json', '{}')
        self.put('b.json', '{}')
        for names, expected in [(['a.json','b.json'],0),(['../x','b.json'],2)]:
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                status = kit.main(['json-diff', '--root', str(self.root), *names])
            self.assertEqual(status, expected)
            self.assertEqual(json.loads(out.getvalue())['schema_version'], 1)


if __name__ == '__main__':
    unittest.main(verbosity=2)
