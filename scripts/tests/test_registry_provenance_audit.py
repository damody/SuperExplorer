import hashlib,tempfile,unittest
from pathlib import Path
from scripts.registry_provenance_audit import audit
class RegistryAuditTests(unittest.TestCase):
 def fixture(self):
  temporary=tempfile.TemporaryDirectory();home=Path(temporary.name);cache=home/'registry/cache/index';cache.mkdir(parents=True);data=b'crate';(cache/'demo-1.0.0.crate').write_bytes(data);digest=hashlib.sha256(data).hexdigest();lock={'package':[{'name':'demo','version':'1.0.0','source':'registry+https://example.invalid/index','checksum':digest}]};meta={'packages':[{'name':'demo','version':'1.0.0','source':'registry+https://example.invalid/index','license':'MIT','license_file':None}]};return temporary,home,lock,meta
 def test_accepts_bound_cache_provenance_license_and_checksum(self):
  t,h,l,m=self.fixture();self.addCleanup(t.cleanup);self.assertEqual(audit(l,m,h),[])
 def test_rejects_missing_license_and_checksum_drift(self):
  t,h,l,m=self.fixture();self.addCleanup(t.cleanup);m['packages'][0]['license']=None;(h/'registry/cache/index/demo-1.0.0.crate').write_bytes(b'tampered');messages='\n'.join(audit(l,m,h));self.assertIn('missing license',messages);self.assertIn('checksum mismatch',messages)
if __name__=='__main__':unittest.main()
