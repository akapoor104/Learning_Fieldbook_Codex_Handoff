#!/usr/bin/env python3
"""Probe the two status-only endpoints on a local test instance."""
import argparse
import json
from urllib.error import HTTPError,URLError
from urllib.parse import urlsplit
from urllib.request import build_opener,HTTPRedirectHandler,ProxyHandler
import sys

class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self,req,fp,code,msg,headers,newurl):
        return None

def verify(status,body,expected):
    payload=json.loads(body)
    if status!=expected or payload!={"status":"ok" if expected==200 else "unavailable"}:
        raise ValueError("Unexpected HTTP status or status-only body")
def probe(base,unready=False):
    url=urlsplit(base)
    if url.scheme not in {"http","https"} or url.hostname not in {"127.0.0.1","::1","localhost"} or url.username or url.password or url.path not in {"","/"} or url.query or url.fragment:
        raise ValueError("Supply the origin of the local test instance, for example http://127.0.0.1:8080")
    client=build_opener(ProxyHandler({}),NoRedirect())
    for path,expected in [("/healthz",200),("/readyz",503 if unready else 200)]:
        try: response=client.open(base.rstrip("/")+path,timeout=5)
        except HTTPError as problem:response=problem
        with response:
            verify(response.status,response.read(1025),expected)
            print("PASS",path,response.status)
if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("origin");parser.add_argument("--expect-not-ready",action="store_true")
    a=parser.parse_args()
    try:probe(a.origin,a.expect_not_ready)
    except (ValueError,URLError,OSError) as error:
        print("FAIL",error,file=sys.stderr);sys.exit(1)
