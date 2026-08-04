#!/usr/bin/env python3
from __future__ import annotations
import argparse,hashlib,json,os,subprocess,sys,tomllib
from pathlib import Path
from typing import Any

def audit(lock:dict[str,Any],metadata:dict[str,Any],cargo_home:Path)->list[str]:
    issues=[]; packages={(p['name'],p['version']):p for p in metadata.get('packages',[]) if isinstance(p,dict)}
    cache=list((cargo_home/'registry'/'cache').glob('**/*.crate')) if (cargo_home/'registry'/'cache').exists() else []
    archives={p.name:p for p in cache}
    for item in lock.get('package',[]):
        source=item.get('source','')
        if not source.startswith('registry+'):
            continue
        key=(item.get('name'),item.get('version')); checksum=item.get('checksum')
        if not isinstance(checksum,str) or len(checksum)!=64: issues.append(f'{key[0]} {key[1]} missing lock checksum'); continue
        package=packages.get(key)
        if package is None: issues.append(f'{key[0]} {key[1]} missing metadata provenance'); continue
        if package.get('source')!=source: issues.append(f'{key[0]} {key[1]} registry provenance mismatch')
        if not package.get('license') and not package.get('license_file'): issues.append(f'{key[0]} {key[1]} missing license metadata')
        archive=archives.get(f'{key[0]}-{key[1]}.crate')
        if archive is None: issues.append(f'{key[0]} {key[1]} missing registry cache archive')
        elif hashlib.sha256(archive.read_bytes()).hexdigest()!=checksum: issues.append(f'{key[0]} {key[1]} cache archive checksum mismatch')
    return issues

def main()->int:
    parser=argparse.ArgumentParser();parser.add_argument('--root',type=Path,action='append',required=True);parser.add_argument('--cargo-home',type=Path);args=parser.parse_args()
    cargo_home=args.cargo_home or Path(os.environ.get('CARGO_HOME',Path.home()/'.cargo'))
    issues=[]
    for root in args.root:
        result=subprocess.run(['cargo','metadata','--locked','--offline','--format-version','1'],cwd=root,text=True,encoding='utf-8',errors='replace',capture_output=True)
        if result.returncode: issues.append(f'{root}: cargo metadata failed: {result.stderr.strip()}');continue
        metadata=json.loads(result.stdout);lock=tomllib.loads((root/'Cargo.lock').read_text(encoding='utf-8'))
        issues.extend(f'{root}: {issue}' for issue in audit(lock,metadata,cargo_home))
    for issue in issues: print(issue,file=sys.stderr)
    return 1 if issues else 0
if __name__=='__main__':raise SystemExit(main())
