"""Synthetic reproduction of a PTY child blocked by undrained exit output."""
import os
import pty
import subprocess
import sys
from pty_helpers import wait_exit

master, slave = pty.openpty()
process = subprocess.Popen([sys.executable, '-c',
    'import os; data=b"x"*1048576; pos=0\nwhile pos<len(data): pos+=os.write(1,data[pos:])'],
    stdin=slave, stdout=slave, stderr=slave)
try:
    try:
        process.wait(timeout=.1)
        raise AssertionError('Fixture failed to reproduce PTY backpressure')
    except subprocess.TimeoutExpired:
        pass
    captured = bytearray()
    code, elapsed = wait_exit(process, master, captured)
    assert code == 0 and len(captured) == 1048576, (code, len(captured))
    print(f'PTY output backpressure reproduced; drain-while-wait completed in {elapsed:.3f}s')
finally:
    if process.poll() is None:
        process.kill()
        process.wait()
    os.close(master)
    os.close(slave)
