// Rebuild BAE benchmark scene B1 in After Effects from `bae export-json` output, so both
// apps render the identical scene (same random values, keys, blend modes, assets).
(function () {
    var root = "/private/tmp/bae-speed";
    var f = new File(root + "/b1.json"); f.encoding = "UTF-8"; f.open("r"); var txt = f.read(); f.close();
    var doc = eval("(" + txt + ")");
    var FLICKS = 705600000;
    function child(t, name) { for (var i = 0; i < t.children.length; i++) if (t.children[i].name === name) return t.children[i]; return null; }
    var top = {}; for (var i = 0; i < doc.prims.length; i++) top[doc.prims[i].name] = doc.prims[i];
    var assets = {}; var at = top["assets"].children; for (var a = 0; a < at.length; a++) assets["/assets/" + at[a].name] = at[a];
    var layers = child(child(top["comps"], "b1"), "layers").children;

    app.beginSuppressDialogs();
    try { app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES); } catch (e) {}
    app.newProject();
    var comp = app.project.items.addComp("b1", 1920, 1080, 1, 10, 30);
    var imported = {};
    function v(attr) { return attr && attr.value ? attr.value.value : null; }
    function apply(prop, attr, map) {
        if (!attr) return;
        if (attr.keyframes) {
            var keys = []; for (var t in attr.keyframes) keys.push(t);
            keys.sort(function (x, y) { return Number(x) - Number(y); });
            for (var k = 0; k < keys.length; k++) prop.setValueAtTime(Number(keys[k]) / FLICKS, map(attr.keyframes[keys[k]].value.value));
            for (var n = 1; n <= prop.numKeys; n++) {
                prop.setInterpolationTypeAtKey(n, KeyframeInterpolationType.LINEAR, KeyframeInterpolationType.LINEAR);
                if (prop.isSpatial) prop.setSpatialTangentsAtKey(n, [0, 0, 0], [0, 0, 0]);
            }
        } else if (attr.value) {
            prop.setValue(map(attr.value.value));
        }
    }
    function id(x) { return x; }
    var blend = { add: BlendingMode.ADD, screen: BlendingMode.SCREEN, multiply: BlendingMode.MULTIPLY, overlay: BlendingMode.OVERLAY };
    for (var li = layers.length - 1; li >= 0; li--) {   // bottom first: AE adds new layers on top
        var L = layers[li], A = L.attrs, src = assets[v(A["source"])], layer;
        if (v(A["kind"]) === "solid") {
            var c = v(src.attrs["solid_color"]);
            layer = comp.layers.addSolid([c[0], c[1], c[2]], L.name, v(src.attrs["width"]), v(src.attrs["height"]), 1, 10);
        } else {
            var path = v(src.attrs["file:original"]);
            if (!imported[path]) imported[path] = app.project.importFile(new ImportOptions(new File(path)));
            layer = comp.layers.add(imported[path], 10);
            layer.name = L.name;
        }
        var tr = layer.property("ADBE Transform Group");
        apply(tr.property("ADBE Anchor Point"), A["transform:anchor_point"], id);
        apply(tr.property("ADBE Position"), A["transform:position"], id);
        apply(tr.property("ADBE Rotate Z"), A["transform:rotation"], id);
        apply(tr.property("ADBE Scale"), A["transform:scale"], id);
        apply(tr.property("ADBE Opacity"), A["transform:opacity"], id);
        if (A["blend_mode"]) layer.blendingMode = blend[v(A["blend_mode"])];
    }
    // Probe available render-settings / output-module templates for aerender.
    var rq = app.project.renderQueue.items.add(comp);
    var info = { layers: comp.numLayers, rs: rq.templates.join("|"), om: rq.outputModule(1).templates.join("|") };
    rq.remove();
    app.project.save(new File(root + "/B1.aep"));
    app.endSuppressDialogs(false);
    return info.layers + "\nRS: " + info.rs + "\nOM: " + info.om;
})();
