"""Public VERSION is authoritative; Cargo retains a valid internal SemVer."""
from pathlib import Path
import re
import tomllib

def load_version(root=Path('.')):
    public=(root/'VERSION').read_text().rstrip('\r\n')
    if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.0',public):
        raise ValueError('VERSION must be canonical A.B.C.0; nonzero revision mapping requires an explicit policy change')
    package=tomllib.loads((root/'Cargo.toml').read_text())['package']['version']
    if package != public.rsplit('.',1)[0]:
        raise ValueError('VERSION and Cargo package version mapping disagree')
    return public,package
