"""Portable, credential-free subprocess setup for synthetic schema fixtures."""
import json
import os
from pathlib import Path


def fixture_env(home, source=None):
    source = os.environ if source is None else source
    # Windows process/network initialization needs SystemRoot/WINDIR. Preserve
    # casing from the host; match case-insensitively, without copying credentials,
    # proxy variables or user provider/model overrides.
    allowed = {'PATH', 'SYSTEMROOT', 'WINDIR', 'TMP', 'TEMP', 'COMSPEC', 'PATHEXT'}
    env = {k: v for k, v in source.items() if k.upper() in allowed}
    env.update(HOME=str(home), USERPROFILE=str(home),
               BLUE_DRAGON_HOME=str(Path(home) / 'state'),
               BLUE_DRAGON_INPUT_BUDGET='8192', NO_COLOR='1', TERM='dumb')
    return env


def require_progress(run, packets, minimum_requests, sessions, tool_results=None, minimum_results=0):
    """Fail with bounded synthetic CLI diagnostics before indexing empty lists."""
    sessions = list(sessions)
    if (len(packets) >= minimum_requests and len(sessions) == 1
            and (tool_results is None or len(tool_results) >= minimum_results)):
        return
    raise AssertionError(json.dumps({
        'fixture_failure': 'missing_expected_cli_progress',
        'exit_code': run.returncode,
        'provider_requests_received': len(packets),
        'minimum_requests': minimum_requests,
        'session_files': len(sessions),
        'tool_results': None if tool_results is None else len(tool_results),
        'minimum_results': minimum_results,
        # These fixtures use only generated local data and a filtered environment.
        # No request bodies, environment values or private sessions are exported.
        'stdout_tail': run.stdout[-4000:],
        'stderr_tail': run.stderr[-4000:],
    }, ensure_ascii=True))
