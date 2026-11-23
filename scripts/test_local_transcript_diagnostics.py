import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import local_transcript_diagnostics as d
import local_developer_toolkit as cli


class DiagnosticsTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def run_doc(self, events=None, **kw):
        doc = dict(schema_version=1, sanitized=True, provenance='synthetic', complete=True, events=events or [])
        doc.update(kw)
        (self.root/'input.json').write_text(json.dumps(doc))
        return d.diagnose(self.root, ['input.json'])

    def recall(self, offset=0):
        return dict(type='recall_returned', message_index=2, offset=offset, count=10, reran_tool=False)

    def test_categories_and_duplicate_distinction(self):
        r = self.run_doc([self.recall(), self.recall(), {'type':'path_rejected','reason':'outside_workspace'},
                          {'type':'provider_stop','kind':'empty_response'}, {'type':'provider_stop','kind':'response_limit'},
                          {'type':'provider_stop','kind':'duplicate_tool_call_identity'}])
        self.assertTrue(r['ok'])
        self.assertEqual(list(r['inputs'][0]['counts'].values()), [2,1,1,1,1,1])
        self.assertFalse(r['causality_established'])

    def test_pages_not_duplicate(self):
        r=self.run_doc([self.recall(),self.recall(10)])
        self.assertEqual(r['inputs'][0]['counts']['duplicate_recall_observation'],0)

    def test_partial(self):
        self.assertFalse(self.run_doc([self.recall()],complete=False)['ok'])

    def test_empty(self):
        self.assertFalse(self.run_doc()['ok'])

    def test_unknown_provenance(self):
        self.assertFalse(self.run_doc([self.recall()],provenance='unknown')['ok'])

    def test_historical_not_current_verified(self):
        r=self.run_doc([self.recall()],provenance='historical')
        self.assertEqual(r['inputs'][0]['provenance'],'historical')
        self.assertFalse(r['execution_verified'])

    def test_stray_text_no_classification_or_secret_echo(self):
        for text in ['recall_returned empty_response SECRET', '{"token":"SECRET"}', '{']:
            (self.root/'SECRET.json').write_text(text)
            r=d.diagnose(self.root,['SECRET.json'])
            self.assertNotIn('SECRET',json.dumps(r))
            self.assertNotIn('counts',r['inputs'][0])

    def test_partial_malformed_rejects_whole_receipt(self):
        r=self.run_doc([self.recall(), {'type':'provider_stop','kind':'SECRET'}])
        self.assertNotIn('counts',r['inputs'][0])
        self.assertNotIn('SECRET',json.dumps(r))

    def test_unsanitized_and_extra_keys(self):
        self.assertFalse(self.run_doc([self.recall()],sanitized=False)['ok'])
        self.assertFalse(self.run_doc([self.recall()],excerpt='SECRET')['ok'])

    def test_duplicate_json_keys(self):
        (self.root/'input.json').write_text('{"events":[],"events":[]}')
        self.assertFalse(d.diagnose(self.root,['input.json'])['ok'])

    def test_bounds_and_bool_numbers(self):
        self.assertFalse(self.run_doc([self.recall()]*257)['ok'])
        event=self.recall();event['offset']=True
        self.assertFalse(self.run_doc([event])['ok'])
        (self.root/'large').write_text('x'*262145)
        self.assertFalse(d.diagnose(self.root,['large'])['ok'])

    def test_missing_and_symlink(self):
        self.assertFalse(d.diagnose(self.root,['missing'])['ok'])
        (self.root/'link').symlink_to(self.root/'missing')
        self.assertFalse(d.diagnose(self.root,['link'])['ok'])

    def test_cli_no_mutation_network_or_execution(self):
        self.run_doc([self.recall()])
        before=(self.root/'input.json').read_bytes()
        out=io.StringIO()
        with patch('subprocess.Popen',side_effect=AssertionError('execute')), patch('socket.socket',side_effect=AssertionError('network')), contextlib.redirect_stdout(out):
            code=cli.main(['diagnose','--root',str(self.root),'input.json'])
        self.assertEqual(code,0)
        self.assertEqual(before,(self.root/'input.json').read_bytes())
        self.assertFalse(json.loads(out.getvalue())['execution_verified'])

    def test_cli_root_error_redacted(self):
        out=io.StringIO()
        with contextlib.redirect_stdout(out):
            code=cli.main(['diagnose','--root',str(self.root/'SECRET'),'input'])
        self.assertEqual(code,2)
        self.assertNotIn('SECRET',out.getvalue())

if __name__ == '__main__':
    unittest.main(verbosity=2)
