// Extract factual reference data from Adobe After Effects via ExtendScript.
// Outputs raw JSON files into the directory specified by BAE_AE_REF_OUT
// (default: E:/bae-test/ref-ae/raw).

(function () {
    var outDirPath = $.getenv("BAE_AE_REF_OUT") || "E:/bae-test/ref-ae/raw";
    var mediaDirPath = $.getenv("BAE_AE_REF_MEDIA") || "E:/bae-test/ref-ae";
    var onlyEffects = ($.getenv("BAE_AE_ONLY_EFFECTS") === "1");
    var startIndex = parseInt($.getenv("BAE_AE_START_INDEX") || "0", 10);
    if (isNaN(startIndex) || startIndex < 0) startIndex = 0;
    var skipEnv = $.getenv("BAE_AE_SKIP_EFFECTS") || "";
    var skipMap = {};
    if (skipEnv.length > 0) {
        var parts = skipEnv.split(",");
        for (var sp = 0; sp < parts.length; sp++) {
            var trimmed = parts[sp].replace(/^\s+|\s+$/g, "");
            if (trimmed.length > 0) skipMap[trimmed] = true;
        }
    }

    var outFolder = new Folder(outDirPath);
    if (!outFolder.exists) {
        outFolder.create();
    }

    var logLines = [];
    if (!Date.prototype.toISOString) {
        Date.prototype.toISOString = function () {
            function pad(n) { return n < 10 ? "0" + n : "" + n; }
            return this.getUTCFullYear() + "-" +
                pad(this.getUTCMonth() + 1) + "-" +
                pad(this.getUTCDate()) + "T" +
                pad(this.getUTCHours()) + ":" +
                pad(this.getUTCMinutes()) + ":" +
                pad(this.getUTCSeconds()) + "Z";
        };
    }

    function writeTextFile(filename, text) {
        var f = new File(outDirPath + "/" + filename);
        f.encoding = "UTF-8";
        if (f.open("w")) {
            f.write(text);
            f.close();
        }
    }

    function appendLine(filename, line) {
        var f = new File(outDirPath + "/" + filename);
        f.encoding = "UTF-8";
        if (f.open("a")) {
            f.writeln(line);
            f.close();
        }
    }

    var stepLogFile = new File(outDirPath + "/step_log.txt");
    if (stepLogFile.exists && startIndex === 0) stepLogFile.remove();

    function log(msg) {
        var line = "[" + new Date().toISOString() + "] " + msg;
        logLines.push(line);
        appendLine("step_log.txt", line);
    }

    var escRe = /[\\\"\x00-\x1f]/g;
    var escMap = {
        "\\": "\\\\",
        "\"": "\\\"",
        "\b": "\\b",
        "\f": "\\f",
        "\n": "\\n",
        "\r": "\\r",
        "\t": "\\t"
    };
    function jsonEscape(str) {
        return '"' + str.replace(escRe, function (ch) {
            if (escMap[ch]) return escMap[ch];
            var hex = ch.charCodeAt(0).toString(16);
            while (hex.length < 4) hex = "0" + hex;
            return "\\u" + hex;
        }) + '"';
    }

    function toJson(val) {
        if (val === null || val === undefined) return "null";
        var t = typeof val;
        if (t === "boolean") return val ? "true" : "false";
        if (t === "number") {
            if (isNaN(val) || !isFinite(val)) return "null";
            return "" + val;
        }
        if (t === "string") return jsonEscape(val);
        if (val instanceof Array) {
            var arrParts = [];
            for (var i = 0; i < val.length; i++) {
                arrParts.push(toJson(val[i]));
            }
            return "[" + arrParts.join(",") + "]";
        }
        var objParts = [];
        for (var k in val) {
            if (val.hasOwnProperty(k)) {
                var v = val[k];
                if (v !== undefined && typeof v !== "function") {
                    objParts.push(jsonEscape(k) + ":" + toJson(v));
                }
            }
        }
        return "{" + objParts.join(",") + "}";
    }

    function writeJsonFile(filename, data) {
        var f = new File(outDirPath + "/" + filename);
        f.encoding = "UTF-8";
        if (!f.open("w")) {
            throw new Error("Failed to open file for writing: " + f.fsName);
        }
        f.write(toJson(data));
        f.close();
    }

    function mapPropertyType(pt) {
        if (pt === PropertyType.PROPERTY) return "PROPERTY";
        if (pt === PropertyType.INDEXED_GROUP) return "INDEXED_GROUP";
        if (pt === PropertyType.NAMED_GROUP) return "NAMED_GROUP";
        return "UNKNOWN(" + pt + ")";
    }

    function mapPropertyValueType(pvt) {
        if (pvt === PropertyValueType.NO_VALUE) return "NO_VALUE";
        if (pvt === PropertyValueType.ThreeD_SPATIAL) return "ThreeD_SPATIAL";
        if (pvt === PropertyValueType.ThreeD) return "ThreeD";
        if (pvt === PropertyValueType.TwoD_SPATIAL) return "TwoD_SPATIAL";
        if (pvt === PropertyValueType.TwoD) return "TwoD";
        if (pvt === PropertyValueType.OneD) return "OneD";
        if (pvt === PropertyValueType.COLOR) return "COLOR";
        if (pvt === PropertyValueType.CUSTOM_VALUE) return "CUSTOM_VALUE";
        if (pvt === PropertyValueType.MARKER) return "MARKER";
        if (pvt === PropertyValueType.LAYER_INDEX) return "LAYER_INDEX";
        if (pvt === PropertyValueType.MASK_INDEX) return "MASK_INDEX";
        if (pvt === PropertyValueType.SHAPE) return "SHAPE";
        if (pvt === PropertyValueType.TEXT_DOCUMENT) return "TEXT_DOCUMENT";
        return "UNKNOWN(" + pvt + ")";
    }

    function cleanNumber(n) {
        if (typeof n !== "number" || isNaN(n) || !isFinite(n)) return null;
        return Math.round(n * 1000000) / 1000000;
    }

    function serializeValue(val, valueTypeStr) {
        if (val === null || val === undefined) return null;
        if (valueTypeStr === "TEXT_DOCUMENT") {
            var td = {};
            var safeKeys = [
                "font", "fontSize", "applyFill", "applyStroke",
                "tracking", "autoLeading", "baselineShift", "fauxBold",
                "fauxItalic", "allCaps", "smallCaps", "superscript",
                "subscript", "verticalScale", "horizontalScale",
                "pointText", "boxText"
            ];
            for (var i = 0; i < safeKeys.length; i++) {
                var k = safeKeys[i];
                try {
                    var v = val[k];
                    if (typeof v === "number") td[k] = cleanNumber(v);
                    else if (typeof v === "boolean" || typeof v === "string") td[k] = v;
                } catch (e) {}
            }
            if (td.applyFill) {
                try {
                    var fc = val.fillColor;
                    if (fc instanceof Array) {
                        var fa = [];
                        for (var fj = 0; fj < fc.length; fj++) fa.push(cleanNumber(fc[fj]));
                        td.fillColor = fa;
                    }
                } catch (e) {}
            }
            if (td.applyStroke) {
                try {
                    var sc = val.strokeColor;
                    if (sc instanceof Array) {
                        var sa = [];
                        for (var sj = 0; sj < sc.length; sj++) sa.push(cleanNumber(sc[sj]));
                        td.strokeColor = sa;
                    }
                    td.strokeWidth = cleanNumber(val.strokeWidth);
                    td.strokeOverFill = !!val.strokeOverFill;
                } catch (e) {}
            }
            if (td.autoLeading === false) {
                try { td.leading = cleanNumber(val.leading); } catch (e) {}
            }
            try {
                if (val.justification === ParagraphJustification.LEFT_JUSTIFY) td.justification = "LEFT_JUSTIFY";
                else if (val.justification === ParagraphJustification.RIGHT_JUSTIFY) td.justification = "RIGHT_JUSTIFY";
                else if (val.justification === ParagraphJustification.CENTER_JUSTIFY) td.justification = "CENTER_JUSTIFY";
                else if (val.justification === ParagraphJustification.FULL_JUSTIFY_LASTLINE_LEFT) td.justification = "FULL_JUSTIFY_LASTLINE_LEFT";
                else if (val.justification === ParagraphJustification.FULL_JUSTIFY_LASTLINE_RIGHT) td.justification = "FULL_JUSTIFY_LASTLINE_RIGHT";
                else if (val.justification === ParagraphJustification.FULL_JUSTIFY_LASTLINE_CENTER) td.justification = "FULL_JUSTIFY_LASTLINE_CENTER";
                else if (val.justification === ParagraphJustification.FULL_JUSTIFY_LASTLINE_FULL) td.justification = "FULL_JUSTIFY_LASTLINE_FULL";
            } catch (e) {}
            return td;
        }
        if (valueTypeStr === "SHAPE") {
            var sh = {};
            try { sh.closed = !!val.closed; } catch (e) {}
            try { sh.verticesCount = val.vertices ? val.vertices.length : 0; } catch (e) {}
            return sh;
        }
        if (valueTypeStr === "MARKER") {
            var mk = {};
            try { mk.comment = "" + val.comment; } catch (e) {}
            try { mk.duration = cleanNumber(val.duration); } catch (e) {}
            return mk;
        }
        if (typeof val === "number") return cleanNumber(val);
        if (typeof val === "boolean" || typeof val === "string") return val;
        if (val instanceof Array) {
            var arr = [];
            for (var m = 0; m < val.length; m++) {
                arr.push(typeof val[m] === "number" ? cleanNumber(val[m]) : val[m]);
            }
            return arr;
        }
        return null;
    }

    function inferDimension(val, valueTypeStr) {
        if (val instanceof Array) return val.length;
        if (valueTypeStr === "OneD" || valueTypeStr === "LAYER_INDEX" || valueTypeStr === "MASK_INDEX") return 1;
        if (valueTypeStr === "TwoD" || valueTypeStr === "TwoD_SPATIAL") return 2;
        if (valueTypeStr === "ThreeD" || valueTypeStr === "ThreeD_SPATIAL") return 3;
        if (valueTypeStr === "COLOR") return 4;
        return 0;
    }

    function dumpProperty(prop, depth, isEffectTree) {
        if (depth > 12) return { name: prop.name, matchName: prop.matchName, truncated: true };
        var ptStr = "UNKNOWN";
        try { ptStr = mapPropertyType(prop.propertyType); } catch (e) {}

        var node = {
            name: "",
            matchName: "",
            propertyType: ptStr
        };
        try { node.name = prop.name; } catch (e) {}
        try { node.matchName = prop.matchName; } catch (e) {}

        if (ptStr === "PROPERTY") {
            var pvtStr = "UNKNOWN";
            try { pvtStr = mapPropertyValueType(prop.propertyValueType); } catch (e) {}
            node.propertyValueType = pvtStr;

            var rawVal = null;
            if (pvtStr !== "NO_VALUE" && pvtStr !== "CUSTOM_VALUE") {
                try { rawVal = prop.value; } catch (e) {}
            }
            node.defaultValue = serializeValue(rawVal, pvtStr);
            node.dimension = inferDimension(rawVal, pvtStr);

            try {
                node.hasMin = !!prop.hasMin;
                if (node.hasMin) {
                    var mn = prop.minValue;
                    node.minValue = (mn instanceof Array) ? serializeValue(mn, pvtStr) : cleanNumber(mn);
                }
            } catch (e) {
                node.hasMin = false;
            }
            try {
                node.hasMax = !!prop.hasMax;
                if (node.hasMax) {
                    var mx = prop.maxValue;
                    node.maxValue = (mx instanceof Array) ? serializeValue(mx, pvtStr) : cleanNumber(mx);
                }
            } catch (e) {
                node.hasMax = false;
            }
            try {
                var u = prop.unitsText;
                if (u && u.length > 0) node.unitsText = u;
            } catch (e) {}

            // Only query propertyParameters on OneD integer popup candidates inside effects
            if (isEffectTree && pvtStr === "OneD" && node.hasMin && node.hasMax &&
                node.minValue === 1 && typeof node.maxValue === "number" &&
                node.maxValue >= 2 && node.maxValue <= 200 &&
                Math.floor(node.maxValue) === node.maxValue &&
                typeof node.defaultValue === "number" &&
                Math.floor(node.defaultValue) === node.defaultValue) {
                try {
                    var pp = prop.propertyParameters;
                    if (pp && pp instanceof Array && pp.length > 0) {
                        var opts = [];
                        for (var oi = 0; oi < pp.length; oi++) opts.push("" + pp[oi]);
                        node.dropdownOptions = opts;
                    }
                } catch (e) {}
            }

            try { node.canVaryOverTime = !!prop.canVaryOverTime; } catch (e) {}
            try { node.canSetExpression = !!prop.canSetExpression; } catch (e) {}
            try {
                if (prop.isSeparationLeader) node.isSeparationLeader = true;
                if (prop.isSeparationFollower) node.isSeparationFollower = true;
            } catch (e) {}
        } else {
            var children = [];
            var count = 0;
            try { count = prop.numProperties; } catch (e) {}
            for (var i = 1; i <= count; i++) {
                try {
                    var child = prop.property(i);
                    if (child) {
                        children.push(dumpProperty(child, depth + 1, isEffectTree));
                    }
                } catch (e) {
                    children.push({ unavailable: "property_read_error" });
                }
            }
            node.properties = children;
        }
        return node;
    }

    function dumpLayer(layer) {
        var info = {
            name: layer.name,
            matchName: layer.matchName,
            threeDLayer: false,
            adjustmentLayer: false,
            nullLayer: false,
            hasVideo: false,
            hasAudio: false,
            properties: []
        };
        try { info.threeDLayer = !!layer.threeDLayer; } catch (e) {}
        try { info.adjustmentLayer = !!layer.adjustmentLayer; } catch (e) {}
        try { info.nullLayer = !!layer.nullLayer; } catch (e) {}
        try { info.hasVideo = !!layer.hasVideo; } catch (e) {}
        try { info.hasAudio = !!layer.hasAudio; } catch (e) {}

        var count = 0;
        try { count = layer.numProperties; } catch (e) {}
        for (var i = 1; i <= count; i++) {
            try {
                var p = layer.property(i);
                if (p) {
                    info.properties.push(dumpProperty(p, 1, false));
                }
            } catch (e) {
                info.properties.push({ unavailable: "property_read_error" });
            }
        }
        return info;
    }

    var thirdPartyCats = {
        "FMT": true,
        "BGRA": true,
        "BSKL": true,
        "FilmConvert": true,
        "Pixel Sorter Studio": true,
        "Plugin Everything": true,
        "Swarm": true,
        "TiM": true,
        "Zaebects": true
    };

    function isAdobeOrBundledEffect(matchName, category) {
        if (category && category.indexOf("Sapphire") === 0) return false;
        if (category && thirdPartyCats[category]) return false;
        if (matchName === "mochaProAE") return false;
        return true;
    }

    function safeAdd(group, matchName, addedLog, failedLog) {
        try {
            if (group.canAddProperty && !group.canAddProperty(matchName)) {
                failedLog.push({ matchName: matchName, unavailable: "cannot_add" });
                return null;
            }
            var p = group.addProperty(matchName);
            addedLog.push(matchName);
            return p;
        } catch (e) {
            failedLog.push({ matchName: matchName, unavailable: "add_failed" });
            return null;
        }
    }

    app.beginSuppressDialogs();
    var summary = { ok: false };

    try {
        log("Starting AE reference extraction");
        try {
            app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES);
        } catch (e) {}
        app.newProject();

        var comp = app.project.items.addComp("Ref Comp", 1920, 1080, 1.0, 10.0, 30.0);
        var renderers = [];
        try {
            for (var r = 0; r < comp.renderers.length; r++) {
                renderers.push(comp.renderers[r]);
            }
        } catch (e) {}

        // Probe dropdown propertyParameters on Gaussian Blur 2
        var reflectProbe = {};
        try {
            var probeSolid = comp.layers.addSolid([0.5, 0.5, 0.5], "Probe", 100, 100, 1.0, 1.0);
            var gb = probeSolid.property("ADBE Effect Parade").addProperty("ADBE Gaussian Blur 2");
            var blurDim = gb.property(2);
            var ppVal = null;
            try { ppVal = blurDim.propertyParameters; } catch (e) { ppVal = "ERR:" + e; }
            reflectProbe = {
                effect: "ADBE Gaussian Blur 2",
                paramName: blurDim ? blurDim.name : null,
                paramMatchName: blurDim ? blurDim.matchName : null,
                propertyValueType: blurDim ? mapPropertyValueType(blurDim.propertyValueType) : null,
                hasMin: blurDim ? blurDim.hasMin : null,
                minValue: blurDim && blurDim.hasMin ? blurDim.minValue : null,
                hasMax: blurDim ? blurDim.hasMax : null,
                maxValue: blurDim && blurDim.hasMax ? blurDim.maxValue : null,
                propertyParameters: ppVal
            };
            probeSolid.remove();
        } catch (e) {
            reflectProbe = { error: "" + e };
        }

        // 1. Metadata
        var metaRaw = {
            appVersion: app.version,
            buildName: app.buildName,
            buildNumber: app.buildNumber,
            isoLanguage: app.isoLanguage,
            effectsTotalCount: app.effects.length,
            compRenderers: renderers,
            defaultCompRenderer: comp.renderer,
            propertyReflectProbe: reflectProbe
        };
        writeJsonFile("meta_raw.json", metaRaw);
        log("Wrote meta_raw.json");

        // 2. Effects catalog
        var effectsCatalog = [];
        for (var ci = 0; ci < app.effects.length; ci++) {
            var cef = app.effects[ci];
            effectsCatalog.push({
                displayName: cef.displayName,
                matchName: cef.matchName,
                category: cef.category
            });
        }
        writeJsonFile("effects_catalog_raw.json", { catalog: effectsCatalog });
        log("Wrote effects_catalog_raw.json: " + effectsCatalog.length);

        // 3 & 4. Layer property trees & Render Queue (skip when running effects-only pass)
        if (!onlyEffects) {
        log("Extracting layers...");
        var layersData = {};
        var layerNotes = {};

        var solidLayer = comp.layers.addSolid([1, 0, 0], "Ref Solid", 1920, 1080, 1.0, 10.0);
        layersData.solid = dumpLayer(solidLayer);
        log("Dumped solid");

        var solid3d = comp.layers.addSolid([0, 1, 0], "Ref Solid 3D", 1920, 1080, 1.0, 10.0);
        solid3d.threeDLayer = true;
        var maskAdded = [], maskFailed = [];
        safeAdd(solid3d.property("ADBE Mask Parade"), "ADBE Mask Atom", maskAdded, maskFailed);
        layersData.solid_3d_and_mask = dumpLayer(solid3d);
        log("Dumped solid_3d_and_mask");

        var textLayer = comp.layers.addText("Sample");
        var textAdded = [], textFailed = [];
        try {
            var animators = textLayer.property("ADBE Text Properties").property("ADBE Text Animators");
            var animator = safeAdd(animators, "ADBE Text Animator", textAdded, textFailed);
            if (animator) {
                var selectors = animator.property("ADBE Text Selectors");
                safeAdd(selectors, "ADBE Text Selector", textAdded, textFailed);
                safeAdd(selectors, "ADBE Text Wiggly Selector", textAdded, textFailed);
                safeAdd(selectors, "ADBE Text Expressible Selector", textAdded, textFailed);

                var animProps = animator.property("ADBE Text Animator Properties");
                var textAnimPropMatchNames = [
                    "ADBE Text Anchor Point 3D",
                    "ADBE Text Position 3D",
                    "ADBE Text Scale 3D",
                    "ADBE Text Skew",
                    "ADBE Text Skew Axis",
                    "ADBE Text Rotation",
                    "ADBE Text Rotation X",
                    "ADBE Text Rotation Y",
                    "ADBE Text Opacity",
                    "ADBE Text Fill Color",
                    "ADBE Text Fill Hue",
                    "ADBE Text Fill Saturation",
                    "ADBE Text Fill Brightness",
                    "ADBE Text Fill Opacity",
                    "ADBE Text Stroke Color",
                    "ADBE Text Stroke Hue",
                    "ADBE Text Stroke Saturation",
                    "ADBE Text Stroke Brightness",
                    "ADBE Text Stroke Opacity",
                    "ADBE Text Stroke Width",
                    "ADBE Text Line Anchor",
                    "ADBE Text Line Spacing",
                    "ADBE Text Track Type",
                    "ADBE Text Tracking Amount",
                    "ADBE Text Character Change Type",
                    "ADBE Text Character Range",
                    "ADBE Text Character Offset",
                    "ADBE Text Character Value",
                    "ADBE Text Blur"
                ];
                for (var t = 0; t < textAnimPropMatchNames.length; t++) {
                    safeAdd(animProps, textAnimPropMatchNames[t], textAdded, textFailed);
                }
            }
        } catch (e) {
            textFailed.push({ matchName: "ADBE Text Animator", error: "" + e });
        }
        layersData.text = dumpLayer(textLayer);
        layerNotes.text = { added: textAdded, failed: textFailed };
        log("Dumped text");

        var shapeLayer = comp.layers.addShape();
        shapeLayer.name = "Ref Shape";
        var shapeAdded = [], shapeFailed = [];
        var rootVec = shapeLayer.property("ADBE Root Vectors Group");
        var shapeGroup = safeAdd(rootVec, "ADBE Vector Group", shapeAdded, shapeFailed);
        var vecContents = shapeGroup ? shapeGroup.property("ADBE Vectors Group") : rootVec;
        var shapeMatchNames = [
            "ADBE Vector Shape - Rect",
            "ADBE Vector Shape - Ellipse",
            "ADBE Vector Shape - Star",
            "ADBE Vector Shape - Group",
            "ADBE Vector Graphic - Fill",
            "ADBE Vector Graphic - Stroke",
            "ADBE Vector Graphic - G-Fill",
            "ADBE Vector Graphic - G-Stroke",
            "ADBE Vector Filter - Merge",
            "ADBE Vector Filter - Offset",
            "ADBE Vector Filter - PB",
            "ADBE Vector Filter - Repeater",
            "ADBE Vector Filter - RC",
            "ADBE Vector Filter - Trim",
            "ADBE Vector Filter - Twist",
            "ADBE Vector Filter - Roughen",
            "ADBE Vector Filter - Wiggler",
            "ADBE Vector Filter - Zigzag"
        ];
        for (var s = 0; s < shapeMatchNames.length; s++) {
            var addedShapeProp = safeAdd(vecContents, shapeMatchNames[s], shapeAdded, shapeFailed);
            if (addedShapeProp && (shapeMatchNames[s] === "ADBE Vector Graphic - Stroke" || shapeMatchNames[s] === "ADBE Vector Graphic - G-Stroke")) {
                try {
                    var dashes = addedShapeProp.property("ADBE Vector Stroke Dashes");
                    if (dashes) {
                        safeAdd(dashes, "ADBE Vector Stroke Dash 1", shapeAdded, shapeFailed);
                        safeAdd(dashes, "ADBE Vector Stroke Gap 1", shapeAdded, shapeFailed);
                        safeAdd(dashes, "ADBE Vector Stroke Offset", shapeAdded, shapeFailed);
                    }
                } catch (e) {}
            }
        }
        layersData.shape = dumpLayer(shapeLayer);
        layerNotes.shape = { added: shapeAdded, failed: shapeFailed };
        log("Dumped shape");

        var camTwoNode = comp.layers.addCamera("Ref Camera Two-Node", [960, 540]);
        layersData.camera_two_node = dumpLayer(camTwoNode);

        var camOneNode = comp.layers.addCamera("Ref Camera One-Node", [960, 540]);
        try { camOneNode.autoOrient = AutoOrientType.NO_AUTO_ORIENT; } catch (e) {}
        layersData.camera_one_node = dumpLayer(camOneNode);
        log("Dumped cameras");

        var lightDefs = [
            { key: "light_parallel", name: "Ref Light Parallel", type: LightType.PARALLEL },
            { key: "light_spot", name: "Ref Light Spot", type: LightType.SPOT },
            { key: "light_point", name: "Ref Light Point", type: LightType.POINT },
            { key: "light_ambient", name: "Ref Light Ambient", type: LightType.AMBIENT }
        ];
        try {
            if (typeof LightType.ENVIRONMENT !== "undefined") {
                lightDefs.push({ key: "light_environment", name: "Ref Light Environment", type: LightType.ENVIRONMENT });
            }
        } catch (e) {}

        var lightNotes = [];
        for (var l = 0; l < lightDefs.length; l++) {
            try {
                var lyr = comp.layers.addLight(lightDefs[l].name, [960, 540]);
                lyr.lightType = lightDefs[l].type;
                layersData[lightDefs[l].key] = dumpLayer(lyr);
            } catch (e) {
                lightNotes.push({ key: lightDefs[l].key, error: "" + e });
            }
        }
        layerNotes.lights = lightNotes;
        log("Dumped lights");

        var nullLayer = comp.layers.addNull(10.0);
        nullLayer.name = "Ref Null";
        layersData.null_layer = dumpLayer(nullLayer);

        var adjLayer = comp.layers.addSolid([1, 1, 1], "Ref Adjustment", 1920, 1080, 1.0, 10.0);
        adjLayer.adjustmentLayer = true;
        layersData.adjustment = dumpLayer(adjLayer);

        var footageNotes = [];
        try {
            var pngFile = new File(mediaDirPath + "/dummy.png");
            if (pngFile.exists) {
                var pngItem = app.project.importFile(new ImportOptions(pngFile));
                var pngLayer = comp.layers.add(pngItem);
                layersData.footage_image = dumpLayer(pngLayer);
            } else {
                footageNotes.push("dummy.png not found");
            }
        } catch (e) {
            footageNotes.push("footage_image error: " + e);
        }

        try {
            var wavFile = new File(mediaDirPath + "/dummy.wav");
            if (wavFile.exists) {
                var wavItem = app.project.importFile(new ImportOptions(wavFile));
                var wavLayer = comp.layers.add(wavItem);
                layersData.footage_audio = dumpLayer(wavLayer);
            } else {
                footageNotes.push("dummy.wav not found");
            }
        } catch (e) {
            footageNotes.push("footage_audio error: " + e);
        }
        layerNotes.footage = footageNotes;
        log("Dumped null, adjustment, footage");

        var subComp = app.project.items.addComp("Ref SubComp", 1920, 1080, 1.0, 5.0, 30.0);
        subComp.layers.addSolid([0.2, 0.4, 0.8], "Sub Solid", 1920, 1080, 1.0, 5.0);
        try {
            var wavFile2 = new File(mediaDirPath + "/dummy.wav");
            if (wavFile2.exists) {
                var wavItem2 = app.project.importFile(new ImportOptions(wavFile2));
                subComp.layers.add(wavItem2);
            }
        } catch (e) {}
        var precompLayer = comp.layers.add(subComp);
        try { precompLayer.timeRemapEnabled = true; } catch (e) {}
        layersData.precomp = dumpLayer(precompLayer);
        log("Dumped precomp");

        var rendererGeometry = {};
        for (var rn = 0; rn < renderers.length; rn++) {
            var rName = renderers[rn];
            try {
                var rComp = app.project.items.addComp("Ref Renderer " + rn, 1920, 1080, 1.0, 5.0, 30.0);
                rComp.renderer = rName;
                var rShape = rComp.layers.addShape();
                rShape.threeDLayer = true;
                var geom = rShape.property("ADBE Geometry Options Group");
                var mat = rShape.property("ADBE Material Options Group");
                rendererGeometry[rName] = {
                    geometryOptions: geom ? dumpProperty(geom, 1, false) : null,
                    materialOptions: mat ? dumpProperty(mat, 1, false) : null
                };
            } catch (e) {
                rendererGeometry[rName] = { error: "" + e };
            }
        }

        writeJsonFile("layers_raw.json", {
            layers: layersData,
            notes: layerNotes,
            rendererGeometry: rendererGeometry
        });
        log("Wrote layers_raw.json");

        // 4. Render Queue Settings and Output Modules
        var rqData = {};
        try {
            var rqItem = app.project.renderQueue.items.add(comp);
            var rqTemplates = [];
            try {
                for (var rt = 0; rt < rqItem.templates.length; rt++) {
                    rqTemplates.push(rqItem.templates[rt]);
                }
            } catch (e) {}

            var rqSettable = {};
            var rqCurrent = {};
            try { rqSettable = rqItem.getSettings(GetSettingsFormat.STRING_SETTABLE); } catch (e) { rqSettable = { error: "" + e }; }
            try { rqCurrent = rqItem.getSettings(GetSettingsFormat.STRING); } catch (e) { rqCurrent = { error: "" + e }; }

            var om = rqItem.outputModule(1);
            var omTemplates = [];
            try {
                for (var ot = 0; ot < om.templates.length; ot++) {
                    omTemplates.push(om.templates[ot]);
                }
            } catch (e) {}

            var omSettable = {};
            var omCurrent = {};
            try { omSettable = om.getSettings(GetSettingsFormat.STRING_SETTABLE); } catch (e) { omSettable = { error: "" + e }; }
            try { omCurrent = om.getSettings(GetSettingsFormat.STRING); } catch (e) { omCurrent = { error: "" + e }; }

            rqData = {
                renderSettings: {
                    templates: rqTemplates,
                    stringSettable: rqSettable,
                    stringCurrent: rqCurrent
                },
                outputModule: {
                    templates: omTemplates,
                    stringSettable: omSettable,
                    stringCurrent: omCurrent
                }
            };
        } catch (e) {
            rqData = { error: "" + e };
        }
        writeJsonFile("render_output_raw.json", rqData);
        log("Wrote render_output_raw.json");
        } // end if (!onlyEffects)

        // 5. Effects parameter extraction with per-effect progress flush
        var effectsDetailed = [];
        var effectsSkipped = [];

        if (startIndex === 0) {
            var partialFile = new File(outDirPath + "/effects_detailed_partial.jsonl");
            if (partialFile.exists) partialFile.remove();
            var skippedFile = new File(outDirPath + "/effects_skipped_partial.jsonl");
            if (skippedFile.exists) skippedFile.remove();
        }

        var fxComp = app.project.items.addComp("FX Comp", 640, 360, 1.0, 2.0, 30.0);
        var fxSolid = fxComp.layers.addSolid([0.5, 0.5, 0.5], "FX Host", 640, 360, 1.0, 2.0);
        var fxParade = fxSolid.property("ADBE Effect Parade");
        var appliedCounter = 0;

        for (var i = startIndex; i < effectsCatalog.length; i++) {
            var ef = effectsCatalog[i];
            if (!isAdobeOrBundledEffect(ef.matchName, ef.category)) {
                continue;
            }
            if (skipMap[ef.matchName]) {
                var skSkip = {
                    displayName: ef.displayName,
                    matchName: ef.matchName,
                    category: ef.category,
                    unavailable: "skipped_modal_or_unsupported"
                };
                effectsSkipped.push(skSkip);
                appendLine("effects_skipped_partial.jsonl", toJson(skSkip));
                continue;
            }

            writeTextFile("current_effect.txt", i + "/" + effectsCatalog.length + " ADD " + ef.matchName + " | " + ef.displayName);

            try {
                if (appliedCounter > 0 && appliedCounter % 40 === 0) {
                    fxSolid.remove();
                    fxSolid = fxComp.layers.addSolid([0.5, 0.5, 0.5], "FX Host", 640, 360, 1.0, 2.0);
                    fxParade = fxSolid.property("ADBE Effect Parade");
                }

                if (!fxParade.canAddProperty(ef.matchName)) {
                    var skNoAdd = {
                        displayName: ef.displayName,
                        matchName: ef.matchName,
                        category: ef.category,
                        unavailable: "cannot_apply"
                    };
                    effectsSkipped.push(skNoAdd);
                    appendLine("effects_skipped_partial.jsonl", toJson(skNoAdd));
                    continue;
                }

                var applied = fxParade.addProperty(ef.matchName);
                appliedCounter++;
                writeTextFile("current_effect.txt", i + "/" + effectsCatalog.length + " DUMP " + ef.matchName + " | " + ef.displayName);
                var dumped = dumpProperty(applied, 1, true);
                var rec = {
                    displayName: ef.displayName,
                    matchName: ef.matchName,
                    category: ef.category,
                    properties: dumped.properties || []
                };
                effectsDetailed.push(rec);
                appendLine("effects_detailed_partial.jsonl", toJson(rec));
                applied.remove();
            } catch (e) {
                var skErr = {
                    displayName: ef.displayName,
                    matchName: ef.matchName,
                    category: ef.category,
                    unavailable: "apply_error"
                };
                effectsSkipped.push(skErr);
                appendLine("effects_skipped_partial.jsonl", toJson(skErr));
                try { fxSolid.remove(); } catch (e2) {}
                fxSolid = fxComp.layers.addSolid([0.5, 0.5, 0.5], "FX Host", 640, 360, 1.0, 2.0);
                fxParade = fxSolid.property("ADBE Effect Parade");
            }
        }
        try { fxComp.remove(); } catch (e) {}

        if (startIndex === 0) {
            writeJsonFile("effects_raw.json", {
                catalog: effectsCatalog,
                detailed: effectsDetailed,
                skipped: effectsSkipped
            });
        }
        log("Finished effects loop from " + startIndex + ": detailed=" + effectsDetailed.length + ", skipped=" + effectsSkipped.length);

        try { comp.openInViewer(); } catch (e) {}

        summary = {
            ok: true,
            appVersion: app.version,
            buildName: app.buildName,
            isoLanguage: app.isoLanguage,
            startIndex: startIndex,
            effectsTotal: effectsCatalog.length,
            effectsDetailed: effectsDetailed.length,
            effectsSkipped: effectsSkipped.length
        };
        log("Extraction succeeded");
    } catch (err) {
        log("FATAL ERROR: " + err + " (line " + (err.line || "?") + ")");
        summary = { ok: false, error: "" + err, line: err.line || null };
    } finally {
        app.endSuppressDialogs(false);
        writeTextFile("log.txt", logLines.join("\n"));
        writeJsonFile("done.json", summary);
    }
})();
