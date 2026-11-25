// Builds every case of cases.json in After Effects and renders frame `frame`
// (default 0) of each to PNG (comp.saveFrameToPng: RGBA, premultiplied). Run with
// tools/ae/aectl:
//
//   tools/ae/aectl run crates/bae-render/tests/ae/render_cases.jsx
//
// Only some cases (comma-separated name prefixes; inner comps are always built):
//
//   tools/ae/aectl eval 'var BAE_AE_ONLY="threed_,mb_"; $.evalFile("<repo>/crates/bae-render/tests/ae/render_cases.jsx")'
//
// Inputs:  cases.json next to this script; footage in BAE_AE_ASSETS
//          (default /private/tmp/bae-04-ae/assets, written by the ae_compare example).
// Outputs: BAE_AE_OUT/<case>.png (default /private/tmp/bae-04-ae/ae).
// Everything is created inside a project folder that is removed afterwards.

(function () {
    var here = File($.fileName).parent;
    var assets = (typeof BAE_AE_ASSETS !== "undefined") ? BAE_AE_ASSETS : "/private/tmp/bae-04-ae/assets";
    var outDir = (typeof BAE_AE_OUT !== "undefined") ? BAE_AE_OUT : "/private/tmp/bae-04-ae/ae";
    var only = (typeof BAE_AE_ONLY !== "undefined" && BAE_AE_ONLY) ? BAE_AE_ONLY.split(",") : null;
    function wanted(c) {
        if (!only || c.inner) { return true; }
        for (var q = 0; q < only.length; q++) {
            if (c.name.indexOf(only[q]) === 0) { return true; }
        }
        return false;
    }
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

    // 3D layers and cameras take 3-component positions, scales and points.
    function v3(v, z) {
        return v.length >= 3 ? v : [v[0], v[1], z];
    }
    // Linear keys (temporal and spatial), like BAE's Keyframe::linear.
    function setKeys(prop, keys, threeD) {
        for (var n = 0; n < keys.length; n++) {
            var val = keys[n][1];
            prop.setValueAtTime(keys[n][0] / 30, (val instanceof Array && threeD) ? v3(val, 0) : val);
        }
        for (var m = 1; m <= prop.numKeys; m++) {
            prop.setInterpolationTypeAtKey(m, KeyframeInterpolationType.LINEAR, KeyframeInterpolationType.LINEAR);
            if (prop.isSpatial) {
                prop.setSpatialAutoBezierAtKey(m, false);
                prop.setSpatialContinuousAtKey(m, false);
                // AE stores Position with three components even on 2D layers.
                var zero = [];
                for (var c = 0; c < prop.keyValue(m).length; c++) { zero.push(0); }
                prop.setSpatialTangentsAtKey(m, zero, zero);
            }
        }
    }

    var comps = {};
    function build(c) {
        var comp = proj.items.addComp(c.name, c.size[0], c.size[1], 1.0, 1.0, 30);
        comp.parentFolder = folder;
        comp.bgColor = [0, 0, 0];
        comp.resolutionFactor = [1, 1];
        if (c.motion_blur) {
            comp.motionBlur = true;
            comp.shutterAngle = c.motion_blur.angle;
            comp.shutterPhase = c.motion_blur.phase;
            comp.motionBlurSamplesPerFrame = c.motion_blur.samples;
        }
        var i, l, layer;
        // AE adds new layers on top: add bottom-first so layers[0] ends up as index 1.
        for (i = c.layers.length - 1; i >= 0; i--) {
            l = c.layers[i];
            if (l.camera) {
                layer = comp.layers.addCamera(l.name, [c.size[0] / 2, c.size[1] / 2]);
                if (l.camera.type === "one_node") { layer.autoOrient = AutoOrientType.NO_AUTO_ORIENT; }
                layer.property("ADBE Camera Options Group").property("ADBE Camera Zoom").setValue(l.camera.zoom);
                if (l.poi) { layer.property("ADBE Transform Group").property("ADBE Anchor Point").setValue(l.poi); }
            } else if (l.light) {
                layer = comp.layers.addLight(l.name, [c.size[0] / 2, c.size[1] / 2]);
                var lt = l.light;
                layer.lightType = {ambient: LightType.AMBIENT, point: LightType.POINT, spot: LightType.SPOT, parallel: LightType.PARALLEL}[lt.type];
                var lo = layer.property("ADBE Light Options Group");
                lo.property("ADBE Light Intensity").setValue(lt.intensity !== undefined ? lt.intensity : 100);
                lo.property("ADBE Light Color").setValue(lt.color || [1, 1, 1]);
                if (lt.shadows) {
                    lo.property("ADBE Casts Shadows").setValue(1);
                    if (lt.dark !== undefined) { lo.property("ADBE Light Shadow Darkness").setValue(lt.dark); }
                    if (lt.diff !== undefined) { lo.property("ADBE Light Shadow Diffusion").setValue(lt.diff); }
                }
                if (lt.cone !== undefined) { lo.property("ADBE Light Cone Angle").setValue(lt.cone); }
                if (lt.feather !== undefined) { lo.property("ADBE Light Cone Feather 2").setValue(lt.feather); }
                if (lt.falloff) {
                    lo.property("ADBE Light Falloff Type").setValue({none: 1, smooth: 2, inverse_square_clamped: 3}[lt.falloff]);
                    if (lt.radius !== undefined) { lo.property("ADBE Light Falloff Start").setValue(lt.radius); }
                    if (lt.dist !== undefined) { lo.property("ADBE Light Falloff Distance").setValue(lt.dist); }
                }
                if (l.poi) { layer.property("ADBE Transform Group").property("ADBE Anchor Point").setValue(l.poi); }
                if (l.adjustment) { layer.adjustmentLayer = true; }
            } else if (l.solid) {
                layer = comp.layers.addSolid([l.solid[0] / 255, l.solid[1] / 255, l.solid[2] / 255],
                    c.name + "_" + l.name, l.size[0], l.size[1], 1.0);
                layer.source.parentFolder = folder;
            } else if (l.sequence) {
                var io = new ImportOptions(new File(assets + "/" + l.sequence));
                io.sequence = true;
                var seq = proj.importFile(io);
                seq.parentFolder = folder;
                seq.mainSource.conformFrameRate = l.seq_rate || 10;
                seq.mainSource.alphaMode = AlphaMode.STRAIGHT;
                layer = comp.layers.add(seq);
            } else if (l.footage) {
                layer = comp.layers.add(importFootage(l.footage));
            } else if (l.comp) {
                layer = comp.layers.add(comps[l.comp]);
            }
            layer.name = l.name;
            var threeD = !!(l.three_d || l.camera || l.light);
            if (!l.camera && !l.light) { layer.quality = LayerQuality.BEST; }
            if (l.three_d) { layer.threeDLayer = true; }
            if (l.mb) { layer.motionBlur = true; }
            var tg = layer.property("ADBE Transform Group");
            var keys = l.keys || {};
            if (keys.pos) {
                setKeys(tg.property("ADBE Position"), keys.pos, threeD);
            } else if (l.pos) {
                tg.property("ADBE Position").setValue(threeD ? v3(l.pos, 0) : l.pos);
            }
            if (l.scale) { tg.property("ADBE Scale").setValue(threeD ? v3(l.scale, 100) : l.scale); }
            if (l.rot) { tg.property("ADBE Rotate Z").setValue(l.rot); }
            if (keys.rot) { setKeys(tg.property("ADBE Rotate Z"), keys.rot, false); }
            if (l.rot3) {
                tg.property("ADBE Rotate X").setValue(l.rot3[0]);
                tg.property("ADBE Rotate Y").setValue(l.rot3[1]);
                tg.property("ADBE Rotate Z").setValue(l.rot3[2]);
            }
            if (l.orient) { tg.property("ADBE Orientation").setValue(l.orient); }
            if (l.stretch !== undefined) { layer.stretch = l.stretch; }
            if (l.start_time !== undefined) { layer.startTime = l.start_time; }
            if (l.remap) {
                layer.timeRemapEnabled = true;
                var tr = layer.property("ADBE Time Remapping");
                // Time Remap cannot lose all its keys: add ours, then drop the
                // two default ones (where ours are not).
                var olds = [];
                for (var ok = 1; ok <= tr.numKeys; ok++) { olds.push(tr.keyTime(ok)); }
                var ours = [];
                for (var rk = 0; rk < l.remap.length; rk++) {
                    ours.push(l.remap[rk][0] / 30);
                    tr.setValueAtTime(l.remap[rk][0] / 30, l.remap[rk][1]);
                }
                for (var od = olds.length - 1; od >= 0; od--) {
                    var mine = false;
                    for (var oc = 0; oc < ours.length; oc++) { if (Math.abs(ours[oc] - olds[od]) < 1e-6) { mine = true; } }
                    if (!mine) { tr.removeKey(tr.nearestKeyIndex(olds[od])); }
                }
                for (var rm = 1; rm <= tr.numKeys; rm++) {
                    var it = l.remap_hold ? KeyframeInterpolationType.HOLD : KeyframeInterpolationType.LINEAR;
                    tr.setInterpolationTypeAtKey(rm, it, it);
                }
                // The layer may run past the source with remapping on.
                layer.outPoint = comp.duration;
            }
            if (l.mat) {
                var mo = layer.property("ADBE Material Options Group");
                if (l.mat.amb !== undefined) { mo.property("ADBE Ambient Coefficient").setValue(l.mat.amb); }
                if (l.mat.diff !== undefined) { mo.property("ADBE Diffuse Coefficient").setValue(l.mat.diff); }
                if (l.mat.spec !== undefined) { mo.property("ADBE Specular Coefficient").setValue(l.mat.spec); }
                if (l.mat.shin !== undefined) { mo.property("ADBE Shininess Coefficient").setValue(l.mat.shin); }
                if (l.mat.metal !== undefined) { mo.property("ADBE Metal Coefficient").setValue(l.mat.metal); }
                if (l.mat.accepts === false) { mo.property("ADBE Accepts Lights").setValue(0); }
                if (l.mat.casts !== undefined) { mo.property("ADBE Casts Shadows").setValue(l.mat.casts ? 1 : 0); }
                if (l.mat.accepts_shadows !== undefined) { mo.property("ADBE Accepts Shadows").setValue(l.mat.accepts_shadows ? 1 : 0); }
                if (l.mat.trans !== undefined) { mo.property("ADBE Light Transmission").setValue(l.mat.trans); }
            }
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
        if (!wanted(c)) { continue; }
        var comp = build(c);
        if (!c.inner) {
            var file = new File(outDir + "/" + c.name + ".png");
            if (file.exists) { file.remove(); }
            comp.saveFrameToPng((c.frame || 0) / 30, file);
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
