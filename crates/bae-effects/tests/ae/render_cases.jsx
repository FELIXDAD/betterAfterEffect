// Renders every case of cases.json in After Effects (run with tools/ae/aectl on
// a machine that has AE):
//
//   tools/ae/aectl run crates/bae-effects/tests/ae/render_cases.jsx
//
// Inputs come from `cargo run -p bae-effects --example ae_compare -- prepare`
// (/private/tmp/bae-07-ae/inputs/*.png); outputs go to /private/tmp/bae-07-ae/ae/.
// 8 bpc project, no linearized working space, no colour management, frame 0,
// layer quality Best. Parameters are set by matchName ("<effect>-NNNN").
// No alert(): results are returned as a string.

(function () {
    // A wrapper script may set BAE_07_ROOT (probe runs).
    var ROOT = (typeof BAE_07_ROOT !== "undefined") ? BAE_07_ROOT : "/private/tmp/bae-07-ae";
    var casesFile = new File(ROOT + "/cases.json");
    if (!casesFile.exists) {
        throw new Error("run `ae_compare prepare` first (copies cases.json to " + ROOT + ")");
    }
    casesFile.open("r");
    var cases = eval("(" + casesFile.read() + ")").cases;
    casesFile.close();
    var outDir = new Folder(ROOT + "/ae");
    if (!outDir.exists && !outDir.create()) {
        throw new Error("cannot create " + outDir.fsName);
    }

    // newProject() asks to save an unsaved project (a modal dialog): close it first.
    if (app.project) {
        app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES);
    }
    app.newProject();
    app.project.bitsPerChannel = 8;
    app.project.linearBlending = false;
    try { app.project.workingSpace = ""; } catch (ignored) {}

    var footage = {};
    function input(name) {
        if (!footage[name]) {
            var io = new ImportOptions(new File(ROOT + "/inputs/" + name + ".png"));
            footage[name] = app.project.importFile(io);
            footage[name].mainSource.alphaMode = AlphaMode.STRAIGHT;
        }
        return footage[name];
    }

    // saveFrameToPng returns before the file is written; poll with a fresh File
    // object each time (a File caches exists/length).
    function waitFor(path) {
        for (var i = 0; i < 600; i++) {
            var probe = new File(path);
            if (probe.exists && probe.length > 0) {
                return true;
            }
            $.sleep(50);
        }
        return false;
    }

    var done = [];
    for (var i = 0; i < cases.length; i++) {
        var c = cases[i];
        app.project.bitsPerChannel = c.bpc || 8;
        var src = input(c.input);
        var comp = app.project.items.addComp(c.name, src.width, src.height, 1, 1, 30);
        var layer = comp.layers.add(src);
        layer.quality = LayerQuality.BEST;
        var fx = layer.property("ADBE Effect Parade").addProperty(c.effect);
        for (var id in c.params) {
            var pad = ("0000" + id).slice(-4);
            var prop = fx.property(c.effect + "-" + pad);
            if (prop === null) {
                throw new Error(c.name + ": no parameter " + c.effect + "-" + pad);
            }
            prop.setValue(c.params[id]);
        }
        var out = new File(outDir.fsName + "/" + c.name + ".png");
        if (out.exists) {
            out.remove();
        }
        comp.saveFrameToPng(0, out);
        if (!waitFor(out.fsName)) {
            throw new Error(c.name + ": saveFrameToPng timed out");
        }
        done.push(c.name);
    }
    return "rendered " + done.length + " cases: " + done.join(", ");
})();
