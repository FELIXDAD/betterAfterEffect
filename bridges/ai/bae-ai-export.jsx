// bae-ai-export.jsx - export an Illustrator document to the BAE Illustrator bridge format.
//
// Format: bridges/ai/FORMAT.md (format "bae-ai-bridge", format_version 0.1.0); the Rust
// reader is crates/bae-interop-ai (src/bridge.rs).
// Runs in Illustrator CC 2017 (21) and later. ExtendScript is ES3: no JSON object, no
// Array.prototype.indexOf/forEach/map, no String.prototype.trim, no Object.keys.
//
// Interactive use: File > Scripts > Other Script... and pick this file; it asks where to
// save and reports what it wrote.
// Headless use: define a global BAE_AI_EXPORT_OPTIONS object before evaluating this file:
//     var BAE_AI_EXPORT_OPTIONS = {outputPath: "/path/out.bae-ai.json"};
//     $.evalFile("/path/to/bridges/ai/bae-ai-export.jsx");
// (tools/ai/aictl runs such wrappers unattended). Options:
//     outputPath     where to write (required headless)
//     document       the Document to export (default: app.activeDocument)
//     textOutlines   also export each text frame converted to outlines (default true)
//     pretty         indent the JSON (default true)
//     stable         leave out the export time and the document's absolute path, so the
//                    output only changes when the artwork does (test data)
// The script never saves the document. It adds a temporary layer while expanding symbols
// and outlining text and removes it again, so the document is marked modified.
// Tests define BAE_AI_EXPORT_NO_AUTORUN = true and call BAE_AI_EXPORT.* directly.

(function (global) {
    var EXP = {};
    EXP.VERSION = "0.1.0";
    EXP.FORMAT = "bae-ai-bridge";
    EXP.FORMAT_VERSION = "0.1.0";
    var TMP_LAYER = "__bae_ai_export_tmp__";

    // ------------------------------------------------------------------ utilities

    function isArray(v) {
        return Object.prototype.toString.call(v) === "[object Array]";
    }

    function hasOwn(obj, key) {
        return Object.prototype.hasOwnProperty.call(obj, key);
    }

    // Property read that never throws (Illustrator throws "Internal error" / "No such
    // element" for some properties of some items).
    function get(obj, key, fallback) {
        try {
            var v = obj[key];
            return (v === undefined) ? fallback : v;
        } catch (e) {
            return fallback;
        }
    }

    // 1e-4 pt is far below anything visible and keeps files small and diffable.
    function num(v) {
        var r = Math.round(v * 10000) / 10000;
        return (r === 0) ? 0 : r;
    }

    function enumToken(value) {
        // "BlendModes.MULTIPLY" -> "MULTIPLY"
        var s = String(value);
        var dot = s.lastIndexOf(".");
        return dot >= 0 ? s.substring(dot + 1) : s;
    }

    function jsonString(s) {
        s = String(s);
        var out = '"';
        for (var i = 0; i < s.length; i++) {
            var c = s.charAt(i);
            var code = s.charCodeAt(i);
            if (c === '"') { out += '\\"'; }
            else if (c === "\\") { out += "\\\\"; }
            else if (c === "\n") { out += "\\n"; }
            else if (c === "\r") { out += "\\r"; }
            else if (c === "\t") { out += "\\t"; }
            else if (code < 0x20 || code === 0x2028 || code === 0x2029) {
                var h = code.toString(16);
                while (h.length < 4) { h = "0" + h; }
                out += "\\u" + h;
            } else { out += c; }
        }
        return out + '"';
    }

    function isFlat(arr) {
        for (var i = 0; i < arr.length; i++) {
            if (arr[i] !== null && typeof arr[i] === "object") { return false; }
        }
        return true;
    }

    // JSON writer. Arrays of numbers stay on one line so path points remain compact.
    function stringify(v, indent, depth) {
        if (v === null || v === undefined) { return "null"; }
        var t = typeof v;
        if (t === "number") { return isFinite(v) ? String(num(v)) : "null"; }
        if (t === "boolean") { return v ? "true" : "false"; }
        if (t === "string") { return jsonString(v); }
        var pad = "", padIn = "", nl = "", sep = ",", colon = ":";
        if (indent) {
            nl = "\n"; colon = ": ";
            for (var p = 0; p < depth; p++) { pad += indent; }
            padIn = pad + indent;
        }
        var parts = [], i;
        if (isArray(v)) {
            if (v.length === 0) { return "[]"; }
            if (isFlat(v)) {
                for (i = 0; i < v.length; i++) { parts.push(stringify(v[i], null, 0)); }
                return "[" + parts.join(indent ? ", " : ",") + "]";
            }
            for (i = 0; i < v.length; i++) { parts.push(padIn + stringify(v[i], indent, depth + 1)); }
            return "[" + nl + parts.join(sep + nl) + nl + pad + "]";
        }
        for (var k in v) {
            if (hasOwn(v, k) && v[k] !== undefined) {
                parts.push(padIn + jsonString(k) + colon + stringify(v[k], indent, depth + 1));
            }
        }
        if (parts.length === 0) { return "{}"; }
        return "{" + nl + parts.join(sep + nl) + nl + pad + "}";
    }
    EXP.stringify = function (v, pretty) { return stringify(v, pretty ? "  " : null, 0); };

    function isoNow() {
        var d = new Date();
        function two(n) { return (n < 10 ? "0" : "") + n; }
        return d.getUTCFullYear() + "-" + two(d.getUTCMonth() + 1) + "-" + two(d.getUTCDate()) + "T" +
            two(d.getUTCHours()) + ":" + two(d.getUTCMinutes()) + ":" + two(d.getUTCSeconds()) + "Z";
    }

    // ------------------------------------------------------------------ geometry
    // Illustrator's scripting space is y-up; the bridge format is y-down (negate y).

    function pt(p) { return [num(p[0]), num(-p[1])]; }

    function bounds(b) {
        // [left, top, right, bottom] y-up -> y-down
        return [num(b[0]), num(-b[1]), num(b[2]), num(-b[3])];
    }

    // Illustrator Matrix (PostScript order, y-up) -> [a, b, c, d, tx, ty] in y-down space.
    function matrix(m) {
        return [num(m.mValueA), num(-m.mValueB), num(-m.mValueC), num(m.mValueD), num(m.mValueTX), num(-m.mValueTY)];
    }

    // Apply [a, b, c, d, tx, ty] (y-up PostScript convention) to [x, y].
    function apply(m, x, y) {
        return [m.mValueA * x + m.mValueC * y + m.mValueTX, m.mValueB * x + m.mValueD * y + m.mValueTY];
    }
    EXP._apply = apply;

    function subpath(pathItem) {
        var pts = [];
        var pp = pathItem.pathPoints;
        for (var i = 0; i < pp.length; i++) {
            var p = pp[i];
            var a = p.anchor, l = p.leftDirection, r = p.rightDirection;
            pts.push([num(a[0]), num(-a[1]), num(l[0]), num(-l[1]), num(r[0]), num(-r[1])]);
        }
        return { closed: !!pathItem.closed, points: pts };
    }

    // ------------------------------------------------------------------ colours

    var BLEND = {
        NORMAL: "normal", MULTIPLY: "multiply", SCREEN: "screen", OVERLAY: "overlay",
        SOFTLIGHT: "soft_light", HARDLIGHT: "hard_light", COLORDODGE: "color_dodge",
        COLORBURN: "color_burn", DARKEN: "darken", LIGHTEN: "lighten", DIFFERENCE: "difference",
        EXCLUSION: "exclusion", HUE: "hue", SATURATIONBLEND: "saturation", COLORBLEND: "color",
        LUMINOSITY: "luminosity"
    };

    function blendToken(mode) {
        var t = enumToken(mode);
        return hasOwn(BLEND, t) ? BLEND[t] : t.toLowerCase();
    }
    EXP.blendToken = blendToken;

    function clamp01(v) { return v < 0 ? 0 : (v > 1 ? 1 : v); }

    // Colour conversion through Illustrator's colour settings when available (CS6+),
    // otherwise naive formulas.
    function convert(space, values) {
        try {
            var r = app.convertSampleColor(space, values, ImageColorSpace.RGB, ColorConvertPurpose.defaultpurpose);
            return [clamp01(r[0] / 255), clamp01(r[1] / 255), clamp01(r[2] / 255)];
        } catch (e) {
            return null;
        }
    }

    function cmykNaive(c, m, y, k) {
        return [(1 - c / 100) * (1 - k / 100), (1 - m / 100) * (1 - k / 100), (1 - y / 100) * (1 - k / 100)];
    }

    // -> {rgb: [r, g, b] 0..1, source: {...}} or null for NoColor / unsupported.
    function solid(c, ctx) {
        var t = c.typename, rgb, src;
        if (t === "RGBColor") {
            rgb = [c.red / 255, c.green / 255, c.blue / 255];
            src = { model: "rgb", values: [num(c.red), num(c.green), num(c.blue)] };
        } else if (t === "CMYKColor") {
            src = { model: "cmyk", values: [num(c.cyan), num(c.magenta), num(c.yellow), num(c.black)] };
            rgb = convert(ImageColorSpace.CMYK, [c.cyan, c.magenta, c.yellow, c.black]) ||
                cmykNaive(c.cyan, c.magenta, c.yellow, c.black);
        } else if (t === "GrayColor") {
            src = { model: "gray", values: [num(c.gray)] };
            rgb = convert(ImageColorSpace.GrayScale, [c.gray]);
            if (!rgb) { var g = 1 - c.gray / 100; rgb = [g, g, g]; }
        } else if (t === "LabColor") {
            src = { model: "lab", values: [num(c.l), num(c.a), num(c.b)] };
            rgb = convert(ImageColorSpace.LAB, [c.l, c.a, c.b]);
            if (!rgb) { ctx.warn("Lab colour converted to grey"); rgb = [c.l / 100, c.l / 100, c.l / 100]; }
        } else if (t === "SpotColor") {
            var base = solid(c.spot.color, ctx);
            if (!base) { return null; }
            var tint = get(c, "tint", 100) / 100;
            rgb = [1 + (base.rgb[0] - 1) * tint, 1 + (base.rgb[1] - 1) * tint, 1 + (base.rgb[2] - 1) * tint];
            src = { model: "spot", spot: String(c.spot.name), tint: num(tint * 100), base: base.source };
        } else {
            return null;
        }
        return { rgb: [num(clamp01(rgb[0])), num(clamp01(rgb[1])), num(clamp01(rgb[2]))], source: src };
    }

    function gradientPaint(gc, ctx) {
        var g = gc.gradient;
        var type = enumToken(g.type).toLowerCase(); // linear | radial (freeform: other)
        var stops = [];
        var gs = g.gradientStops;
        for (var i = 0; i < gs.length; i++) {
            var s = gs[i];
            var c = solid(s.color, ctx) || { rgb: [0, 0, 0], source: { model: "none" } };
            var opacity = get(s, "opacity", 100);
            stops.push({
                offset: num(s.rampPoint / 100),
                midpoint: num(s.midPoint / 100),
                color: [c.rgb[0], c.rgb[1], c.rgb[2], num(opacity / 100)],
                source: c.source
            });
        }
        var m = gc.matrix;
        var origin = gc.origin, angle = gc.angle, length = gc.length;
        var rad = angle * Math.PI / 180;
        // Gradient space -> document (y-up). Verified against Illustrator's own SVG export:
        // start = M * origin, end = M * (origin + length * (cos a, sin a)).
        var start = apply(m, origin[0], origin[1]);
        var end = apply(m, origin[0] + length * Math.cos(rad), origin[1] + length * Math.sin(rad));
        var paint = {
            type: "gradient",
            gradient: type,
            name: String(get(g, "name", "")),
            stops: stops,
            start: pt(start),
            end: pt(end),
            matrix: matrix(m),
            raw: {
                origin: [num(origin[0]), num(origin[1])],
                angle: num(angle),
                length: num(length),
                hilite_angle: num(get(gc, "hiliteAngle", 0)),
                hilite_length: num(get(gc, "hiliteLength", 0)),
                matrix: [num(m.mValueA), num(m.mValueB), num(m.mValueC), num(m.mValueD), num(m.mValueTX), num(m.mValueTY)]
            }
        };
        if (type === "radial") {
            // The highlight is measured in gradient space, clockwise, as a fraction of the
            // radius (checked with SVG focal points imported into Illustrator 30).
            var ha = -get(gc, "hiliteAngle", 0) * Math.PI / 180;
            var hl = get(gc, "hiliteLength", 0) * length;
            paint.focal = pt(apply(m, origin[0] + hl * Math.cos(ha), origin[1] + hl * Math.sin(ha)));
        } else if (type !== "linear") {
            ctx.warn("gradient type " + type + " exported with its colour stops only");
        }
        return paint;
    }

    // Fill or stroke colour -> paint object or null.
    function paint(color, ctx) {
        if (!color) { return null; }
        var t = color.typename;
        if (t === "NoColor") { return null; }
        if (t === "GradientColor") { return gradientPaint(color, ctx); }
        if (t === "PatternColor") {
            ctx.warn("pattern fill/stroke not exported (" + get(color.pattern, "name", "pattern") + ")");
            return { type: "pattern", name: String(get(color.pattern, "name", "")) };
        }
        var c = solid(color, ctx);
        if (!c) {
            ctx.warn("unsupported colour " + t);
            return null;
        }
        return { type: "color", color: [c.rgb[0], c.rgb[1], c.rgb[2], 1], source: c.source };
    }

    var CAP = { BUTTENDCAP: "butt", ROUNDENDCAP: "round", PROJECTINGENDCAP: "projecting" };
    var JOIN = { MITERENDJOIN: "miter", ROUNDENDJOIN: "round", BEVELENDJOIN: "bevel" };

    function strokeOf(item, ctx) {
        if (!get(item, "stroked", false)) { return null; }
        var p = paint(get(item, "strokeColor", null), ctx);
        if (!p) { return null; }
        var dashes = [];
        var d = get(item, "strokeDashes", []);
        for (var i = 0; i < d.length; i++) { dashes.push(num(d[i])); }
        return {
            paint: p,
            width: num(get(item, "strokeWidth", 1)),
            cap: CAP[enumToken(get(item, "strokeCap", "BUTTENDCAP"))] || "butt",
            join: JOIN[enumToken(get(item, "strokeJoin", "MITERENDJOIN"))] || "miter",
            miter_limit: num(get(item, "strokeMiterLimit", 10)),
            dashes: dashes,
            dash_offset: num(get(item, "strokeDashOffset", 0))
        };
    }

    function fillOf(item, ctx) {
        if (!get(item, "filled", false)) { return null; }
        return paint(get(item, "fillColor", null), ctx);
    }

    // ------------------------------------------------------------------ items

    function common(item, type) {
        var o = { type: type };
        var name = String(get(item, "name", ""));
        if (name !== "") { o.name = name; }
        o.hidden = !!get(item, "hidden", false);
        o.locked = !!get(item, "locked", false);
        o.opacity = num(get(item, "opacity", 100) / 100);
        o.blend = blendToken(get(item, "blendingMode", "BlendModes.NORMAL"));
        var vb = get(item, "visibleBounds", null);
        if (vb) { o.bounds = bounds(vb); }
        return o;
    }

    function exportPath(item, ctx) {
        var o = common(item, "path");
        o.compound = false;
        o.subpaths = [subpath(item)];
        o.fill_rule = get(item, "evenodd", false) ? "even_odd" : "non_zero";
        o.fill = fillOf(item, ctx);
        o.stroke = strokeOf(item, ctx);
        return o;
    }

    function exportCompound(item, ctx) {
        var o = common(item, "path");
        o.compound = true;
        var subs = [], parts = item.pathItems;
        for (var i = 0; i < parts.length; i++) { subs.push(subpath(parts[i])); }
        o.subpaths = subs;
        // A compound path draws with one appearance; scripting exposes it on its parts.
        var first = parts.length ? parts[0] : null;
        o.fill_rule = (first && get(first, "evenodd", false)) ? "even_odd" : "non_zero";
        o.fill = first ? fillOf(first, ctx) : null;
        o.stroke = first ? strokeOf(first, ctx) : null;
        return o;
    }

    // The clipping item of a clip group and its sub-paths.
    function isClipping(item) {
        var t = item.typename;
        if (t === "PathItem") { return !!get(item, "clipping", false); }
        if (t === "CompoundPathItem") {
            return item.pathItems.length > 0 && !!get(item.pathItems[0], "clipping", false);
        }
        return false;
    }

    function clipSubpaths(item) {
        if (item.typename === "PathItem") { return [subpath(item)]; }
        var subs = [];
        for (var i = 0; i < item.pathItems.length; i++) { subs.push(subpath(item.pathItems[i])); }
        return subs;
    }

    function exportGroup(item, ctx) {
        var o = common(item, "group");
        o.clipped = !!get(item, "clipped", false);
        var children = childrenOf(item);
        if (o.clipped) {
            var clip = [], rest = [];
            for (var i = 0; i < children.length; i++) {
                if (isClipping(children[i])) {
                    var subs = clipSubpaths(children[i]);
                    for (var k = 0; k < subs.length; k++) { clip.push(subs[k]); }
                } else {
                    rest.push(children[i]);
                }
            }
            if (clip.length === 0) {
                ctx.warn("clip group without a path clipping mask (text or other mask) exported unclipped");
            }
            o.clip = clip;
            children = rest;
        }
        ctx.push(o.name || "group");
        o.items = exportItems(children, ctx);
        ctx.pop();
        return o;
    }

    var JUSTIFY = {
        LEFT: "left", CENTER: "center", RIGHT: "right", FULLJUSTIFYLASTLINELEFT: "justify_left",
        FULLJUSTIFYLASTLINECENTER: "justify_center", FULLJUSTIFYLASTLINERIGHT: "justify_right",
        FULLJUSTIFY: "justify_all"
    };

    function runOf(range, ctx) {
        var a = range.characterAttributes;
        var font = get(a, "textFont", null);
        var r = {
            font: font ? String(get(font, "name", "")) : "",
            family: font ? String(get(font, "family", "")) : "",
            style: font ? String(get(font, "style", "")) : "",
            size: num(get(a, "size", 12)),
            leading: num(get(a, "leading", 0)),
            auto_leading: !!get(a, "autoLeading", true),
            tracking: num(get(a, "tracking", 0)),
            baseline_shift: num(get(a, "baselineShift", 0)),
            h_scale: num(get(a, "horizontalScale", 100)),
            v_scale: num(get(a, "verticalScale", 100)),
            fill: get(a, "fillColor", null) ? paint(a.fillColor, ctx) : null,
            stroke: null
        };
        var sc = get(a, "strokeColor", null);
        if (sc && sc.typename !== "NoColor") {
            r.stroke = paint(sc, ctx);
            r.stroke_width = num(get(a, "strokeWeight", 1));
        }
        return r;
    }

    function runKey(r) {
        var copy = {};
        for (var k in r) { if (hasOwn(r, k) && k !== "text") { copy[k] = r[k]; } }
        return stringify(copy, null, 0);
    }

    var MAX_RUN_CHARS = 4000;

    function exportText(item, ctx) {
        var o = common(item, "text");
        o.text_kind = enumToken(get(item, "kind", "POINTTEXT")).replace(/TEXT$/, "").toLowerCase();
        o.contents = String(get(item, "contents", ""));
        o.anchor = pt(get(item, "anchor", [0, 0]));
        var m = get(item, "matrix", null);
        // Only the linear part: the matrix translation is in Illustrator's internal space.
        if (m) { o.matrix = [num(m.mValueA), num(-m.mValueB), num(-m.mValueC), num(m.mValueD)]; }
        o.orientation = enumToken(get(item, "orientation", "HORIZONTAL")).toLowerCase();
        if (o.text_kind !== "point") {
            // Area text: the frame it flows in; path text: the path it runs along. The
            // anchor of such frames is meaningless.
            var tp = get(item, "textPath", null);
            if (tp && get(tp, "pathPoints", null)) { o.frame = subpath(tp); }
            delete o.anchor;
        }
        var paras = [];
        try {
            for (var p = 0; p < item.paragraphs.length; p++) {
                var j = enumToken(get(item.paragraphs[p].paragraphAttributes, "justification", "LEFT"));
                paras.push({ justification: JUSTIFY[j] || j.toLowerCase() });
            }
        } catch (e) { /* empty frame */ }
        o.paragraphs = paras;
        // Style runs: Illustrator's textRanges are single characters; merge equal ones.
        var runs = [];
        try {
            var ranges = item.textRanges;
            if (ranges.length > MAX_RUN_CHARS) {
                ctx.warn("text longer than " + MAX_RUN_CHARS + " characters: styles taken from its first character");
                var only = runOf(item.textRange, ctx);
                only.text = o.contents;
                runs.push(only);
            } else {
                var prevKey = null;
                for (var i = 0; i < ranges.length; i++) {
                    var r = runOf(ranges[i], ctx);
                    var key = runKey(r);
                    if (key === prevKey) {
                        runs[runs.length - 1].text += ranges[i].contents;
                    } else {
                        r.text = String(ranges[i].contents);
                        runs.push(r);
                        prevKey = key;
                    }
                }
            }
        } catch (e2) {
            ctx.warn("could not read text styles: " + e2);
        }
        o.runs = runs;
        if (ctx.options.textOutlines && o.contents !== "") {
            o.outline = outlineOf(item, ctx);
        }
        return o;
    }

    function tmpLayer(ctx) {
        if (!ctx.tmp) {
            var doc = ctx.doc;
            ctx.tmp = doc.layers.add();
            ctx.tmp.name = TMP_LAYER;
        }
        return ctx.tmp;
    }

    function outlineOf(item, ctx) {
        try {
            var dup = item.duplicate(tmpLayer(ctx), ElementPlacement.PLACEATBEGINNING);
            dup.hidden = false;
            var g = dup.createOutline();
            ctx.push("outline");
            var o = exportGroup(g, ctx);
            ctx.pop();
            g.remove();
            delete o.name;
            return o;
        } catch (e) {
            ctx.warn("could not outline text: " + e);
            return null;
        }
    }

    // Symbol instances are exported as the artwork they show: a copy is placed in a
    // temporary group, its link broken, and the expanded content exported.
    function exportSymbol(item, ctx) {
        var o = common(item, "group");
        o.clipped = false;
        o.symbol = String(get(get(item, "symbol", {}), "name", ""));
        try {
            var holder = tmpLayer(ctx).groupItems.add();
            var dup = item.duplicate(holder, ElementPlacement.PLACEATBEGINNING);
            dup.hidden = false;
            dup.locked = false;
            dup.breakLink();
            var children = childrenOf(holder);
            if (children.length === 1 && children[0].typename === "SymbolItem") {
                ctx.warn("could not expand symbol " + o.symbol);
                o.items = [];
            } else {
                ctx.push("symbol " + o.symbol);
                o.items = exportItems(children, ctx);
                ctx.pop();
            }
            holder.remove();
        } catch (e) {
            ctx.warn("could not expand symbol " + o.symbol + ": " + e);
            o.items = [];
        }
        return o;
    }

    function exportImage(item, kind, ctx) {
        var o = common(item, "image");
        o.image_kind = kind;
        o.embedded = (kind === "raster") ? !!get(item, "embedded", true) : false;
        var f = get(item, "file", null);
        if (f) { o.file = String(f.fsName); }
        var m = get(item, "matrix", null);
        if (m) { o.matrix = matrix(m); }
        ctx.warn(kind + " image exported as a reference only");
        return o;
    }

    function exportOther(item, ctx) {
        var o = common(item, "other");
        o.item_type = item.typename;
        ctx.warn(item.typename + " not exported (expand it in Illustrator first)");
        return o;
    }

    function exportItem(item, ctx) {
        var t = item.typename;
        if (t === "PathItem") {
            if (get(item, "guides", false)) { return null; }
            return exportPath(item, ctx);
        }
        if (t === "CompoundPathItem") { return exportCompound(item, ctx); }
        if (t === "GroupItem") { return exportGroup(item, ctx); }
        if (t === "TextFrame") { return exportText(item, ctx); }
        if (t === "SymbolItem") { return exportSymbol(item, ctx); }
        if (t === "RasterItem") { return exportImage(item, "raster", ctx); }
        if (t === "PlacedItem") { return exportImage(item, "placed", ctx); }
        if (t === "Layer") { return exportLayer(item, ctx); }
        return exportOther(item, ctx);
    }

    function exportItems(items, ctx) {
        var out = [];
        for (var i = 0; i < items.length; i++) {
            ctx.push(i);
            var o = exportItem(items[i], ctx);
            ctx.pop();
            if (o) { out.push(o); }
        }
        return out;
    }

    // Direct children in stacking order, topmost first. Layers mix sublayers and page
    // items; absoluteZOrderPosition orders them (it throws for some items, then sublayers
    // go first, which is where Illustrator puts new sublayers).
    function childrenOf(container) {
        var items = [], i;
        var pis = container.pageItems;
        for (i = 0; i < pis.length; i++) { items.push(pis[i]); }
        if (container.typename !== "Layer" || container.layers.length === 0) { return items; }
        var subs = [];
        for (i = 0; i < container.layers.length; i++) {
            if (container.layers[i].name !== TMP_LAYER) { subs.push(container.layers[i]); }
        }
        var all = subs.concat(items);
        var keyed = [], ok = true;
        for (i = 0; i < all.length; i++) {
            var z = get(all[i], "absoluteZOrderPosition", null);
            if (typeof z !== "number") { ok = false; break; }
            keyed.push({ z: z, item: all[i] });
        }
        if (!ok) { return all; }
        keyed.sort(function (a, b) { return b.z - a.z; });
        var sorted = [];
        for (i = 0; i < keyed.length; i++) { sorted.push(keyed[i].item); }
        return sorted;
    }

    function layerColor(layer) {
        var c = get(layer, "color", null);
        if (!c || c.typename !== "RGBColor") { return undefined; }
        return [num(c.red / 255), num(c.green / 255), num(c.blue / 255)];
    }

    function exportLayer(layer, ctx) {
        var o = { type: "layer", name: String(layer.name) };
        o.visible = !!get(layer, "visible", true);
        o.locked = !!get(layer, "locked", false);
        o.printable = !!get(layer, "printable", true);
        o.preview = !!get(layer, "preview", true);
        o.opacity = num(get(layer, "opacity", 100) / 100);
        o.blend = blendToken(get(layer, "blendingMode", "BlendModes.NORMAL"));
        o.color = layerColor(layer);
        ctx.push(o.name);
        o.items = exportItems(childrenOf(layer), ctx);
        ctx.pop();
        return o;
    }

    // ------------------------------------------------------------------ document

    function Context(doc, options) {
        this.doc = doc;
        this.options = options;
        this.warnings = [];
        this.path = [];
        this.tmp = null;
        this.seen = {};
    }
    Context.prototype.push = function (p) { this.path.push(p); };
    Context.prototype.pop = function () { this.path.pop(); };
    Context.prototype.warn = function (msg) {
        var where = this.path.join(" / ");
        var key = msg;
        // One line per message kind and place is enough.
        if (hasOwn(this.seen, key + "@" + where)) { return; }
        this.seen[key + "@" + where] = true;
        this.warnings.push(where ? where + ": " + msg : msg);
    };

    function withDefaults(options) {
        var o = { textOutlines: true, pretty: true };
        if (options) { for (var k in options) { if (hasOwn(options, k)) { o[k] = options[k]; } } }
        return o;
    }

    EXP.exportDocument = function (doc, options) {
        options = withDefaults(options);
        var ctx = new Context(doc, options);
        // breakLink() and createOutline() silently do nothing in a document that is not
        // the active one.
        if (app.activeDocument !== doc) { app.activeDocument = doc; }
        var savedCoords = null;
        try {
            savedCoords = app.coordinateSystem;
            app.coordinateSystem = CoordinateSystem.DOCUMENTCOORDINATESYSTEM;
        } catch (e) { savedCoords = null; }
        var result;
        try {
            var artboards = [];
            for (var i = 0; i < doc.artboards.length; i++) {
                var ab = doc.artboards[i];
                artboards.push({ name: String(ab.name), rect: bounds(ab.artboardRect) });
            }
            // Snapshot the layer list first: the temporary layer is added while exporting.
            var layers = [];
            for (i = 0; i < doc.layers.length; i++) { layers.push(doc.layers[i]); }
            var outLayers = [];
            for (i = 0; i < layers.length; i++) { outLayers.push(exportLayer(layers[i], ctx)); }
            var name = String(doc.name).replace(/\.[^.]*$/, "");
            var source = { name: String(doc.name) };
            if (!options.stable) {
                try { if (String(doc.path) !== "") { source.path = String(doc.fullName.fsName); } } catch (e2) { /* unsaved */ }
            }
            result = {
                format: EXP.FORMAT,
                format_version: EXP.FORMAT_VERSION,
                generator: { name: "bae-ai-export", version: EXP.VERSION },
                host: {
                    app: String(app.name),
                    version: String(app.version),
                    os: String($.os),
                    locale: String(get(app, "locale", ""))
                },
                exported_at: options.stable ? undefined : isoNow(),
                source: source,
                document: {
                    name: name,
                    color_space: enumToken(doc.documentColorSpace).toLowerCase(),
                    units: "pt",
                    y_axis: "down"
                },
                artboards: artboards,
                layers: outLayers,
                report: { warnings: ctx.warnings }
            };
        } finally {
            if (ctx.tmp) { try { ctx.tmp.remove(); } catch (e3) { /* keep going */ } }
            if (savedCoords !== null) { try { app.coordinateSystem = savedCoords; } catch (e4) { /* ignore */ } }
        }
        return result;
    };

    EXP.writeJSON = function (obj, path, pretty) {
        var f = new File(path);
        if (f.parent && !f.parent.exists) { f.parent.create(); }
        f.encoding = "UTF-8";
        f.lineFeed = "Unix";
        if (!f.open("w")) { throw new Error("cannot write " + path); }
        f.write(EXP.stringify(obj, pretty));
        f.write("\n");
        f.close();
        return f;
    };

    // Export the document; returns a one-line summary.
    EXP.run = function (options) {
        options = withDefaults(options);
        var doc = options.document || app.activeDocument;
        if (!options.outputPath) { throw new Error("outputPath missing"); }
        var data = EXP.exportDocument(doc, options);
        EXP.writeJSON(data, options.outputPath, options.pretty);
        var n = 0;
        for (var i = 0; i < data.layers.length; i++) { n += data.layers[i].items.length; }
        return data.artboards.length + " artboards, " + data.layers.length + " layers, " + n +
            " top-level items, " + data.report.warnings.length + " warnings -> " + options.outputPath;
    };

    function interactive() {
        if (app.documents.length === 0) {
            alert("BAE: open a document first.");
            return;
        }
        var doc = app.activeDocument;
        var base = String(doc.name).replace(/\.[^.]*$/, "");
        var start = doc.path && String(doc.path) !== "" ? doc.path : Folder.myDocuments;
        var target = new File(start + "/" + base + ".bae-ai.json").saveDlg("BAE: export Illustrator document", "*.json");
        if (!target) { return; }
        var summary = EXP.run({ outputPath: target.fsName, document: doc });
        alert("BAE export\n" + summary);
    }

    global.BAE_AI_EXPORT = EXP;
    if (global.BAE_AI_EXPORT_NO_AUTORUN) { return; }
    if (typeof global.BAE_AI_EXPORT_OPTIONS !== "undefined" && global.BAE_AI_EXPORT_OPTIONS) {
        var opts = global.BAE_AI_EXPORT_OPTIONS;
        global.BAE_AI_EXPORT_OPTIONS = undefined;
        global.BAE_AI_EXPORT_RESULT = EXP.run(opts);
        $.writeln(global.BAE_AI_EXPORT_RESULT);
    } else {
        interactive();
    }
// $.global, not `this`: a file run with $.evalFile from inside a function sees that
// function as `this` in Illustrator.
}($.global));
