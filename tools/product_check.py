"""Run the selected production suite from the actual app checkout."""
import argparse
from pathlib import Path
import shutil
import subprocess
import sys

SUITES={
    "rust":(".",["Cargo.toml","Cargo.lock"],[["cargo","fmt","--all","--","--check"],["cargo","check","--workspace","--locked"],["cargo","test","--workspace","--locked"]]),
    "web":("web",["package.json","package-lock.json"],[["npm","ci"],["npm","run","check"],["npm","test"],["npm","run","build"]]),
    "home-config":(".",["compose.yaml",".env.example"],[["docker","compose","--env-file",".env.example","config","--quiet"]])
}

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--repo",type=Path,required=True)
    p.add_argument("--suite",choices=[*SUITES,"all"],required=True)
    p.add_argument("--show",action="store_true")
    a=p.parse_args()
    for name in (SUITES if a.suite=="all" else [a.suite]):
        folder,required,commands=SUITES[name]
        work=(a.repo/folder).resolve()
        if not a.show and any(not (work/f).is_file() for f in required):
            sys.exit("MISSING production input for "+name+" in "+str(work)+": "+", ".join(required))
        for cmd in commands:
            print(str(work)+"  "+" ".join(cmd),flush=True)
            if a.show:continue
            tool=shutil.which(cmd[0])
            if tool is None:sys.exit("MISSING tool "+cmd[0])
            result=subprocess.run([tool,*cmd[1:]],cwd=work,check=False)
            if result.returncode:return result.returncode
    return 0
if __name__=="__main__":sys.exit(main())
