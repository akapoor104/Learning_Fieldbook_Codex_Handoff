import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"tools"))
from fieldbook import catalog,select,template

def run(*args):
    return subprocess.run([sys.executable,*args],cwd=ROOT,text=True,capture_output=True,check=False)

class CommandTests(unittest.TestCase):
    def test_suites_preview(self):
        result=run("tools/product_check.py","--repo",".","--suite","all","--show")
        self.assertEqual(result.returncode,0);self.assertIn("cargo test --workspace --locked",result.stdout)
        self.assertIn("npm run build",result.stdout);self.assertIn("config --quiet",result.stdout)
    def test_missing_production_is_not_a_pass(self):
        with tempfile.TemporaryDirectory() as td:
            result=run("tools/product_check.py","--repo",td,"--suite","rust")
            self.assertNotEqual(result.returncode,0);self.assertIn("MISSING",result.stderr)
    def test_evidence_digest(self):
        with tempfile.TemporaryDirectory() as td:
            path=Path(td)/"example.log";path.write_bytes(b"synthetic")
            result=run("tools/evidence_hash.py",str(path))
            self.assertEqual(result.stdout.strip(),hashlib.sha256(b"synthetic").hexdigest())
    def test_integrated_gate_and_mixed_commit(self):
        with tempfile.TemporaryDirectory() as td:
            path=Path(td)
            art=path/"test-only.log";art.write_text("Unit test data for the evidence checker.\n")
            digest=hashlib.sha256(art.read_bytes()).hexdigest()
            for key in catalog()["gates"]["notebook"]:
                ticket=select(catalog(),key)[0]
                rec=template(ticket);rec.update(revision="a"*40,implementer="builder",reviewer="reviewer")
                for c in rec["checks"].values():
                    c.update(state="passed",kind="command",exit_code=0,procedure="unit example",observed="synthetic",artifact=art.name,sha256=digest)
                (path/(key+".json")).write_text(json.dumps(rec))
            args=["tools/fieldbook.py","gate","notebook","--evidence",td]
            self.assertEqual(run(*args).returncode,0)
            rec["revision"]="b"*40;(path/(key+".json")).write_text(json.dumps(rec))
            bad=run(*args);self.assertNotEqual(bad.returncode,0);self.assertIn("different candidate",bad.stdout)
