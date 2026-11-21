#!/bin/bash
# 3 runs each of AE tiff/png x 300/1 inside the running AE; prints "mode frames run render_ms open_ms files".
A=/Users/a4/Documents/Orca/betterAfterEffect/tools/ae/aectl
for spec in tiff:300 tiff:1 png:300 png:1; do
  mode=${spec%%:*}; frames=${spec##*:}
  for r in 1 2 3; do
    out=/private/tmp/bae-speed/ae-$mode-$frames; rm -rf $out; mkdir -p $out
    res=$($A eval "var BAE_MODE='$mode', BAE_FRAMES=$frames, BAE_OUT='$out'; \$.evalFile('/private/tmp/bae-speed/scripts/ae_time.jsx');" --timeout 1200 --lock-timeout 1800)
    val=$(echo "$res" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['value'] if d['ok'] else 'ERR '+str(d['error']))")
    echo "$mode $frames $r $val files=$(ls $out | wc -l | tr -d ' ')"
  done
done
