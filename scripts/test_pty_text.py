"""Regression: colored wordmarks must not fail PTY smoke title detection."""
import unittest
from pty_helpers import contains_styled_text

class StyledTextTests(unittest.TestCase):
    def test_split_color_wordmark(self):
        data = b'\x1b[38;2;89;229;255m BLUE \x1b[38;2;255;255;255mDRAGON\x1b[0m'
        self.assertNotIn(b'BLUE DRAGON', data)
        self.assertTrue(contains_styled_text(data, b'BLUE DRAGON'))

    def test_chunked_sgr(self):
        data = bytearray(b'BLUE \x1b[38;2;')
        self.assertFalse(contains_styled_text(data, b'BLUE DRAGON'))
        data.extend(b'255;255;255mDRAGON')
        self.assertTrue(contains_styled_text(data, b'BLUE DRAGON'))

    def test_title_still_required(self):
        for data in [b'', b'BLUE', b'DRAGON', b'BLUE OTHER DRAGON']:
            self.assertFalse(contains_styled_text(data, b'BLUE DRAGON'))

    def test_cursor_controls_not_removed(self):
        for separator in [b'\x1b[2J', b'\x1b[1;1H', b'\r\n', b'\x1b[K']:
            self.assertFalse(contains_styled_text(b'BLUE '+separator+b'DRAGON', b'BLUE DRAGON'))

    def test_plain_and_colon_sgr(self):
        self.assertTrue(contains_styled_text(b'BLUE DRAGON', b'BLUE DRAGON'))
        self.assertTrue(contains_styled_text(b'BLUE \x1b[38:2::255:255:255mDRAGON', b'BLUE DRAGON'))

if __name__ == '__main__':
    unittest.main()
