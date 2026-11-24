#!/usr/bin/env python3
"""Small, offline project-handoff commands. Requires Python 3.9+."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def catalog():
    return read_json(ROOT / "tickets/index.json")


def select(data, key):
    for ticket in data["tickets"]:
        if key == ticket["id"]:
            return ticket, None
        for part in ticket["slices"]:
            if key == part["id"]:
                return ticket, part
    raise ValueError(f"Unknown ticket {key}. Try the list command.")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_kit(data, sealed):
    tickets = data["tickets"]
    require([t["id"] for t in tickets] == [f"B{i}" for i in range(26)], "Expected B0 through B25.")
    require([t["test"] for t in tickets] == [f"T{i}" for i in range(26)], "Expected T0 through T25.")
    parts = {s["id"]: s for t in tickets for s in t["slices"]}
    require(len(parts) == sum(len(t["slices"]) for t in tickets), "Duplicate slice ID.")
    seen = set()
    def visit(key, path):
        require(key in parts, f"Unknown prerequisite {key}")
        require(key not in path, f"Prerequisite cycle at {key}")
        if key in seen:
            return
        for dep in parts[key]["depends"]:
            visit(dep, path | {key})
        seen.add(key)
    for key in parts:
        visit(key, set())
    originals = (ROOT / "reference/Learning_Fieldbook_Design_Spec_v0.8.md").read_text(encoding="utf-8").splitlines()
    covered = set()
    cases = read_json(ROOT / "validation/cases.json")
    by_case = {c["id"]: c for c in cases}
    require(len(cases) == len(by_case), "Duplicate adapter case ID.")
    for t in tickets:
        card = (ROOT / "tickets" / (t["id"] + ".md")).read_text(encoding="utf-8")
        require(t["acceptance"] in card, f"Lost {t['test']} acceptance.")
        require(any(t["acceptance"] in line for line in originals), f"{t['test']} differs from design.")
        for span in t["spec_lines"].split(","):
            low, high = map(int, span.split("-"))
            require(0 < low <= high <= len(originals), f"Bad source range in {t['id']}")
        for case in t["cases"]:
            require(by_case[case]["ticket"] == t["id"], f"Wrong case owner for {case}")
        covered.update(t["scenarios"])
    require(covered == set(data["scenarios"]) == {f"S{i}" for i in range(1,27)}, "Scenario coverage incomplete.")
    require({c for t in tickets for c in t["cases"]} == set(by_case), "Unowned adapter case.")
    for name, members in data["gates"].items():
        for t in tickets:
            if t["id"] in members:
                require(all(d.split(".")[0] in members for s in t["slices"] for d in s["depends"]), f"Missing dependency in {name}")
    household = read_json(ROOT / "fixtures/household.json")
    require((len(household["moments"]),len(household["highlights"]),len(household["frozen_editions"])) == (6,1,1), "Wrong base fixture counts.")
    require({x["household"] for x in household["actors"]} == {"A","B"}, "Missing second household.")
    assets = read_json(ROOT / "fixtures/assets.json")
    require(len(assets) == 4 and len(household["external_links"]) == 1, "Wrong asset inventory.")
    for a in assets:
        path = ROOT / "fixtures" / a["path"]
        require(path.stat().st_size == a["byte_length"] and digest(path) == a["sha256"], f"Damaged {a['id']}")
    api = read_json(ROOT / "contracts/core.openapi.json")
    require(api["openapi"] == "3.1.1" and len(api["paths"]) == 4, "Unexpected starter API.")
    if sealed:
        for line in (ROOT / "RELEASE_SHA256SUMS").read_text(encoding="utf-8").splitlines():
            expected, name = line.split("  ", 1)
            require(digest(ROOT/name) == expected, f"Release file differs: {name}")
    print(f"PACKET PASS · {len(tickets)} build tickets, {len(parts)} slices, {len(covered)} scenarios, {len(cases)} adapter cases and 4 verified originals.")
    print("This checks the handoff. Product tests run against a production checkout separately.")


def brief(data, key, review):
    t, s = select(data, key)
    if s is None:
        print((ROOT / "tickets" / (key+".md")).read_text(encoding="utf-8"))
        return
    print(("Independently review" if review else "Implement") + f" only {key} — {s['title']}.")
    print(f"Product design v0.8 · parent {t['id']} / {t['test']}. Read AGENTS.md and tickets/{t['id']}.md in the handoff.")
    print(f"Relevant design lines {t['spec_lines']}. Scope: {s['layer']}.")
    print(f"Prerequisites: {', '.join(s['depends']) or 'none'}. First inspect the actual repository and existing tests.")
    print(s['work'])
    print('Input: '+s['fixture']+'. Suggested locations: '+s['suggested_paths']+'.')
    print('Verify: '+s['verify'])
    print("Observe success: " + s["pass_case"])
    print("Observe the counterexample: " + s["adverse_case"])
    print("Parent acceptance, for context: " + t["acceptance"])
    if t["cases"]:
        print("Available parent-level adapter cases: "+", ".join(t["cases"])+".")
    if review:
        print("Trace the normal user entry to its persistence boundary. Rerun the normal and adverse cases on the candidate commit. Record observations and missing proof.")
    else:
        print("Run the smallest real checks for this slice. Report changed files, exact commands, actual outcomes and remaining untested paths. Keep evidence synthetic.")
    print("Model suggestion: " + s["model"]+".")


def template(t):
    check_ids=[s["id"] for s in t["slices"]] + [t["test"]] + t["cases"]
    return {"ticket":t["id"],"revision":"","implementer":"","reviewer":"",
            "checks":{k:{"state":"not-run","kind":"command","procedure":"","observed":"","artifact":"","sha256":""} for k in check_ids}}


def verify_artifact(directory, item):
    name=item.get("artifact","")
    require(isinstance(name,str) and bool(name.strip()), "Missing evidence path")
    path=(directory / name).resolve()
    require(not Path(name).is_absolute() and directory.resolve() in path.parents and path.is_file(), f"Evidence must be a file inside the evidence directory: {name}")
    require(path.stat().st_size>0 and digest(path)==item.get("sha256"), f"Missing or mismatched evidence digest for {name}")


def clean_text(value):
    return isinstance(value,str) and bool(value.strip())


def inspect_receipt(t, rec, directory, revision=None):
    require(rec.get("ticket")==t["id"],"Ticket ID differs")
    sha=rec.get("revision","")
    require(isinstance(sha,str) and re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}",sha), "Missing full candidate commit SHA")
    require(revision is None or sha==revision, "Evidence is from another candidate commit")
    require(clean_text(rec.get("implementer")), "Missing implementer")
    if t["risk"] in ("access","recovery"):
        require(clean_text(rec.get("reviewer")) and rec["reviewer"].strip().casefold()!=rec["implementer"].strip().casefold(),"Independent reviewer needed for this access/recovery ticket")
    checks=rec.get("checks")
    require(isinstance(checks,dict),"Missing checks")
    for key in template(t)["checks"]:
        item=checks.get(key)
        require(isinstance(item,dict) and item.get("state")=="passed",f"{key} missing, untested or unsuccessful")
        require(item.get("kind") in ("command","device","manual"),f"{key} needs a valid check type")
        require(clean_text(item.get("procedure")) and clean_text(item.get("observed")),f"{key} needs the actual procedure and observation")
        if item["kind"]=="command":
            require(type(item.get("exit_code")) is int and item["exit_code"]==0,f"{key} needs successful command exit_code")
        verify_artifact(directory,item)
    return sha


def inspect_profiles(directory, revision):
    profile=read_json(directory/"profiles.json")
    require(profile.get("revision")==revision,"Host rehearsal belongs to a different candidate")
    selected=profile.get("selected")
    require(selected in ("home","render","google"),"Select a deployment profile")
    for name in ("home","render","google"):
        item=profile.get("hosts",{}).get(name,{})
        state=item.get("state")
        require(state in {"live-tested","config-only","not-run"},f"{name} needs an explicit state")
        if state=="not-run":
            raise ValueError(f"{name} still needs the B0 configuration check")
        require(name!=selected or state=="live-tested","Selected host requires a live synthetic rehearsal")
        require(clean_text(item.get("observed")),f"Missing {name} rehearsal observation")
        verify_artifact(directory,item)
    return selected


def gate(data, args):
    directory=Path(args.evidence)
    errors=[]
    found=set()
    for key in data["gates"][args.gate]:
        t,_=select(data,key)
        pathname=directory/(key+".json")
        if not pathname.is_file():
            errors.append(key+" missing receipt")
            continue
        try:
            found.add(inspect_receipt(t,read_json(pathname),directory,args.revision))
        except (ValueError,OSError,TypeError,KeyError) as err:
            errors.append(key+" "+str(err))
    if len(found)>1:
        errors.append("Gate receipts refer to different candidate commits. Rerun on the release candidate.")
    if args.gate!="notebook":
        if not (directory/"profiles.json").is_file():
            errors.append("Missing profiles.json with selected host and observed rehearsal")
        elif args.revision or len(found)==1:
            try: inspect_profiles(directory,args.revision or next(iter(found)))
            except (ValueError,OSError,TypeError,KeyError) as err: errors.append(str(err))
    if errors:
        print("PRODUCT GATE BLOCKED · "+args.gate)
        print("\n".join("  "+e for e in errors))
        return 1
    print(f"PRODUCT EVIDENCE COMPLETE · {args.gate} · {next(iter(found))}")
    print("Review the linked observations and decide whether to release. No deployment was performed.")
    return 0


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest="action",required=True)
    cmd=sub.add_parser("check",help="validate the packet offline");cmd.add_argument("--sealed",action="store_true",help="verify all issued files")
    sub.add_parser("list",help="list all parent tickets")
    cmd=sub.add_parser("ready",help="print eligible slices");cmd.add_argument("--done",nargs="*",default=[])
    for name in ("brief","review","template"):
        cmd=sub.add_parser(name);cmd.add_argument("id")
    cmd=sub.add_parser("gate",help="inspect product evidence");cmd.add_argument("gate",choices=["notebook","family-pilot","full"]);cmd.add_argument("--evidence",default="evidence");cmd.add_argument("--revision")
    a=parser.parse_args(argv); data=catalog()
    if a.action=="check":validate_kit(data,a.sealed)
    elif a.action=="list":
        for t in data["tickets"]:print(f"{t['id']:>3}  {t['title']}  ({t['test']})")
    elif a.action=="ready":
        parts={s["id"]:s for t in data["tickets"] for s in t["slices"]}
        done=set(a.done);require(done<=parts.keys(),"Unknown completed slice")
        for s in parts.values():
            if s["id"] not in done and set(s["depends"])<=done:print(s["id"]+"  "+s["title"])
    elif a.action in ("brief","review"):brief(data,a.id,a.action=="review")
    elif a.action=="template":
        t,s=select(data,a.id);require(s is None,"Choose the parent ticket for a receipt");print(json.dumps(template(t),indent=2))
    elif a.action=="gate":return gate(data,a)
    return 0

if __name__=="__main__":
    try: sys.exit(main())
    except (ValueError,KeyError,TypeError,OSError) as error:
        print("ERROR · "+str(error),file=sys.stderr)
        sys.exit(2)
