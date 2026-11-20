// bae-export.jsx - export an After Effects project to the BAE interchange format.
//
// Format: docs/interop/bae-interchange.md (format_version 0.1.0).
// Runs in After Effects CC 2018 (15.0) and later. ExtendScript is ES3: no JSON object,
// no Array.prototype.indexOf/forEach/map, no String.prototype.trim, no Object.keys.
//
// Interactive use: File > Scripts > Run Script File... and pick this file.
// Headless use: define a global BAE_EXPORT_OPTIONS object before evaluating this file,
// e.g. from a wrapper script run with `AfterFX -r wrapper.jsx`:
//     var BAE_EXPORT_OPTIONS = {outputPath: "C:/out/project.bae-interchange.json"};
//     $.evalFile("C:/bae/bridges/ae/bae-export.jsx");
// Tests define BAE_EXPORT_NO_AUTORUN = true and call BAE_EXPORT.buildDocument directly.

(function (global) {
    var BAE = {};
    BAE.VERSION = "0.1.0";
    BAE.FORMAT_VERSION = "0.1.0";
    var FLICKS_PER_SECOND = 705600000;
    var MAX_ATTR_NAME = 256;

    // ------------------------------------------------------------------ utilities

    function hasOwn(obj, key) {
        return Object.prototype.hasOwnProperty.call(obj, key);
    }

    function isArray(v) {
        return Object.prototype.toString.call(v) === "[object Array]";
    }

    function isFiniteNumber(v) {
        return typeof v === "number" && isFinite(v);
    }

    function pad(n, width) {
        var s = String(n);
        while (s.length < width) {
            s = "0" + s;
        }
        return s;
    }

    function isoDate(d) {
        return d.getUTCFullYear() + "-" + pad(d.getUTCMonth() + 1, 2) + "-" + pad(d.getUTCDate(), 2) +
            "T" + pad(d.getUTCHours(), 2) + ":" + pad(d.getUTCMinutes(), 2) + ":" + pad(d.getUTCSeconds(), 2) + "Z";
    }

    function hex(code, width) {
        return pad(code.toString(16).toUpperCase(), width);
    }

    /** Read obj[key], returning undefined instead of throwing (AE throws on many accessors). */
    function tryGet(obj, key) {
        try {
            return obj[key];
        } catch (e) {
            return undefined;
        }
    }

    BAE.toFlicks = function (seconds) {
        return Math.round(seconds * FLICKS_PER_SECOND);
    };

    // ------------------------------------------------------------------ JSON (ES3)

    function quote(s) {
        var out = ['"'];
        for (var i = 0; i < s.length; i++) {
            var c = s.charAt(i);
            var code = s.charCodeAt(i);
            if (c === '"') {
                out.push('\\"');
            } else if (c === "\\") {
                out.push("\\\\");
            } else if (code === 10) {
                out.push("\\n");
            } else if (code === 13) {
                out.push("\\r");
            } else if (code === 9) {
                out.push("\\t");
            } else if (code === 8) {
                out.push("\\b");
            } else if (code === 12) {
                out.push("\\f");
            } else if (code < 32 || code === 0x2028 || code === 0x2029) {
                out.push("\\u" + hex(code, 4));
            } else {
                out.push(c);
            }
        }
        out.push('"');
        return out.join("");
    }

    function stringifyInto(v, indent, depth, out) {
        if (v === null || v === undefined) {
            out.push("null");
            return;
        }
        var t = typeof v;
        if (t === "number") {
            // Callers replace non-finite numbers before this point; this is the last guard.
            out.push(isFinite(v) ? (v === 0 ? "0" : String(v)) : "null");
            return;
        }
        if (t === "boolean") {
            out.push(v ? "true" : "false");
            return;
        }
        if (t === "string") {
            out.push(quote(v));
            return;
        }
        var nl = "";
        var inner = "";
        var outer = "";
        if (indent) {
            nl = "\n";
            outer = new Array(depth + 1).join(indent);
            inner = outer + indent;
        }
        var first = true;
        if (isArray(v)) {
            if (v.length === 0) {
                out.push("[]");
                return;
            }
            // Arrays of numbers stay on one line: vectors, colors, tangents.
            var flat = true;
            for (var j = 0; j < v.length; j++) {
                if (typeof v[j] === "object" && v[j] !== null) {
                    flat = false;
                    break;
                }
            }
            out.push("[");
            for (var i = 0; i < v.length; i++) {
                if (!first) {
                    out.push(flat && indent ? ", " : ",");
                }
                if (!flat) {
                    out.push(nl + inner);
                }
                stringifyInto(v[i], indent, depth + 1, out);
                first = false;
            }
            out.push(flat ? "]" : nl + outer + "]");
            return;
        }
        out.push("{");
        for (var k in v) {
            if (!hasOwn(v, k) || v[k] === undefined || typeof v[k] === "function") {
                continue;
            }
            if (!first) {
                out.push(",");
            }
            out.push(nl + inner + quote(k) + (indent ? ": " : ":"));
            stringifyInto(v[k], indent, depth + 1, out);
            first = false;
        }
        out.push(first ? "}" : nl + outer + "}");
    }

    /** JSON.stringify replacement. `indent` is a string such as "  " or "" for compact output. */
    BAE.stringify = function (v, indent) {
        var out = [];
        stringifyInto(v, indent || "", 0, out);
        return out.join("");
    };

    // ------------------------------------------------------------------ names

    function isAsciiAlnum(code) {
        return (code >= 48 && code <= 57) || (code >= 65 && code <= 90) || (code >= 97 && code <= 122);
    }

    /** Non-ASCII characters that Rust's char::is_alphanumeric accepts (a conservative subset). */
    function isKeptNonAscii(code) {
        return (code >= 0xC0 && code <= 0x24F && code !== 0xD7 && code !== 0xF7) ||
            (code >= 0x391 && code <= 0x3A9) || (code >= 0x3B1 && code <= 0x3C9) ||
            (code >= 0x401 && code <= 0x451 && code !== 0x40D && code !== 0x450) ||
            (code >= 0x3041 && code <= 0x3096) || (code >= 0x30A1 && code <= 0x30FA) || code === 0x30FC ||
            (code >= 0x3400 && code <= 0x4DBF) || (code >= 0x4E00 && code <= 0x9FFF) ||
            (code >= 0xAC00 && code <= 0xD7A3) ||
            (code >= 0xFF10 && code <= 0xFF19) || (code >= 0xFF21 && code <= 0xFF3A) || (code >= 0xFF41 && code <= 0xFF5A);
    }

    /** Turn a display name into a bae-model prim name (docs 4.5). */
    BAE.sanitizeName = function (name, fallback) {
        var s = String(name === undefined || name === null ? "" : name);
        var out = [];
        for (var i = 0; i < s.length; i++) {
            var code = s.charCodeAt(i);
            if (isAsciiAlnum(code) || code === 95 || code === 45 || isKeptNonAscii(code)) {
                out.push(s.charAt(i));
            } else {
                out.push("_");
            }
        }
        var r = out.join("").replace(/_+/g, "_").replace(/^_+/, "").replace(/_+$/, "");
        if (r === "") {
            return fallback;
        }
        if (r.charAt(0) === "-") {
            return fallback + "_" + r;
        }
        return r;
    };

    /** Hands out unique sibling names. */
    function NameRegistry() {
        this.used = {};
    }

    NameRegistry.prototype.take = function (display, fallback) {
        var base = BAE.sanitizeName(display, fallback);
        var name = base;
        var n = 2;
        while (hasOwn(this.used, "$" + name)) {
            name = base + "_" + n;
            n++;
        }
        this.used["$" + name] = true;
        return name;
    };

    /** Encode one matchName as an attribute-name segment (docs 4.8). */
    BAE.encodeSegment = function (s) {
        var out = [];
        for (var i = 0; i < s.length; i++) {
            var code = s.charCodeAt(i);
            if (isAsciiAlnum(code)) {
                out.push(s.charAt(i));
            } else if (code === 32) {
                out.push("_");
            } else if (code === 45) {
                out.push("--");
            } else if (code < 128) {
                out.push("-" + hex(code, 2));
            } else {
                out.push("-u" + hex(code, 4));
            }
        }
        return out.join("");
    };

    /** Inverse of encodeSegment. Returns {name, occurrence}; occurrence is 1 unless "-n<k>" ends the segment. */
    BAE.decodeSegment = function (s) {
        var out = [];
        var occurrence = 1;
        var i = 0;
        while (i < s.length) {
            var c = s.charAt(i);
            if (c === "_") {
                out.push(" ");
                i++;
            } else if (c !== "-") {
                out.push(c);
                i++;
            } else {
                var next = s.charAt(i + 1);
                if (next === "-") {
                    out.push("-");
                    i += 2;
                } else if (next === "u") {
                    out.push(String.fromCharCode(parseInt(s.substr(i + 2, 4), 16)));
                    i += 6;
                } else if (next === "n") {
                    occurrence = parseInt(s.substr(i + 2), 10);
                    i = s.length;
                } else {
                    out.push(String.fromCharCode(parseInt(s.substr(i + 1, 2), 16)));
                    i += 3;
                }
            }
        }
        return {name: out.join(""), occurrence: occurrence};
    };

    /** segments: array of {matchName, occurrence}. */
    function attrKey(segments) {
        var parts = [];
        for (var i = 0; i < segments.length; i++) {
            var enc = BAE.encodeSegment(segments[i].matchName);
            if (segments[i].occurrence > 1) {
                enc += "-n" + segments[i].occurrence;
            }
            parts.push(enc);
        }
        return "ae:" + parts.join(":");
    }

    function segmentPathText(segments) {
        var parts = [];
        for (var i = 0; i < segments.length; i++) {
            parts.push(segments[i].matchName);
        }
        return parts.join(" / ");
    }

    // ------------------------------------------------------------------ enums

    /** Enum token tables, built lazily from the AE enum objects by constant name. */
    var ENUM_NAMES = {
        BlendingMode: ["ADD", "ALPHA_ADD", "CLASSIC_COLOR_BURN", "CLASSIC_COLOR_DODGE", "CLASSIC_DIFFERENCE", "COLOR",
            "COLOR_BURN", "COLOR_DODGE", "DANCING_DISSOLVE", "DARKEN", "DARKER_COLOR", "DIFFERENCE", "DISSOLVE", "DIVIDE",
            "EXCLUSION", "HARD_LIGHT", "HARD_MIX", "HUE", "LIGHTEN", "LIGHTER_COLOR", "LINEAR_BURN", "LINEAR_DODGE",
            "LINEAR_LIGHT", "LUMINESCENT_PREMUL", "LUMINOSITY", "MULTIPLY", "NORMAL", "OVERLAY", "PIN_LIGHT", "SATURATION",
            "SCREEN", "SILHOUETE_ALPHA=silhouette_alpha", "SILHOUETTE_ALPHA", "SILHOUETTE_LUMA", "SOFT_LIGHT",
            "STENCIL_ALPHA", "STENCIL_LUMA", "SUBTRACT", "VIVID_LIGHT"],
        TrackMatteType: ["NO_TRACK_MATTE", "ALPHA", "ALPHA_INVERTED", "LUMA", "LUMA_INVERTED"],
        AutoOrientType: ["NO_AUTO_ORIENT", "ALONG_PATH", "CAMERA_OR_POINT_OF_INTEREST", "CHARACTERS_TOWARD_CAMERA"],
        FrameBlendingType: ["NO_FRAME_BLEND", "FRAME_MIX", "PIXEL_MOTION"],
        LayerQuality: ["BEST", "DRAFT", "WIREFRAME"],
        LayerSamplingQuality: ["BICUBIC", "BILINEAR"],
        LightType: ["PARALLEL", "SPOT", "POINT", "AMBIENT"],
        MaskMode: ["NONE", "ADD", "SUBTRACT", "INTERSECT", "LIGHTEN", "DARKEN", "DIFFERENCE"],
        MaskMotionBlur: ["SAME_AS_LAYER", "ON", "OFF"],
        MaskFeatherFalloff: ["FFO_LINEAR=linear", "FFO_SMOOTH=smooth"],
        AlphaMode: ["IGNORE", "STRAIGHT", "PREMULTIPLIED"],
        FieldSeparationType: ["OFF", "UPPER_FIELD_FIRST", "LOWER_FIELD_FIRST"],
        PulldownPhase: ["OFF", "SSWWW", "SWWWS", "SWWWW_24P_ADVANCE", "WSSWW", "WSWWW_24P_ADVANCE", "WWSSW",
            "WWSWW_24P_ADVANCE", "WWWSS", "WWWSW_24P_ADVANCE", "WWWWS_24P_ADVANCE"],
        ParagraphJustification: ["LEFT_JUSTIFY", "RIGHT_JUSTIFY", "CENTER_JUSTIFY", "FULL_JUSTIFY_LASTLINE_LEFT",
            "FULL_JUSTIFY_LASTLINE_RIGHT", "FULL_JUSTIFY_LASTLINE_CENTER", "FULL_JUSTIFY_LASTLINE_FULL",
            "MULTIPLE_JUSTIFICATIONS"],
        KeyframeInterpolationType: ["LINEAR", "BEZIER", "HOLD"],
        TimeDisplayType: ["TIMECODE", "FRAMES"],
        FramesCountType: ["FC_START_0=start_0", "FC_START_1=start_1", "FC_TIMECODE_CONVERSION=timecode_conversion"],
        FeetFramesFilmType: ["MM16=mm16", "MM35=mm35"],
        FootageTimecodeDisplayStartType: ["FTCS_START_0=start_0", "FTCS_USE_SOURCE_MEDIA=use_source_media"],
        GpuAccelType: ["CUDA", "METAL", "OPENCL", "SOFTWARE", "VULKAN", "DIRECTX"]
    };
    var enumCache = {};

    function enumTable(enumName) {
        if (hasOwn(enumCache, enumName)) {
            return enumCache[enumName];
        }
        var table = {};
        var obj = global[enumName];
        var names = ENUM_NAMES[enumName] || [];
        if (obj) {
            for (var i = 0; i < names.length; i++) {
                var parts = names[i].split("=");
                var constant = parts[0];
                var token = parts.length > 1 ? parts[1] : constant.toLowerCase();
                var v = tryGet(obj, constant);
                if (v !== undefined && !hasOwn(table, "$" + v)) {
                    table["$" + v] = token;
                }
            }
        }
        enumCache[enumName] = table;
        return table;
    }

    // ------------------------------------------------------------------ export context

    function Exporter(options) {
        this.options = options || {};
        this.warnings = [];
        this.unsupported = [];
        this.stats = {folders: 0, assets: 0, comps: 0, layers: 0, properties: 0, keyframes: 0, expressions: 0,
            effects: 0, masks: 0, markers: 0};
        this.itemPaths = {};     // "$<item id>" -> prim path
        this.manifest = [];
    }

    Exporter.prototype.warn = function (code, path, attr, message) {
        var w = {code: code, path: path, message: message};
        if (attr) {
            w.attr = attr;
        }
        this.warnings.push(w);
    };

    Exporter.prototype.unsupportedItem = function (code, path, attr, matchName, message) {
        var u = {code: code, path: path, message: message};
        if (attr) {
            u.attr = attr;
        }
        if (matchName) {
            u.match_name = matchName;
        }
        this.unsupported.push(u);
    };

    Exporter.prototype.enumToken = function (enumName, value, path, attr) {
        if (value === undefined || value === null) {
            return undefined;
        }
        var table = enumTable(enumName);
        if (hasOwn(table, "$" + value)) {
            return table["$" + value];
        }
        this.warn("unknown_enum", path, attr, enumName + " value " + value + " has no known name");
        return "unknown_" + value;
    };

    Exporter.prototype.num = function (v, path, attr) {
        if (isFiniteNumber(v)) {
            return v;
        }
        this.warn("non_finite", path, attr, "non-finite number " + v + " written as 0");
        return 0;
    };

    // Value constructors (bae-model Value JSON).
    function vFloat(v) {
        return {type: "float", value: v};
    }

    function vInt(v) {
        return {type: "int", value: Math.round(v)};
    }

    function vBool(v) {
        return {type: "bool", value: !!v};
    }

    function vString(v) {
        return {type: "string", value: String(v)};
    }

    function vEnum(v) {
        return {type: "enum", value: v};
    }

    function vRef(path) {
        return {type: "prim_ref", value: path};
    }

    /** Attribute setters on a prim: each takes the raw AE value and skips undefined. */
    Exporter.prototype.setAttr = function (prim, name, value) {
        if (value === undefined) {
            return;
        }
        if (!prim.attrs) {
            prim.attrs = {};
        }
        prim.attrs[name] = {value: value};
    };

    Exporter.prototype.setNum = function (prim, name, raw, path) {
        if (raw === undefined || raw === null) {
            return;
        }
        this.setAttr(prim, name, vFloat(this.num(raw, path, name)));
    };

    Exporter.prototype.setInt = function (prim, name, raw, path) {
        if (raw === undefined || raw === null) {
            return;
        }
        this.setAttr(prim, name, vInt(this.num(raw, path, name)));
    };

    Exporter.prototype.setTime = function (prim, name, seconds, path) {
        if (seconds === undefined || seconds === null) {
            return;
        }
        this.setAttr(prim, name, vInt(BAE.toFlicks(this.num(seconds, path, name))));
    };

    Exporter.prototype.setBool = function (prim, name, raw) {
        if (raw === undefined || raw === null) {
            return;
        }
        this.setAttr(prim, name, vBool(raw));
    };

    Exporter.prototype.setString = function (prim, name, raw) {
        if (raw === undefined || raw === null) {
            return;
        }
        this.setAttr(prim, name, vString(raw));
    };

    Exporter.prototype.setEnum = function (prim, name, enumName, raw, path) {
        var token = this.enumToken(enumName, raw, path, name);
        if (token !== undefined) {
            this.setAttr(prim, name, vEnum(token));
        }
    };

    Exporter.prototype.color = function (raw, path, attr) {
        var c = [];
        for (var i = 0; i < 4; i++) {
            c.push(i < raw.length ? this.num(raw[i], path, attr) : 1);
        }
        return {type: "color", value: c};
    };

    function newPrim(name, primType) {
        var p = {name: name, specifier: "def"};
        if (primType) {
            p.prim_type = primType;
        }
        return p;
    }

    function addChild(parent, child) {
        if (!parent.children) {
            parent.children = [];
        }
        parent.children.push(child);
        return child;
    }

    /** A prim plus what the property walker needs to know about it. */
    function PrimCtx(prim, path, layerCtx) {
        this.prim = prim;
        this.path = path;
        this.layer = layerCtx;      // LayerCtx or null
        this.containers = {};       // indexed-group matchName -> container prim
        this.containerNames = new NameRegistry();
    }

    PrimCtx.prototype.container = function (matchName) {
        if (hasOwn(this.containers, "$" + matchName)) {
            return this.containers["$" + matchName];
        }
        var alias = CONTAINER_ALIAS[matchName] || matchName;
        var name = this.containerNames.take(alias, "group");
        var c = addChild(this.prim, newPrim(name, null));
        c.__path = this.path + "/" + name;
        c.__names = new NameRegistry();
        this.containers["$" + matchName] = c;
        return c;
    };

    // ------------------------------------------------------------------ property tree

    var CONTAINER_ALIAS = {
        "ADBE Mask Parade": "masks",
        "ADBE Effect Parade": "effects",
        "ADBE Root Vectors Group": "contents",
        "ADBE Vectors Group": "contents",
        "ADBE Text Animators": "animators",
        "ADBE Text Selectors": "selectors"
    };

    var PRIM_TYPE_BY_MATCH_NAME = {
        "ADBE Mask Atom": "Mask",
        "ADBE Vector Group": "ShapeGroup",
        "ADBE Vector Shape - Group": "Path",
        "ADBE Vector Shape - Rect": "Rect",
        "ADBE Vector Shape - Ellipse": "Ellipse",
        "ADBE Vector Shape - Star": "Star",
        "ADBE Vector Graphic - Fill": "Fill",
        "ADBE Vector Graphic - Stroke": "Stroke",
        "ADBE Vector Graphic - G-Fill": "GradientFill",
        "ADBE Vector Graphic - G-Stroke": "GradientStroke",
        "ADBE Text Animator": "TextAnimator",
        "ADBE Text Selector": "TextSelector",
        "ADBE Text Wiggly Selector": "TextSelector",
        "ADBE Text Expressible Selector": "TextSelector"
    };

    var SHAPE_OPERATOR_PREFIX = "ADBE Vector Filter - ";
    var SHAPE_OPERATORS = {
        "Merge": "merge",
        "Offset": "offset",
        "PB": "pucker_bloat",
        "Repeater": "repeater",
        "RC": "round_corners",
        "Trim": "trim",
        "Twist": "twist",
        "Roughen": "roughen",
        "Wiggler": "wiggle_paths",
        "Zigzag": "zig_zag",
        "Wiggle Transform": "wiggle_transform"
    };

    /** Properties handled elsewhere or never readable. */
    var SKIP_MATCH_NAMES = {
        "ADBE Marker": true
    };

    function childPrimType(parentMatchName, matchName) {
        if (parentMatchName === "ADBE Effect Parade") {
            return "Effect";
        }
        if (hasOwn(PRIM_TYPE_BY_MATCH_NAME, matchName)) {
            return PRIM_TYPE_BY_MATCH_NAME[matchName];
        }
        if (matchName.indexOf(SHAPE_OPERATOR_PREFIX) === 0) {
            return "ShapeOperator";
        }
        return "ae:Group";
    }

    /** Walk the children of an AE property group into `ctx`; `segments` is the path from ctx.prim. */
    Exporter.prototype.walkGroup = function (group, ctx, segments) {
        var seen = {};
        var n = group.numProperties;
        for (var i = 1; i <= n; i++) {
            var p;
            try {
                p = group.property(i);
            } catch (e) {
                this.warn("read_error", ctx.path, null, "property " + i + " of " + group.matchName + ": " + e);
                continue;
            }
            if (!p || hasOwn(SKIP_MATCH_NAMES, p.matchName) || this.skipForLayer(p, ctx)) {
                continue;
            }
            var key = "$" + p.matchName;
            seen[key] = (seen[key] || 0) + 1;
            var seg = segments.concat([{matchName: p.matchName, occurrence: seen[key]}]);
            if (p.propertyType === PropertyType.PROPERTY) {
                this.exportProperty(p, ctx, seg);
            } else if (p.propertyType === PropertyType.INDEXED_GROUP) {
                this.walkIndexedGroup(p, ctx, seg);
            } else {
                this.walkGroup(p, ctx, seg);
            }
        }
    };

    /** Layer-level groups that cannot be read in the layer's current state. */
    Exporter.prototype.skipForLayer = function (p, ctx) {
        if (!ctx.layer || ctx.prim !== ctx.layer.prim) {
            return false;
        }
        var layer = ctx.layer.layer;
        if (p.matchName === "ADBE Time Remapping") {
            return !tryGet(layer, "timeRemapEnabled");
        }
        if (p.matchName === "ADBE Audio Group") {
            return !tryGet(layer, "hasAudio");
        }
        return false;
    };

    Exporter.prototype.walkIndexedGroup = function (group, ctx, segments) {
        var n = group.numProperties;
        var seen = {};
        for (var i = 1; i <= n; i++) {
            var p;
            try {
                p = group.property(i);
            } catch (e) {
                this.warn("read_error", ctx.path, null, "property " + i + " of " + group.matchName + ": " + e);
                continue;
            }
            if (!p) {
                continue;
            }
            if (p.propertyType === PropertyType.PROPERTY) {
                var key = "$" + p.matchName;
                seen[key] = (seen[key] || 0) + 1;
                this.exportProperty(p, ctx, segments.concat([{matchName: p.matchName, occurrence: seen[key]}]));
                continue;
            }
            var container = ctx.container(group.matchName);
            var primType = childPrimType(group.matchName, p.matchName);
            var displayName = tryGet(p, "name");
            var name;
            if (group.matchName === "ADBE Mask Parade" && ctx.layer) {
                name = ctx.layer.maskNames[i - 1];
                container.__names.take(name, "mask");
            } else {
                name = container.__names.take(displayName, fallbackName(primType));
            }
            var child = addChild(container, newPrim(name, primType));
            var childPath = container.__path + "/" + name;
            this.setString(child, "match_name", p.matchName);
            this.setString(child, "name", displayName);
            if (tryGet(p, "canSetEnabled") !== false) {
                this.setBool(child, "enabled", tryGet(p, "enabled"));
            }
            if (primType === "Mask") {
                this.maskAttrs(p, child, childPath);
                this.stats.masks++;
            } else if (primType === "Effect") {
                this.stats.effects++;
            } else if (primType === "ShapeOperator") {
                var op = p.matchName.substr(SHAPE_OPERATOR_PREFIX.length);
                this.setAttr(child, "operator", vEnum(hasOwn(SHAPE_OPERATORS, op) ? SHAPE_OPERATORS[op] : p.matchName));
            }
            this.walkGroup(p, new PrimCtx(child, childPath, ctx.layer), []);
        }
    };

    function fallbackName(primType) {
        var map = {Mask: "mask", Effect: "effect", ShapeGroup: "group", Path: "path", Rect: "rect", Ellipse: "ellipse",
            Star: "star", Fill: "fill", Stroke: "stroke", GradientFill: "gradient_fill", GradientStroke: "gradient_stroke",
            ShapeOperator: "operator", TextAnimator: "animator", TextSelector: "selector"};
        return hasOwn(map, primType) ? map[primType] : "item";
    }

    Exporter.prototype.maskAttrs = function (mask, prim, path) {
        this.setEnum(prim, "mode", "MaskMode", tryGet(mask, "maskMode"), path);
        this.setBool(prim, "inverted", tryGet(mask, "inverted"));
        this.setBool(prim, "roto_bezier", tryGet(mask, "rotoBezier"));
        this.setEnum(prim, "motion_blur", "MaskMotionBlur", tryGet(mask, "maskMotionBlur"), path);
        this.setBool(prim, "locked", tryGet(mask, "locked"));
        var c = tryGet(mask, "color");
        if (c) {
            this.setAttr(prim, "color", this.color(c, path, "color"));
        }
        this.setEnum(prim, "feather_falloff", "MaskFeatherFalloff", tryGet(mask, "maskFeatherFalloff"), path);
    };

    Exporter.prototype.exportProperty = function (p, ctx, segments) {
        var vt = p.propertyValueType;
        if (vt === PropertyValueType.NO_VALUE && p.matchName !== "ADBE Vector Grad Colors") {
            return;
        }
        var key = attrKey(segments);
        if (key.length > MAX_ATTR_NAME) {
            this.unsupportedItem("attr_name_too_long", ctx.path, null, p.matchName,
                "encoded attribute name longer than " + MAX_ATTR_NAME + ": " + segmentPathText(segments));
            return;
        }
        if (p.matchName === "ADBE Vector Grad Colors") {
            this.unsupportedItem("gradient_colors", ctx.path, key, p.matchName,
                "gradient color stops are not readable from ExtendScript");
            return;
        }
        if (vt === PropertyValueType.CUSTOM_VALUE) {
            this.unsupportedItem("custom_value", ctx.path, key, p.matchName,
                "CUSTOM_VALUE property is not readable from ExtendScript: " + segmentPathText(segments));
            return;
        }
        if (vt === PropertyValueType.MARKER) {
            return;
        }
        var opts = this.options;
        var numKeys = 0;
        var expr = "";
        try {
            numKeys = p.numKeys;
            expr = p.canSetExpression ? p.expression : "";
            if (opts.onlyModified && numKeys === 0 && !expr && tryGet(p, "isModified") === false) {
                return;
            }
        } catch (e0) {
            this.warn("read_error", ctx.path, key, String(e0));
            return;
        }
        var spec = {};
        var info = {vt: vt, path: ctx.path, key: key, ctx: ctx, spatial: !!tryGet(p, "isSpatial")};
        try {
            if (numKeys > 0) {
                spec.keyframes = this.keyframes(p, info);
            } else {
                var v = this.convertValue(p.valueAtTime(0, true), info);
                if (v !== undefined) {
                    spec.value = v;
                }
            }
        } catch (e1) {
            this.warn("read_error", ctx.path, key, String(e1));
            return;
        }
        if (expr) {
            spec.expression = {source: expr, enabled: !!tryGet(p, "expressionEnabled")};
            this.stats.expressions++;
            var err = tryGet(p, "expressionError");
            if (err) {
                this.warn("expression_error", ctx.path, key, String(err));
            }
        }
        if (spec.value === undefined && spec.keyframes === undefined && spec.expression === undefined) {
            return;
        }
        if (!ctx.prim.attrs) {
            ctx.prim.attrs = {};
        }
        ctx.prim.attrs[key] = spec;
        this.stats.properties++;
    };

    Exporter.prototype.keyframes = function (p, info) {
        var out = {};
        var n = p.numKeys;
        for (var i = 1; i <= n; i++) {
            var value = this.convertValue(p.keyValue(i), info);
            if (value === undefined) {
                continue;
            }
            var kf = {value: value};
            var tin = tryCall(p, "keyInInterpolationType", i);
            var tout = tryCall(p, "keyOutInterpolationType", i);
            if (tin !== undefined) {
                kf.interp_in = this.enumToken("KeyframeInterpolationType", tin, info.path, info.key);
            }
            if (tout !== undefined) {
                kf.interp_out = this.enumToken("KeyframeInterpolationType", tout, info.path, info.key);
            }
            var ein = this.eases(tryCall(p, "keyInTemporalEase", i), info);
            var eout = this.eases(tryCall(p, "keyOutTemporalEase", i), info);
            if (ein) {
                kf.ease_in = ein;
            }
            if (eout) {
                kf.ease_out = eout;
            }
            var flags = {};
            if (tryCall(p, "keyTemporalContinuous", i)) {
                flags.temporal_continuous = true;
            }
            if (tryCall(p, "keyTemporalAutoBezier", i)) {
                flags.temporal_auto_bezier = true;
            }
            if (info.spatial) {
                var sin = tryCall(p, "keyInSpatialTangent", i);
                var sout = tryCall(p, "keyOutSpatialTangent", i);
                if (sin) {
                    kf.spatial_in = this.vec3(sin, info);
                }
                if (sout) {
                    kf.spatial_out = this.vec3(sout, info);
                }
                if (tryCall(p, "keySpatialContinuous", i)) {
                    flags.spatial_continuous = true;
                }
                if (tryCall(p, "keySpatialAutoBezier", i)) {
                    flags.spatial_auto_bezier = true;
                }
                if (tryCall(p, "keyRoving", i)) {
                    flags.roving = true;
                }
            }
            for (var f in flags) {
                if (hasOwn(flags, f)) {
                    kf.flags = flags;
                    break;
                }
            }
            var label = tryCall(p, "keyLabel", i);
            if (label) {
                kf.label = label;
            }
            out[String(BAE.toFlicks(p.keyTime(i)))] = kf;
            this.stats.keyframes++;
        }
        return out;
    };

    function tryCall(obj, method, arg) {
        try {
            return obj[method](arg);
        } catch (e) {
            return undefined;
        }
    }

    Exporter.prototype.eases = function (list, info) {
        if (!list || !list.length) {
            return undefined;
        }
        var out = [];
        for (var i = 0; i < list.length; i++) {
            out.push({speed: this.num(list[i].speed, info.path, info.key),
                influence: this.num(list[i].influence, info.path, info.key)});
        }
        return out;
    };

    Exporter.prototype.vec3 = function (a, info) {
        return [this.num(a[0], info.path, info.key), this.num(a.length > 1 ? a[1] : 0, info.path, info.key),
            this.num(a.length > 2 ? a[2] : 0, info.path, info.key)];
    };

    /** Convert a raw AE property value to a bae-model Value (docs 4.1). Undefined = not representable. */
    Exporter.prototype.convertValue = function (raw, info) {
        var PVT = PropertyValueType;
        var vt = info.vt;
        var path = info.path;
        var key = info.key;
        if (vt === PVT.OneD) {
            return vFloat(this.num(raw, path, key));
        }
        if (vt === PVT.TwoD || vt === PVT.TwoD_SPATIAL) {
            return {type: "vec2", value: [this.num(raw[0], path, key), this.num(raw[1], path, key)]};
        }
        if (vt === PVT.ThreeD || vt === PVT.ThreeD_SPATIAL) {
            return {type: "vec3", value: this.vec3(raw, info)};
        }
        if (vt === PVT.COLOR) {
            return this.color(raw, path, key);
        }
        if (vt === PVT.SHAPE) {
            return this.bezier(raw, info);
        }
        if (vt === PVT.TEXT_DOCUMENT) {
            return this.textDocument(raw, info);
        }
        if (vt === PVT.LAYER_INDEX) {
            var layerIndex = Math.round(raw);
            if (layerIndex <= 0) {
                return undefined;
            }
            var lp = info.ctx.layer && info.ctx.layer.comp.layerPaths[layerIndex];
            if (!lp) {
                this.warn("read_error", path, key, "layer index " + layerIndex + " not found");
                return undefined;
            }
            return vRef(lp);
        }
        if (vt === PVT.MASK_INDEX) {
            var maskIndex = Math.round(raw);
            if (maskIndex <= 0) {
                return undefined;
            }
            var mp = info.ctx.layer && info.ctx.layer.maskPaths[maskIndex - 1];
            if (!mp) {
                this.warn("read_error", path, key, "mask index " + maskIndex + " not found");
                return undefined;
            }
            return vRef(mp);
        }
        this.warn("read_error", path, key, "unhandled PropertyValueType " + vt);
        return undefined;
    };

    Exporter.prototype.points = function (list, info) {
        var out = [];
        for (var i = 0; i < list.length; i++) {
            out.push([this.num(list[i][0], info.path, info.key), this.num(list[i][1], info.path, info.key)]);
        }
        return out;
    };

    Exporter.prototype.bezier = function (shape, info) {
        var segs = tryGet(shape, "featherSegLocs");
        if (segs && segs.length > 0 && !info.featherReported) {
            info.featherReported = true;
            this.unsupportedItem("mask_feather_points", info.path, info.key, null,
                segs.length + " mask feather point(s) not exported");
        }
        return {type: "bezier", value: {
            vertices: this.points(shape.vertices, info),
            in_tangents: this.points(shape.inTangents, info),
            out_tangents: this.points(shape.outTangents, info),
            closed: !!shape.closed
        }};
    };

    var TEXT_FIELDS = [
        // [AE attribute, BAE key, kind]
        ["font", "font", "string"],
        ["fontFamily", "font_family", "string"],
        ["fontStyle", "font_style", "string"],
        ["fontSize", "font_size", "number"],
        ["applyFill", "apply_fill", "bool"],
        ["applyStroke", "apply_stroke", "bool"],
        ["fillColor", "fill_color", "color"],
        ["strokeColor", "stroke_color", "color"],
        ["strokeWidth", "stroke_width", "number"],
        ["strokeOverFill", "stroke_over_fill", "bool"],
        ["justification", "justification", "justification"],
        ["tracking", "tracking", "number"],
        ["leading", "leading", "number"],
        ["autoLeading", "auto_leading", "bool"],
        ["baselineShift", "baseline_shift", "number"],
        ["horizontalScale", "horizontal_scale", "number"],
        ["verticalScale", "vertical_scale", "number"],
        ["fauxBold", "faux_bold", "bool"],
        ["fauxItalic", "faux_italic", "bool"],
        ["allCaps", "all_caps", "bool"],
        ["smallCaps", "small_caps", "bool"],
        ["superscript", "superscript", "bool"],
        ["subscript", "subscript", "bool"],
        ["tsume", "tsume", "number"],
        ["boxText", "box_text", "bool"],
        ["pointText", "point_text", "bool"],
        ["boxTextSize", "box_size", "point"],
        ["boxTextPos", "box_position", "point"]
    ];

    Exporter.prototype.textDocument = function (td, info) {
        var o = {text: String(td.text)};
        for (var i = 0; i < TEXT_FIELDS.length; i++) {
            var f = TEXT_FIELDS[i];
            var v = tryGet(td, f[0]);
            if (v === undefined || v === null) {
                continue;
            }
            if (f[2] === "string") {
                o[f[1]] = String(v);
            } else if (f[2] === "number") {
                o[f[1]] = this.num(v, info.path, info.key);
            } else if (f[2] === "bool") {
                o[f[1]] = !!v;
            } else if (f[2] === "color") {
                o[f[1]] = this.color(v, info.path, info.key).value;
            } else if (f[2] === "point") {
                o[f[1]] = [this.num(v[0], info.path, info.key), this.num(v[1], info.path, info.key)];
            } else if (f[2] === "justification") {
                o[f[1]] = this.enumToken("ParagraphJustification", v, info.path, info.key);
            }
        }
        // Box-only fields are meaningless on point text; AE throws or returns junk there.
        if (o.box_text === false) {
            delete o.box_size;
            delete o.box_position;
        }
        return {type: "text_document", value: o};
    };

    // ------------------------------------------------------------------ markers

    Exporter.prototype.markers = function (markerProp, owner, ownerPath) {
        if (!markerProp) {
            return;
        }
        var n = 0;
        try {
            n = markerProp.numKeys;
        } catch (e) {
            return;
        }
        if (n === 0) {
            return;
        }
        var container = addChild(owner, newPrim("markers", null));
        for (var i = 1; i <= n; i++) {
            var name = "marker_" + i;
            var path = ownerPath + "/markers/" + name;
            var m = addChild(container, newPrim(name, "Marker"));
            var mv = markerProp.keyValue(i);
            this.setTime(m, "time", markerProp.keyTime(i), path);
            this.setTime(m, "duration", tryGet(mv, "duration"), path);
            var strings = [["comment", "comment"], ["chapter", "chapter"], ["url", "url"],
                ["frameTarget", "frame_target"], ["cuePointName", "cue_point_name"]];
            for (var s = 0; s < strings.length; s++) {
                var sv = tryGet(mv, strings[s][0]);
                if (sv) {
                    this.setString(m, strings[s][1], sv);
                }
            }
            if (tryGet(mv, "eventCuePoint")) {
                this.setBool(m, "event_cue_point", true);
            }
            var label = tryGet(mv, "label");
            if (label) {
                this.setInt(m, "label", label, path);
            }
            if (tryGet(mv, "protectedRegion")) {
                this.setBool(m, "protected_region", true);
            }
            var params = tryCall(mv, "getParameters");
            if (params) {
                for (var k in params) {
                    if (hasOwn(params, k)) {
                        this.setString(m, "param:" + BAE.encodeSegment(k), params[k]);
                    }
                }
            }
            this.stats.markers++;
        }
    };

    // ------------------------------------------------------------------ project items

    function itemKey(item) {
        return "$" + item.id;
    }

    Exporter.prototype.itemCommon = function (item, prim, path) {
        this.setString(prim, "name", item.name);
        this.setInt(prim, "ae_id", item.id, path);
        var parent = tryGet(item, "parentFolder");
        if (parent && hasOwn(this.itemPaths, itemKey(parent))) {
            this.setAttr(prim, "folder", vRef(this.itemPaths[itemKey(parent)]));
        }
        var label = tryGet(item, "label");
        if (label) {
            this.setInt(prim, "label", label, path);
        }
        var comment = tryGet(item, "comment");
        if (comment) {
            this.setString(prim, "comment", comment);
        }
    };

    function isSolidSource(src) {
        if (typeof SolidSource !== "undefined" && src instanceof SolidSource) {
            return true;
        }
        return src && src.color !== undefined && src.file === undefined;
    }

    function isPlaceholderSource(src) {
        return typeof PlaceholderSource !== "undefined" && src instanceof PlaceholderSource;
    }

    var IMAGE_EXT = /\.(png|jpe?g|tiff?|tga|exr|dpx|cin|bmp|psd|gif|webp|hdr|sgi|rgb|iff|pxr|pct|pic|crw|cr2|dng|nef|arw|heic)$/i;

    Exporter.prototype.assetPrim = function (item, prim, path) {
        this.itemCommon(item, prim, path);
        var src = item.mainSource;
        var file = tryGet(item, "file");
        var kind = "file";
        if (isSolidSource(src)) {
            kind = "solid";
        } else if (!file && isPlaceholderSource(src)) {
            kind = "placeholder";
        } else if (!file && !item.footageMissing) {
            kind = "placeholder";
        } else if (src && src.isStill === false && IMAGE_EXT.test(fileName(item))) {
            kind = "sequence";
        }
        this.setAttr(prim, "asset_kind", vEnum(kind));
        this.setInt(prim, "width", tryGet(item, "width"), path);
        this.setInt(prim, "height", tryGet(item, "height"), path);
        this.setNum(prim, "pixel_aspect", tryGet(item, "pixelAspect"), path);
        this.setTime(prim, "duration", tryGet(item, "duration"), path);
        this.setNum(prim, "frame_rate", tryGet(item, "frameRate"), path);
        this.setBool(prim, "has_video", tryGet(item, "hasVideo"));
        this.setBool(prim, "has_audio", tryGet(item, "hasAudio"));
        if (item.footageMissing) {
            this.setBool(prim, "missing", true);
            this.warn("missing_footage", path, null, "footage is missing: " + fileName(item));
        }
        if (src) {
            if (kind === "solid") {
                this.setAttr(prim, "color", this.color(src.color, path, "color"));
            }
            this.setBool(prim, "is_still", tryGet(src, "isStill"));
            this.setEnum(prim, "alpha_mode", "AlphaMode", tryGet(src, "alphaMode"), path);
            var premul = tryGet(src, "premulColor");
            if (premul) {
                this.setAttr(prim, "premul_color", this.color(premul, path, "premul_color"));
            }
            this.setBool(prim, "invert_alpha", tryGet(src, "invertAlpha"));
            this.setEnum(prim, "field_separation", "FieldSeparationType", tryGet(src, "fieldSeparationType"), path);
            this.setEnum(prim, "remove_pulldown", "PulldownPhase", tryGet(src, "removePulldown"), path);
            this.setInt(prim, "loop", tryGet(src, "loop"), path);
            this.setNum(prim, "conform_frame_rate", tryGet(src, "conformFrameRate"), path);
            this.setNum(prim, "native_frame_rate", tryGet(src, "nativeFrameRate"), path);
        }
        if (kind === "file" || kind === "sequence") {
            this.manifestEntry(item, path, kind);
        }
    };

    function fileName(item) {
        var file = tryGet(item, "file");
        if (file) {
            return file.name ? decodeName(file.name) : String(file);
        }
        var missing = item.mainSource ? tryGet(item.mainSource, "missingFootagePath") : "";
        return missing ? String(missing).replace(/^.*[\\\/]/, "") : "";
    }

    function decodeName(name) {
        try {
            return File.decode(name);
        } catch (e) {
            return name;
        }
    }

    function slashPath(p) {
        return String(p).replace(/\\/g, "/");
    }

    /** Relative path from directory `fromDir` to `to` (both absolute). Undefined across drives or UNC shares. */
    BAE.relativePath = function (fromDir, to) {
        var a = slashPath(fromDir).replace(/\/+$/, "").split("/");
        var b = slashPath(to).split("/");
        var windows = /^[A-Za-z]:$/.test(a[0]) || /^[A-Za-z]:$/.test(b[0]);
        function same(x, y) {
            return windows ? String(x).toLowerCase() === String(y).toLowerCase() : x === y;
        }
        // Drive letters must match; UNC paths ("//server/share/...") must share server and share.
        var root = a[0] === "" && a[1] === "" ? 4 : 1;
        for (var r = 0; r < root; r++) {
            if (!same(a[r], b[r])) {
                return undefined;
            }
        }
        var i = 0;
        while (i < a.length && i < b.length - 1 && same(a[i], b[i])) {
            i++;
        }
        var parts = [];
        for (var j = i; j < a.length; j++) {
            parts.push("..");
        }
        for (var k = i; k < b.length; k++) {
            parts.push(b[k]);
        }
        return parts.join("/");
    };

    Exporter.prototype.manifestEntry = function (item, path, kind) {
        var file = tryGet(item, "file");
        var entry = {prim: path};
        var abs = file ? slashPath(file.fsName) : slashPath(tryGet(item.mainSource, "missingFootagePath") || "");
        entry.path_absolute = abs;
        if (this.options.outputDir) {
            var rel = BAE.relativePath(this.options.outputDir, abs);
            if (rel !== undefined) {
                entry.path_relative = rel;
            }
        }
        if (this.options.projectDir) {
            var arel = BAE.relativePath(this.options.projectDir, abs);
            if (arel !== undefined) {
                entry.aep_relative = arel;
            }
        }
        if (item.footageMissing) {
            entry.missing = true;
        } else if (file && file.exists) {
            entry.size = file.length;
            if (file.modified) {
                entry.modified = isoDate(file.modified);
            }
        }
        if (kind === "sequence") {
            entry.sequence = {first: fileName(item)};
            var frames = file && !item.footageMissing ? sequenceFiles(file) : [];
            if (frames.length) {
                entry.sequence.count = frames.length;
            }
            entry.__frames = frames;
        }
        entry.__file = file;
        this.manifest.push(entry);
    };

    /** Files in the same folder that share the frame's prefix and extension (best effort). */
    function sequenceFiles(file) {
        var name = decodeName(file.name);
        var m = /^(.*?)(\d+)(\.[^.]+)$/.exec(name);
        if (!m) {
            return [file];
        }
        var prefix = m[1];
        var ext = m[3].toLowerCase();
        var found = file.parent.getFiles(function (f) {
            if (!(f instanceof File)) {
                return false;
            }
            var n = decodeName(f.name);
            var fm = /^(.*?)(\d+)(\.[^.]+)$/.exec(n);
            return !!fm && fm[1] === prefix && fm[3].toLowerCase() === ext;
        });
        found.sort(function (x, y) {
            return x.name < y.name ? -1 : (x.name > y.name ? 1 : 0);
        });
        return found;
    }

    Exporter.prototype.compPrim = function (comp, prim, path) {
        this.itemCommon(comp, prim, path);
        this.setInt(prim, "width", comp.width, path);
        this.setInt(prim, "height", comp.height, path);
        this.setNum(prim, "pixel_aspect", comp.pixelAspect, path);
        this.setNum(prim, "frame_rate", comp.frameRate, path);
        this.setTime(prim, "duration", comp.duration, path);
        this.setTime(prim, "display_start_time", tryGet(comp, "displayStartTime"), path);
        var bg = tryGet(comp, "bgColor");
        if (bg) {
            this.setAttr(prim, "bg_color", this.color(bg, path, "bg_color"));
        }
        this.setTime(prim, "work_area_start", tryGet(comp, "workAreaStart"), path);
        this.setTime(prim, "work_area_duration", tryGet(comp, "workAreaDuration"), path);
        this.setNum(prim, "shutter_angle", tryGet(comp, "shutterAngle"), path);
        this.setNum(prim, "shutter_phase", tryGet(comp, "shutterPhase"), path);
        this.setInt(prim, "motion_blur_samples", tryGet(comp, "motionBlurSamplesPerFrame"), path);
        this.setInt(prim, "motion_blur_adaptive_limit", tryGet(comp, "motionBlurAdaptiveSampleLimit"), path);
        this.setBool(prim, "motion_blur", tryGet(comp, "motionBlur"));
        this.setBool(prim, "frame_blending", tryGet(comp, "frameBlending"));
        this.setBool(prim, "draft_3d", tryGet(comp, "draft3d"));
        this.setBool(prim, "hide_shy_layers", tryGet(comp, "hideShyLayers"));
        this.setBool(prim, "preserve_nested_frame_rate", tryGet(comp, "preserveNestedFrameRate"));
        this.setBool(prim, "preserve_nested_resolution", tryGet(comp, "preserveNestedResolution"));
        this.setBool(prim, "drop_frame", tryGet(comp, "dropFrame"));
        this.setString(prim, "renderer", tryGet(comp, "renderer"));
        var rf = tryGet(comp, "resolutionFactor");
        if (rf) {
            this.setAttr(prim, "resolution_factor", {type: "vec2", value: [this.num(rf[0], path), this.num(rf[1], path)]});
        }
        this.markers(tryGet(comp, "markerProperty"), prim, path);

        var layersPrim = null;
        var compCtx = this.compCtx(comp, path);
        for (var i = 1; i <= comp.numLayers; i++) {
            if (!layersPrim) {
                layersPrim = addChild(prim, newPrim("layers", null));
            }
            var layer = comp.layer(i);
            var lp = compCtx.layerPaths[i];
            var lprim = addChild(layersPrim, newPrim(lp.substr(lp.lastIndexOf("/") + 1), "Layer"));
            this.layerPrim(layer, lprim, lp, compCtx);
        }
    };

    /** Precompute layer paths so parents, track mattes and layer parameters can refer to any layer. */
    Exporter.prototype.compCtx = function (comp, compPath) {
        var names = new NameRegistry();
        var paths = {};
        for (var i = 1; i <= comp.numLayers; i++) {
            paths[i] = compPath + "/layers/" + names.take(comp.layer(i).name, "layer");
        }
        return {comp: comp, path: compPath, layerPaths: paths};
    };

    function layerKind(layer) {
        var mn = layer.matchName;
        if (mn === "ADBE Text Layer") {
            return "text";
        }
        if (mn === "ADBE Vector Layer") {
            return "shape";
        }
        if (mn === "ADBE Camera Layer") {
            return "camera";
        }
        if (mn === "ADBE Light Layer") {
            return "light";
        }
        if (tryGet(layer, "nullLayer")) {
            return "null";
        }
        if (tryGet(layer, "adjustmentLayer")) {
            return "adjustment";
        }
        var src = tryGet(layer, "source");
        if (src && src instanceof CompItem) {
            return "pre_comp";
        }
        if (src && src.mainSource && isSolidSource(src.mainSource)) {
            return "solid";
        }
        if (src && !src.hasVideo && src.hasAudio) {
            return "audio";
        }
        return "footage";
    }

    Exporter.prototype.layerPrim = function (layer, prim, path, compCtx) {
        var kind = layerKind(layer);
        this.stats.layers++;
        this.setAttr(prim, "kind", vEnum(kind));
        this.setString(prim, "match_name", layer.matchName);
        this.setString(prim, "name", layer.name);
        var comment = tryGet(layer, "comment");
        if (comment) {
            this.setString(prim, "comment", comment);
        }
        var label = tryGet(layer, "label");
        if (label) {
            this.setInt(prim, "label", label, path);
        }
        this.setInt(prim, "ae_index", layer.index, path);
        this.setInt(prim, "ae_id", tryGet(layer, "id"), path);

        var src = tryGet(layer, "source");
        if (src && hasOwn(this.itemPaths, itemKey(src))) {
            var sp = this.itemPaths[itemKey(src)];
            this.setAttr(prim, "source", src instanceof CompItem ? vRef(sp) : {type: "asset_ref", value: sp});
        }
        var parent = tryGet(layer, "parent");
        if (parent) {
            this.setAttr(prim, "parent", vRef(compCtx.layerPaths[parent.index]));
        }
        this.setTime(prim, "in_point", layer.inPoint, path);
        this.setTime(prim, "out_point", layer.outPoint, path);
        this.setTime(prim, "start_time", layer.startTime, path);
        this.setNum(prim, "stretch", tryGet(layer, "stretch"), path);
        this.setBool(prim, "enabled", layer.enabled);
        this.setBool(prim, "audio_enabled", tryGet(layer, "audioEnabled"));
        this.setBool(prim, "solo", layer.solo);
        this.setBool(prim, "shy", layer.shy);
        this.setBool(prim, "locked", layer.locked);
        this.setBool(prim, "guide", tryGet(layer, "guideLayer"));
        this.setBool(prim, "adjustment", tryGet(layer, "adjustmentLayer"));
        this.setBool(prim, "environment", tryGet(layer, "environmentLayer"));
        this.setBool(prim, "three_d", tryGet(layer, "threeDLayer"));
        if (kind === "text") {
            this.setBool(prim, "three_d_per_char", tryGet(layer, "threeDPerChar"));
        }
        this.setEnum(prim, "auto_orient", "AutoOrientType", tryGet(layer, "autoOrient"), path);
        this.setEnum(prim, "blend_mode", "BlendingMode", tryGet(layer, "blendingMode"), path);
        this.setBool(prim, "preserve_transparency", tryGet(layer, "preserveTransparency"));
        var tmt = tryGet(layer, "trackMatteType");
        this.setEnum(prim, "track_matte", "TrackMatteType", tmt, path);
        if (tmt !== undefined && typeof TrackMatteType !== "undefined" && tmt !== TrackMatteType.NO_TRACK_MATTE) {
            var matteLayer = tryGet(layer, "trackMatteLayer");
            var matteIndex = matteLayer ? matteLayer.index : (matteLayer === undefined ? layer.index - 1 : 0);
            if (matteIndex > 0 && compCtx.layerPaths[matteIndex]) {
                this.setAttr(prim, "track_matte_layer", vRef(compCtx.layerPaths[matteIndex]));
            }
        }
        this.setBool(prim, "is_track_matte", tryGet(layer, "isTrackMatte"));
        this.setBool(prim, "motion_blur", tryGet(layer, "motionBlur"));
        this.setBool(prim, "effects_active", tryGet(layer, "effectsActive"));
        this.setBool(prim, "collapse_transformation", tryGet(layer, "collapseTransformation"));
        this.setBool(prim, "time_remap_enabled", tryGet(layer, "timeRemapEnabled"));
        this.setEnum(prim, "frame_blending_type", "FrameBlendingType", tryGet(layer, "frameBlendingType"), path);
        this.setEnum(prim, "quality", "LayerQuality", tryGet(layer, "quality"), path);
        this.setEnum(prim, "sampling_quality", "LayerSamplingQuality", tryGet(layer, "samplingQuality"), path);
        if (kind === "light") {
            this.setEnum(prim, "light_type", "LightType", tryGet(layer, "lightType"), path);
        }
        var position = null;
        try {
            position = layer.property("ADBE Transform Group").property("ADBE Position");
        } catch (e) {
            position = null;
        }
        if (position && tryGet(position, "dimensionsSeparated")) {
            this.setBool(prim, "position_separated", true);
        }

        var layerCtx = {layer: layer, prim: prim, comp: compCtx, maskNames: [], maskPaths: []};
        var masks = null;
        try {
            masks = layer.property("ADBE Mask Parade");
        } catch (e2) {
            masks = null;
        }
        if (masks) {
            var maskNames = new NameRegistry();
            for (var m = 1; m <= masks.numProperties; m++) {
                var mname = maskNames.take(masks.property(m).name, "mask");
                layerCtx.maskNames.push(mname);
                layerCtx.maskPaths.push(path + "/masks/" + mname);
            }
        }
        var ctx = new PrimCtx(prim, path, layerCtx);
        // Reserve "markers" so an indexed group can never take that container name.
        ctx.containerNames.take("markers", "markers");
        this.walkGroup(layer, ctx, []);
        var markerProp = null;
        try {
            markerProp = layer.property("ADBE Marker");
        } catch (e3) {
            markerProp = null;
        }
        this.markers(markerProp, prim, path);
    };

    Exporter.prototype.projectPrim = function (project) {
        var prim = newPrim("project", "Project");
        var path = "/project";
        if (project.file) {
            this.setString(prim, "name", decodeName(project.file.name));
        }
        this.setInt(prim, "bits_per_channel", tryGet(project, "bitsPerChannel"), path);
        this.setBool(prim, "linear_blending", tryGet(project, "linearBlending"));
        this.setBool(prim, "linearize_working_space", tryGet(project, "linearizeWorkingSpace"));
        this.setString(prim, "working_space", tryGet(project, "workingSpace"));
        this.setNum(prim, "working_gamma", tryGet(project, "workingGamma"), path);
        this.setString(prim, "expression_engine", tryGet(project, "expressionEngine"));
        this.setInt(prim, "display_start_frame", tryGet(project, "displayStartFrame"), path);
        this.setEnum(prim, "frames_count_type", "FramesCountType", tryGet(project, "framesCountType"), path);
        this.setEnum(prim, "time_display_type", "TimeDisplayType", tryGet(project, "timeDisplayType"), path);
        this.setEnum(prim, "feet_frames_film_type", "FeetFramesFilmType", tryGet(project, "feetFramesFilmType"), path);
        this.setEnum(prim, "footage_timecode_display_start_type", "FootageTimecodeDisplayStartType",
            tryGet(project, "footageTimecodeDisplayStartType"), path);
        this.setBool(prim, "frames_use_feet_frames", tryGet(project, "framesUseFeetFrames"));
        this.setEnum(prim, "gpu_accel_type", "GpuAccelType", tryGet(project, "gpuAccelType"), path);
        return prim;
    };

    /** Items to export: everything, or the selected comps plus what they use. */
    Exporter.prototype.selectItems = function (project) {
        var all = [];
        for (var i = 1; i <= project.numItems; i++) {
            all.push(project.item(i));
        }
        if (!this.options.selectedCompsOnly) {
            return all;
        }
        var keep = {};
        var queue = [];
        var sel = project.selection || [];
        for (var s = 0; s < sel.length; s++) {
            if (sel[s] instanceof CompItem) {
                queue.push(sel[s]);
            }
        }
        while (queue.length) {
            var comp = queue.pop();
            if (hasOwn(keep, itemKey(comp))) {
                continue;
            }
            keep[itemKey(comp)] = true;
            for (var l = 1; l <= comp.numLayers; l++) {
                var src = tryGet(comp.layer(l), "source");
                if (!src) {
                    continue;
                }
                if (src instanceof CompItem) {
                    queue.push(src);
                } else {
                    keep[itemKey(src)] = true;
                }
            }
        }
        var out = [];
        for (var j = 0; j < all.length; j++) {
            if (all[j] instanceof FolderItem || hasOwn(keep, itemKey(all[j]))) {
                out.push(all[j]);
            }
        }
        return out;
    };

    /** Build the interchange document as a plain object. Does not touch the file system except to stat footage. */
    BAE.buildDocument = function (options) {
        var started = new Date().getTime();
        var ex = new Exporter(options);
        var project = app.project;
        var items = ex.selectItems(project);

        var folders = newPrim("folders", null);
        var assets = newPrim("assets", null);
        var comps = newPrim("comps", null);
        var folderNames = new NameRegistry();
        var assetNames = new NameRegistry();
        var compNames = new NameRegistry();
        var i;
        var item;
        // Pass 1: assign every item a prim path so references resolve regardless of order.
        for (i = 0; i < items.length; i++) {
            item = items[i];
            if (item instanceof FolderItem) {
                ex.itemPaths[itemKey(item)] = "/folders/" + folderNames.take(item.name, "folder");
            } else if (item instanceof CompItem) {
                ex.itemPaths[itemKey(item)] = "/comps/" + compNames.take(item.name, "comp");
            } else if (item instanceof FootageItem) {
                ex.itemPaths[itemKey(item)] = "/assets/" + assetNames.take(item.name, "asset");
            }
        }
        // Pass 2: export.
        for (i = 0; i < items.length; i++) {
            item = items[i];
            var path = ex.itemPaths[itemKey(item)];
            if (!path) {
                continue;
            }
            var name = path.substr(path.lastIndexOf("/") + 1);
            if (item instanceof FolderItem) {
                ex.itemCommon(item, addChild(folders, newPrim(name, "Folder")), path);
                ex.stats.folders++;
            } else if (item instanceof CompItem) {
                ex.compPrim(item, addChild(comps, newPrim(name, "Comp")), path);
                ex.stats.comps++;
            } else {
                ex.assetPrim(item, addChild(assets, newPrim(name, "Asset")), path);
                ex.stats.assets++;
            }
        }
        var prims = [ex.projectPrim(project)];
        if (folders.children) {
            prims.push(folders);
        }
        if (assets.children) {
            prims.push(assets);
        }
        if (comps.children) {
            prims.push(comps);
        }

        var doc = {
            format: "bae-interchange",
            format_version: BAE.FORMAT_VERSION,
            profile: "ae",
            generator: {name: "bae-ae-export", version: BAE.VERSION},
            host: {app: "After Effects", version: String(tryGet(app, "version")), build: String(tryGet(app, "buildName")),
                os: String($.os), language: String(tryGet(app, "isoLanguage"))},
            exported_at: isoDate(new Date()),
            source: {},
            time: {unit: "flicks", flicks_per_second: FLICKS_PER_SECOND, key_time_base: "comp"},
            prims: prims,
            assets: ex.manifest,
            report: {warnings: ex.warnings, unsupported: ex.unsupported, stats: ex.stats}
        };
        if (project.file) {
            doc.source.project_path = slashPath(project.file.fsName);
            doc.source.project_name = decodeName(project.file.name);
        }
        ex.stats.seconds = (new Date().getTime() - started) / 1000;
        return doc;
    };

    // ------------------------------------------------------------------ file output

    /** `name`, or `name_2.ext`, `name_3.ext` ... if already used (case-insensitive, for Windows and macOS). */
    function uniqueFileName(used, name) {
        var dot = name.lastIndexOf(".");
        var stem = dot > 0 ? name.substr(0, dot) : name;
        var ext = dot > 0 ? name.substr(dot) : "";
        var candidate = name;
        var n = 2;
        while (hasOwn(used, "$" + candidate.toLowerCase())) {
            candidate = stem + "_" + n + ext;
            n++;
        }
        used["$" + candidate.toLowerCase()] = true;
        return candidate;
    }

    /** Copy footage next to the interchange file and fill in `collected`. */
    function collectAssets(doc, outFile, warn) {
        var base = outFile.name.replace(/\.json$/i, "").replace(/\.bae-interchange$/i, "");
        var dirName = base + ".assets";
        var dir = new Folder(outFile.parent.fsName + "/" + dirName);
        if (!dir.exists) {
            dir.create();
        }
        var taken = new NameRegistry();
        var usedFiles = {};
        for (var i = 0; i < doc.assets.length; i++) {
            var entry = doc.assets[i];
            var file = entry.__file;
            if (!file || entry.missing) {
                continue;
            }
            if (entry.sequence) {
                var sub = taken.take(entry.prim.substr(entry.prim.lastIndexOf("/") + 1), "sequence");
                var subDir = new Folder(dir.fsName + "/" + sub);
                if (!subDir.exists) {
                    subDir.create();
                }
                var list = [];
                var frames = entry.__frames || [];
                for (var f = 0; f < frames.length; f++) {
                    var fname = decodeName(frames[f].name);
                    if (frames[f].copy(subDir.fsName + "/" + fname)) {
                        list.push(dirName + "/" + sub + "/" + fname);
                    } else {
                        warn("collect_failed", entry.prim, "could not copy " + frames[f].fsName);
                    }
                }
                entry.collected = list;
            } else {
                var target = uniqueFileName(usedFiles, decodeName(file.name));
                if (file.copy(dir.fsName + "/" + target)) {
                    entry.collected = dirName + "/" + target;
                } else {
                    warn("collect_failed", entry.prim, "could not copy " + file.fsName);
                }
            }
        }
    }

    function stripPrivate(doc) {
        for (var i = 0; i < doc.assets.length; i++) {
            delete doc.assets[i].__file;
            delete doc.assets[i].__frames;
        }
        stripTree(doc.prims);
    }

    function stripTree(list) {
        if (!list) {
            return;
        }
        for (var i = 0; i < list.length; i++) {
            delete list[i].__path;
            delete list[i].__names;
            stripTree(list[i].children);
        }
    }

    BAE.finish = function (doc) {
        stripPrivate(doc);
        return doc;
    };

    /** Export to options.outputPath. Returns the document. */
    BAE.exportProject = function (options) {
        var outFile = new File(options.outputPath);
        options.outputDir = slashPath(outFile.parent.fsName);
        if (app.project.file) {
            options.projectDir = slashPath(app.project.file.parent.fsName);
        }
        var doc = BAE.buildDocument(options);
        if (options.collectAssets) {
            collectAssets(doc, outFile, function (code, path, message) {
                doc.report.warnings.push({code: code, path: path, message: message});
            });
        }
        BAE.finish(doc);
        var text = BAE.stringify(doc, options.pretty === false ? "" : "  ");
        outFile.encoding = "UTF-8";
        outFile.lineFeed = "Unix";
        if (!outFile.open("w")) {
            throw new Error("Cannot open " + outFile.fsName + " for writing: " + outFile.error);
        }
        outFile.write(text);
        outFile.write("\n");
        outFile.close();
        return doc;
    };

    // ------------------------------------------------------------------ UI

    // Non-ASCII text is \u-escaped: ExtendScript reads BOM-less .jsx files in the system code page.
    var STRINGS = {
        en: {
            title: "Export to BAE",
            noProject: "Open a project first.",
            selectedOnly: "Selected compositions only (plus what they use)",
            collect: "Collect footage files next to the export",
            pretty: "Indented JSON (larger, easier to diff)",
            onlyModified: "Only modified properties (smaller, needs BAE defaults)",
            ok: "Export",
            cancel: "Cancel",
            save: "Save BAE interchange file",
            done: "Exported %1 comps, %2 layers, %3 keyframes.\n%4 warnings, %5 unsupported items (see report in the file).",
            failed: "Export failed: ",
            writePref: "\n\nEnable Preferences > Scripting & Expressions > \"Allow Scripts to Write Files and Access Network\"."
        },
        zh: {
            title: "\u5BFC\u51FA\u5230 BAE",
            noProject: "\u8BF7\u5148\u6253\u5F00\u4E00\u4E2A\u5DE5\u7A0B\u3002",
            selectedOnly: "\u53EA\u5BFC\u51FA\u9009\u4E2D\u7684\u5408\u6210\uFF08\u4EE5\u53CA\u5B83\u4EEC\u7528\u5230\u7684\u7D20\u6750\u548C\u9884\u5408\u6210\uFF09",
            collect: "\u628A\u7D20\u6750\u6587\u4EF6\u6536\u96C6\u5230\u5BFC\u51FA\u6587\u4EF6\u65C1\u8FB9",
            pretty: "\u7F29\u8FDB JSON\uFF08\u6587\u4EF6\u66F4\u5927\uFF0C\u4FBF\u4E8E\u5BF9\u6BD4\uFF09",
            onlyModified: "\u53EA\u5BFC\u51FA\u4FEE\u6539\u8FC7\u7684\u5C5E\u6027\uFF08\u6587\u4EF6\u66F4\u5C0F\uFF0C\u4F9D\u8D56 BAE \u7684\u9ED8\u8BA4\u503C\uFF09",
            ok: "\u5BFC\u51FA",
            cancel: "\u53D6\u6D88",
            save: "\u4FDD\u5B58 BAE \u4EA4\u6362\u6587\u4EF6",
            done: "\u5DF2\u5BFC\u51FA %1 \u4E2A\u5408\u6210\u3001%2 \u4E2A\u56FE\u5C42\u3001%3 \u4E2A\u5173\u952E\u5E27\u3002\n%4 \u6761\u8B66\u544A\uFF0C%5 \u9879\u4E0D\u652F\u6301\uFF08\u8BE6\u89C1\u6587\u4EF6\u91CC\u7684 report\uFF09\u3002",
            failed: "\u5BFC\u51FA\u5931\u8D25\uFF1A",
            writePref: "\n\n\u8BF7\u5728 \u9996\u9009\u9879 > \u811A\u672C\u548C\u8868\u8FBE\u5F0F \u4E2D\u52FE\u9009\u300C\u5141\u8BB8\u811A\u672C\u5199\u5165\u6587\u4EF6\u548C\u8BBF\u95EE\u7F51\u7EDC\u300D\u3002"
        }
    };

    function tr(key) {
        var lang = String(tryGet(app, "isoLanguage") || "en");
        var table = lang.substr(0, 2) === "zh" ? STRINGS.zh : STRINGS.en;
        return table[key];
    }

    function format(s, args) {
        for (var i = 0; i < args.length; i++) {
            s = s.split("%" + (i + 1)).join(String(args[i]));
        }
        return s;
    }

    function askOptions() {
        var w = new Window("dialog", tr("title"));
        w.orientation = "column";
        w.alignChildren = "left";
        var selectedOnly = w.add("checkbox", undefined, tr("selectedOnly"));
        var collect = w.add("checkbox", undefined, tr("collect"));
        var pretty = w.add("checkbox", undefined, tr("pretty"));
        var onlyModified = w.add("checkbox", undefined, tr("onlyModified"));
        pretty.value = true;
        var buttons = w.add("group");
        buttons.add("button", undefined, tr("ok"), {name: "ok"});
        buttons.add("button", undefined, tr("cancel"), {name: "cancel"});
        if (w.show() !== 1) {
            return null;
        }
        var base = app.project.file ? decodeName(app.project.file.name).replace(/\.aepx?$/i, "") : "project";
        var dir = app.project.file ? app.project.file.parent.fsName : Folder.desktop.fsName;
        var target = new File(dir + "/" + base + ".bae-interchange.json").saveDlg(tr("save"), "*.json");
        if (!target) {
            return null;
        }
        var outPath = target.fsName;
        if (!/\.json$/i.test(outPath)) {
            outPath += ".bae-interchange.json";
        }
        return {outputPath: outPath, selectedCompsOnly: selectedOnly.value, collectAssets: collect.value,
            pretty: pretty.value, onlyModified: onlyModified.value};
    }

    function main() {
        // AE keeps globals between script runs: consume the headless options so a later
        // interactive run does not silently reuse them.
        var headless = typeof BAE_EXPORT_OPTIONS !== "undefined" && BAE_EXPORT_OPTIONS;
        global.BAE_EXPORT_OPTIONS = undefined;
        if (!app.project) {
            if (!headless) {
                alert(tr("noProject"));
            }
            return;
        }
        var options = headless || askOptions();
        if (!options) {
            return;
        }
        try {
            var doc = BAE.exportProject(options);
            var s = doc.report.stats;
            var msg = format(tr("done"), [s.comps, s.layers, s.keyframes, doc.report.warnings.length,
                doc.report.unsupported.length]);
            if (headless) {
                $.writeln(msg);
            } else {
                alert(msg, tr("title"));
            }
        } catch (e) {
            var text = tr("failed") + e + (e.line ? " (line " + e.line + ")" : "");
            if (/permission|write|open/i.test(String(e))) {
                text += tr("writePref");
            }
            if (headless) {
                $.writeln(text);
            } else {
                alert(text, tr("title"), true);
            }
        }
    }

    BAE.Exporter = Exporter;
    BAE.PrimCtx = PrimCtx;

    /**
     * AttrSpec of one standalone property, for tools such as bae-anim-samples.jsx.
     * Returns {spec, warnings, unsupported}; spec is undefined when the property has no exportable value.
     */
    BAE.propertyAttrSpec = function (prop) {
        var ex = new Exporter({});
        var prim = newPrim("property", null);
        ex.exportProperty(prop, new PrimCtx(prim, "/property", null), [{matchName: prop.matchName, occurrence: 1}]);
        var spec;
        for (var k in prim.attrs || {}) {
            if (hasOwn(prim.attrs, k)) {
                spec = prim.attrs[k];
            }
        }
        return {spec: spec, warnings: ex.warnings, unsupported: ex.unsupported};
    };

    global.BAE_EXPORT = BAE;
    // Like BAE_EXPORT_OPTIONS, the no-autorun flag is consumed so it does not outlive this run.
    var noAutorun = typeof BAE_EXPORT_NO_AUTORUN !== "undefined" && BAE_EXPORT_NO_AUTORUN;
    global.BAE_EXPORT_NO_AUTORUN = undefined;
    if (!noAutorun) {
        main();
    }
}(this));
