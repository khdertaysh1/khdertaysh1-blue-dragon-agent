import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import local_toolkit_evidence as evidence
import local_developer_toolkit as toolkit


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.left, self.right = self.root / 'left', self.root / 'right'
        self.left.mkdir()
        self.right.mkdir()

    def put(self, root, path, text):
        target = root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def document(self, path, value):
        self.put(self.root, path, json.dumps(value))

    def declarations(self, names=None):
        self.document('decl.json', {'revision': 'rev1', 'tests': names or ['a', 'b', 'c']})

    def receipt(self, path, results, revision='rev1'):
        self.document(path, {'revision': revision, 'results': [{'name': n, 'status': s} for n, s in results]})

    def inspect(self, receipts=None):
        return evidence.receipt_review(self.root, 'decl.json', receipts or ['receipt.json'])

    def test_all_tree_statuses_and_hashes(self):
        for root in (self.left, self.right):
            self.put(root, 'same', 'same\n')
        self.put(self.left, 'changed', 'before\n')
        self.put(self.right, 'changed', 'after\n')
        self.put(self.left, 'removed', 'old\n')
        self.put(self.right, 'added', 'new\n')
        r = evidence.tree_compare(self.left, self.right, ['same', 'changed', 'removed', 'added'])
        self.assertTrue(r['ok'])
        self.assertEqual([f['status'] for f in r['files']], ['unchanged', 'modified', 'removed', 'added'])
        self.assertIn('-before', r['files'][1]['diff'])
        self.assertEqual(r['files'][0]['left']['sha256'], r['files'][0]['right']['sha256'])

    def test_absent_both_is_not_unchanged(self):
        r = evidence.tree_compare(self.left, self.right, ['missing'])
        self.assertFalse(r['coverage_complete'])
        self.assertEqual(r['files'][0]['status'], 'unreadable')

    def test_unreadable_not_added_or_removed(self):
        (self.left / 'a').write_bytes(b'\xff')
        self.put(self.right, 'a', 'valid')
        r = evidence.tree_compare(self.left, self.right, ['a'])
        self.assertEqual(r['files'][0]['status'], 'unreadable')
        self.assertFalse(r['ok'])

    def test_missing_roots_and_duplicate_selection_rejected(self):
        with self.assertRaises(OSError):
            evidence.tree_compare(self.root / 'absent', self.right, ['a'])
        with self.assertRaises(ValueError):
            evidence.tree_compare(self.left, self.right, ['a', 'a'])

    def test_tree_paths_links_and_limits(self):
        self.put(self.left, 'a', 'x')
        (self.right / 'a').symlink_to(self.left / 'a')
        self.assertFalse(evidence.tree_compare(self.left, self.right, ['a'])['ok'])
        self.assertFalse(evidence.tree_compare(self.left, self.right, ['../a'])['ok'])
        self.put(self.left, 'big', 'x' * 5000)
        self.put(self.right, 'big', 'y' * 5000)
        r = evidence.tree_compare(self.left, self.right, ['big'])
        self.assertEqual(r['files'][0]['status'], 'modified')
        self.assertFalse(r['files'][0]['diff_complete'])

    def test_empty_added_and_final_newline(self):
        self.put(self.right, 'empty', '')
        self.put(self.left, 'a', 'x')
        self.put(self.right, 'a', 'x\n')
        r = evidence.tree_compare(self.left, self.right, ['empty', 'a'])
        self.assertEqual(r['files'][0]['status'], 'added')
        self.assertFalse(r['files'][1]['left_final_newline'])
        self.assertTrue(r['files'][1]['right_final_newline'])

    def test_diff_output_limit(self):
        self.put(self.left, 'a', 'old\n')
        self.put(self.right, 'a', 'new\n')
        with patch.object(evidence, 'DIFF_CHARS', 8):
            r = evidence.tree_compare(self.left, self.right, ['a'])
        self.assertEqual(len(r['files'][0]['diff']), 8)
        self.assertFalse(r['ok'])

    def test_receipts_pass_fail_skip_separately(self):
        self.declarations()
        self.receipt('receipt.json', [('a', 'passed'), ('b', 'failed'), ('c', 'skipped')])
        r = self.inspect()
        self.assertEqual([t['state'] for t in r['tests']], ['passed', 'failed', 'skipped'])
        self.assertTrue(r['coverage_complete'])
        self.assertFalse(r['all_declared_reported_passed'])
        self.assertFalse(r['execution_verified'])

    def test_missing_and_undeclared_results_separate(self):
        self.declarations(['a', 'missing'])
        self.receipt('receipt.json', [('a', 'passed'), ('unlisted', 'passed')])
        r = self.inspect()
        self.assertEqual(r['tests'][1]['state'], 'missing_execution_evidence')
        self.assertEqual(r['undeclared_results'][0]['state'], 'absent_declaration')
        self.assertFalse(r['all_declared_reported_passed'])

    def test_stale_pass_never_promoted(self):
        self.declarations(['a'])
        self.receipt('receipt.json', [('a', 'passed')], revision='old')
        r = self.inspect()
        self.assertEqual(r['tests'][0]['state'], 'stale')
        self.assertFalse(r['all_declared_reported_passed'])

    def test_conflicting_or_duplicate_receipts_uncertain(self):
        self.declarations(['a'])
        self.receipt('one.json', [('a', 'passed')])
        self.receipt('two.json', [('a', 'failed')])
        self.assertEqual(self.inspect(['one.json','two.json'])['tests'][0]['state'], 'uncertain')

    def test_malformed_receipt_cannot_supply_partial_pass(self):
        self.declarations(['a'])
        self.receipt('receipt.json', [('a', 'passed'), ('bad', 'unknown')])
        r = self.inspect()
        self.assertEqual(r['receipts'][0]['state'], 'malformed')
        self.assertEqual(r['tests'][0]['state'], 'missing_execution_evidence')

    def test_malformed_other_receipt_prevents_pass_claim(self):
        self.declarations(['a'])
        self.receipt('one.json', [('a', 'passed')])
        self.put(self.root, 'broken.json', '{')
        self.assertEqual(self.inspect(['one.json','broken.json'])['tests'][0]['state'], 'uncertain')

    def test_duplicate_test_rows_and_keys_rejected(self):
        self.declarations(['a'])
        self.receipt('receipt.json', [('a', 'passed'), ('a', 'passed')])
        self.assertEqual(self.inspect()['receipts'][0]['state'], 'malformed')
        self.put(self.root, 'receipt.json', '{"revision":"rev1","revision":"old","results":[]}')
        self.assertEqual(self.inspect()['receipts'][0]['state'], 'malformed')

    def test_invalid_declarations_fail_entire_review(self):
        self.document('decl.json', {'revision':'rev1','tests':['a','a']})
        with self.assertRaises(ValueError):
            self.inspect()

    def test_absent_receipt_is_not_pass(self):
        self.declarations(['a'])
        r = self.inspect()
        self.assertFalse(r['ok'])
        self.assertEqual(r['tests'][0]['state'], 'missing_execution_evidence')

    def test_matching_pass_still_unverified(self):
        self.declarations(['a'])
        self.receipt('receipt.json', [('a','passed')])
        r = self.inspect()
        self.assertTrue(r['all_declared_reported_passed'])
        self.assertFalse(r['execution_verified'])

    def test_tree_cli_and_help(self):
        self.put(self.left, 'a', 'before')
        self.put(self.right, 'a', 'after')
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = toolkit.main(['trees', '--left', str(self.left), '--right', str(self.right), 'a'])
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(out.getvalue())['files'][0]['status'], 'modified')
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            toolkit.main(['--help'])
        self.assertIn('trees', out.getvalue())
        self.assertIn('receipts', out.getvalue())

    def test_byte_and_batch_limits(self):
        self.put(self.left, 'large', 'x' * (evidence.local.CAP + 1))
        self.put(self.right, 'large', 'small')
        self.assertEqual(evidence.tree_compare(self.left, self.right, ['large'])['files'][0]['status'], 'unreadable')
        with self.assertRaises(ValueError):
            evidence.tree_compare(self.left, self.right, [str(i) for i in range(33)])
        self.declarations(['a'])
        with self.assertRaises(ValueError):
            self.inspect([str(i) for i in range(33)])

    def test_current_and_stale_evidence_remain_separate(self):
        self.declarations(['a'])
        self.receipt('old.json', [('a', 'failed')], revision='old')
        self.receipt('new.json', [('a', 'passed')])
        r = self.inspect(['old.json', 'new.json'])
        self.assertEqual(r['tests'][0]['state'], 'passed')
        self.assertEqual(r['receipts'][0]['state'], 'stale')
        self.assertEqual(len(r['tests'][0]['evidence']), 2)
        self.assertFalse(r['execution_verified'])

    def test_cli_integration_and_no_execution(self):
        self.declarations(['a'])
        self.receipt('receipt.json', [('a','passed')])
        before = {p.name: p.read_bytes() for p in self.root.iterdir() if p.is_file()}
        out = io.StringIO()
        with patch('socket.socket', side_effect=AssertionError('network')), contextlib.redirect_stdout(out):
            code = toolkit.main(['receipts','--root',str(self.root),'--declarations','decl.json','receipt.json'])
        self.assertEqual(code,0)
        self.assertFalse(json.loads(out.getvalue())['execution_verified'])
        self.assertEqual(before,{p.name:p.read_bytes() for p in self.root.iterdir() if p.is_file()})


if __name__ == '__main__':
    unittest.main(verbosity=2)
