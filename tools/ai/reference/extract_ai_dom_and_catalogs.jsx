// Extract factual metadata, scripting catalogs, and DOM object schemas from Adobe Illustrator.
// Designed for ExtendScript (ES3) invoked via Windows COM: $ai.DoJavaScriptFile(<path>).
// Output path is read from the global variable __BAE_OUTPUT_PATH__ or defaults to E:\bae-test\ref-ai\dom_and_catalogs_raw.json.

#target illustrator

(function () {
    var OUTPUT_PATH = (typeof __BAE_OUTPUT_PATH__ === "string" && __BAE_OUTPUT_PATH__.length > 0)
        ? __BAE_OUTPUT_PATH__
        : "E:\\bae-test\\ref-ai\\dom_and_catalogs_raw.json";

    // Minimal ES3 JSON serializer.
    function escapeJsonString(str) {
        var out = "";
        for (var i = 0; i < str.length; i++) {
            var ch = str.charAt(i);
            var code = str.charCodeAt(i);
            if (ch === "\\") { out += "\\\\"; }
            else if (ch === "\"") { out += "\\\""; }
            else if (ch === "\b") { out += "\\b"; }
            else if (ch === "\f") { out += "\\f"; }
            else if (ch === "\n") { out += "\\n"; }
            else if (ch === "\r") { out += "\\r"; }
            else if (ch === "\t") { out += "\\t"; }
            else if (code < 0x20) {
                var hex = code.toString(16);
                while (hex.length < 4) { hex = "0" + hex; }
                out += "\\u" + hex;
            } else {
                out += ch;
            }
        }
        return "\"" + out + "\"";
    }

    function toJson(val, indent) {
        var pad = indent || "";
        var nextPad = pad + "  ";
        if (val === null || val === undefined) { return "null"; }
        var t = typeof val;
        if (t === "boolean") { return val ? "true" : "false"; }
        if (t === "number") {
            if (isNaN(val) || !isFinite(val)) { return "null"; }
            return String(val);
        }
        if (t === "string") { return escapeJsonString(val); }
        if (val instanceof Array) {
            if (val.length === 0) { return "[]"; }
            var items = [];
            for (var i = 0; i < val.length; i++) {
                items.push(nextPad + toJson(val[i], nextPad));
            }
            return "[\n" + items.join(",\n") + "\n" + pad + "]";
        }
        if (t === "object") {
            var keys = [];
            for (var k in val) {
                if (val.hasOwnProperty(k)) { keys.push(k); }
            }
            if (keys.length === 0) { return "{}"; }
            var pairs = [];
            for (var j = 0; j < keys.length; j++) {
                var key = keys[j];
                pairs.push(nextPad + escapeJsonString(key) + ": " + toJson(val[key], nextPad));
            }
            return "{\n" + pairs.join(",\n") + "\n" + pad + "}";
        }
        return escapeJsonString(String(val));
    }

    function safeGet(fn, fallback) {
        try { return fn(); } catch (e) { return fallback; }
    }

    function arrayFromList(listObj) {
        var arr = [];
        if (!listObj) { return arr; }
        try {
            for (var i = 0; i < listObj.length; i++) {
                arr.push(String(listObj[i]));
            }
        } catch (e) {}
        return arr;
    }

    function summarizeValue(val, propName) {
        if (val === null) { return { runtimeType: "null", sample: null }; }
        if (val === undefined) { return { runtimeType: "undefined", sample: null }; }
        // Avoid dumping machine-specific font names or filesystem paths.
        if (propName === "textFont" || propName === "fullName" || propName === "path") {
            var tn = safeGet(function () { return val.typename; }, typeof val);
            return { runtimeType: tn, sample: "<redacted:" + tn + ">" };
        }
        var t = typeof val;
        if (t === "boolean" || t === "number") {
            return { runtimeType: t, sample: val };
        }
        if (t === "string") {
            var s = val.length > 60 ? val.substring(0, 60) + "..." : val;
            return { runtimeType: "string", sample: s };
        }
        if (val instanceof Array) {
            var sampleArr = [];
            var limit = val.length < 6 ? val.length : 6;
            for (var i = 0; i < limit; i++) {
                var el = val[i];
                if (typeof el === "number") {
                    sampleArr.push(Math.round(el * 1000) / 1000);
                } else if (typeof el === "string" || typeof el === "boolean") {
                    sampleArr.push(el);
                } else {
                    sampleArr.push(safeGet(function () { return el.typename; }, typeof el));
                }
            }
            return { runtimeType: "Array(" + val.length + ")", sample: sampleArr };
        }
        var typename = safeGet(function () { return val.typename; }, "");
        if (typename) {
            return { runtimeType: typename, sample: "<" + typename + ">" };
        }
        var str = safeGet(function () { return String(val); }, "<object>");
        if (str.length > 80) { str = str.substring(0, 80) + "..."; }
        return { runtimeType: t, sample: str };
    }

    function inspectObject(obj, label) {
        var info = {
            label: label,
            typename: safeGet(function () { return obj.typename; }, typeof obj),
            properties: [],
            methods: []
        };
        if (!obj || !obj.reflect) { return info; }

        var props = safeGet(function () { return obj.reflect.properties; }, []);
        var seenProps = {};
        for (var i = 0; i < props.length; i++) {
            var p = props[i];
            var pname = p.name;
            if (pname === "__proto__" || pname === "reflect" || seenProps[pname]) { continue; }
            seenProps[pname] = true;

            var entry = {
                name: pname,
                access: safeGet(function () { return String(p.type); }, "unknown"),
                declaredType: safeGet(function () { return String(p.dataType); }, "unknown")
            };
            try {
                var v = obj[pname];
                var summary = summarizeValue(v, pname);
                entry.runtimeType = summary.runtimeType;
                entry.sample = summary.sample;
            } catch (err) {
                entry.runtimeType = "unreadable";
                entry.readError = String(err.message || err);
            }
            info.properties.push(entry);
        }
        info.properties.sort(function (a, b) {
            return a.name < b.name ? -1 : (a.name > b.name ? 1 : 0);
        });

        var methods = safeGet(function () { return obj.reflect.methods; }, []);
        var seenMethods = {};
        for (var m = 0; m < methods.length; m++) {
            var mname = methods[m].name;
            if (mname === "toSource" || mname === "toString" || mname === "valueOf" ||
                mname === "hasOwnProperty" || mname === "isPrototypeOf" ||
                mname === "propertyIsEnumerable" || mname === "watch" || mname === "unwatch" ||
                seenMethods[mname]) {
                continue;
            }
            seenMethods[mname] = true;
            info.methods.push(mname);
        }
        info.methods.sort();
        return info;
    }

    function inspectEnum(globalName) {
        var obj = null;
        try { obj = $.global[globalName]; } catch (e) {}
        if (!obj) {
            try { obj = eval(globalName); } catch (e) {}
        }
        if (!obj) { return null; }
        var members = [];
        var seen = {};
        try {
            if (obj.reflect && obj.reflect.properties) {
                var props = obj.reflect.properties;
                for (var i = 0; i < props.length; i++) {
                    var n = props[i].name;
                    if (n === "__proto__" || n === "reflect" || seen[n]) { continue; }
                    seen[n] = true;
                    members.push(n);
                }
            }
        } catch (e) {}
        try {
            for (var k in obj) {
                if (!seen[k] && k !== "reflect") {
                    seen[k] = true;
                    members.push(k);
                }
            }
        } catch (e) {}
        members.sort();
        return members;
    }

    function inspectOptionClass(className) {
        var inst = null;
        try {
            var Ctor = $.global[className];
            if (!Ctor) { return null; }
            inst = new Ctor();
        } catch (e) {
            return { className: className, error: String(e.message || e) };
        }
        var reflected = inspectObject(inst, className);
        var propMap = [];
        for (var i = 0; i < reflected.properties.length; i++) {
            var p = reflected.properties[i];
            if (p.name === "typename") { continue; }
            propMap.push({
                name: p.name,
                access: p.access,
                declaredType: p.declaredType,
                runtimeType: p.runtimeType,
                defaultValue: p.sample
            });
        }
        return {
            className: className,
            typename: reflected.typename,
            properties: propMap
        };
    }

    var prevInteraction = app.userInteractionLevel;
    app.userInteractionLevel = UserInteractionLevel.DONTDISPLAYALERTS;

    var result = {
        meta: {},
        catalogs: {
            enums: {},
            saveOptions: {},
            exportOptions: {},
            presets: {}
        },
        dom: {},
        errors: []
    };

    try {
        // 1. Application metadata
        result.meta = {
            appName: safeGet(function () { return String(app.name); }, ""),
            version: safeGet(function () { return String(app.version); }, ""),
            buildNumber: safeGet(function () { return String(app.buildNumber); }, ""),
            scriptingVersion: safeGet(function () { return String(app.scriptingVersion); }, ""),
            locale: safeGet(function () { return String(app.locale); }, ""),
            extendScriptVersion: safeGet(function () { return String($.version); }, ""),
            os: safeGet(function () { return String($.os); }, ""),
            availableFontCount: safeGet(function () { return app.textFonts.length; }, -1)
        };

        // 2. Scripting enums
        var enumNames = [
            "ExportType",
            "ExportForScreensType",
            "DocumentColorSpace",
            "DocumentPresetType",
            "DocumentRasterResolution",
            "DocumentLayoutStyle",
            "Compatibility",
            "PDFCompatibility",
            "EPSCompatibility",
            "PhotoshopCompatibility",
            "BlendModes",
            "StrokeCap",
            "StrokeJoin",
            "GradientType",
            "TextType",
            "TextOrientation",
            "Justification",
            "AutoKernType",
            "FontCapsOption",
            "FontBaselineOption",
            "FontOpenTypePositionOption",
            "AlternateGlyphsForm",
            "FigureStyleType",
            "StyleRunAlignmentType",
            "TabStopAlignment",
            "PolarityValues",
            "PointType",
            "ColorModel",
            "SpotColorKind",
            "RulerUnits",
            "LayerOrderType",
            "ElementPlacement",
            "SaveOptions",
            "AntiAliasingMethod",
            "ArtClippingOption",
            "RasterizationColorModel",
            "ImageColorSpace",
            "KnockoutState",
            "CompressionQuality",
            "MonochromeCompression",
            "SVGDocumentEncoding",
            "SVGCSSPropertyLocation",
            "SVGFontSubsetting",
            "SVGFontType",
            "SVGPreserveEditability",
            "AutoCADCompatibility",
            "AutoCADExportFileFormat",
            "AutoCADUnit",
            "TiffByteOrder",
            "Transformation",
            "ZOrderMethod",
            "LibraryType"
        ];
        for (var ei = 0; ei < enumNames.length; ei++) {
            var ename = enumNames[ei];
            var members = inspectEnum(ename);
            if (members && members.length > 0) {
                result.catalogs.enums[ename] = members;
            }
        }

        // 3. Save options & Export options classes
        var saveOptionClasses = [
            "IllustratorSaveOptions",
            "PDFSaveOptions",
            "EPSSaveOptions",
            "FXGSaveOptions"
        ];
        for (var si = 0; si < saveOptionClasses.length; si++) {
            var sc = saveOptionClasses[si];
            var sInfo = inspectOptionClass(sc);
            if (sInfo) { result.catalogs.saveOptions[sc] = sInfo; }
        }

        var exportOptionClasses = [
            "ExportOptionsPNG24",
            "ExportOptionsPNG8",
            "ExportOptionsJPEG",
            "ExportOptionsGIF",
            "ExportOptionsSVG",
            "ExportOptionsWebOptimizedSVG",
            "ExportOptionsTIFF",
            "ExportOptionsPhotoshop",
            "ExportOptionsAutoCAD",
            "ExportOptionsFlash",
            "ExportOptionsWebP",
            "ExportOptionsAVIF",
            "ExportForScreensOptionsPNG24",
            "ExportForScreensOptionsPNG8",
            "ExportForScreensOptionsJPEG",
            "ExportForScreensOptionsWebOptimizedSVG",
            "ExportForScreensOptionsWebP",
            "ExportForScreensOptionsAVIF",
            "ExportForScreensPDFOptions",
            "ExportForScreensItemToExport",
            "RasterizeOptions",
            "DocumentPreset"
        ];
        for (var xi = 0; xi < exportOptionClasses.length; xi++) {
            var xc = exportOptionClasses[xi];
            var xInfo = inspectOptionClass(xc);
            if (xInfo) { result.catalogs.exportOptions[xc] = xInfo; }
        }

        // 4. Built-in preset lists accessible via app
        result.catalogs.presets = {
            startupPresetsList: arrayFromList(safeGet(function () { return app.startupPresetsList; }, [])),
            pdfPresetsList: arrayFromList(safeGet(function () { return app.PDFPresetsList; }, [])),
            flattenerPresetsList: arrayFromList(safeGet(function () { return app.flattenerPresetsList; }, [])),
            tracingPresetsList: arrayFromList(safeGet(function () { return app.tracingPresetsList; }, []))
        };

        // 5. DOM inspection on a temporary document
        var doc = app.documents.add(DocumentColorSpace.RGB, 800, 600);
        try {
            // Artboards
            doc.artboards[0].name = "Main Artboard";
            var ab2 = doc.artboards.add([850, 600, 1250, 200]);
            ab2.name = "Secondary Artboard";

            // Layers & sublayer
            var mainLayer = doc.layers[0];
            mainLayer.name = "Vector Layer";
            var subLayer = mainLayer.layers.add();
            subLayer.name = "Sublayer";

            // Colors
            var rgbColor = new RGBColor();
            rgbColor.red = 42;
            rgbColor.green = 128;
            rgbColor.blue = 220;

            var strokeRgb = new RGBColor();
            strokeRgb.red = 20;
            strokeRgb.green = 20;
            strokeRgb.blue = 20;

            var cmykColor = new CMYKColor();
            cmykColor.cyan = 10;
            cmykColor.magenta = 40;
            cmykColor.yellow = 80;
            cmykColor.black = 5;

            var grayColor = new GrayColor();
            grayColor.gray = 50;

            var noColor = new NoColor();

            var spot = doc.spots.add();
            spot.name = "Sample Spot";
            spot.colorType = ColorModel.SPOT;
            spot.color = rgbColor;
            var spotColor = new SpotColor();
            spotColor.spot = spot;
            spotColor.tint = 75;

            // Closed PathItem (rectangle with fill, stroke, dashes, opacity, blend mode)
            var rect = mainLayer.pathItems.rectangle(500, 50, 200, 120);
            rect.name = "SampleRect";
            rect.filled = true;
            rect.fillColor = rgbColor;
            rect.stroked = true;
            rect.strokeColor = strokeRgb;
            rect.strokeWidth = 3;
            rect.strokeDashes = [8, 4];
            rect.strokeDashOffset = 2;
            rect.strokeCap = StrokeCap.ROUNDENDCAP;
            rect.strokeJoin = StrokeJoin.ROUNDENDJOIN;
            rect.strokeMiterLimit = 4;
            rect.opacity = 85;
            rect.blendingMode = BlendModes.MULTIPLY;

            // Open Bezier PathItem + PathPoint
            var bezier = mainLayer.pathItems.add();
            bezier.name = "SampleOpenBezier";
            bezier.closed = false;
            bezier.filled = false;
            bezier.stroked = true;
            bezier.strokeColor = strokeRgb;
            bezier.strokeWidth = 2;
            var pt0 = bezier.pathPoints.add();
            pt0.anchor = [100, 300];
            pt0.leftDirection = [100, 300];
            pt0.rightDirection = [140, 340];
            pt0.pointType = PointType.SMOOTH;
            var pt1 = bezier.pathPoints.add();
            pt1.anchor = [220, 300];
            pt1.leftDirection = [180, 260];
            pt1.rightDirection = [220, 300];
            pt1.pointType = PointType.CORNER;

            // CompoundPathItem (outer rect + inner hole)
            var compound = mainLayer.compoundPathItems.add();
            compound.name = "SampleCompound";
            var outer = compound.pathItems.rectangle(450, 300, 140, 140);
            var inner = compound.pathItems.rectangle(410, 340, 60, 60);
            outer.filled = true;
            outer.fillColor = rgbColor;
            inner.filled = true;
            inner.fillColor = rgbColor;

            // Regular GroupItem
            var grp = mainLayer.groupItems.add();
            grp.name = "SampleGroup";
            grp.opacity = 90;
            grp.blendingMode = BlendModes.SCREEN;
            var gChild = grp.pathItems.ellipse(300, 300, 80, 80);
            gChild.filled = true;
            gChild.fillColor = rgbColor;

            // Clipping mask GroupItem
            var clipGrp = mainLayer.groupItems.add();
            clipGrp.name = "SampleClippingGroup";
            var clipContent = clipGrp.pathItems.rectangle(260, 420, 100, 100);
            clipContent.filled = true;
            clipContent.fillColor = rgbColor;
            var clipMask = clipGrp.pathItems.ellipse(250, 430, 80, 80);
            clipMask.clipping = true;
            clipGrp.clipped = true;

            // Gradients (Linear & Radial)
            var linGrad = doc.gradients.add();
            linGrad.name = "SampleLinearGradient";
            linGrad.type = GradientType.LINEAR;
            linGrad.gradientStops[0].color = rgbColor;
            linGrad.gradientStops[0].rampPoint = 0;
            linGrad.gradientStops[0].opacity = 100;
            linGrad.gradientStops[1].color = strokeRgb;
            linGrad.gradientStops[1].rampPoint = 100;
            linGrad.gradientStops[1].opacity = 50;

            var linGradColor = new GradientColor();
            linGradColor.gradient = linGrad;
            linGradColor.origin = [50, 150];
            linGradColor.angle = 45;
            linGradColor.length = 120;

            var gradRect = mainLayer.pathItems.rectangle(200, 50, 120, 80);
            gradRect.filled = true;
            gradRect.fillColor = linGradColor;

            var radGrad = doc.gradients.add();
            radGrad.name = "SampleRadialGradient";
            radGrad.type = GradientType.RADIAL;
            radGrad.gradientStops[0].color = rgbColor;
            radGrad.gradientStops[1].color = strokeRgb;

            var radGradColor = new GradientColor();
            radGradColor.gradient = radGrad;
            radGradColor.origin = [240, 160];
            radGradColor.length = 60;
            radGradColor.hiliteLength = 25;
            radGradColor.hiliteAngle = 30;

            // TextFrames (Point, Area, Path)
            var pointText = mainLayer.textFrames.pointText([50, 80]);
            pointText.name = "SamplePointText";
            pointText.contents = "Hello BAE";
            pointText.textRange.characterAttributes.size = 24;
            pointText.textRange.characterAttributes.fillColor = rgbColor;
            pointText.textRange.paragraphAttributes.justification = Justification.LEFT;

            var areaBox = mainLayer.pathItems.rectangle(180, 300, 160, 80);
            var areaText = mainLayer.textFrames.areaText(areaBox);
            areaText.name = "SampleAreaText";
            areaText.contents = "Area text sample";

            var textCurve = mainLayer.pathItems.add();
            textCurve.setEntirePath([[500, 100], [650, 140]]);
            var pathText = mainLayer.textFrames.pathText(textCurve);
            pathText.name = "SamplePathText";
            pathText.contents = "Path text sample";

            // Symbol & SymbolItem
            var symSource = mainLayer.pathItems.rectangle(550, 550, 40, 40);
            symSource.filled = true;
            symSource.fillColor = rgbColor;
            var symDef = doc.symbols.add(symSource);
            symDef.name = "SampleSymbol";
            var symInst = mainLayer.symbolItems.add(symDef);
            symInst.name = "SampleSymbolInstance";

            // RasterItem via rasterize
            var rasterSrc = mainLayer.pathItems.rectangle(550, 620, 40, 40);
            rasterSrc.filled = true;
            rasterSrc.fillColor = rgbColor;
            var rOpts = new RasterizeOptions();
            rOpts.resolution = 72;
            rOpts.transparency = true;
            var rasterItem = safeGet(function () {
                return doc.rasterize(rasterSrc, rasterSrc.geometricBounds, rOpts);
            }, null);

            // Matrix
            var mtx = app.getScaleMatrix(100, 100);

            // Collect DOM schemas
            result.dom.Document = inspectObject(doc, "Document");
            result.dom.Artboard = inspectObject(doc.artboards[0], "Artboard");
            result.dom.Layer = inspectObject(mainLayer, "Layer");
            result.dom.PathItem = inspectObject(rect, "PathItem");
            result.dom.PathPoint = inspectObject(pt0, "PathPoint");
            result.dom.CompoundPathItem = inspectObject(compound, "CompoundPathItem");
            result.dom.GroupItem = inspectObject(grp, "GroupItem");
            result.dom.GroupItem_ClippingMask = inspectObject(clipGrp, "GroupItem (clipping mask)");
            result.dom.TextFrame_Point = inspectObject(pointText, "TextFrame (point text)");
            result.dom.TextFrame_Area = inspectObject(areaText, "TextFrame (area text)");
            result.dom.TextFrame_Path = inspectObject(pathText, "TextFrame (path text)");
            result.dom.TextRange = inspectObject(pointText.textRange, "TextRange");
            result.dom.CharacterAttributes = inspectObject(pointText.textRange.characterAttributes, "CharacterAttributes");
            result.dom.ParagraphAttributes = inspectObject(pointText.textRange.paragraphAttributes, "ParagraphAttributes");
            result.dom.Gradient = inspectObject(linGrad, "Gradient");
            result.dom.GradientStop = inspectObject(linGrad.gradientStops[0], "GradientStop");
            result.dom.GradientColor_Linear = inspectObject(linGradColor, "GradientColor (linear)");
            result.dom.GradientColor_Radial = inspectObject(radGradColor, "GradientColor (radial)");
            result.dom.RGBColor = inspectObject(rgbColor, "RGBColor");
            result.dom.CMYKColor = inspectObject(cmykColor, "CMYKColor");
            result.dom.GrayColor = inspectObject(grayColor, "GrayColor");
            result.dom.Spot = inspectObject(spot, "Spot");
            result.dom.SpotColor = inspectObject(spotColor, "SpotColor");
            result.dom.NoColor = inspectObject(noColor, "NoColor");
            if (doc.patterns.length > 0) {
                var patColor = new PatternColor();
                patColor.pattern = doc.patterns[0];
                result.dom.PatternColor = inspectObject(patColor, "PatternColor");
                result.dom.Pattern = inspectObject(doc.patterns[0], "Pattern");
            }
            result.dom.Symbol = inspectObject(symDef, "Symbol");
            result.dom.SymbolItem = inspectObject(symInst, "SymbolItem");
            if (doc.swatches.length > 0) {
                result.dom.Swatch = inspectObject(doc.swatches[0], "Swatch");
            }
            if (doc.swatchGroups.length > 0) {
                result.dom.SwatchGroup = inspectObject(doc.swatchGroups[0], "SwatchGroup");
            }
            if (doc.graphicStyles.length > 0) {
                result.dom.GraphicStyle = inspectObject(doc.graphicStyles[0], "GraphicStyle");
            }
            if (doc.brushes.length > 0) {
                result.dom.Brush = inspectObject(doc.brushes[0], "Brush");
            }
            if (rasterItem) {
                result.dom.RasterItem = inspectObject(rasterItem, "RasterItem");
            }
            result.dom.Matrix = inspectObject(mtx, "Matrix");
        } finally {
            try { doc.close(SaveOptions.DONOTSAVECHANGES); } catch (closeErr) {
                result.errors.push("close doc failed: " + String(closeErr));
            }
        }
    } catch (topErr) {
        result.errors.push("top-level error: " + String(topErr.message || topErr));
    } finally {
        app.userInteractionLevel = prevInteraction;
    }

    var outFile = new File(OUTPUT_PATH);
    if (outFile.parent && !outFile.parent.exists) {
        outFile.parent.create();
    }
    outFile.encoding = "UTF-8";
    outFile.open("w");
    outFile.write(toJson(result, ""));
    outFile.close();

    return "OK:" + OUTPUT_PATH + " errors=" + result.errors.length;
})();
