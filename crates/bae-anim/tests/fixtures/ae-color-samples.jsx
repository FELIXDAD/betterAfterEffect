// Color interpolation reference samples (03 bae-anim). Writes bae-anim-samples JSON.
(function () {
    var F = 705600000;
    var cases = [
        {id: "color_linear", keys: [[0, [1, 0, 0, 1], "L", null], [2, [0, 0, 1, 1], "L", null]]},
        {id: "color_ease", keys: [[0, [1, 0, 0, 1], "B", [0, 50]], [2, [0, 1, 0.5, 1], "B", [0, 50]]]},
        {id: "color_hold", keys: [[0, [1, 1, 0, 1], "H", null], [1, [0, 1, 1, 1], "H", null]]},
        // Non-zero speeds: which "distance" AE relates colour speed to (AE-ANI-034).
        {id: "color_speed_one_channel", keys: [[0, [0, 0, 0, 1], "B", [200, 60]], [2, [0, 1, 0, 1], "B", [0, 40]]]},
        {id: "color_speed_two_channels", keys: [[0, [1, 0, 0, 1], "B", [200, 60]], [2, [0, 0, 1, 1], "B", [0, 40]]]},
        {id: "color_speed_three_channels", keys: [[0, [0, 0, 0, 1], "B", [300, 30]], [2, [1, 0.5, 0.25, 1], "B", [-50, 70]]]}
    ];
    function n(x) { return x.toPrecision(15); }
    function nm(x) { if (x === KeyframeInterpolationType.LINEAR) return "linear"; if (x === KeyframeInterpolationType.HOLD) return "hold"; return "bezier"; }
    function arr(a) { var o = []; for (var i = 0; i < a.length; i++) o.push(n(a[i])); return "[" + o.join(",") + "]"; }
    var results = [];
    for (var c = 0; c < cases.length; c++) {
        var cs = cases[c];
        var comp = app.project.items.addComp("bae03_" + cs.id, 1920, 1080, 1, 4, 30);
        var layer = comp.layers.addNull(4);
        var fx = layer.property("ADBE Effect Parade").addProperty("ADBE Color Control");
        var prop = fx.property("ADBE Color Control-0001");
        for (var k = 0; k < cs.keys.length; k++) prop.setValueAtTime(cs.keys[k][0], cs.keys[k][1]);
        for (var k = 0; k < cs.keys.length; k++) {
            var kind = cs.keys[k][2], idx = k + 1;
            var t = KeyframeInterpolationType.BEZIER;
            if (kind === "L") t = KeyframeInterpolationType.LINEAR;
            if (kind === "H") t = KeyframeInterpolationType.HOLD;
            prop.setInterpolationTypeAtKey(idx, t, t);
            if (kind === "B") {
                var e = new KeyframeEase(cs.keys[k][3][0], cs.keys[k][3][1]);
                prop.setTemporalEaseAtKey(idx, [e], [e]);
            }
        }
        var keyJson = [];
        for (var k = 1; k <= prop.numKeys; k++) {
            var it = prop.keyInInterpolationType(k), ot = prop.keyOutInterpolationType(k);
            var ei = prop.keyInTemporalEase(k)[0], eo = prop.keyOutTemporalEase(k)[0];
            keyJson.push('"' + Math.round(prop.keyTime(k) * F) + '":{"value":{"type":"color","value":' + arr(prop.keyValue(k)) +
                '},"interp_in":"' + nm(it) + '","interp_out":"' + nm(ot) + '","ease_in":[{"speed":' + n(ei.speed) + ',"influence":' + n(ei.influence) +
                '}],"ease_out":[{"speed":' + n(eo.speed) + ',"influence":' + n(eo.influence) + '}]}');
        }
        var times = [], vals = [];
        for (var i = 0; i <= 240; i++) {
            times.push(i * 11760000);
            vals.push(arr(prop.valueAtTime(i / 60, true)));
        }
        results.push('{"id":"' + cs.id + '","comp":{"width":1920,"height":1080,"frame_rate":30,"duration":2822400000},' +
            '"layer":{"start_time":0,"stretch":100,"in_point":0,"out_point":2822400000,"three_d":false},' +
            '"property":{"path":["ADBE Effect Parade","ADBE Color Control","ADBE Color Control-0001"],"value_type":"color","spatial":false,"dimensions":4},' +
            '"attr":{"keyframes":{' + keyJson.join(",") + '}},"times":[' + times.join(",") + '],"pre_expression":[' + vals.join(",") + "]}");
        comp.remove();
    }
    var f = new File("/private/tmp/bae-03/color-samples2.json");
    f.encoding = "UTF-8";
    f.open("w");
    f.write('{"format":"bae-anim-samples","format_version":"0.1.0","generator":{"name":"bae-03 color-samples.jsx","version":"1"},' +
        '"host":{"app":"After Effects","version":"' + app.version + '"},"time":{"unit":"flicks","flicks_per_second":705600000,"key_time_base":"comp"},' +
        '"results":[' + results.join(",") + "]}");
    f.close();
    return "ok " + results.length;
})();
