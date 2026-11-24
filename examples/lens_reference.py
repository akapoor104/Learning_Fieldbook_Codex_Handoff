"""Offline teaching adapter for B12. Demonstrates only the borrowed-method selection rule."""
import json
import sys

def choose(data):
    focus=data["focus"]
    omitted={(x["id"],x["version"]) for x in data["dismissed"]+data["kept"] if x["focus"]==focus["id"]}
    chosen=[]
    for card in data["cards"]:
        if card["field"]==focus["field"] or focus["mechanism"] not in card["mechanisms"]:
            continue
        if focus["requested_field"] is not None and card["field"]!=focus["requested_field"]:
            continue
        if card["minutes"]>focus["minutes"] or not set(card["materials"])<=set(focus["materials"]):
            continue
        if (card["id"],card["version"]) not in omitted:
            chosen.append(card)
    return min(chosen,key=lambda c:(data["last_kept"].get(c["field"],""),c["id"]),default=None)

if __name__=="__main__":
    packet=json.load(sys.stdin)
    if not packet["case"].startswith("lens-"):
        sys.exit("The teaching example supports lens cases only.")
    card=choose(packet["input"])
    print(json.dumps({"candidate":card["id"] if card else None,"version":card["version"] if card else None,"external_calls":0}))
