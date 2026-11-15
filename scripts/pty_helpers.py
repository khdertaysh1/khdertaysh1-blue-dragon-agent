"""PTY shutdown helpers: keep draining while waiting so rendering cannot deadlock."""
import errno
import os
import platform
import select
import time


def wait_exit(process, master, captured, seconds=5):
    started = time.monotonic()
    def drain(wait):
        if select.select([master], [], [], wait)[0]:
            try:
                data = os.read(master, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                return False
            captured.extend(data)
            # Diagnostics need the tail, not an unbounded terminal transcript.
            if len(captured) > 1024 * 1024:
                del captured[:-1024 * 1024]
            return bool(data)
        return False
    while process.poll() is None and time.monotonic() - started < seconds:
        drain(.02)
    elapsed = time.monotonic() - started
    if process.poll() is None:
        raise AssertionError(f'PTY exit timeout after {elapsed:.3f}s; platform={platform.platform()} '
                             f'pid={process.pid}; tail={bytes(captured[-4096:])!r}')
    while drain(.01):
        pass
    return process.returncode, elapsed
