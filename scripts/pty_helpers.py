"""PTY shutdown helpers: keep draining while waiting so rendering cannot deadlock."""
import errno
import os
import platform
import re
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


_SGR = re.compile(rb"\x1b\[[0-9;:]*m")


def contains_styled_text(captured, text):
    """Match contiguous text across SGR styling only, not cursor movement.

    The accumulated capture may end halfway through an SGR sequence. Keeping
    incomplete/non-SGR control sequences prevents false matches; the next read
    supplies remaining bytes. This is a text-emission oracle, not an emulator.
    """
    return text in _SGR.sub(b"", bytes(captured))


def exact_file_chat(command, prompt, env, path, expected):
    """Drive a bracketed paste and native approval in the actual chat terminal."""
    import fcntl
    import pty
    import subprocess
    import termios

    def controlling():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)

    master, slave = pty.openpty()
    output = bytearray()
    def wait_for(marker):
        deadline = time.monotonic() + 15
        while marker not in output and time.monotonic() < deadline:
            if select.select([master], [], [], 0.1)[0]:
                output.extend(os.read(master, 65536))
        assert marker in output, bytes(output[-1800:])

    proc = subprocess.Popen(command, env=env, stdin=slave, stdout=slave, stderr=slave,
                            preexec_fn=controlling)
    try:
        wait_for(b'blue-dragon>')
        os.write(master, b'\x1b[200~' + prompt.encode() + b'\x1b[201~\r')
        wait_for(b'Approve this ONE action?')
        assert not path.exists()
        size = len(expected.encode())
        assert f'UTF-8 bytes: {size} | ends_with_newline: true'.encode() in output
        os.write(master, b'y')
        wait_for(f'Verified {path.name}: {size} bytes, ends_with_newline=true.'.encode())
        os.write(master, b'/exit\r')
        wait_exit(proc, master, output)
        return output.decode(errors='replace'), proc.returncode
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        os.close(master)
        os.close(slave)
