#!/usr/bin/env python3
"""Adversarial tests of the independent edit workflow oracle."""
import json
from pathlib import Path
import tempfile
import unittest
import benchmark_alpha_workflows as bench


class WorkflowOracleTests(unittest.TestCase):
    def fixture(self, root, content=bench.BASE):
        (root/'config.txt').write_bytes(content)
        (root/'unrelated.txt').write_bytes(bench.SIBLING)

    def test_success_prose_cannot_replace_actual_edit(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);self.fixture(root)
            bench.validate_step('{"ok":true,"content":"Edited successfully"}','blue','edit',bench.CHANGED,None)
            with self.assertRaisesRegex(AssertionError,'config.txt'):bench.assert_disk(root,bench.CHANGED)

    def test_correct_target_does_not_hide_sibling_damage(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);self.fixture(root,bench.CHANGED);(root/'unrelated.txt').write_bytes(b'changed')
            with self.assertRaisesRegex(AssertionError,'unrelated.txt'):bench.assert_disk(root,bench.CHANGED)

    def test_rejection_message_does_not_hide_partial_write(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);self.fixture(root,bench.CHANGED)
            bench.validate_step('{"ok":false,"error":"Expected exactly one match"}','blue','edit',bench.BASE,'absent')
            with self.assertRaises(AssertionError):bench.assert_disk(root,bench.BASE)

    def test_large_or_wrong_type_replacement_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);self.fixture(root,b'x'*1024*1024)
            with self.assertRaises(AssertionError):bench.assert_disk(root,bench.BASE)
            (root/'config.txt').unlink();(root/'config.txt').mkdir()
            with self.assertRaisesRegex(AssertionError,'type'):bench.assert_disk(root,bench.BASE)

    def test_denied_unknown_stale_cannot_count_as_success_or_match_rejection(self):
        for engine in ['blue','alpha']:
            for flag in [{'denied':True},{'execution_status':'unknown'},{'cached':True},{'cached_result':True},
                         {'reused_local_copy':True},{'fresh_evidence':False},{'original_evidence_id':'E1'},
                         {'reused_evidence_id':'E1'},{'status':'denied by user'},{'stale':True}]:
                with self.subTest(engine=engine,flag=flag):
                    with self.assertRaises(AssertionError):
                        bench.validate_step(json.dumps({'ok':True,**flag}),engine,'edit',bench.CHANGED,None)
                    for rejection in ['absent','ambiguous']:
                        with self.assertRaises(AssertionError):
                            bench.validate_step(json.dumps({'ok':False,'error':'Expected exactly one match',**flag}),engine,'edit',bench.BASE,rejection)

    def test_permission_error_and_success_are_not_expected_rejections(self):
        for engine in ['blue','alpha']:
            for value in [{'ok':False,'error':'Tool permission denied'},{'ok':True,'error':'Expected exactly one match'}, {'ok':False,'error':'Unrelated I/O failure'}]:
                with self.subTest(engine=engine,value=value):
                    with self.assertRaises(AssertionError):bench.validate_step(json.dumps(value),engine,'edit',bench.BASE,'absent')

    def test_stale_readback_is_rejected(self):
        for engine in ['blue','alpha']:
            value=json.dumps({'content':bench.BASE.decode()}) if engine=='blue' else bench.BASE.decode()
            with self.assertRaises(AssertionError):bench.validate_step(value,engine,'read',bench.CHANGED,None)

    def test_positive_and_native_error_controls(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);self.fixture(root,bench.CHANGED);bench.assert_disk(root,bench.CHANGED)
        bench.validate_step(json.dumps({'content':bench.CHANGED.decode()}),'blue','read',bench.CHANGED,None)
        bench.validate_step('    1\tport=8000\n    2\tmode=prod\n','alpha','read',bench.CHANGED,None)
        bench.validate_step('Error: old_string not found in config.txt','alpha','edit',bench.BASE,'absent')
        bench.validate_step('Error: old_string found 2 times in the file','alpha','edit',bench.BASE,'ambiguous')


if __name__=='__main__':unittest.main()
