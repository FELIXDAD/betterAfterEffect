// anim-harness.js - tests for bae-anim-samples.jsx against the mock DOM.
// The mock does not evaluate like AE (linear only, no expressions): this checks the case
// builder, the AttrSpec -> AE -> AttrSpec round trip and the output format, not the numbers.

var J = JSON;
var failures = 0;
var checks = 0;

function check(cond, message) {
    checks++;
    if (!cond) {
        failures++;
        print("FAIL: " + message);
    }
}

function eq(actual, expected, message) {
    var as = J.stringify(actual);
    var es = J.stringify(expected);
    check(as === es, message + "\n    expected " + es + "\n    actual   " + as);
}

load("mock-ae.js");
app.project = new Project();
var CASES_PATH = "/repo/bridges/ae/samples/anim-cases.json";
var OUT_PATH = "/repo/out/anim-samples.mock.json";
var casesText = readFile("../samples/anim-cases.json");
MOCK_TEXT[CASES_PATH] = casesText;
var cases = J.parse(casesText);
$.fileName = "/repo/bridges/ae/bae-anim-samples.jsx";
var evalFiles = [];
$.evalFile = function (path) {
    evalFiles.push(path);
    load("../bae-export.jsx");
};

load("es3-guard.js");
var BAE_ANIM_SAMPLES_OPTIONS = {casesPath: CASES_PATH, outputPath: OUT_PATH};
load("../bae-anim-samples.jsx");

eq(evalFiles, ["/repo/bridges/ae/bae-export.jsx"], "loads bae-export.jsx next to itself");
check(typeof BAE_ANIM_SAMPLES_OPTIONS === "undefined" || !BAE_ANIM_SAMPLES_OPTIONS, "options consumed");
check(/62 cases, 0 failed/.test($.written.join("\n")), "summary line: " + $.written.join(" | "));
eq(app._undoDepth, 0, "undo group closed");
eq(app.project.numItems, 0, "scratch comps and folder removed");

var written = MOCK_WRITES[OUT_PATH];
check(!!written, "output written");
var doc = J.parse(written.text);
eq(doc.format, "bae-anim-samples", "format");
eq(doc.results.length, cases.cases.length, "one result per case");

var byId = {};
for (var i = 0; i < doc.results.length; i++) {
    byId[doc.results[i].id] = doc.results[i];
}

/** Every field the case set must come back unchanged (the mock stores what it is given). */
function compareAttr(id, input, back) {
    if (input.value) {
        eq(back.value, input.value, id + ": static value");
    }
    if (input.expression) {
        eq(back.expression, input.expression, id + ": expression");
    }
    var inKeys = input.keyframes || {};
    var backKeys = back.keyframes || {};
    for (var t in inKeys) {
        var a = inKeys[t];
        var b = backKeys[t];
        check(!!b, id + ": keyframe at " + t + " read back");
        if (!b) {
            continue;
        }
        var v = a.value.value;
        var bv = b.value.value;
        if (a.value.type === "vec3" && b.value.type === "vec3") {
            eq(bv, v, id + ": value at " + t);
        } else {
            eq(b.value.type, a.value.type, id + ": value type at " + t);
            eq(bv, v, id + ": value at " + t);
        }
        eq(b.interp_in, a.interp_in, id + ": interp_in at " + t);
        eq(b.interp_out, a.interp_out, id + ": interp_out at " + t);
        var fields = ["spatial_in", "spatial_out", "flags"];
        for (var f = 0; f < fields.length; f++) {
            if (a[fields[f]] !== undefined) {
                eq(b[fields[f]], a[fields[f]], id + ": " + fields[f] + " at " + t);
            }
        }
        if (a.ease_in) {
            eq(b.ease_in[0], a.ease_in[0], id + ": ease_in at " + t);
            eq(b.ease_in.length, a.ease_in.length === 1 ? b.ease_in.length : a.ease_in.length, id + ": ease dims");
        }
    }
}

for (var c = 0; c < cases.cases.length; c++) {
    var cs = cases.cases[c];
    var r = byId[cs.id];
    check(!!r, cs.id + ": result present");
    if (!r) {
        continue;
    }
    check(!r.error, cs.id + ": no error: " + r.error);
    if (r.error) {
        continue;
    }
    compareAttr(cs.id, cs.attr, r.attr);
    eq(r.pre_expression.length, r.times.length, cs.id + ": one pre-expression sample per time");
    if (cs.attr.expression) {
        eq(r.post_expression.length, r.times.length, cs.id + ": one post-expression sample per time");
    } else {
        check(r.post_expression === undefined, cs.id + ": no post-expression samples without an expression");
    }
    var times = r.times;
    for (var k = 1; k < times.length; k++) {
        check(times[k] > times[k - 1], cs.id + ": times increase");
    }
    eq(r.pre_expression[0].length, r.property.dimensions, cs.id + ": sample width = dimensions");
}

eq(byId.linear_two_keys.times.length, 241, "4 s at 30 fps, 2 samples per frame, inclusive");
eq(byId.linear_two_keys.times[1], 11760000, "half-frame step at 30 fps in flicks");
eq(byId.comp_29_97.times[2], 23543520, "29.97 frame in flicks");
eq(byId.keys_every_frame.times.length, 97, "custom sample range");
eq(byId.linear_two_keys.pre_expression[60], [180], "mock linear midpoint (t = 1 s)");
eq(byId.color_linear.property.value_type, "color", "color property type");
eq(byId.spatial_3d.layer.three_d, true, "3D layer flag");
eq(byId.layer_start_offset.layer.start_time, 705600000, "layer start time");
eq(byId.scale_per_dimension_ease.attr.keyframes["0"].ease_out.length, 3, "3 eases for ThreeD scale");

print((failures ? "FAILED: " + failures : "OK") + " (" + checks + " checks)");
// --dump <path>: keep the mock output, e.g. to validate it against the JSON Schema.
var ARGS = typeof HARNESS_ARGS !== "undefined" ? HARNESS_ARGS : (typeof arguments !== "undefined" ? arguments : []);
for (var ai = 0; ai < ARGS.length - 1; ai++) {
    if (ARGS[ai] === "--dump") {
        writeFile(ARGS[ai + 1], written.text);
    }
}
if (failures && typeof quit === "function") {
    quit(1);
}
