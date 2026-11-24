import contextlib
import hashlib
import io
import json
import tempfile
from pathlib import Path
import subprocess
import sys
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"tools"))
from fieldbook import catalog,inspect_receipt,inspect_profiles,select,template,validate_kit
from run_cases import compare
from probe_health import probe,verify

def cli(*args):
    return subprocess.run([sys.executable,*args],cwd=ROOT,capture_output=True,text=True,check=False)
class PacketTests(unittest.TestCase):
    def setUp(self):self.data=catalog()
    def test_structure(self):
        with contextlib.redirect_stdout(io.StringIO()):validate_kit(self.data,False)
    def test_only_two_starters(self):
        r=cli("tools/fieldbook.py","ready");self.assertEqual(r.returncode,0,r.stderr)
        self.assertEqual([s.split()[0] for s in r.stdout.splitlines()],["B0.1","B12.1"])
    def test_brief_and_review(self):
        for action in ("brief","review"):
            r=cli("tools/fieldbook.py",action,"B23.3");self.assertEqual(r.returncode,0)
            self.assertIn("counterexample",r.stdout);self.assertIn("B21.3",r.stdout)
    def test_unknown_slice(self):
        self.assertNotEqual(cli("tools/fieldbook.py","brief","B32.1").returncode,0)
    def test_missing_evidence_remains_red(self):
        with tempfile.TemporaryDirectory() as td:
            r=cli("tools/fieldbook.py","gate","full","--evidence",td)
            self.assertEqual(r.returncode,1);self.assertIn("B23 missing",r.stdout)
    def test_archives_and_checklist_paths(self):
        v=json.loads((ROOT/"fixtures/archive-vectors.json").read_text());self.assertEqual([k["schema"] for k in v["versions"]],list(range(1,8)))
        project=json.loads((ROOT/"fixtures/project.json").read_text())
        ids={n["id"] for n in project["nodes"]};self.assertEqual(len(ids),len(project["nodes"]))
        self.assertEqual(len({n["code"] for n in project["nodes"]}),len(ids))
        for n in project["nodes"]:self.assertTrue(n["parent"] is None or n["parent"] in ids)
    def test_openapi_example_ids_and_order(self):
        data=json.loads((ROOT/"contracts/planning-examples.json").read_text())
        graph=data["graph_response"];ids={n["id"] for n in graph["nodes"]}
        for edge in graph["edges"]:self.assertTrue(edge["from"] in ids and edge["to"] in ids)
        result=data["split_response"];self.assertEqual(result["selected_id"],result["created_ids"][0])
        self.assertEqual(result["map_revision"],data["split_request"]["expected_revision"]+1)
    def test_runnable_lens_adapter(self):
        r=cli("tools/run_cases.py","--ticket","B12","--",sys.executable,"examples/lens_reference.py")
        self.assertEqual(r.returncode,0,r.stderr);self.assertIn("4/4",r.stdout)
    def test_wrong_adapter_output_fails(self):
        r=cli("tools/run_cases.py","--case","lens-match","--",sys.executable,"-c","print('{}')")
        self.assertEqual(r.returncode,1);self.assertIn("FAIL lens-match",r.stderr)
    def test_adapter_missing_case_fails(self):
        self.assertNotEqual(cli("tools/run_cases.py","--case","made-up","--list").returncode,0)
    def test_strict_projection(self):
        self.assertFalse(compare({"n":True},{"n":1}));self.assertFalse(compare({"visible":1,"extra":"hidden"},{"visible":1}))
    def test_probe_body(self):
        verify(200,b'{"status":"ok"}',200);verify(503,b'{"status":"unavailable"}',503)
        with self.assertRaises(ValueError):verify(200,b'{"status":"ok","extra":"private"}',200)
    def test_probe_local(self):
        with self.assertRaises(ValueError):probe("https://example.invalid")
    def test_all_source_scenarios_are_mapped(self):
        self.assertEqual(len(self.data["scenarios"]),26);self.assertTrue(all(self.data["scenarios"].values()))

class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.directory=Path(self.temp.name)
        self.ticket=select(catalog(),"B4")[0]
        log=self.directory/"run.log";log.write_text("Placeholder evidence for the handoff tool unit test only.\n")
        self.sha=hashlib.sha256(log.read_bytes()).hexdigest()
        self.rev="4"*40
        self.rec=template(self.ticket)
        self.rec.update(revision=self.rev,implementer="one",reviewer="two")
        for row in self.rec["checks"].values():
            row.update(state="passed",kind="command",exit_code=0,procedure="unit test example",observed="unit-only result",artifact="run.log",sha256=self.sha)
    def test_complete_receipt(self):self.assertEqual(inspect_receipt(self.ticket,self.rec,self.directory,self.rev),self.rev)
    def test_missing_check(self):
        self.rec["checks"].pop("T4")
        with self.assertRaises(ValueError):inspect_receipt(self.ticket,self.rec,self.directory)
    def test_nonzero_exit(self):
        self.rec["checks"]["T4"]["exit_code"]=1
        with self.assertRaises(ValueError):inspect_receipt(self.ticket,self.rec,self.directory)
    def test_untested_never_passes(self):
        self.rec["checks"]["T4"]["state"]="not-run"
        with self.assertRaises(ValueError):inspect_receipt(self.ticket,self.rec,self.directory)
    def test_tampered_evidence(self):
        (self.directory/"run.log").write_text("changed")
        with self.assertRaises(ValueError):inspect_receipt(self.ticket,self.rec,self.directory)
    def test_other_commit(self):
        with self.assertRaises(ValueError):inspect_receipt(self.ticket,self.rec,self.directory,"a"*40)
    def test_separate_reviewer(self):
        self.rec["reviewer"]="ONE"
        with self.assertRaises(ValueError):inspect_receipt(self.ticket,self.rec,self.directory)
    def test_config_is_not_live(self):
        rows={n:{"state":"config-only","observed":"validated example","artifact":"run.log","sha256":self.sha} for n in ("home","render","google")}
        def save(): (self.directory/"profiles.json").write_text(json.dumps({"selected":"home","revision":self.rev,"hosts":rows}))
        save()
        with self.assertRaises(ValueError):inspect_profiles(self.directory,self.rev)
        rows["home"]["state"]="live-tested";save();self.assertEqual(inspect_profiles(self.directory,self.rev),"home")

if __name__=="__main__":unittest.main()
