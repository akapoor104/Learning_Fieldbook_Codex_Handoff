"""Print the SHA-256 of a ticket evidence file using only the standard library."""
import argparse
import hashlib
from pathlib import Path

if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path",type=Path)
    args=parser.parse_args()
    h=hashlib.sha256()
    with args.path.open("rb") as f:
        for chunk in iter(lambda:f.read(1024*1024),b""):
            h.update(chunk)
    print(h.hexdigest())
