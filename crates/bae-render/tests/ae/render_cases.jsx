// Builds every case of cases.json in After Effects and renders frame 0 of each to
// PNG (comp.saveFrameToPng: RGBA, premultiplied). Run with tools/ae/aectl:
//
//   tools/ae/aectl run crates/bae-render/tests/ae/render_cases.jsx
//
// Inputs:  cases.json next to this script; footage in BAE_AE_ASSETS
//          (default /private/tmp/bae-04-ae/assets, written by the ae_compare example).
// Outputs: BAE_AE_OUT/<case>.png (default /private/tmp/bae-04-ae/ae).
// Everything is created inside a project folder that is removed afterwards.

(function () {
    var here = File($.fileName).parent;
    var assets = (typeof BAE_AE_ASSETS !== "undefined") ? BAE_AE_ASSETS : "/private/tmp/bae-04-ae/assets";
    var outDir = (typeof BAE_AE_OUT !== "undefined") ? BAE_AE_OUT : "/private/tmp/bae-04-ae/ae";
    var f = new File(here.fsName + "/cases.json");
    f.encoding = "UTF-8";
    if (!f.open("r")) { throw new Error("cannot read " + f.fsName); }
    var spec = eval("(" + f.read() + ")");
    f.close();
    var out = new Folder(outDir);
    if (!out.exists && !out.create()) { throw new Error("cannot create " + outDir); }

    var proj = app.project;
    proj.bitsPerChannel = 8;
    proj.linearBlending = false;
    try { proj.workingSpace = ""; } catch (ignored) {}
    var folder = proj.items.addFolder("bae04_compare");

    var footage = {};
    function importFootage(name) {
        if (footage[name]) { return footage[name]; }
        var item = proj.importFile(new ImportOptions(new File(assets + "/" + name)));
        item.parentFolder = folder;
        item.mainSource.alphaMode = AlphaMode.STRAIGHT;
        footage[name] = item;
        return item;
    }

    // AE's ellipse handle factor (4/3 tan(pi/8)); vertices top, right, bottom, left.
    var K = 0.5522847498307936;
    function maskShape(md) {
        var sh = new Shape();
        var v, it, ot, n, q;
        if (md.rect) {
            q = md.rect;
            v = [[q[0], q[1]], [q[2], q[1]], [q[2], q[3]], [q[0], q[3]]];
            it = [[0, 0], [0, 0], [0, 0], [0, 0]];
            ot = [[0, 0], [0, 0], [0, 0], [0, 0]];
        } else if (md.ellipse) {
            q = md.ellipse;
            var cx = (q[0] + q[2]) / 2, cy = (q[1] + q[3]) / 2, rx = (q[2] - q[0]) / 2, ry = (q[3] - q[1]) / 2;
            v = [[cx, q[1]], [q[2], cy], [cx, q[3]], [q[0], cy]];
            it = [[-rx * K, 0], [0, -ry * K], [rx * K, 0], [0, ry * K]];
            ot = [[rx * K, 0], [0, ry * K], [-rx * K, 0], [0, -ry * K]];
        } else {
            v = md.poly;
            it = [];
            ot = [];
            for (n = 0; n < v.length; n++) { it.push([0, 0]); ot.push([0, 0]); }
        }
        sh.vertices = v;
        sh.inTangents = it;
        sh.outTangents = ot;
        sh.closed = true;
        return sh;
    }
    // Masks top first (Mask 1 = md[0]), in layer pixels (AE-CMP-036).
    function addMasks(layer, masks) {
        for (var mi = 0; mi < masks.length; mi++) {
            var md = masks[mi];
            var atom = layer.property("ADBE Mask Parade").addProperty("ADBE Mask Atom");
            atom.property("ADBE Mask Shape").setValue(maskShape(md));
            atom.maskMode = MaskMode[(md.mode || "add").toUpperCase()];
            if (md.inverted) { atom.inverted = true; }
            if (md.opacity !== undefined) { atom.property("ADBE Mask Opacity").setValue(md.opacity); }
            if (md.feather) { atom.property("ADBE Mask Feather").setValue(md.feather); }
            if (md.expansion) { atom.property("ADBE Mask Offset").setValue(md.expansion); }
        }
    }

    var comps = {};
    function build(c) {
        var comp = proj.items.addComp(c.name, c.size[0], c.size[1], 1.0, 1.0, 30);
        comp.parentFolder = folder;
        comp.bgColor = [0, 0, 0];
        comp.resolutionFactor = [1, 1];
        var i, l, layer;
        // AE adds new layers on top: add bottom-first so layers[0] ends up as index 1.
        for (i = c.layers.length - 1; i >= 0; i--) {
            l = c.layers[i];
            if (l.solid) {
                layer = comp.layers.addSolid([l.solid[0] / 255, l.solid[1] / 255, l.solid[2] / 255],
                    c.name + "_" + l.name, l.size[0], l.size[1], 1.0);
                layer.source.parentFolder = folder;
            } else if (l.footage) {
                layer = comp.layers.add(importFootage(l.footage));
            } else if (l.comp) {
                layer = comp.layers.add(comps[l.comp]);
            }
            layer.name = l.name;
            layer.quality = LayerQuality.BEST;
            layer.property("ADBE Transform Group").property("ADBE Position").setValue(l.pos);
            if (l.scale) { layer.property("ADBE Transform Group").property("ADBE Scale").setValue(l.scale); }
            if (l.rot) { layer.property("ADBE Transform Group").property("ADBE Rotate Z").setValue(l.rot); }
            if (l.opacity !== undefined) { layer.property("ADBE Transform Group").property("ADBE Opacity").setValue(l.opacity); }
            if (l.mode) { layer.blendingMode = BlendingMode[l.mode === "silhouette_alpha" ? "SILHOUETE_ALPHA" : l.mode.toUpperCase()]; }
            if (l.preserve) { layer.preserveTransparency = true; }
            if (l.masks) { addMasks(layer, l.masks); }
        }
        for (i = 0; i < c.layers.length; i++) {
            l = c.layers[i];
            if (l.matte) {
                comp.layer(i + 1).setTrackMatte(comp.layer(l.matte.layer + 1), TrackMatteType[l.matte.mode.toUpperCase()]);
            }
            if (l.enabled === false) { comp.layer(i + 1).enabled = false; }
        }
        comps[c.name] = comp;
        return comp;
    }

    var written = [];
    for (var k = 0; k < spec.cases.length; k++) {
        var c = spec.cases[k];
        if (c.hsl_grid) {
            c.layers = [];
            for (var row = 0; row < spec.hsl_pairs.length; row++) {
                var pair = spec.hsl_pairs[row];
                for (var j = 0; j < 4; j++) {
                    for (var col = 0; col < 4; col++) {
                        var pos = [4 * (col + 4*j) + 2, 4*row + 2];
                        var id = "" + row + j + col;
                        c.layers.push({name:"s"+id, solid:pair[1], size:[4,4], pos:pos, opacity:(col+1)*25, mode:c.hsl_grid});
                        c.layers.push({name:"b"+id, solid:pair[0], size:[4,4], pos:pos, opacity:(j+1)*25});
                    }
                }
            }
        }
        var comp = build(c);
        if (!c.inner) {
            var file = new File(outDir + "/" + c.name + ".png");
            if (file.exists) { file.remove(); }
            comp.saveFrameToPng(0, file);
            // The save is asynchronous; let it finish before queueing more work.
            for (var wait = 0; wait < 100 && !file.exists; wait++) { $.sleep(20); }
            written.push(c.name);
        }
    }
    var missing = [];
    for (var m = 0; m < written.length; m++) {
        if (!new File(outDir + "/" + written[m] + ".png").exists) { missing.push(written[m]); }
    }
    folder.remove();
    return "rendered " + written.length + " cases to " + outDir + (missing.length ? "; missing: " + missing.join(", ") : "");
})();
