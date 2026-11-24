"""Small, offline behavior model for B24/B25. Production acceptance uses the real Rust app."""
import copy
import json
import sys
from pathlib import Path

FIXTURE = Path(__file__).resolve().parents[1] / "fixtures" / "project-trials.json"


class Refused(Exception):
    pass


class Project:
    def __init__(self, fixture):
        self.trials = {r["id"]: copy.deepcopy(r) for r in fixture}
        self.builds = {}
        self.revivals = {}
        self.entries = []
        self.receipts = {}
        self.errors = []

    def trial(self, key):
        if key not in self.trials:
            raise Refused("scope")
        return self.trials[key]

    def observation(self, row, revision):
        for result in row["observations"]:
            if result["revision"] == revision:
                return result
        raise Refused("revision")

    def build(self, action):
        token = action["key"]
        if token in self.receipts:
            if self.receipts[token] != action:
                raise Refused("conflict")
            return
        refs = [tuple(pair) for pair in action["sources"]]
        if not refs or len(set(k for k, _ in refs)) != len(refs) or not action["why"].strip():
            raise Refused("invalid")
        for key, rev in refs:
            row = self.trial(key)
            if row["kind"] != "pre" or not row["observations"]:
                raise Refused("unobserved")
            if row["observations"][-1]["revision"] != rev:
                raise Refused("revision")
        parent = action.get("parent")
        if parent and parent not in self.builds:
            raise Refused("scope")
        if action["id"] in self.trials:
            raise Refused("conflict")
        self.builds[action["id"]] = {"sources":action["sources"],"why":action["why"],"parent":parent}
        self.trials[action["id"]] = {"id":action["id"],"kind":"build","observations":[]}
        self.receipts[token] = copy.deepcopy(action)

    def apply(self, a):
        action = a["op"]
        if action == "build":
            self.build(a)
            return
        row = self.trial(a["id"])
        if action == "record":
            revision = len(row["observations"])+1
            row["observations"].append({"revision":revision,"at":a["at"],"signal":a["signal"],"text":a["text"],"lesson":a["lesson"],"setback":a.get("setback","")})
        elif action in ("select","unselect"):
            o = self.observation(row,a["revision"])
            ref = [row["id"],o["revision"]]
            if action == "select":
                if o["signal"] not in ("Missed","Needs a change") and not o.get("setback"):
                    raise Refused("ineligible")
                if ref not in self.entries:
                    self.entries.append(ref)
            elif ref in self.entries:
                self.entries.remove(ref)
        elif action == "revive":
            if a["new"] in self.trials or not a["changed"].strip():
                raise Refused("invalid")
            self.revivals[a["new"]] = {"from":row["id"],"changed":a["changed"]}
            self.trials[a["new"]] = {"id":a["new"],"kind":"pre","observations":[]}
        else:
            raise ValueError("Unknown operation")

    def projection(self, name):
        if name == "builds":
            return {k:v["sources"] for k,v in self.builds.items()}
        if name == "destinations":
            return {k:[b for b,v in self.builds.items() if k in [source[0] for source in v["sources"]]]
                    for k,row in self.trials.items() if row["kind"]=="pre" and row["observations"]}
        if name == "resume":
            return [{"id":key, "text":o["text"], "lesson":o["lesson"], "setback":o.get("setback","")}
                    for key,rev in sorted(self.entries,key=lambda pair:(self.observation(self.trial(pair[0]),pair[1])["at"],pair[0])) for o in [self.observation(self.trial(key),rev)]]
        if name == "revivals":
            return self.revivals
        if name == "results":
            return {key:[o["signal"] for o in row["observations"]] for key,row in self.trials.items()}
        if name == "errors":
            return self.errors
        raise ValueError("Unknown projection")


def run(data):
    fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))
    project = Project(fixture["projects"][data["project"]])
    for action in data["steps"]:
        try:
            project.apply(action)
        except Refused as error:
            project.errors.append(str(error))
    return {name:project.projection(name) for name in data["read"]}


if __name__ == "__main__":
    print(json.dumps(run(json.load(sys.stdin)["input"]), ensure_ascii=False))
