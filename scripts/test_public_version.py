"""Validate public/Cargo mapping, emitted tag and optionally the actual CLI version."""
import argparse,os,subprocess,tempfile
from pathlib import Path
from release_version import load_version

p=argparse.ArgumentParser();p.add_argument('--binary',type=Path);p.add_argument('--check-build-script',action='store_true');a=p.parse_args()
repo=Path(__file__).resolve().parent.parent
public,package=load_version(repo)
assert public=='1.1.9.0' and package=='1.1.9'
if a.binary:
    assert subprocess.check_output([str(a.binary.resolve()),'--version'],text=True).strip()==f'blue-dragon {public}'
with tempfile.TemporaryDirectory(prefix='blue-version-') as folder:
    root=Path(folder)
    def write(version,pkg):
        (root/'VERSION').write_text(version+'\n')
        (root/'Cargo.toml').write_text('[package]\nname="fixture"\nversion="'+pkg+'"\n')
    checker=root/('version-check.exe' if os.name=='nt' else 'version-check')
    if a.check_build_script:
        subprocess.run(['rustc',str(repo/'build.rs'),'-o',str(checker)],check=True,capture_output=True)
    def check_build(pkg,success):
        if a.check_build_script:
            result=subprocess.run([str(checker)],cwd=root,env=dict(os.environ,CARGO_PKG_VERSION=pkg),capture_output=True,text=True)
            assert (result.returncode==0)==success,(pkg,result.stdout,result.stderr)
    for v,pkg in [('1.1.9.0','1.1.9'),('1.2.1.0','1.2.1')]:
        write(v,pkg);assert load_version(root)==(v,pkg);check_build(pkg,True)
    for v,pkg in [('1.1.9.0','1.1.8'),('1.1.9','1.1.9'),('1.1.9.0-alpha','1.1.9'),('01.1.9.0','1.1.9'),('1.1.9.1','1.1.9'),('1.1.9.0\nINJECT','1.1.9')]:
        write(v,pkg)
        check_build(pkg,False)
        try:load_version(root)
        except ValueError:pass
        else:raise AssertionError((v,pkg))
print('Public VERSION/Cargo mapping verified; next requested public version maps to Cargo 1.2.1; invalid mappings rejected')
