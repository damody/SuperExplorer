from __future__ import annotations
import copy,json,unittest
from pathlib import Path
from scripts.coordination_policy_validator import validate_adjustment,validate_handoff,validate_policy

ROOT=Path(__file__).parents[2]
POLICY=json.loads((ROOT/'openspec/changes/build-extensible-plugin-platform/coordination/coordination-policy.json').read_text(encoding='utf-8'))

class CoordinationPolicyTests(unittest.TestCase):
    def test_policy_and_complete_handoff_pass(self):
        self.assertEqual(validate_policy(POLICY),[])
        self.assertEqual(validate_handoff({field:[] for field in POLICY['handoff_required_fields']}),[])
    def test_two_owners_for_one_mutable_path_fail(self):
        policy=copy.deepcopy(POLICY); policy['roles'][1]['owned_paths'].append('crates/explorer-extension-host/**')
        self.assertIn('ownership overlap','\n'.join(validate_policy(policy)))
    def test_a_refinement_preserves_ids_and_lineage(self):
        good={'class':'A','l3_ids_before':['1.3.4'],'l3_ids_after':['1.3.4'],'evidence_lineage_preserved':True}
        self.assertEqual(validate_adjustment(good),[])
        bad={**good,'l3_ids_after':['1.3.40']}; self.assertIn('permanent L3','\n'.join(validate_adjustment(bad)))
    def test_b_correction_stales_pauses_and_revalidates(self):
        good={'class':'B','affected_work_paused':True,'stale_dependent_evidence_ids':['event-1'],'openspec_validation':{'command':'openspec validate build-extensible-plugin-platform --strict','exit_code':0}}
        self.assertEqual(validate_adjustment(good),[])
        self.assertIn('mark dependent evidence stale','\n'.join(validate_adjustment({**good,'stale_dependent_evidence_ids':[]})))
    def test_c_change_requires_user_approval(self):
        self.assertIn('explicit user approval','\n'.join(validate_adjustment({'class':'C','protected_change':'public-abi'})))
        self.assertEqual(validate_adjustment({'class':'C','protected_change':'permission','user_approval':{'approval_id':'user-42','approved':True}}),[])

if __name__=='__main__': unittest.main()
