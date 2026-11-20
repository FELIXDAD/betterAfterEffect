// Renders the colour-management comparison cases in After Effects.
// Run with: tools/ae/aectl run crates/bae-color/tools/ae/render_cases.jsx
// Inputs: /private/tmp/bae-color-ae/chart_*.png (make_charts.py).
// Outputs: /private/tmp/bae-color-ae/out/<case>_00000.png (16-bit) and
// cases.json describing each case.
var DIR = "/private/tmp/bae-color-ae";
var CASES = [
    // name, engine (0 Adobe / 1 OCIO), working space, linearize, blend 1.0, compensate, chart
    ["a_srgb_identity", 0, "sRGB IEC61966-2.1", false, false, true, "srgb"],
    ["a_untagged_in_srgb", 0, "sRGB IEC61966-2.1", false, false, true, "untagged"],
    ["a_p3_to_srgb", 0, "sRGB IEC61966-2.1", false, false, true, "p3"],
    ["a_rec709_to_srgb_comp", 0, "sRGB IEC61966-2.1", false, false, true, "rec709"],
    ["a_rec709_to_srgb_nocomp", 0, "sRGB IEC61966-2.1", false, false, false, "rec709"],
    ["a_srgb_linearized", 0, "sRGB IEC61966-2.1", true, false, true, "srgb"],
    ["a_srgb_to_g24", 0, "Rec.709 Gamma 2.4", false, false, true, "srgb"],
    ["a_srgb_to_acescg", 0, "ACES CG Linear (Academy Color Encoding System AP1)", false, false, true, "srgb"],
    ["a_srgb_to_p3", 0, "Display P3", false, false, true, "srgb"],
    ["a_srgb_to_pq_w100", 0, "Rec.2100 PQ W100", false, false, true, "srgb"],
    ["a_srgb_to_pq_w203", 0, "Rec.2100 PQ W203", false, false, true, "srgb"],
    ["a_pq_to_srgb", 0, "sRGB IEC61966-2.1", false, false, true, "pq2020"],
    ["a_rec2020_to_srgb", 0, "sRGB IEC61966-2.1", false, false, true, "rec2020"]
    // OCIO cases are not rendered: in OCIO mode AE 26.2 exposes neither the
    // working nor the output colour space to scripts. The OCIO maths is
    // checked against the OCIO library with AE's own configs instead
    // (tests/ocio.rs, tools/gen_ocio_vectors.py).
];

function dumpColorProps(p) {
    var out = [];
    var ps = p.reflect.properties;
    for (var i = 0; i < ps.length; i++) {
        var n = ps[i].name;
        if (/olor|ocio|OCIO|orking|isplay|inear/i.test(n)) { try { out.push(n + "=" + p[n]); } catch (e) { out.push(n + "=ERR"); } }
    }
    return out.join("; ");
}

function render(c) {
    if (c[1] === 1) {
        // OCIO cases get a fresh project so the engine switch applies before import.
        app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES);
        app.newProject();
    }
    var p = app.project;
    p.bitsPerChannel = 32;
    p.colorManagementSystem = c[1];
    if (c[1] === 1) {
        p.ocioConfigurationFile = "ACES 1.3 CG v1.0";
    } else {
        p.workingSpace = c[2];
        p.linearizeWorkingSpace = c[3];
        p.linearBlending = c[4];
        p.compensateForSceneReferredProfiles = c[5];
    }
    var f = p.importFile(new ImportOptions(new File(DIR + "/chart_" + c[6] + ".png")));
    var comp = p.items.addComp(c[0], 32, 8, 1, 1, 24);
    comp.layers.add(f);
    var rq = p.renderQueue.items.add(comp);
    rq.timeSpanStart = 0;
    rq.timeSpanDuration = comp.frameDuration;
    var om = rq.outputModule(1);
    // Format / Depth are read-only for scripts; this hidden template is a
    // 16-bit PNG sequence (RGBA, straight).
    om.applyTemplate("_HIDDEN X-Factor 16");
    om.file = new File(DIR + "/out/" + c[0] + "_[#####].png");
    p.renderQueue.render();
    rq.remove();
    return {
        name: c[0],
        engine: p.colorManagementSystem,
        working: c[1] === 1 ? "ocio" : p.workingSpace,
        ocio: p.ocioConfigurationFile,
        linearize: p.linearizeWorkingSpace,
        blend1: p.linearBlending,
        compensate: p.compensateForSceneReferredProfiles,
        chart: c[6],
        props: dumpColorProps(p)
    };
}

if (app.project) app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES);
app.newProject();
app.beginSuppressDialogs();
var outDir = new Folder(DIR + "/out");
if (!outDir.exists) outDir.create();
var results = [];
for (var i = 0; i < CASES.length; i++) {
    try { results.push(render(CASES[i])); } catch (e) { results.push({ name: CASES[i][0], error: String(e) }); }
}
app.endSuppressDialogs(false);
app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES);
var jf = new File(DIR + "/cases.json");
jf.open("w"); jf.write(results.toSource()); jf.close();
results.toSource();
