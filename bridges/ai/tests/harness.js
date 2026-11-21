// harness.js - tests for bae-ai-export.jsx without Illustrator: a small mock of the
// Illustrator scripting DOM, ES3 builtins only (es3-guard.js). Run with run.sh (macOS jsc,
// or node through node-run.js). The real-Illustrator check is regen.sh + the Rust tests in
// crates/bae-interop-ai/tests/illustrator.rs.

var NJSON = JSON; // the guard removes JSON; the export script has its own writer
var failures = 0, checks = 0;
function check(cond, what) {
    checks++;
    if (!cond) { failures++; print("FAIL " + what); }
}
function eq(actual, expected, what) {
    var a = NJSON.stringify(actual), e = NJSON.stringify(expected);
    check(a === e, what + "\n  actual:   " + a + "\n  expected: " + e);
}

// ------------------------------------------------------------------ mock DOM

function Enum(name, keys) {
    var e = {};
    for (var i = 0; i < keys.length; i++) {
        (function (k) { e[k] = { toString: function () { return name + "." + k; } }; }(keys[i]));
    }
    return e;
}
var BlendModes = Enum("BlendModes", ["NORMAL", "MULTIPLY", "SCREEN", "SATURATIONBLEND", "COLORBLEND"]);
var StrokeCap = Enum("StrokeCap", ["BUTTENDCAP", "ROUNDENDCAP", "PROJECTINGENDCAP"]);
var StrokeJoin = Enum("StrokeJoin", ["MITERENDJOIN", "ROUNDENDJOIN", "BEVELENDJOIN"]);
var CoordinateSystem = Enum("CoordinateSystem", ["DOCUMENTCOORDINATESYSTEM", "ARTBOARDCOORDINATESYSTEM"]);
var ImageColorSpace = Enum("ImageColorSpace", ["RGB", "CMYK", "GrayScale", "LAB"]);
var ColorConvertPurpose = Enum("ColorConvertPurpose", ["defaultpurpose"]);
var DocumentColorSpace = Enum("DocumentColorSpace", ["RGB", "CMYK"]);
var GradientType = Enum("GradientType", ["LINEAR", "RADIAL"]);
var TextType = Enum("TextType", ["POINTTEXT", "AREATEXT", "PATHTEXT"]);
var Justification = Enum("Justification", ["LEFT", "CENTER", "RIGHT"]);
var TextOrientation = Enum("TextOrientation", ["HORIZONTAL", "VERTICAL"]);
var ElementPlacement = Enum("ElementPlacement", ["PLACEATBEGINNING", "PLACEATEND"]);

var $ = { global: this, os: "Mock OS" };
var app = {
    name: "Adobe Illustrator", version: "30.4.0", locale: "en_US",
    coordinateSystem: CoordinateSystem.ARTBOARDCOORDINATESYSTEM,
    convertSampleColor: function () { throw new Error("not in the mock"); }
};

function rgb(r, g, b) { return { typename: "RGBColor", red: r, green: g, blue: b }; }
function cmyk(c, m, y, k) { return { typename: "CMYKColor", cyan: c, magenta: m, yellow: y, black: k }; }
var NONE = { typename: "NoColor" };
function ident() { return { mValueA: 1, mValueB: 0, mValueC: 0, mValueD: 1, mValueTX: 0, mValueTY: 0 }; }
function pt(x, y, lx, ly, rx, ry) {
    if (lx === undefined) { lx = x; ly = y; rx = x; ry = y; }
    return { anchor: [x, y], leftDirection: [lx, ly], rightDirection: [rx, ry] };
}
function path(props) {
    var p = {
        typename: "PathItem", name: "", hidden: false, locked: false, opacity: 100,
        blendingMode: BlendModes.NORMAL, visibleBounds: [0, 0, 0, 0], pathPoints: [], closed: true,
        filled: false, fillColor: NONE, stroked: false, strokeColor: NONE, strokeWidth: 1,
        strokeCap: StrokeCap.BUTTENDCAP, strokeJoin: StrokeJoin.MITERENDJOIN, strokeMiterLimit: 10,
        strokeDashes: [], strokeDashOffset: 0, evenodd: false, guides: false, clipping: false
    };
    for (var k in props) { if (props.hasOwnProperty(k)) { p[k] = props[k]; } }
    return p;
}
function rectPoints(l, t, r, b) { return [pt(l, t), pt(r, t), pt(r, b), pt(l, b)]; }

var linear = {
    typename: "GradientColor",
    gradient: {
        type: GradientType.LINEAR, name: "Ramp", gradientStops: [
            { rampPoint: 0, midPoint: 30, color: rgb(255, 0, 0), opacity: 50 },
            { rampPoint: 100, midPoint: 50, color: rgb(0, 0, 255), opacity: 100 }
        ]
    },
    origin: [10, 50], angle: 0, length: 100, hiliteAngle: 0, hiliteLength: 0,
    // Rotate 90 degrees counter-clockwise (y up) about the origin.
    matrix: { mValueA: 0, mValueB: 1, mValueC: -1, mValueD: 0, mValueTX: 0, mValueTY: 0 }
};
var radial = {
    typename: "GradientColor",
    gradient: { type: GradientType.RADIAL, name: "Glow", gradientStops: [
        { rampPoint: 0, midPoint: 50, color: rgb(255, 255, 0), opacity: 100 },
        { rampPoint: 100, midPoint: 50, color: rgb(0, 128, 0), opacity: 100 }] },
    origin: [100, 100], angle: 0, length: 50, hiliteAngle: 90, hiliteLength: 0.5, matrix: ident()
};

var mask = path({ name: "Mask", clipping: true, pathPoints: rectPoints(0, 100, 50, 50) });
var clipped = {
    typename: "GroupItem", name: "", hidden: false, locked: false, opacity: 100,
    blendingMode: BlendModes.NORMAL, visibleBounds: [0, 200, 200, 0], clipped: true,
    pageItems: [mask, path({ name: "Big", filled: true, fillColor: rgb(0, 255, 0), pathPoints: rectPoints(0, 200, 200, 0) })]
};
var compound = {
    typename: "CompoundPathItem", name: "Donut", hidden: false, locked: false, opacity: 50,
    blendingMode: BlendModes.SATURATIONBLEND, visibleBounds: [0, 10, 10, 0],
    pathItems: [
        path({ filled: true, fillColor: cmyk(0, 0, 0, 100), evenodd: true, pathPoints: rectPoints(2, 8, 8, 2) }),
        path({ filled: true, fillColor: cmyk(0, 0, 0, 100), evenodd: true, pathPoints: rectPoints(0, 10, 10, 0) })
    ]
};
var curve = path({
    name: "Curve", closed: false, stroked: true, strokeColor: rgb(255, 128, 0), strokeWidth: 3,
    strokeCap: StrokeCap.ROUNDENDCAP, strokeJoin: StrokeJoin.BEVELENDJOIN, strokeDashes: [6, 3],
    pathPoints: [pt(0, 0, -5, 0, 5, 0), pt(20, 10, 15, 10, 25, 10)], fillColor: linear, filled: true
});
var guide = path({ guides: true });
var glow = path({ name: "Glow", filled: true, fillColor: radial, pathPoints: rectPoints(50, 150, 150, 50) });
function range(text, size, color) {
    return { contents: text, characterAttributes: {
        textFont: { name: "MyriadPro-Regular", family: "Myriad Pro", style: "Regular" },
        size: size, leading: size * 1.2, autoLeading: true, tracking: 0, baselineShift: 0,
        horizontalScale: 100, verticalScale: 100, fillColor: color, strokeColor: NONE
    } };
}
var ranges = [range("H", 20, rgb(0, 0, 0)), range("i", 20, rgb(0, 0, 0)), range("!", 30, rgb(255, 0, 0))];
var text = {
    typename: "TextFrame", name: "Greeting", hidden: false, locked: false, opacity: 100,
    blendingMode: BlendModes.NORMAL, visibleBounds: [10, 40, 60, 20], kind: TextType.POINTTEXT,
    contents: "Hi!", anchor: [10, 25], orientation: TextOrientation.HORIZONTAL,
    matrix: { mValueA: 0.866, mValueB: 0.5, mValueC: -0.5, mValueD: 0.866, mValueTX: -9999, mValueTY: 9999 },
    paragraphs: [{ paragraphAttributes: { justification: Justification.CENTER } }],
    textRanges: ranges, textRange: ranges[0]
};
var mesh = { typename: "MeshItem", name: "", hidden: false, locked: false, opacity: 100,
    blendingMode: BlendModes.NORMAL, visibleBounds: [0, 1, 1, 0] };
var sub = {
    typename: "Layer", name: "Sub", visible: false, locked: false, printable: true, preview: true,
    opacity: 100, blendingMode: BlendModes.NORMAL, color: rgb(255, 0, 0), layers: [],
    pageItems: [glow], absoluteZOrderPosition: 5
};
curve.absoluteZOrderPosition = 9;
clipped.absoluteZOrderPosition = 3;
compound.absoluteZOrderPosition = 2;
text.absoluteZOrderPosition = 7;
mesh.absoluteZOrderPosition = 1;
guide.absoluteZOrderPosition = 0;
var layer = {
    typename: "Layer", name: "图层 1", visible: true, locked: true, printable: true, preview: true,
    opacity: 80, blendingMode: BlendModes.MULTIPLY, color: rgb(0, 0, 255),
    layers: [sub], pageItems: [curve, text, clipped, compound, mesh, guide]
};
var doc = {
    name: "mock.ai", path: "", documentColorSpace: DocumentColorSpace.RGB,
    artboards: [{ name: "Board", artboardRect: [0, 200, 300, 0] }],
    layers: [layer]
};
doc.layers.add = function () { throw new Error("the test document needs no temporary layer"); };
app.activeDocument = doc;

// ------------------------------------------------------------------ run

load("es3-guard.js");
var BAE_AI_EXPORT_NO_AUTORUN = true;
load("../bae-ai-export.jsx");
var E = BAE_AI_EXPORT;

// JSON writer.
eq(NJSON.parse(E.stringify({ s: "a\"b\\c\r\n\t\u0001 ", n: [1 / 3, -0, 1e-9, 2.5], o: {}, l: [] }, true)),
    { s: "a\"b\\c\r\n\t\u0001 ", n: [0.3333, 0, 0, 2.5], o: {}, l: [] }, "stringify round trip");
check(E.stringify({ a: [1, 2] }, false) === "{\"a\":[1,2]}", "compact");
check(E.stringify({ a: [[1, 2]] }, true).indexOf("[1, 2]") > 0, "number arrays stay on one line");
eq(E.blendToken(BlendModes.SATURATIONBLEND), "saturation", "blend token");

var out = NJSON.parse(E.stringify(E.exportDocument(doc, { textOutlines: false }), false));
check(out.format === "bae-ai-bridge" && out.format_version === "0.1.0", "header");
eq(out.document, { name: "mock", color_space: "rgb", units: "pt", y_axis: "down" }, "document");
eq(out.artboards, [{ name: "Board", rect: [0, -200, 300, 0] }], "artboards (y down)");
check(app.coordinateSystem === CoordinateSystem.ARTBOARDCOORDINATESYSTEM, "coordinate system restored");

var L = out.layers[0];
eq([L.name, L.visible, L.locked, L.opacity, L.blend, L.color], ["图层 1", true, true, 0.8, "multiply", [0, 0, 1]], "layer");
var order = [];
for (var i = 0; i < L.items.length; i++) { order.push(L.items[i].name || L.items[i].type); }
eq(order, ["Curve", "Greeting", "Sub", "group", "Donut", "other"], "z-order with the sublayer, guide left out");

var c = L.items[0];
eq(c.subpaths, [{ closed: false, points: [[0, 0, -5, 0, 5, 0], [20, -10, 15, -10, 25, -10]] }], "path points (y down)");
eq([c.stroke.width, c.stroke.cap, c.stroke.join, c.stroke.dashes, c.stroke.paint.color], [3, "round", "bevel", [6, 3], [1, 0.502, 0, 1]], "stroke");
eq([c.fill.gradient, c.fill.start, c.fill.end], ["linear", [-50, -10], [-50, -110]], "linear gradient through its matrix");
eq([c.fill.stops[0].color, c.fill.stops[0].midpoint], [[1, 0, 0, 0.5], 0.3], "stop opacity and midpoint");

var t = L.items[1];
eq([t.text_kind, t.contents, t.anchor, t.matrix, t.paragraphs], ["point", "Hi!", [10, -25], [0.866, -0.5, 0.5, 0.866], [{ justification: "center" }]], "text");
eq([t.runs.length, t.runs[0].text, t.runs[1].text, t.runs[1].size, t.runs[1].fill.color], [2, "Hi", "!", 30, [1, 0, 0, 1]], "text runs merged");
check(t.outline === undefined, "no outline when textOutlines is off");

var s = L.items[2];
eq([s.type, s.visible, s.items[0].name], ["layer", false, "Glow"], "sublayer");
var g = s.items[0].fill;
eq([g.gradient, g.start, g.end, g.focal], ["radial", [100, -100], [150, -100], [100, -75]], "radial gradient and focal point");

var clip = L.items[3];
eq([clip.clipped, clip.clip.length, clip.clip[0].points[0], clip.items.length, clip.items[0].name], [true, 1, [0, -100, 0, -100, 0, -100], 1, "Big"], "clip group");

var d = L.items[4];
eq([d.compound, d.subpaths.length, d.fill_rule, d.opacity, d.blend, d.fill.source.model], [true, 2, "even_odd", 0.5, "saturation", "cmyk"], "compound path");
eq(d.fill.color, [0, 0, 0, 1], "naive CMYK fallback when Illustrator cannot convert");

eq([L.items[5].type, L.items[5].item_type], ["other", "MeshItem"], "unsupported item");
check(out.report.warnings.length === 1 && out.report.warnings[0].indexOf("MeshItem") >= 0, "warning for the mesh: " + out.report.warnings);

print(checks + " checks, " + failures + " failed");
quit(failures ? 1 : 0);
