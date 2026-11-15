#!/usr/bin/env python3
"""Negative controls for comparison evidence; never infer success from model prose."""
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch
import urllib.request
import benchmark_alpha_reads as bench


class EvidenceTests(unittest.TestCase):
    def test_success_exit_and_claim_without_tool_result_are_not_evidence(self):
        with patch.object(bench.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, 'all cases passed', '')):
            with self.assertRaisesRegex(AssertionError, 'No paired native tool result'):
                bench.run_case(Path('/unused'), 'blue', 'small')

    def drive(self, command, **kwargs):
        env = kwargs['env']
        self.assertNotIn('AWS_SECRET_ACCESS_KEY', env)
        self.assertEqual(kwargs['input'], '')
        if '--base-url' in command:
            url = command[command.index('--base-url')+1]; tool = 'read_file_window'; stream = False
        else:
            url = env['ALPHACODE_OPENAI_COMPAT_API_BASE']; tool = 'read'; stream = True
            self.assertEqual(env['ALPHACODE_RUN_AUTO_POKE'], '0')
        def post(body):
            request = urllib.request.Request(url+'/chat/completions', data=json.dumps(body).encode(), headers={'Content-Type':'application/json'})
            with urllib.request.urlopen(request) as response: return response.read().decode()
        first = post({'stream':stream, 'tools':[{'type':'function','function':{'name':tool}}], 'messages':[]})
        if stream:
            chunk = json.loads(first.splitlines()[0][6:]); call = chunk['choices'][0]['delta']['tool_calls'][0]
        else: call = json.loads(first)['choices'][0]['message']['tool_calls'][0]
        self.assertEqual(call['function']['name'], tool)
        post({'stream':stream, 'messages':[{'role':'tool', 'tool_call_id':self.call_id, 'content':self.result}]})
        return subprocess.CompletedProcess(command, 0, 'success claim', '')

    def test_wrong_tool_reference_cannot_pass(self):
        self.call_id='wrong'; self.result=json.dumps({'content':bench.FIXTURES['small']})
        with patch.object(bench.subprocess, 'run', side_effect=self.drive):
            with self.assertRaisesRegex(AssertionError, 'No paired native tool result'):
                bench.run_case(Path('/unused'), 'blue', 'small')

    def test_outside_range_content_is_rejected(self):
        self.call_id='fixture_read'; self.result=json.dumps({'content':bench.FIXTURES['range']})
        with patch.object(bench.subprocess, 'run', side_effect=self.drive):
            with self.assertRaises(AssertionError): bench.run_case(Path('/unused'), 'blue', 'range')

    def test_failed_denied_unknown_and_stale_content_never_passes(self):
        flags = [{'ok':False}, {'denied':True}, {'execution_status':'unknown'},
                 {'cached':True}, {'cached_result':True}, {'reused_local_copy':True},
                 {'fresh_evidence':False}, {'reused_evidence_id':'E1'}, {'original_evidence_id':'E1'},
                 {'stale':True}, {'status':'denied by user'}, {'error':'Tool failed'}, {'status':'failed'}]
        for engine in ['blue', 'alpha']:
            for case in ['small', 'utf8', 'range']:
                content = 'SELECTED_SENTINEL\n' if case=='range' else bench.FIXTURES[case]
                for flag in flags:
                    with self.subTest(engine=engine,case=case,flag=flag):
                        with self.assertRaises(AssertionError):
                            bench.validate_result(json.dumps({'content':content, **flag}),engine,case,content)

    def test_missing_requires_specific_native_error(self):
        for engine in ['blue', 'alpha']:
            for value in [{'ok':False,'error':'Tool permission denied'}, {'ok':False}, {'ok':True,'error':'File not found'},
                          {'ok':False,'content':'File not found','error':'Tool failed'},
                          {'ok':False,'error':'File not found','denied':True},
                          {'ok':False,'error':'File not found','cached':True},
                          {'ok':False,'error':'File not found','execution_status':'unknown'}]:
                with self.subTest(engine=engine,value=value):
                    with self.assertRaises(AssertionError):bench.validate_result(json.dumps(value),engine,'missing','')
        bench.validate_result(json.dumps({'ok':False,'error':'No such file or directory'}),'blue','missing','')
        bench.validate_result('Error: File not found: missing.txt','alpha','missing','')

    def test_native_error_prefix_cannot_pass_as_content(self):
        with self.assertRaises(AssertionError):
            bench.validate_result('Error: permission denied\n'+bench.FIXTURES['small'],'alpha','small',bench.FIXTURES['small'])

    def test_windows_missing_is_specific_and_not_content_or_denial(self):
        error = 'Cannot access missing.txt: The system cannot find the file specified. (os error 2)'
        bench.validate_result(json.dumps({'ok':False,'error':error}), 'blue', 'missing', '')
        for value in [
                {'ok':True,'error':error}, {'error':error},
                {'ok':False,'content':error,'error':'Tool failed'},
                {'ok':False,'error':'The system cannot find the path specified. (os error 3)'},
                {'ok':False,'error':'Access is denied. (os error 5)'},
                {'ok':False,'error':'The system cannot find the file specified. (os error 5)'},
                {'ok':False,'error':'Unrelated failure (os error 2)'},
                {'ok':False,'error':'Permission denied: '+error},
                *[{'ok':False,'error':error,**flag} for flag in [
                    {'denied':True},{'stale':True},{'execution_status':'unknown'},{'fresh_evidence':False}]]]:
            with self.subTest(value=value):
                with self.assertRaises(AssertionError): bench.validate_result(json.dumps(value),'blue','missing','')
        self.call_id='fixture_read'; self.result=json.dumps({'ok':False,'error':error})
        with patch.object(bench.subprocess, 'run', side_effect=self.drive):
            self.assertEqual(bench.run_case(Path('/unused'),'blue','missing')['status'],'passed')

    def test_failed_case_preserves_native_diagnostic_evidence(self):
        self.call_id='fixture_read'; self.result=json.dumps({'ok':False,'error':'Access is denied. (os error 5)'})
        with patch.object(bench.subprocess, 'run', side_effect=self.drive):
            with self.assertRaises(bench.CaseFailure) as failure:
                bench.run_case(Path('/unused'),'blue','missing')
        evidence=failure.exception.evidence
        self.assertEqual(evidence['native_result_excerpt'],self.result)
        self.assertEqual(evidence['native_result_sha256'],bench.hashlib.sha256(self.result.encode()).hexdigest())
        self.assertEqual(evidence['cli_exit_code'],0)
        self.assertEqual(evidence['provider_requests'],2)

    def test_denied_content_rejected_through_provider_transport(self):
        self.call_id='fixture_read'; self.result=json.dumps({'ok':False,'denied':True,'content':bench.FIXTURES['small']})
        with patch.object(bench.subprocess, 'run', side_effect=self.drive):
            with self.assertRaisesRegex(AssertionError,'Denied'):
                bench.run_case(Path('/unused'),'blue','small')

    def test_sse_adapter_fixture_and_unicode_control(self):
        self.call_id='fixture_read'; self.result=bench.FIXTURES['utf8']
        with patch.object(bench.subprocess, 'run', side_effect=self.drive):
            result=bench.run_case(Path('/unused'), 'alpha', 'utf8')
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(result['provider_requests'], 2)


if __name__ == '__main__': unittest.main()
