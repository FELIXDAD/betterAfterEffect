(function () {
    function n(x) { return x.toPrecision(15); }
    function arr(a) { var o = []; for (var i = 0; i < a.length; i++) o.push(n(a[i])); return "[" + o.join(",") + "]"; }
    var out = [];
    var comp = app.project.items.addComp("bae03_auto", 1920, 1080, 1, 10, 30);
    // Temporal auto-Bezier, unequal spacing (rotation).
    var L = comp.layers.addNull(10);
    var r = L.property("ADBE Transform Group").property("ADBE Rotate Z");
    var tk = [[0, 0], [0.5, 100], [2, 30], [2.5, 200], [5, -50]];
    for (var i = 0; i < tk.length; i++) r.setValueAtTime(tk[i][0], tk[i][1]);
    for (var i = 1; i <= r.numKeys; i++) {
        r.setInterpolationTypeAtKey(i, KeyframeInterpolationType.BEZIER, KeyframeInterpolationType.BEZIER);
        r.setTemporalContinuousAtKey(i, true);
        r.setTemporalAutoBezierAtKey(i, true);
    }
    var o = [];
    for (var i = 1; i <= r.numKeys; i++) {
        var ei = r.keyInTemporalEase(i)[0], eo = r.keyOutTemporalEase(i)[0];
        o.push("[" + n(r.keyTime(i)) + "," + n(r.keyValue(i)) + "," + n(ei.speed) + "," + n(ei.influence) + "," + n(eo.speed) + "," + n(eo.influence) + "]");
    }
    out.push('"temporal":[' + o.join(",") + "]");
    // Spatial auto-Bezier, unequal spacing in time and distance (position), temporally linear.
    var P = L.property("ADBE Transform Group").property("ADBE Position");
    var pk = [[0, [100, 100]], [0.5, [400, 150]], [2, [500, 700]], [2.5, [1500, 650]], [5, [1600, 100]]];
    for (var i = 0; i < pk.length; i++) P.setValueAtTime(pk[i][0], pk[i][1]);
    for (var i = 1; i <= P.numKeys; i++) {
        P.setInterpolationTypeAtKey(i, KeyframeInterpolationType.LINEAR, KeyframeInterpolationType.LINEAR);
        P.setSpatialContinuousAtKey(i, true);
        P.setSpatialAutoBezierAtKey(i, true);
    }
    o = [];
    for (var i = 1; i <= P.numKeys; i++) {
        o.push("[" + n(P.keyTime(i)) + "," + arr(P.keyValue(i)) + "," + arr(P.keyInSpatialTangent(i)) + "," + arr(P.keyOutSpatialTangent(i)) + "]");
    }
    out.push('"spatial":[' + o.join(",") + "]");
    // Roving: same path, keys 2..4 roving, ends eased.
    for (var i = 1; i <= P.numKeys; i++) {
        P.setInterpolationTypeAtKey(i, KeyframeInterpolationType.BEZIER, KeyframeInterpolationType.BEZIER);
    }
    for (var i = 2; i <= P.numKeys - 1; i++) P.setRovingAtKey(i, true);
    o = [];
    for (var i = 1; i <= P.numKeys; i++) {
        var ei = P.keyInTemporalEase(i)[0], eo = P.keyOutTemporalEase(i)[0];
        o.push("[" + n(P.keyTime(i)) + "," + arr(P.keyValue(i)) + "," + arr(P.keyInSpatialTangent(i)) + "," + arr(P.keyOutSpatialTangent(i)) +
            "," + n(eo.speed) + "," + n(eo.influence) + "," + n(ei.speed) + "," + n(ei.influence) + "," + P.keyRoving(i) + "]");
    }
    out.push('"roving":[' + o.join(",") + "]");
    comp.remove();
    var f = new File("/private/tmp/bae-03/auto.json");
    f.open("w"); f.write("{" + out.join(",") + "}"); f.close();
    return "ok";
})();
