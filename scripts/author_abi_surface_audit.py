#!/usr/bin/env python3
from __future__ import annotations
import re,sys
from pathlib import Path
ROOT=Path(__file__).parents[1]
AUTHOR_ROOTS=[ROOT/'sdk/fixtures/rust-folder-size-visual-column',ROOT/'sdk/fixtures/rust-folder-size-map-view',ROOT/'sdk/fixtures/rust-tokei-code-lines-column',ROOT/'sdk/fixtures/rust-lock-owner-column',ROOT/'sdk/fixtures/rust-exif-rename-command',ROOT/'sdk/fixtures/rust-7z-virtual-folder']
FORBIDDEN=[r'extern\s+"C"']
def audit()->list[str]:
 issues=[]
 for root in AUTHOR_ROOTS:
  if not root.exists(): continue
  for path in root.rglob('*.rs'):
   text=path.read_text(encoding='utf-8-sig')
   for pattern in FORBIDDEN:
    if re.search(pattern,text):issues.append(f'{path.relative_to(ROOT)} exposes forbidden author ABI surface: {pattern}')
 api_root=ROOT/'crates/explorer-extension-api/src'
 for path in api_root.rglob('*.rs'):
  for number,line in enumerate(path.read_text(encoding='utf-8-sig').splitlines(),start=1):
   if re.search(r'^\s*pub\s+(?:struct|enum|type|fn).*\b(?:String|Vec|Future|ExplorerState|Window|App)\b',line):issues.append(f'{path.relative_to(ROOT)}:{number} exposes forbidden public ABI type')
 api=(api_root/'lib.rs').read_text(encoding='utf-8-sig')
 if 'ExtensionRegistrarImplementationV1' not in api or 'ExtensionRootModuleV1' not in api:issues.append('SDK-owned Rust-first registrar/root surface is missing')
 return issues
if __name__=='__main__':
 issues=audit();print('\n'.join(issues),file=sys.stderr);raise SystemExit(1 if issues else 0)
