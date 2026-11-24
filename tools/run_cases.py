#!/usr/bin/env python3
"""Run isolated scenario observations produced by a local test adapter."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[1]


def compare(actual, expected):
    def canonical(value):
        return json.dumps(value,ensure_ascii=False,sort_keys=True,allow_nan=False,separators=(",",":"))
    return canonical(actual)==canonical(expected)


def main():
    args=argparse.ArgumentParser(description=__doc__)
    args.add_argument("--ticket",help="run cases owned by B number")
    args.add_argument("--case",action="append",dest="ids",help="choose an individual case, repeatable")
    args.add_argument("--list",action="store_true")
    args.add_argument("--timeout",type=float,default=30)
    args.add_argument("adapter",nargs=argparse.REMAINDER,help="after --, executable and its arguments")
    a=args.parse_args()
    all_cases=json.loads((ROOT/"validation/cases.json").read_text(encoding="utf-8"))
    known={c["id"] for c in all_cases}
    if a.ids and not set(a.ids)<=known:args.error("Unknown case ID")
    cases=[c for c in all_cases if (not a.ticket or c["ticket"]==a.ticket) and (not a.ids or c["id"] in a.ids)]
    if not cases:args.error("No adapter cases selected. Use the ticket's integration or manual checks.")
    if a.list:
        for row in cases:print(row["id"],row["ticket"],row["measures"])
        return 0
    adapter=a.adapter[1:] if a.adapter[:1]==["--"] else a.adapter
    if not adapter:args.error("Supply a local test adapter after --")
    failures=0
    for row in cases:
        packet={"case":row["id"],"input":row["input"]}
        try:
            p=subprocess.run(adapter,input=json.dumps(packet),capture_output=True,text=True,encoding="utf-8",timeout=a.timeout,check=False)
            if p.returncode!=0:raise ValueError(f"adapter exited {p.returncode}: {p.stderr.strip()[:200]}")
            got=json.loads(p.stdout)
            if not compare(got,row["expected"]):
                raise ValueError("projection differs. Expected "+json.dumps(row["expected"],ensure_ascii=False)+"; observed "+json.dumps(got,ensure_ascii=False)[:1100])
            print("PASS",row["id"])
        except (ValueError,OSError,subprocess.TimeoutExpired) as e:
            failures+=1;print("FAIL",row["id"],str(e),file=sys.stderr)
    print(f"{len(cases)-failures}/{len(cases)} scenario projections match.")
    return 1 if failures else 0

if __name__=="__main__":sys.exit(main())
