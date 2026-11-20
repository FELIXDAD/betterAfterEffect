// run-in-ae.jsx - one-shot real-AE test: builds the README T1 scene and the T4 time-base
// comps by script, exports them with bae-export.jsx, runs the bae-anim-samples cases, and
// writes everything plus a step log to an output folder. Everything is built inside the
// open project (one undo group) and removed afterwards unless keep is set.
//
// Run from a shell (macOS), passing the output folder:
//   osascript -e 'tell application "Adobe After Effects 2026" to DoScript
//     "var BAE_AE_TEST_OPTIONS = {outDir: \"/tmp/bae-ae\"}; $.evalFile(\"<repo>/bridges/ae/tests/ae/run-in-ae.jsx\");"'
// Options: outDir (required when run headless), keep (leave the built comps), skipSamples.
// Output: <outDir>/log.txt, fixture.bae-interchange.json, anim-samples.ae-<major>.json, result.json.

(function (global) {
    var opts = (typeof BAE_AE_TEST_OPTIONS !== "undefined" && BAE_AE_TEST_OPTIONS) || {};
    global.BAE_AE_TEST_OPTIONS = undefined;
    var here = new File($.fileName).parent;
    var bridge = here.parent.parent;
    // Folder.create() fails through symlinks, and /tmp is one on macOS: use the real path.
    var outPath = String(opts.outDir || (Folder.temp.fsName + "/bae-ae-test")).replace(/^\/tmp(\/|$)/, "/private/tmp$1");
    var outDir = new Folder(outPath);
    if (!outDir.exists && !outDir.create()) {
        $.writeln("run-in-ae: cannot create " + outPath);
        return;
    }
    var logFile = new File(outDir.fsName + "/log.txt");
    logFile.encoding = "UTF-8";
    logFile.lineFeed = "Unix";  // the macOS default is CR
    logFile.open("w");
    logFile.close();
    var steps = [];
    var started = new Date().getTime();

    function log(s) {
        var line = ((new Date().getTime() - started) / 1000).toFixed(2) + "s " + s;
        // Open, append, close per line: if AE is killed mid-run the log still shows how far it got.
        logFile.open("a");
        logFile.writeln(line);
        logFile.close();
        $.writeln(line);
    }

    /** Run fn, record ok/error, never throw: one broken AE call must not hide the rest. */
    function step(name, fn) {
        try {
            var r = fn();
            steps.push({step: name, ok: true});
            log("ok   " + name);
            return r;
        } catch (e) {
            var msg = String(e) + (e.line ? " (line " + e.line + ")" : "");
            steps.push({step: name, ok: false, error: msg});
            log("FAIL " + name + ": " + msg);
            return undefined;
        }
    }

    // A modal dialog would hang the calling osascript until it times out: suppress AE's own
    // dialogs and never let an exception escape (AE shows uncaught script errors in a dialog).
    app.beginSuppressDialogs();
    try {
        runAll();
    } catch (fatal) {
        log("FATAL " + fatal + (fatal.line ? " (line " + fatal.line + ")" : ""));
        steps.push({step: "fatal", ok: false, error: String(fatal)});
    }
    try {
        writeResult();
    } catch (e) {
        log("cannot write result.json: " + e);
    }
    app.endSuppressDialogs(false);

    function runAll() {
        log("AE " + app.version + " / " + $.os + " / " + app.isoLanguage);
        BAE_EXPORT_NO_AUTORUN = true;
        $.evalFile(bridge.fsName + "/bae-export.jsx");
        BAE_ANIM_SAMPLES_NO_AUTORUN = true;
        $.evalFile(bridge.fsName + "/bae-anim-samples.jsx");
        var B = BAE_EXPORT;
        var K = KeyframeInterpolationType;

        function ease(speed, influence) {
            return new KeyframeEase(speed, influence);
        }

        function prop(group, path) {
            var p = group;
            for (var i = 0; i < path.length; i++) {
                p = p.property(path[i]);
                if (!p) {
                    throw new Error("missing property " + path.slice(0, i + 1).join(" / "));
                }
            }
            return p;
        }

        app.beginUndoGroup("BAE AE test");
        var folder = app.project.items.addFolder("BAE AE test");
        var made = [];
        var main;
        var pre;
        try {
            // ------------------------------------------------------------ T1 scene
            pre = step("create Pre comp", function () {
                var c = app.project.items.addComp("Pre comp", 1920, 1080, 1, 5, 30000 / 1001);
                c.parentFolder = folder;
                made.push(c);
                return c;
            });
            main = step("create Main Comp", function () {
                var c = app.project.items.addComp("Main Comp", 1920, 1080, 1, 10.01, 30000 / 1001);
                c.parentFolder = folder;
                c.bgColor = [0.1, 0.2, 0.3];
                c.comment = "hero shot";
                made.push(c);
                return c;
            });
            var title = step("text layer with animator", function () {
                var l = pre.layers.addText("Hello\rWorld");
                l.name = "Title \u6807\u9898";
                var st = prop(l, ["ADBE Text Properties", "ADBE Text Document"]);
                var td = st.value;
                td.fontSize = 72;
                td.justification = ParagraphJustification.CENTER_JUSTIFY;
                st.setValueAtTime(0, td);
                td.text = "\u4F60\u597D";
                st.setValueAtTime(1.001, td);
                var animator = prop(l, ["ADBE Text Properties", "ADBE Text Animators"]).addProperty("ADBE Text Animator");
                animator.property("ADBE Text Animator Properties").addProperty("ADBE Text Opacity").setValue(0);
                var sel = animator.property("ADBE Text Selectors").addProperty("ADBE Text Selector");
                var start = sel.property("ADBE Text Percent Start");
                start.setValueAtTime(0, 0);
                start.setValueAtTime(1, 100);
                return l;
            });
            step("text path mask", function () {
                var m = title.property("ADBE Mask Parade").addProperty("ADBE Mask Atom");
                m.name = "Path Mask";
                var s = new Shape();
                s.vertices = [[0, 0], [600, 0]];
                s.inTangents = [[0, 0], [-200, 0]];
                s.outTangents = [[200, 0], [0, 0]];
                s.closed = false;
                m.property("ADBE Mask Shape").setValue(s);
                m.maskMode = MaskMode.NONE;
                prop(title, ["ADBE Text Properties", "ADBE Text Path Options", "ADBE Text Path"]).setValue(1);
            });

            // Main comp layers are added bottom-up (AE inserts new layers at the top).
            var solid = step("solid with masks and effects", function () {
                var l = main.layers.addSolid([1, 0, 0], "Red Solid 1", 1920, 1080, 1);
                var masks = l.property("ADBE Mask Parade");
                var m1 = masks.addProperty("ADBE Mask Atom");
                m1.name = "Mask 1";
                var s = new Shape();
                s.vertices = [[100, 100], [900, 100], [900, 700], [100, 700]];
                s.closed = true;
                var mp = m1.property("ADBE Mask Shape");
                mp.setValueAtTime(0, s);
                mp.setValueAtTime(1, s);
                m1.maskMode = MaskMode.ADD;
                m1.inverted = true;
                var m2 = masks.addProperty("ADBE Mask Atom");
                m2.name = "Mask 1";
                m2.property("ADBE Mask Shape").setValue(s);
                var fx = l.property("ADBE Effect Parade");
                fx.addProperty("ADBE Gaussian Blur 2").property("ADBE Gaussian Blur 2-0001").setValue(12.5);
                fx.addProperty("ADBE Set Matte3");
                fx.addProperty("ADBE CurvesCustom");
                return l;
            });
            step("mask feather point", function () {
                var mp = solid.property("ADBE Mask Parade").property(1).property("ADBE Mask Shape");
                var s = mp.keyValue(1);
                s.featherSegLocs = [0];
                s.featherRelSegLocs = [0.5];
                s.featherRadii = [20];
                mp.setValueAtKey(1, s);
            });
            var preLayer = step("pre comp layer (start 1 s, stretch 50%)", function () {
                var l = main.layers.add(pre);
                l.startTime = 1;
                l.stretch = 50;
                return l;
            });
            step("track matte", function () {
                if (typeof solid.setTrackMatte === "function") {
                    solid.setTrackMatte(preLayer, TrackMatteType.ALPHA);
                } else {
                    solid.trackMatteType = TrackMatteType.ALPHA;
                }
            });
            var box = step("shape layer contents", function () {
                var l = main.layers.addShape();
                l.name = "Box";
                var root = l.property("ADBE Root Vectors Group");
                var group = root.addProperty("ADBE Vector Group");
                group.name = "Rectangle 1";
                var contents = group.property("ADBE Vectors Group");
                var rect = contents.addProperty("ADBE Vector Shape - Rect");
                rect.property("ADBE Vector Rect Size").setValue([200, 120]);
                rect.property("ADBE Vector Rect Roundness").setValue(12);
                contents.addProperty("ADBE Vector Graphic - Fill");
                contents.addProperty("ADBE Vector Graphic - G-Fill");
                var path = root.addProperty("ADBE Vector Shape - Group");
                var s = new Shape();
                s.vertices = [[0, 0], [50, 50], [0, 100]];
                s.inTangents = [[0, 0], [0, -20], [0, 0]];
                s.outTangents = [[0, 0], [0, 20], [0, 0]];
                s.closed = true;
                path.property("ADBE Vector Shape").setValue(s);
                var trim = root.addProperty("ADBE Vector Filter - Trim");
                trim.property("ADBE Vector Trim End").setValueAtTime(0, 0);
                trim.property("ADBE Vector Trim End").setValueAtTime(2, 100);
                root.addProperty("ADBE Vector Filter - Repeater");
                return l;
            });
            step("position keys: ease, tangents, roving, hold, label", function () {
                var p = prop(box, ["ADBE Transform Group", "ADBE Position"]);
                p.setValueAtTime(0, [200, 540, 0]);
                p.setValueAtTime(1.001, [1000, 300, 0]);
                p.setValueAtTime(2.002, [1720, 540, 0]);
                p.setInterpolationTypeAtKey(1, K.BEZIER, K.BEZIER);
                p.setInterpolationTypeAtKey(2, K.BEZIER, K.BEZIER);
                p.setInterpolationTypeAtKey(3, K.LINEAR, K.HOLD);
                p.setTemporalEaseAtKey(1, [ease(0, 33.333333)], [ease(0, 33.333333)]);
                p.setTemporalEaseAtKey(2, [ease(812.5, 50)], [ease(812.5, 50)]);
                p.setSpatialTangentsAtKey(1, [0, 0, 0], [100, -50, 0]);
                p.setSpatialTangentsAtKey(2, [-80, 0, 0], [80, 0, 0]);
                p.setSpatialContinuousAtKey(2, true);
                p.setRovingAtKey(2, true);
                try {
                    p.setLabelAtKey(3, 4);
                } catch (e) {
                    log("note: setLabelAtKey unavailable: " + e);
                }
            });
            step("scale keys with per-dimension ease", function () {
                var p = prop(box, ["ADBE Transform Group", "ADBE Scale"]);
                p.setValueAtTime(0, [0, 0, 100]);
                p.setValueAtTime(0.5005, [120, 80, 100]);
                p.setInterpolationTypeAtKey(1, K.BEZIER, K.BEZIER);
                p.setInterpolationTypeAtKey(2, K.BEZIER, K.BEZIER);
                p.setTemporalEaseAtKey(1, [ease(0, 10), ease(0, 20), ease(0, 30)], [ease(0, 75), ease(10, 80), ease(0, 33.3)]);
            });
            step("expressions (one valid, one broken)", function () {
                prop(box, ["ADBE Transform Group", "ADBE Opacity"]).expression = "wiggle(2, 20)";
                prop(box, ["ADBE Transform Group", "ADBE Rotate Z"]).expression = "time * 36 +";
            });
            step("layer marker", function () {
                var mv = new MarkerValue("layer marker");
                try {
                    mv.protectedRegion = true;
                } catch (e) {
                    log("note: protectedRegion unavailable: " + e);
                }
                box.property("ADBE Marker").setValueAtTime(0.5, mv);
            });
            var ctrl = step("null Controller as parent", function () {
                var l = main.layers.addNull();
                l.name = "Controller";
                box.parent = l;
                return l;
            });
            step("Set Matte takes its matte from Controller", function () {
                solid.property("ADBE Effect Parade").property("ADBE Set Matte3").property("ADBE Set Matte3-0001").setValue(ctrl.index);
            });
            step("camera and light", function () {
                main.layers.addLight("Light 1", [960, 540]).lightType = LightType.SPOT;
                main.layers.addCamera("Camera 1", [960, 540]);
            });
            step("comp markers", function () {
                var m1 = new MarkerValue("intro");
                m1.duration = 0.5;
                m1.chapter = "Ch1";
                main.markerProperty.setValueAtTime(1.001, m1);
                var m2 = new MarkerValue("beat");
                m2.setParameters({"cue type": "hit", "power": "2"});
                main.markerProperty.setValueAtTime(2.002, m2);
            });

            // ------------------------------------------------------------ T4 time base
            var t4 = [["T4 start 1s", 100], ["T4 stretch 200", 200]];
            for (var i = 0; i < t4.length; i++) {
                (function (name, stretch) {
                    step(name, function () {
                        var c = app.project.items.addComp(name, 640, 360, 1, 6, 30);
                        c.parentFolder = folder;
                        made.push(c);
                        var l = c.layers.addNull();
                        l.startTime = 1;
                        l.stretch = stretch;
                        var r = prop(l, ["ADBE Transform Group", "ADBE Rotate Z"]);
                        r.setValueAtTime(2, 45);
                        // What the scripting API reports, recorded next to the export for comparison.
                        log("T4 " + name + ": keyTime(1) = " + r.keyTime(1) + " s (comp time 2 s, layer time " +
                            ((2 - 1) * 100 / stretch) + " s)");
                    });
                }(t4[i][0], t4[i][1]));
            }

            // ------------------------------------------------------------ export
            step("export selected test comps", function () {
                for (var j = 1; j <= app.project.numItems; j++) {
                    app.project.item(j).selected = false;
                }
                for (var k = 0; k < made.length; k++) {
                    made[k].selected = true;
                }
                var doc = B.exportProject({outputPath: outDir.fsName + "/fixture.bae-interchange.json",
                    selectedCompsOnly: true, pretty: true});
                var s = doc.report.stats;
                log("export: " + s.comps + " comps, " + s.layers + " layers, " + s.keyframes + " keyframes, " +
                    doc.report.warnings.length + " warnings, " + doc.report.unsupported.length + " unsupported, " +
                    s.seconds + " s");
            });
        } finally {
            if (!opts.keep) {
                for (var m = made.length - 1; m >= 0; m--) {
                    try {
                        made[m].remove();
                    } catch (e) {
                        log("cleanup: " + e);
                    }
                }
                try {
                    folder.remove();
                } catch (e2) {
                    log("cleanup: " + e2);
                }
            }
            app.endUndoGroup();
        }

        // ------------------------------------------------------------ T8 samples
        if (!opts.skipSamples) {
            step("anim samples", function () {
                var major = String(app.version).split(".")[0];
                var r = BAE_ANIM_SAMPLES.run({casesPath: bridge.fsName + "/samples/anim-cases.json",
                    outputPath: outDir.fsName + "/anim-samples.ae-" + major + ".json",
                    progress: function (id) {
                        log("  case " + id);
                    }});
                log("samples: " + r.doc.results.length + " cases, " + r.failed + " failed");
                for (var i = 0; i < r.doc.results.length; i++) {
                    if (r.doc.results[i].error) {
                        log("  case " + r.doc.results[i].id + ": " + r.doc.results[i].error);
                    }
                }
            });
        }
    }

    function writeResult() {
        var B = typeof BAE_EXPORT !== "undefined" ? BAE_EXPORT : null;
        var failed = 0;
        for (var f = 0; f < steps.length; f++) {
            if (!steps[f].ok) {
                failed++;
            }
        }
        var summary = {ae_version: String(app.version), os: String($.os), language: String(app.isoLanguage),
            seconds: (new Date().getTime() - started) / 1000, failed_steps: failed, steps: steps};
        var result = new File(outDir.fsName + "/result.json");
        result.encoding = "UTF-8";
        result.lineFeed = "Unix";
        result.open("w");
        // bae-export.jsx may have failed to load; fall back to a minimal hand-written summary.
        result.write(B ? B.stringify(summary, "  ") : "{\"failed_steps\": " + failed + ", \"export_script_loaded\": false}");
        result.close();
        log("done: " + steps.length + " steps, " + failed + " failed");
    }
}(this));
