"""AE (aerender TIFF) vs BAE (bae render TIFF/PNG) on B1, 3 runs each, median."""
import json, os, shutil, statistics, subprocess, sys, time
ROOT = "/private/tmp/bae-speed"
AER = "/Applications/Adobe After Effects 2026/aerender"
BAE = sys.argv[1]
runs = []

def run(name, cmd, outdir, expect, reps=3):
    times, extra = [], None
    for r in range(reps):
        shutil.rmtree(outdir, ignore_errors=True); os.makedirs(outdir)
        t0 = time.perf_counter()
        p = subprocess.run(cmd, capture_output=True, text=True)
        dt = time.perf_counter() - t0
        n = len([f for f in os.listdir(outdir) if not f.startswith('.')])
        if p.returncode != 0 or n != expect:
            print(f"{name} run {r}: rc={p.returncode} frames={n}\n{p.stdout[-800:]}\n{p.stderr[-800:]}", flush=True)
            sys.exit(1)
        if "--json" in cmd:
            try: extra = json.loads(p.stdout[p.stdout.index("{"):])
            except Exception: pass
        sizes = sum(os.path.getsize(os.path.join(outdir, f)) for f in os.listdir(outdir))
        times.append(dt)
        print(f"{name} run {r}: {dt:.3f} s, {n} frames, {sizes/1e6:.0f} MB", flush=True)
    rec = {"name": name, "frames": expect, "runs_s": [round(t, 3) for t in times], "median_s": round(statistics.median(times), 3)}
    if extra: rec["bae_json_last"] = {k: extra[k] for k in extra if k in ("total_ms", "stages", "timings", "frames", "fps")}
    runs.append(rec)

which = sys.argv[2] if len(sys.argv) > 2 else "all"
if which in ("all", "ae"):
    base = [AER, "-project", f"{ROOT}/B1.aep", "-comp", "b1", "-RStemplate", "Best Settings", "-OMtemplate", "TIFF Sequence with Alpha"]
    run("AE aerender TIFF 300", base + ["-output", f"{ROOT}/ae-tiff/b1_[####].tif"], f"{ROOT}/ae-tiff", 300)
    run("AE aerender TIFF 1", base + ["-s", "0", "-e", "0", "-output", f"{ROOT}/ae-tiff1/b1_[####].tif"], f"{ROOT}/ae-tiff1", 1)
if which in ("all", "bae"):
    for fmt, comp in (("tiff", ["--compression", "none"]), ("png", [])):
        base = [BAE, "render", f"{ROOT}/b1/B1.bae", "--comp", "b1", "--depth", "8", "--resolution", "full", "-f", fmt] + comp + ["--json"]
        run(f"BAE {fmt.upper()} 300", base + ["--frames", "all", "-o", f"{ROOT}/bae-{fmt}/b1_####.{fmt}"], f"{ROOT}/bae-{fmt}", 300)
        run(f"BAE {fmt.upper()} 1", base + ["--frame", "0", "-o", f"{ROOT}/bae-{fmt}1/b1.{fmt}"], f"{ROOT}/bae-{fmt}1", 1)
out = f"{ROOT}/results-{which}.json"
json.dump(runs, open(out, "w"), indent=1)
print("wrote", out)
