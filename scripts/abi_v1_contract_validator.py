#!/usr/bin/env python3
from __future__ import annotations
import hashlib,json,re,sys
from pathlib import Path
ROOT=Path(__file__).parents[1]
REVIEW=ROOT/'openspec/changes/build-extensible-plugin-platform/abi/v1-baseline-review.json'
API=ROOT/'crates/explorer-extension-api/src/lib.rs'
LIFETIME=ROOT/'crates/explorer-extension-host/src/dll_loader.rs'
def validate()->list[str]:
 issues=[]; data=json.loads(REVIEW.read_text(encoding='utf-8'))
 if data.get('required_root')!=['abi_contract','sdk_major','sdk_bundle_id','ui_abi_fingerprint','create_registrar']: issues.append('required root prefix drift')
 if {r.get('role') for r in data.get('reviews',[]) if r.get('decision')=='approved'}!={'contract-owner','architecture-reviewer'}: issues.append('independent reviews missing')
 for key in ('numeric_semantics','unknown_values','ownership','panic_policy'):
  if not data.get(key): issues.append(f'missing {key}')
 api=API.read_text(encoding='utf-8')
 for marker in ('ExtensionRootModuleV1','RegistrarFactoryV1','ExtensionRegistrarImplementationV1','dispose_caught_panic_payload_v1','registration_status_numeric_constants_are_frozen'):
  if marker not in api: issues.append(f'API marker missing: {marker}')
 if not re.search(r'pub const ABI_SCHEMA_V1:\s*AbiSchemaIdV1\s*=\s*AbiSchemaIdV1\(0x5345_0002\);',api): issues.append('ABI schema semantic ID is not frozen at revision 2')
 lifetime=LIFETIME.read_text(encoding='utf-8')
 if 'Keep the map process-resident' not in lifetime or 'resident_load_state' not in lifetime: issues.append('resident DLL lifetime model missing')
 return issues
def digest()->str: return hashlib.sha256(REVIEW.read_bytes()).hexdigest()
if __name__=='__main__':
 problems=validate()
 if problems: print('\n'.join(problems),file=sys.stderr);sys.exit(1)
 print(f'ABI_V1_REVIEW {digest()}')
