import unittest
from provider_traffic import summarize
class TrafficTests(unittest.TestCase):
    def test_started_and_finished_are_one_dispatch_and_pending_survives(self):
        start={'dispatch_id':'a','unix_ms':0,'attempts':1,'estimated_message_tokens':20,'estimated_tool_tokens':10,'max_output_tokens':40}
        done=dict(start,http_status=429)
        pending=dict(start,dispatch_id='b',unix_ms=59000)
        report=summarize([start,done,pending,{'unix_ms':60000,'attempts':0}])
        self.assertEqual(report['dispatches'],2)
        self.assertEqual(report['http_429'],1)
        self.assertEqual(report['peak_rolling_60s_estimated_input_plus_max_output'],140)
        self.assertIsNone(report['peak_rolling_60s_reported_tokens'])
        self.assertEqual(report['blocked_before_dispatch'],1)
    def test_actual_and_estimated_are_not_conflated(self):
        report=summarize([{'unix_ms':0,'attempts':1,'usage_reported':True,'usage':{'total_tokens':33},'estimated_message_tokens':100},{'unix_ms':60000,'attempts':1,'estimated_message_tokens':5}])
        self.assertEqual(report['peak_rolling_60s_requests'],1)
        self.assertEqual(report['peak_rolling_60s_reported_tokens'],33)
        self.assertEqual(report['usage_unreported_requests'],1)
if __name__=='__main__':unittest.main()
