import json,re,unittest
from pathlib import Path
ROOT=Path(__file__).parents[2]
SDK=ROOT/'sdk'
class LocalSnapshotReleasePolicyTests(unittest.TestCase):
 def test_snapshot_binds_authorized_full_revision_and_tree(self):
  item=json.loads((SDK/'snapshot/approved-gpui.json').read_text(encoding='utf-8'))
  self.assertEqual(item['source']['repository'],'https://github.com/damody/gpui-ce-explorer.git');self.assertRegex(item['source']['revision'],r'^[0-9a-f]{40}$');self.assertRegex(item['source']['tree'],r'^[0-9a-f]{40}$')
 def test_candidate_isolated_until_all_gates_and_promotion(self):
  source=(SDK/'scripts/update-gpui-snapshot.ps1').read_text(encoding='utf-8-sig')
  for token in ('state=\'candidate\'','Invoke-GpuiCandidateTransaction','invoke-gpui-update-gates.ps1','Restore-GpuiCheckoutState'):self.assertIn(token,source)
 def test_transaction_has_byte_rollback_and_remote_race_guard(self):
  source=(SDK/'scripts/update-gpui-snapshot.ps1').read_text(encoding='utf-8-sig');support=(SDK/'scripts/update-gpui-snapshot-support.psm1').read_text(encoding='utf-8-sig')
  self.assertIn('remote main advanced during update',source);self.assertIn('Restore-GpuiCheckoutState',source);self.assertIn('Assert-GpuiCandidatePromotionSurface',support)
 def test_non_fast_forward_requires_verifiable_approval(self):
  source=(SDK/'scripts/update-gpui-snapshot.ps1').read_text(encoding='utf-8-sig')
  self.assertIn('approval=$candidateApproval',source);self.assertRegex(source,r'fast.forward|fast-forward')
 def test_release_freeze_has_local_signed_inputs_and_offline_generation(self):
  source=(SDK/'scripts/freeze-release.ps1').read_text(encoding='utf-8-sig');schema=json.loads((SDK/'schemas/release-freeze.schema.json').read_text(encoding='utf-8'))
  self.assertEqual(schema['properties']['release_frozen']['const'],True)
  for token in ('verify-tag --raw','Invoke-GpgvVerifiedPrimaryV1','Assert-ProtectionRecord','--locked --offline','approvedSnapshot.release_frozen = $true'):self.assertIn(token,source)
 def test_frozen_revision_change_requires_new_rc_bundle(self):
  source=(SDK/'scripts/freeze-release.ps1').read_text(encoding='utf-8-sig')
  self.assertIn('Bundle ID is already immutable for a different frozen revision/tree',source)
if __name__=='__main__':unittest.main()
