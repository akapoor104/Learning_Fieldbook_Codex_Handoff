import json
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class TrialsPacketTests(unittest.TestCase):
    def test_cumulative_build_projections(self):
        result = subprocess.run([sys.executable, "tools/run_cases.py", "--ticket", "B24", "--", sys.executable, "examples/trials_reference.py"], cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("3/3", result.stdout)

    def test_failure_resume_projections(self):
        result = subprocess.run([sys.executable, "tools/run_cases.py", "--ticket", "B25", "--", sys.executable, "examples/trials_reference.py"], cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("3/3", result.stdout)

    def test_project_api_and_historical_read(self):
        api = json.loads((ROOT/"contracts/trials.openapi.json").read_text())
        schemas = api["components"]["schemas"]
        self.assertEqual(schemas["Build"]["properties"]["sources"]["minItems"], 1)
        self.assertTrue(schemas["Build"]["properties"]["sources"]["uniqueItems"])
        self.assertCountEqual(schemas["Source"]["required"], ["pretotype_id","observation_revision"])
        method = api["paths"]["/projects/{p}/trials/{t}/observations/{revision}"]["get"]
        self.assertEqual(next(p["schema"]["type"] for p in method["parameters"] if p["name"]=="revision"), "integer")
        path = api["paths"]["/projects/{p}/failure-resume/selections/{observation}"]
        self.assertIn("put", path)
        self.assertIn("delete", path)

    def test_pilot_covers_every_new_slice(self):
        catalog = json.loads((ROOT/"tickets/index.json").read_text())
        for name in ("family-pilot","full"):
            self.assertTrue({"B24","B25"} <= set(catalog["gates"][name]))
        self.assertEqual(catalog["archive_schema"],7)
