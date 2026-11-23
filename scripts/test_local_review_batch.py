import contextlib
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import local_review_batch as review


class LocalBatchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def put(self, path, text):
        p = self.root / path
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(text.encode())
        return p

    def diff(self, body, file='a.txt'):
        return f'--- a/{file}\n+++ b/{file}\n' + body

    def check(self, text, files=None):
        self.put('change.diff', text)
        return review.patch_review(self.root, 'change.diff', files or ['a.txt'])

    def test_patch_success_has_hashes_without_writing(self):
        self.put('a.txt', 'one\ntwo\n')
        r = self.check(self.diff('@@ -1,2 +1,2 @@\n one\n-two\n+new\n'))
        self.assertTrue(r['ok'])
        self.assertFalse(r['applied'])
        self.assertEqual(r['files'][0]['proposed_sha256'], review.sha(b'one\nnew\n'))
        self.assertEqual((self.root / 'a.txt').read_text(), 'one\ntwo\n')

    def test_multifile_diagnostics(self):
        self.put('a.txt', 'a\n')
        self.put('b.txt', 'not b\n')
        text = self.diff('@@ -1 +1 @@\n-a\n+x\n') + self.diff('@@ -1 +1 @@\n-b\n+y\n', 'b.txt')
        r = self.check(text, ['a.txt', 'b.txt'])
        self.assertFalse(r['ok'])
        self.assertTrue(r['files'][0]['applicable'])
        self.assertIn('mismatch', r['files'][1]['diagnostic'])

    def test_insert_at_beginning_and_delete(self):
        self.put('a.txt', 'a\nb\n')
        r = self.check(self.diff('@@ -0,0 +1 @@\n+start\n@@ -2 +2,0 @@\n-b\n'))
        self.assertTrue(r['ok'])
        self.assertEqual(r['files'][0]['proposed_sha256'], review.sha(b'start\na\n'))

    def test_empty_source_insertion(self):
        self.put('a.txt', '')
        self.assertTrue(self.check(self.diff('@@ -0,0 +1 @@\n+new\n'))['ok'])

    def test_unicode_line_separator_is_not_newline(self):
        self.put('a.txt', 'مرحبا\u2028عالم\n')
        self.assertTrue(self.check(self.diff('@@ -1 +1 @@\n-مرحبا\u2028عالم\n+سلام\n'))['ok'])

    def test_malformed_counts_and_empty_patch_rejected(self):
        for text in ('', self.diff('@@ -2,2 +2 @@\n-a\n+b\n'), self.diff('@@ -0 +1 @@\n-a\n+b\n')):
            with self.assertRaises(ValueError):
                review.parse_patch(text)

    def test_extended_headers_renames_binary_and_no_newline_rejected(self):
        for text in ('diff --git a/a b/a\n', 'GIT binary patch\n',
                     '--- a/a\n+++ b/b\n@@ -1 +1 @@\n-a\n+b\n',
                     self.diff('@@ -1 +1 @@\n-a\n+b\n\\ No newline at end of file\n')):
            with self.assertRaises(ValueError):
                review.parse_patch(text)

    def test_selection_must_cover_exact_patch(self):
        self.put('a.txt', 'a\n')
        with self.assertRaises(ValueError):
            self.check(self.diff('@@ -1 +1 @@\n-a\n+b\n'), ['other.txt'])

    def test_duplicate_paths_rejected(self):
        text = self.diff('@@ -1 +1 @@\n-a\n+b\n')
        with self.assertRaises(ValueError):
            review.parse_patch(text + text)

    def test_wrong_new_position_and_overlap_rejected(self):
        self.put('a.txt', 'a\n')
        self.assertFalse(self.check(self.diff('@@ -1 +2 @@\n-a\n+b\n'))['ok'])
        self.assertFalse(self.check(self.diff('@@ -1 +1 @@\n-a\n+b\n@@ -1 +1 @@\n-a\n+c\n'))['ok'])

    def test_source_newline_policy(self):
        for content in ('a', 'a\r\n'):
            self.put('a.txt', content)
            self.assertFalse(self.check(self.diff('@@ -1 +1 @@\n-a\n+b\n'))['ok'])

    def test_atx_headings_hierarchy_query_and_fences(self):
        self.put('guide.md', '# Guide\n## Install ###\n```python\n# not heading\n```\n### Linux\n~~~\n# ignored\n~~~\n')
        r = review.doc_index(self.root, ['guide.md'])
        self.assertEqual([h['title'] for h in r['files'][0]['headings']], ['Guide', 'Install', 'Linux'])
        self.assertEqual(r['files'][0]['headings'][2]['parents'], ['Guide', 'Install'])
        filtered = review.doc_index(self.root, ['guide.md'], 'LINUX')
        self.assertEqual(len(filtered['files'][0]['headings']), 1)

    def test_docs_multifile_error_and_unclosed_fence(self):
        self.put('a.md', '# A\n```\n# hidden\n')
        self.put('b.md', '# B\n')
        r = review.doc_index(self.root, ['a.md', 'b.md', 'missing.md'])
        self.assertFalse(r['ok'])
        self.assertEqual(r['files'][1]['headings'][0]['title'], 'B')
        self.assertIn('Unclosed', r['files'][0]['diagnostic'])

    def test_docs_unsupported_forms_not_claimed_as_commonmark(self):
        self.put('a.md', 'Setext\n======\n    # indented code\n#valid?\n# Good\n')
        r = review.doc_index(self.root, ['a.md'])
        self.assertEqual([h['title'] for h in r['files'][0]['headings']], ['Good'])
        self.assertIn('not full CommonMark', r['grammar'])

    def test_docs_limits_and_unicode(self):
        self.put('a.md', '# عربي\n' * 501)
        r = review.doc_index(self.root, ['a.md'])
        self.assertFalse(r['ok'])
        self.assertEqual(len(r['files'][0]['headings']), 500)

    def test_paths_links_and_bytes_fail_closed(self):
        self.put('a.md', '# A\n')
        (self.root / 'link.md').symlink_to(self.root / 'a.md')
        for path in ('../a.md', '.git/a.md', 'link.md'):
            self.assertFalse(review.doc_index(self.root, [path])['ok'])
        with patch.object(review, 'CAP', 2):
            self.assertFalse(review.doc_index(self.root, ['a.md'])['ok'])
        os.link(self.root / 'a.md', self.root / 'hard.md')
        self.assertFalse(review.doc_index(self.root, ['hard.md'])['ok'])

    def test_no_network_or_writes(self):
        self.put('a.md', '# Title\n')
        before = (self.root / 'a.md').read_bytes()
        with patch('socket.socket', side_effect=AssertionError('network')):
            self.assertTrue(review.doc_index(self.root, ['a.md'])['ok'])
        self.assertEqual((self.root / 'a.md').read_bytes(), before)

    def test_cli_statuses(self):
        self.put('a.md', '# Title\n')
        for paths, expected in [(['a.md'], 0), (['missing.md'], 1), (['a.md', 'a.md'], 2)]:
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                code = review.main(['--root', str(self.root), 'docs', *paths])
            self.assertEqual(code, expected)
            self.assertEqual(json.loads(out.getvalue())['schema_version'], 1)


if __name__ == '__main__':
    unittest.main(verbosity=2)
