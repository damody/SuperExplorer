import importlib.util, unittest
from pathlib import Path
P=Path(__file__).parents[1]/'abi_v1_contract_validator.py';S=importlib.util.spec_from_file_location('abi_v1',P);M=importlib.util.module_from_spec(S);S.loader.exec_module(M)
class AbiV1ContractTests(unittest.TestCase):
 def test_current_baseline_is_approved_and_frozen(self): self.assertEqual(M.validate(),[])
 def test_review_digest_is_stable_sha256(self): self.assertEqual(len(M.digest()),64);int(M.digest(),16)
if __name__=='__main__': unittest.main()
