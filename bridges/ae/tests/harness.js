// harness.js - tests for bae-export.jsx without After Effects.
// Host functions (provided by jsc, or by node-run.js under node): load, print, readFile, writeFile.
// Run from this directory: ./run.sh            (add --update to rewrite the golden example)

var ARGS = typeof HARNESS_ARGS !== "undefined" ? HARNESS_ARGS : (typeof arguments !== "undefined" ? arguments : []);
var UPDATE = false;
for (var a = 0; a < ARGS.length; a++) {
    if (ARGS[a] === "--update") {
        UPDATE = true;
    }
}
var J = JSON; // kept before es3-guard removes it
var GOLDEN = "../examples/mock-project.bae-interchange.json";
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
load("fixture-project.js");

// ---------------------------------------------------------------- ES3 syntax lint (source text)

(function () {
    var src = readFile("../bae-export.jsx");
    var lines = src.split("\n");
    var bad = [
        [/\b(let|const|class)\s/, "ES6 declaration"],
        [/=>/, "arrow function"],
        [/`/, "template literal"],
        [/\.\.\.[A-Za-z_$]/, "spread"],
        [/,\s*[}\]]/, "trailing comma"]
    ];
    for (var i = 0; i < lines.length; i++) {
        // Drop comments: whole-line //, /** ... */ and " * " continuation lines, trailing // comments.
        var line = lines[i].replace(/^\s*(\/\/|\/\*|\*).*$/, "").replace(/\s\/\/ .*$/, "");
        for (var b = 0; b < bad.length; b++) {
            check(!bad[b][0].test(line), "bae-export.jsx:" + (i + 1) + ": " + bad[b][1] + ": " + lines[i]);
        }
    }
    var nonAscii = /[^\x00-\x7f]/;
    for (var k = 0; k < lines.length; k++) {
        check(!nonAscii.test(lines[k]), "bae-export.jsx:" + (k + 1) + ": non-ASCII character (escape it as \\uXXXX)");
    }
}());

load("es3-guard.js");
var BAE_EXPORT_NO_AUTORUN = true;
load("../bae-export.jsx");
var B = BAE_EXPORT;
check(typeof BAE_EXPORT_NO_AUTORUN === "undefined" || !BAE_EXPORT_NO_AUTORUN, "no-autorun flag is consumed");

// ---------------------------------------------------------------- unit tests

(function () {
    var cases = [
        ["ADBE Transform Group", "ADBE_Transform_Group"],
        ["ADBE Position", "ADBE_Position"],
        ["ADBE Position_0", "ADBE_Position-5F0"],
        ["ADBE Vector Shape - Rect", "ADBE_Vector_Shape_--_Rect"],
        ["ADBE Gaussian Blur 2-0001", "ADBE_Gaussian_Blur_2--0001"],
        ["Pseudo/abc-0001", "Pseudo-2Fabc--0001"],
        ["a  b", "a__b"],
        ["x.y:z#", "x-2Ey-3Az-23"],
        ["\u6a21\u7cca", "-u6A21-u7CCA"]
    ];
    for (var i = 0; i < cases.length; i++) {
        eq(B.encodeSegment(cases[i][0]), cases[i][1], "encodeSegment(" + cases[i][0] + ")");
        eq(B.decodeSegment(cases[i][1]).name, cases[i][0], "decodeSegment(" + cases[i][1] + ")");
    }
    eq(B.decodeSegment("ADBE_Position-n3"), {name: "ADBE Position", occurrence: 3}, "decodeSegment occurrence");

    eq(B.sanitizeName("Main Comp", "comp"), "Main_Comp", "sanitize spaces");
    eq(B.sanitizeName("  Title \u6807\u9898!! ", "layer"), "Title_\u6807\u9898", "sanitize CJK and punctuation");
    eq(B.sanitizeName("\ud83d\ude00", "layer"), "layer", "sanitize emoji-only name");
    eq(B.sanitizeName("-dash", "layer"), "layer_-dash", "sanitize leading dash");
    eq(B.sanitizeName("", "comp"), "comp", "sanitize empty");
    eq(B.sanitizeName("caf\u00e9 \u00d7 2", "x"), "caf\u00e9_2", "sanitize latin-1 letter vs multiplication sign");

    eq(B.relativePath("/Users/test/out", "/Users/test/footage/a.mov"), "../footage/a.mov", "relative up");
    eq(B.relativePath("/Users/test/out/", "/Users/test/out/a.mov"), "a.mov", "relative same dir");
    eq(B.relativePath("C:/work/out", "c:/Work/footage/a.mov"), "../footage/a.mov", "relative windows case");
    eq(B.relativePath("C:/work/out", "D:/footage/a.mov"), undefined, "relative across drives");
    eq(B.relativePath("//nas/share/out", "//nas/share/f/a.mov"), "../f/a.mov", "relative UNC same share");
    eq(B.relativePath("//nas/share/out", "//nas/other/a.mov"), undefined, "relative UNC other share");
    eq(B.toFlicks(1001 / 30000), 23543520, "29.97 frame in flicks");
    eq(B.toFlicks(1 / 24), 29400000, "24 fps frame in flicks");

    var sample = {a: [1, -2.5, 0, 1e21], s: "q\"\\\n\r\t\u0001", n: null, b: false, o: {}, e: [], nested: [{x: [1, 2]}]};
    eq(B.stringify(sample, ""), J.stringify(sample), "compact stringify matches JSON.stringify");
    eq(J.parse(B.stringify(sample, "  ")), J.parse(J.stringify(sample)), "indented stringify parses back");
    eq(B.stringify({x: -0, y: undefined}, ""), "{\"x\":0}", "stringify -0 and undefined");
    eq(B.parseJSON(J.stringify(sample)), sample, "parseJSON round trip");
    eq(B.parseJSON(" {\"a\" : [ 1 , 2.5e-3 , -0.5 ] , \"u\":\"\\u4f60\\n\\/\"} "), {a: [1, 0.0025, -0.5], u: "\u4f60\n/"},
        "parseJSON whitespace and escapes");
    var bad = ["{", "[1,]", "{\"a\":1,}", "01", "\"\\x\"", "nul", "[1] 2", "{a:1}", "\"a\nb\""];
    for (var bi = 0; bi < bad.length; bi++) {
        var threw = false;
        try {
            B.parseJSON(bad[bi]);
        } catch (e) {
            threw = /JSON parse error/.test(String(e));
        }
        check(threw, "parseJSON rejects " + bad[bi]);
    }
    eq(B.stringify("\u2028\u2029", ""), "\"\\u2028\\u2029\"", "line separators escaped (ExtendScript eval safety)");
}());

// ---------------------------------------------------------------- full export of the fixture

buildFixtureProject();
var doc = B.buildDocument({outputDir: "/Users/test/out", projectDir: "/Users/test/projects"});
B.finish(doc);
doc.exported_at = "2026-11-20T00:00:00Z";
doc.report.stats.seconds = 0;
var text = B.stringify(doc, "  ") + "\n";
var parsed = J.parse(text);
eq(B.parseJSON(text), parsed, "parseJSON reads the full export like JSON.parse");

var PRIM_NAME = /^[0-9A-Za-z_\u0080-\uffff][0-9A-Za-z_\u0080-\uffff-]*$/;
var ATTR_NAME = /^[0-9A-Za-z_\u0080-\uffff-]([0-9A-Za-z_\u0080-\uffff:-]*[0-9A-Za-z_\u0080-\uffff-])?$/;
var paths = {};
var prims = {};
var refs = [];

function walk(list, parentPath) {
    var siblings = {};
    for (var i = 0; i < list.length; i++) {
        var p = list[i];
        var path = parentPath + "/" + p.name;
        check(PRIM_NAME.test(p.name), "prim name " + path);
        check(!siblings["$" + p.name], "unique sibling " + path);
        siblings["$" + p.name] = true;
        paths[path] = true;
        prims[path] = p;
        for (var k in p.attrs || {}) {
            check(ATTR_NAME.test(k) && k.length <= 256, "attr name " + path + " " + k);
            var spec = p.attrs[k];
            var values = [];
            if (spec.value) {
                values.push(spec.value);
            }
            for (var t in spec.keyframes || {}) {
                check(/^-?[0-9]+$/.test(t), "keyframe time key " + path + " " + k + " " + t);
                values.push(spec.keyframes[t].value);
            }
            for (var v = 0; v < values.length; v++) {
                if (values[v].type === "prim_ref" || values[v].type === "asset_ref") {
                    refs.push([path + " " + k, values[v].value]);
                }
            }
        }
        walk(p.children || [], path);
    }
}

walk(parsed.prims, "");
for (var r = 0; r < refs.length; r++) {
    check(paths[refs[r][1]] === true, "reference " + refs[r][0] + " -> " + refs[r][1] + " resolves");
}

function attr(path, name) {
    var p = prims[path];
    check(!!p, "prim exists: " + path);
    var spec = p && p.attrs ? p.attrs[name] : undefined;
    check(!!spec, "attr exists: " + path + " " + name);
    return spec || {};
}

function hasReport(list, code, path) {
    for (var i = 0; i < list.length; i++) {
        if (list[i].code === code && (!path || list[i].path === path)) {
            return true;
        }
    }
    return false;
}

var MAIN = "/comps/Main_Comp";
var BOX = MAIN + "/layers/Box";
check(paths["/comps/Main_Comp_2"] === true, "duplicate comp names are made unique");
eq(attr("/project", "name").value, {type: "string", value: "demo.aep"}, "project name");
eq(attr(MAIN, "frame_rate").value, {type: "float", value: 30000 / 1001}, "comp frame rate");
eq(attr(MAIN, "duration").value, {type: "int", value: 7063056000}, "comp duration in flicks");
eq(attr(MAIN, "bg_color").value, {type: "color", value: [0.1, 0.2, 0.3, 1]}, "comp bg color gets alpha");
check(prims[MAIN].attrs.folder === undefined, "root-level comp has no folder attr");
eq(attr("/assets/clip_mov", "folder").value, {type: "prim_ref", value: "/folders/Footage"}, "asset folder");
eq(attr(MAIN + "/markers/marker_1", "time").value, {type: "int", value: 706305600}, "comp marker time");
eq(attr(MAIN + "/markers/marker_2", "param:cue_type").value, {type: "string", value: "hit"}, "marker parameter");
eq(attr(BOX, "kind").value, {type: "enum", value: "shape"}, "shape layer kind");
eq(attr(BOX, "parent").value, {type: "prim_ref", value: MAIN + "/layers/Controller"}, "parent ref");
eq(attr(BOX, "blend_mode").value, {type: "enum", value: "silhouette_alpha"}, "AE typo SILHOUETE_ALPHA mapped");
var pos = attr(BOX, "ae:ADBE_Transform_Group:ADBE_Position").keyframes;
eq(pos["0"].spatial_out, [100, -50, 0], "spatial out tangent");
eq(pos["0"].flags, {spatial_continuous: true}, "spatial flags");
eq(pos["706305600"].flags, {temporal_continuous: true, spatial_continuous: true, roving: true}, "roving flags");
eq(pos["1412611200"].interp_out, "hold", "hold out interpolation");
eq(pos["1412611200"].label, 4, "keyframe label");
var scale = attr(BOX, "ae:ADBE_Transform_Group:ADBE_Scale").keyframes;
eq(scale["0"].ease_out.length, 3, "per-dimension ease for ThreeD");
check(scale["0"].spatial_in === undefined, "no spatial tangents on non-spatial properties");
eq(attr(BOX, "ae:ADBE_Transform_Group:ADBE_Opacity").expression, {source: "wiggle(2, 20)", enabled: true},
    "expression");
check(hasReport(parsed.report.warnings, "expression_error", BOX), "expression error reported");
eq(attr(BOX + "/contents/Rectangle_1/contents/Rectangle_Path_1", "ae:ADBE_Vector_Rect_Size").value,
    {type: "vec2", value: [200, 120]}, "nested shape group contents");
eq(prims[BOX + "/contents/Rectangle_1"].prim_type, "ShapeGroup", "shape group type");
eq(prims[BOX + "/contents/Trim_Paths_1"].prim_type, "ShapeOperator", "trim paths type");
eq(attr(BOX + "/contents/Trim_Paths_1", "operator").value, {type: "enum", value: "trim"}, "operator token");
eq(attr(BOX + "/contents/Path_1", "ae:ADBE_Vector_Shape").value.value.in_tangents, [[0, 0], [0, -20], [0, 0]],
    "bezier tangents");
check(hasReport(parsed.report.unsupported, "gradient_colors"), "gradient colors reported");
eq(attr(BOX + "/markers/marker_1", "protected_region").value, {type: "bool", value: true}, "layer marker");

var TITLE = "/comps/Pre_comp/layers/Title_\u6807\u9898";
var st = attr(TITLE, "ae:ADBE_Text_Properties:ADBE_Text_Document").keyframes;
eq(st["0"].value.type, "text_document", "text document type");
eq(st["0"].value.value.justification, "center_justify", "text justification");
eq(st["0"].value.value.fill_color, [1, 1, 1, 1], "text fill color padded");
check(st["0"].ease_in === undefined, "text keyframes have no eases");
eq(attr(TITLE, "ae:ADBE_Text_Properties:ADBE_Text_Path_Options:ADBE_Text_Path").value,
    {type: "prim_ref", value: TITLE + "/masks/Path_Mask"}, "text path mask ref");
eq(prims[TITLE + "/animators/Animator_1"].prim_type, "TextAnimator", "animator type");
eq(prims[TITLE + "/animators/Animator_1/selectors/Range_Selector_1"].prim_type, "TextSelector", "selector type");
check(!!attr(TITLE + "/animators/Animator_1", "ae:ADBE_Text_Animator_Properties:ADBE_Text_Opacity").value,
    "animator properties flattened");
check(!!attr(TITLE + "/animators/Animator_1/selectors/Range_Selector_1",
    "ae:ADBE_Text_Range_Advanced:ADBE_Text_Range_Units").value, "selector advanced flattened");

var SOLID = MAIN + "/layers/Red_Solid_1";
eq(attr(SOLID, "track_matte_layer").value, {type: "prim_ref", value: MAIN + "/layers/Pre_comp"},
    "legacy track matte = layer above");
check(paths[SOLID + "/masks/Mask_1"] && paths[SOLID + "/masks/Mask_1_2"], "duplicate mask names");
eq(attr(SOLID + "/masks/Mask_1", "mode").value, {type: "enum", value: "add"}, "mask mode");
eq(attr(SOLID + "/masks/Mask_1_2", "mode").value, {type: "enum", value: "unknown_9999"}, "unknown enum token");
check(hasReport(parsed.report.warnings, "unknown_enum", SOLID + "/masks/Mask_1_2"), "unknown enum warned");
check(hasReport(parsed.report.unsupported, "mask_feather_points", SOLID + "/masks/Mask_1"), "feather points reported");
eq(attr(SOLID + "/effects/Gaussian_Blur", "match_name").value, {type: "string", value: "ADBE Gaussian Blur 2"},
    "effect match name");
eq(attr(SOLID + "/effects/Gaussian_Blur", "ae:ADBE_Gaussian_Blur_2--0001").value, {type: "float", value: 12.5},
    "effect parameter");
eq(attr(SOLID + "/effects/Set_Matte", "ae:ADBE_Set_Matte3--0001").value,
    {type: "prim_ref", value: MAIN + "/layers/Controller"}, "layer parameter");
eq(attr(SOLID + "/effects/My_Controls", "ae:Pseudo-2Fmy-5Fctrl--0002").value,
    {type: "prim_ref", value: SOLID + "/masks/Mask_1_2"}, "pseudo effect mask parameter");
check(prims[SOLID + "/effects/My_Controls"].attrs["ae:Pseudo-2Fmy-5Fctrl--0003"] === undefined,
    "layer parameter 'none' omitted");
check(hasReport(parsed.report.unsupported, "custom_value", SOLID + "/effects/Curves"), "custom value reported");
check(prims[SOLID].attrs["ae:ADBE_Time_Remapping"] === undefined, "time remap skipped when disabled");

var PRE = MAIN + "/layers/Pre_comp";
eq(attr(PRE, "source").value, {type: "prim_ref", value: "/comps/Pre_comp"}, "precomp source");
eq(attr(PRE, "stretch").value, {type: "float", value: 50}, "stretch");
eq(attr(PRE, "start_time").value, {type: "int", value: 705600000}, "start time");
eq(attr(MAIN + "/layers/Controller", "kind").value, {type: "enum", value: "null"}, "null kind");
eq(attr(MAIN + "/layers/Light_1", "light_type").value, {type: "enum", value: "spot"}, "light type");
eq(attr(MAIN + "/layers/clip_mov", "kind").value, {type: "enum", value: "footage"}, "footage kind");
check(!!prims[MAIN + "/layers/clip_mov"].attrs["ae:ADBE_Audio_Group:ADBE_Audio_Levels"], "audio levels");
check(paths[MAIN + "/layers/clip_mov_2"] === true, "duplicate layer names");
check(paths[MAIN + "/layers/layer"] === true, "emoji-only layer name falls back");
check(paths[MAIN + "/layers/layer_-dash"] === true, "leading-dash layer name");
eq(attr(MAIN + "/layers/layer_-dash", "kind").value, {type: "enum", value: "adjustment"}, "adjustment kind");

eq(attr("/assets/frame_0001-0003_png", "asset_kind").value, {type: "enum", value: "sequence"}, "sequence asset");
eq(attr("/assets/Red_Solid_1", "color").value, {type: "color", value: [1, 0, 0, 1]}, "solid color");
eq(attr("/assets/missing_mov", "missing").value, {type: "bool", value: true}, "missing asset");
var man = {};
for (var m = 0; m < parsed.assets.length; m++) {
    man[parsed.assets[m].prim] = parsed.assets[m];
}
eq(man["/assets/clip_mov"].path_relative, "../footage/clip.mov", "manifest relative path");
eq(man["/assets/clip_mov"].aep_relative, "../footage/clip.mov", "manifest aep-relative path");
eq(man["/assets/clip_mov"].modified, "2026-11-01T08:30:00Z", "manifest modified time");
eq(man["/assets/frame_0001-0003_png"].sequence, {first: "frame_0001.png", count: 3}, "sequence frames");
eq(man["/assets/missing_mov"].path_absolute, "D:/shots/missing.mov", "missing footage path");
check(man["/assets/missing_mov"].path_relative === undefined, "no relative path across drives");
check(man["/assets/Red_Solid_1"] === undefined, "solids are not in the manifest");
eq(attr("/assets/clip_mov", "file:original").value, {type: "string", value: "/Users/test/footage/clip.mov"},
    "bae-model file:original on the Asset prim");
eq(attr("/assets/clip_mov", "file:relative").value, {type: "string", value: "../footage/clip.mov"}, "file:relative");
eq(attr("/assets/clip_mov", "file:size").value, {type: "int", value: 1048576}, "file:size");
eq(attr("/assets/clip_mov", "file:mtime").value, {type: "int", value: Date.UTC(2026, 10, 1, 8, 30, 0)},
    "file:mtime in Unix milliseconds");
eq(attr("/assets/missing_mov", "file:original").value, {type: "string", value: "D:/shots/missing.mov"},
    "missing footage keeps file:original");
check(prims["/assets/missing_mov"].attrs["file:size"] === undefined, "no file:size for missing footage");

// ---------------------------------------------------------------- golden file

if (UPDATE) {
    writeFile(GOLDEN, text);
    print("updated " + GOLDEN);
} else {
    var golden = "";
    try {
        golden = readFile(GOLDEN);
    } catch (e) {
        golden = "";
    }
    check(golden === text, "output matches " + GOLDEN + " (run ./run.sh --update after intended changes)");
}

// ---------------------------------------------------------------- headless entry point with collect

buildFixtureProject();
var BAE_EXPORT_OPTIONS = {outputPath: "/Users/test/out/demo.bae-interchange.json", collectAssets: true, pretty: false};
load("../bae-export.jsx");
check(typeof BAE_EXPORT_OPTIONS === "undefined" || !BAE_EXPORT_OPTIONS, "headless options are consumed");
var written = MOCK_WRITES["/Users/test/out/demo.bae-interchange.json"];
check(!!written, "headless export wrote the file");
if (written) {
    eq(written.encoding, "UTF-8", "written as UTF-8");
    var out = J.parse(written.text);
    var cman = {};
    for (var c = 0; c < out.assets.length; c++) {
        cman[out.assets[c].prim] = out.assets[c];
    }
    eq(cman["/assets/clip_mov"].collected, "demo.assets/clip.mov", "collected file");
    eq(cman["/assets/frame_0001-0003_png"].collected, ["demo.assets/frame_0001-0003_png/frame_0001.png",
        "demo.assets/frame_0001-0003_png/frame_0002.png", "demo.assets/frame_0001-0003_png/frame_0003.png"],
        "collected sequence");
    check(MOCK_FS["/Users/test/out/demo.assets/clip.mov"] !== undefined, "file copied");
    check(written.text.indexOf("__") < 0 || written.text.indexOf("\"__file\"") < 0, "private fields stripped");
}
check($.written.length > 0 && /Exported 3 comps/.test($.written[$.written.length - 1]),
    "headless summary line: " + $.written.join(" | "));

print((failures ? "FAILED: " + failures : "OK") + " (" + checks + " checks)");
if (failures && typeof quit === "function") {
    quit(1);
}
