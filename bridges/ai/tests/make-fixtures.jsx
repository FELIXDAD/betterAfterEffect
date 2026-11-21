// make-fixtures.jsx - build the Illustrator test documents of workstream 16 and save
// them as .ai (PDF compatible, compressed). Run in Illustrator; normally through
// bridges/ai/tests/regen.sh, which also exports each document with bae-ai-export.jsx.
//
// Grown from the fixture script of the Milan 16 agent (interop/ai-pdf, "basic" below).
// Coordinates in this file are artboard space: points, origin at the artboard's top-left,
// y down (the bridge format's convention); helpers convert to Illustrator's y-up space.
// Only fonts that ship with Illustrator are used (MyriadPro-Regular,
// SourceHanSansSC-Regular), so every Illustrator install produces the same files.
//
// Globals (optional):
//   BAE_FIXTURE_DIR    output folder (default ~/Desktop/bae-ai-fixtures)
//   BAE_FIXTURE_ONLY   build just this fixture (e.g. "basic")
// Returns a summary string; documents stay open (the caller exports and closes them).

(function () {
    var dir = (typeof BAE_FIXTURE_DIR !== "undefined") ? BAE_FIXTURE_DIR : "~/Desktop/bae-ai-fixtures";
    var only = (typeof BAE_FIXTURE_ONLY !== "undefined") ? BAE_FIXTURE_ONLY : null;
    var folder = new Folder(dir);
    if (!folder.exists && !folder.create()) { throw new Error("cannot create " + dir); }

    // ------------------------------------------------------------------ helpers

    function rgb(r, g, b) { var c = new RGBColor(); c.red = r; c.green = g; c.blue = b; return c; }
    function cmyk(c, m, y, k) { var o = new CMYKColor(); o.cyan = c; o.magenta = m; o.yellow = y; o.black = k; return o; }
    function gray(v) { var o = new GrayColor(); o.gray = v; return o; }

    // Artboard space (y down) -> document coordinates (y up).
    function P(ab, x, y) { var r = ab.artboardRect; return [r[0] + x, r[1] - y]; }
    function rect(items, ab, x, y, w, h) { var p = P(ab, x, y); return items.rectangle(p[1], p[0], w, h); }
    function ellipse(items, ab, x, y, w, h) { var p = P(ab, x, y); return items.ellipse(p[1], p[0], w, h); }
    function poly(items, ab, pts, closed) {
        var path = items.add();
        var doc = [];
        for (var i = 0; i < pts.length; i++) { doc.push(P(ab, pts[i][0], pts[i][1])); }
        path.setEntirePath(doc);
        path.closed = closed;
        return path;
    }
    // Points: [x, y, inX, inY, outX, outY] in artboard space (absolute handles).
    function bezier(items, ab, pts, closed) {
        var path = items.add();
        for (var i = 0; i < pts.length; i++) {
            var q = path.pathPoints.add();
            q.anchor = P(ab, pts[i][0], pts[i][1]);
            q.leftDirection = P(ab, pts[i][2], pts[i][3]);
            q.rightDirection = P(ab, pts[i][4], pts[i][5]);
            q.pointType = PointType.SMOOTH;
        }
        path.closed = closed;
        return path;
    }
    function fill(item, color) { item.filled = true; item.fillColor = color; }
    function noFill(item) { item.filled = false; }
    function stroke(item, color, width) { item.stroked = true; item.strokeColor = color; item.strokeWidth = width; }
    function noStroke(item) { item.stroked = false; }
    function gradient(doc, type, stops) {
        // stops: [[rampPoint 0..100, color, midPoint 13..87, opacity 0..100], ...]
        var g = doc.gradients.add();
        g.type = type;
        while (g.gradientStops.length < stops.length) { g.gradientStops.add(); }
        for (var i = 0; i < stops.length; i++) {
            var s = g.gradientStops[i];
            s.rampPoint = stops[i][0];
            s.color = stops[i][1];
            if (stops[i].length > 2 && stops[i][2] !== null) { s.midPoint = stops[i][2]; }
            if (stops[i].length > 3) { s.opacity = stops[i][3]; }
        }
        var gc = new GradientColor();
        gc.gradient = g;
        return gc;
    }
    // Rotate / scale only the gradient of an item (not the item) about its centre.
    function turnGradient(item, degrees, sx, sy) {
        var m = app.getRotationMatrix(degrees);
        m = app.concatenateScaleMatrix(m, sx, sy);
        item.transform(m, false, false, true, false, 100, Transformation.CENTER);
    }
    function pointText(layer, ab, x, y, text, font, size, color) {
        var tf = layer.textFrames.add();
        tf.contents = text;
        var a = tf.textRange.characterAttributes;
        a.textFont = app.textFonts.getByName(font);
        a.size = size;
        a.fillColor = color;
        // Put the baseline origin of the first line at (x, y).
        var want = P(ab, x, y);
        tf.translate(want[0] - tf.anchor[0], want[1] - tf.anchor[1]);
        return tf;
    }
    function save(doc, name) {
        var opts = new IllustratorSaveOptions();
        opts.pdfCompatible = true;
        opts.compressed = true;
        opts.embedICCProfile = false;
        opts.embedLinkedFiles = false;
        opts.saveMultipleArtboards = false;
        doc.saveAs(new File(folder.fsName + "/" + name + ".ai"), opts);
    }

    var built = [];
    function build(name, fn) {
        if (only && only !== name) { return; }
        var doc = fn();
        save(doc, name);
        built.push(name);
    }

    // ------------------------------------------------------------------ basic
    // Artboard 1 "Poster" 400x300
    //   layer "Text"     point text "Hello" (36 pt MyriadPro, dark grey), baseline (20, 280)
    //   layer "\u56FE\u5C42 1" ("Layer 1") group "Logo" (opacity 90 %):
    //                      compound path "Frame": rect (20,20,160,120) with a (60,50,80,60)
    //                        hole, red fill (even-odd), blue 4 pt stroke, round caps,
    //                        bevel joins, dashes 6/3
    //                      circle (250,30) d 100, linear gradient orange -> blue (40 %) ->
    //                        black, opacity 50 %, multiply
    //                      rect (220,180,120,90), radial gradient orange -> black
    //   layer "Masked"   clip group: rect (0,150,200,150) clips a green circle (10,135) d 180
    //   layer "Hidden"   hidden and locked, white 10x10 square
    // Artboard 2 "Badge" 200x100, 50 pt right of artboard 1
    //   layer "\u56FE\u5C42 1" ("Layer 1") yellow ellipse (20,10,160,80), 2 pt black stroke
    build("basic", function () {
        var doc = app.documents.add(DocumentColorSpace.RGB, 400, 300);
        var ab1 = doc.artboards[0];
        ab1.name = "Poster";
        var r1 = ab1.artboardRect;
        var ab2 = doc.artboards.add([r1[2] + 50, r1[1], r1[2] + 250, r1[1] - 100]);
        ab2.name = "Badge";

        var hidden = doc.layers[0];
        hidden.name = "Hidden";
        var masked = doc.layers.add(); masked.name = "Masked";
        var shapes = doc.layers.add(); shapes.name = "\u56FE\u5C42 1";
        var textLayer = doc.layers.add(); textLayer.name = "Text";

        var sq = rect(hidden.pathItems, ab1, 0, 0, 10, 10);
        fill(sq, rgb(255, 255, 255)); noStroke(sq);

        var clip = masked.groupItems.add();
        var disc = ellipse(clip.pathItems, ab1, 10, 135, 180, 180);
        fill(disc, rgb(26, 179, 51)); noStroke(disc);
        var mask = rect(clip.pathItems, ab1, 0, 150, 200, 150);
        noFill(mask); noStroke(mask);
        clip.clipped = true;

        var logo = shapes.groupItems.add();
        logo.name = "Logo";
        var radial = rect(logo.pathItems, ab1, 220, 180, 120, 90);
        radial.fillColor = gradient(doc, GradientType.RADIAL, [[0, rgb(255, 128, 0)], [100, rgb(0, 0, 0)]]);
        noStroke(radial);
        var circ = ellipse(logo.pathItems, ab1, 250, 30, 100, 100);
        circ.fillColor = gradient(doc, GradientType.LINEAR,
            [[0, rgb(255, 128, 0)], [40, rgb(0, 64, 255)], [100, rgb(0, 0, 0)]]);
        noStroke(circ);
        circ.opacity = 50;
        circ.blendingMode = BlendModes.MULTIPLY;
        var frame = logo.compoundPathItems.add();
        frame.name = "Frame";
        var outer = rect(frame.pathItems, ab1, 20, 20, 160, 120);
        var inner = rect(frame.pathItems, ab1, 60, 50, 80, 60);
        inner.polarity = PolarityValues.NEGATIVE;
        var parts = [outer, inner];
        for (var i = 0; i < parts.length; i++) {
            var p = parts[i];
            fill(p, rgb(204, 26, 26));
            p.evenodd = true;
            stroke(p, rgb(0, 0, 255), 4);
            p.strokeCap = StrokeCap.ROUNDENDCAP;
            p.strokeJoin = StrokeJoin.BEVELENDJOIN;
            p.strokeDashes = [6, 3];
        }
        logo.opacity = 90;

        pointText(textLayer, ab1, 20, 280, "Hello", "MyriadPro-Regular", 36, rgb(51, 51, 51));

        var badge = ellipse(shapes.pathItems, ab2, 20, 10, 160, 80);
        fill(badge, rgb(255, 230, 0)); stroke(badge, rgb(0, 0, 0), 2);

        hidden.visible = false;
        hidden.locked = true;
        return doc;
    });

    // ------------------------------------------------------------------ paths
    // One artboard "Paths" 400x300, one layer "Paths" (topmost first in the Layers panel):
    //   "Wave"      open smooth curve, 3 pt orange stroke, no fill, round caps
    //   "Zigzag"    open polyline, 6 pt stroke, miter joins, miter limit 2, projecting caps
    //   "Dashed"    closed rect (20,200,120,60), 2 pt stroke, dashes 10/4/2/4 offset 5
    //   "Rings"     compound of two concentric circles, same direction, non-zero (no hole)
    //   "Donut"     compound of two concentric circles, even-odd (hole)
    //   "Triangle"  closed polygon, purple fill, no stroke
    //   "Rounded"   rounded rectangle (240,20,140,80) radius 16
    build("paths", function () {
        var doc = app.documents.add(DocumentColorSpace.RGB, 400, 300);
        var ab = doc.artboards[0];
        ab.name = "Paths";
        var L = doc.layers[0];
        L.name = "Paths";
        var items = L.pathItems;

        var rp = P(ab, 240, 20);
        var rounded = items.roundedRectangle(rp[1], rp[0], 140, 80, 16, 16);
        rounded.name = "Rounded";
        fill(rounded, rgb(0, 160, 200)); noStroke(rounded);

        var tri = poly(items, ab, [[40, 20], [100, 120], [20, 120]], true);
        tri.name = "Triangle";
        fill(tri, rgb(128, 0, 160)); noStroke(tri);

        var donut = L.compoundPathItems.add();
        donut.name = "Donut";
        var d1 = ellipse(donut.pathItems, ab, 130, 20, 100, 100);
        var d2 = ellipse(donut.pathItems, ab, 155, 45, 50, 50);
        var dd = [d1, d2];
        for (var i = 0; i < dd.length; i++) { fill(dd[i], rgb(240, 120, 0)); noStroke(dd[i]); dd[i].evenodd = true; }

        var rings = L.compoundPathItems.add();
        rings.name = "Rings";
        var g1 = ellipse(rings.pathItems, ab, 250, 120, 80, 80);
        var g2 = ellipse(rings.pathItems, ab, 270, 140, 40, 40);
        var gg = [g1, g2];
        for (i = 0; i < gg.length; i++) { fill(gg[i], rgb(0, 140, 70)); noStroke(gg[i]); gg[i].evenodd = false; }

        var dashed = rect(items, ab, 20, 200, 120, 60);
        dashed.name = "Dashed";
        noFill(dashed); stroke(dashed, rgb(0, 0, 0), 2);
        dashed.strokeDashes = [10, 4, 2, 4];
        dashed.strokeDashOffset = 5;

        var zig = poly(items, ab, [[160, 280], [190, 220], [220, 280], [250, 220]], false);
        zig.name = "Zigzag";
        noFill(zig); stroke(zig, rgb(60, 60, 60), 6);
        zig.strokeJoin = StrokeJoin.MITERENDJOIN;
        zig.strokeMiterLimit = 2;
        zig.strokeCap = StrokeCap.PROJECTINGENDCAP;

        var wave = bezier(items, ab, [
            [270, 260, 250, 280, 290, 240],
            [330, 260, 310, 240, 350, 280],
            [390, 260, 370, 280, 395, 255]
        ], false);
        wave.name = "Wave";
        noFill(wave); stroke(wave, rgb(255, 128, 0), 3);
        wave.strokeCap = StrokeCap.ROUNDENDCAP;
        return doc;
    });

    // ------------------------------------------------------------------ paint
    // CMYK document, artboard "Paint" 400x400, layer "Paint":
    //   "Cmyk"      rect (20,20,80,60), CMYK 0/60/100/0 fill
    //   "Gray"      rect (120,20,80,60), 30 % grey fill
    //   "Spot"      rect (220,20,80,60), spot "BAE Orange" at 40 % tint
    //   "LinRot"    rect (20,110,160,80), linear gradient turned 30 degrees, an opacity stop
    //               (50 %) and the midpoint between the two stops moved to 30 %
    //   "Ellipse"   rect (220,110,160,80), radial gradient squashed to 100 x 40 % (elliptical)
    //   "GradStroke" circle (20,220) d 120, no fill, 12 pt linear gradient stroke
    //   "Screen"    rect (180,220,100,100) 70 % opacity, screen blend
    //   "Overlay"   rect (230,270,100,100), overlay blend
    build("paint", function () {
        var doc = app.documents.add(DocumentColorSpace.CMYK, 400, 400);
        var ab = doc.artboards[0];
        ab.name = "Paint";
        var L = doc.layers[0];
        L.name = "Paint";
        var items = L.pathItems;

        var ov = rect(items, ab, 230, 270, 100, 100);
        ov.name = "Overlay"; fill(ov, cmyk(80, 0, 20, 0)); noStroke(ov);
        ov.blendingMode = BlendModes.OVERLAY;
        var sc = rect(items, ab, 180, 220, 100, 100);
        sc.name = "Screen"; fill(sc, cmyk(0, 90, 30, 0)); noStroke(sc);
        sc.opacity = 70;
        sc.blendingMode = BlendModes.SCREEN;

        var gs = ellipse(items, ab, 20, 220, 120, 120);
        gs.name = "GradStroke"; noFill(gs);
        gs.stroked = true;
        gs.strokeWidth = 12;
        gs.strokeColor = gradient(doc, GradientType.LINEAR, [[0, cmyk(100, 0, 0, 0)], [100, cmyk(0, 100, 0, 0)]]);

        var el = rect(items, ab, 220, 110, 160, 80);
        el.name = "Ellipse"; noStroke(el);
        el.fillColor = gradient(doc, GradientType.RADIAL, [[0, cmyk(0, 0, 100, 0)], [100, cmyk(100, 50, 0, 0)]]);
        turnGradient(el, 0, 100, 40);

        var lr = rect(items, ab, 20, 110, 160, 80);
        lr.name = "LinRot"; noStroke(lr);
        lr.fillColor = gradient(doc, GradientType.LINEAR,
            [[0, cmyk(0, 100, 100, 0), 30, 50], [100, cmyk(100, 100, 0, 0)]]);
        turnGradient(lr, 30, 100, 100);

        var sp = doc.spots.add();
        sp.name = "BAE Orange";
        sp.color = cmyk(0, 50, 100, 0);
        sp.colorType = ColorModel.SPOT;
        var spot = new SpotColor(); spot.spot = sp; spot.tint = 40;
        var s = rect(items, ab, 220, 20, 80, 60);
        s.name = "Spot"; fill(s, spot); noStroke(s);

        var gy = rect(items, ab, 120, 20, 80, 60);
        gy.name = "Gray"; fill(gy, gray(30)); noStroke(gy);

        var c = rect(items, ab, 20, 20, 80, 60);
        c.name = "Cmyk"; fill(c, cmyk(0, 60, 100, 0)); noStroke(c);
        return doc;
    });

    // ------------------------------------------------------------------ structure
    // Two artboards: "A" 300x200 and "B" 300x200, 100 pt apart.
    //   layer "Top" (sublayers and items interleaved, topmost first):
    //       sublayer "Sub2" (empty)
    //       "ItemB"  rect (20,60,40,30) on A
    //       sublayer "Sub1": "InSub1" circle (80,20) d 40 on A
    //       "ItemA"  rect (20,20,40,30) on A
    //   layer "Nested": group "Outer" > group "Inner" > clip group (rect (150,40,100,100)
    //       clips a circle (130,20) d 140) and a rect "Beside" (150,150,100,30);
    //       "Bridge" rect from x = 250 on A to x = 70 on B (spans both artboards)
    //   layer "Symbols": symbol "Star" (a 5-point star) placed at (60,120) on B
    //   layer "Guides": a guide line (skipped by the exporter) and a rect far outside both
    //       artboards (dropped by the importer)
    //   layer "Template": printable off, preview off (Illustrator's template look)
    build("structure", function () {
        var doc = app.documents.add(DocumentColorSpace.RGB, 300, 200);
        var a = doc.artboards[0];
        a.name = "A";
        var ra = a.artboardRect;
        var b = doc.artboards.add([ra[2] + 100, ra[1], ra[2] + 400, ra[1] - 200]);
        b.name = "B";

        var tmpl = doc.layers[0]; tmpl.name = "Template";
        var tr = rect(tmpl.pathItems, a, 0, 0, 300, 200);
        fill(tr, rgb(230, 230, 230)); noStroke(tr);
        var guides = doc.layers.add(); guides.name = "Guides";
        var symbols = doc.layers.add(); symbols.name = "Symbols";
        var nested = doc.layers.add(); nested.name = "Nested";
        var top = doc.layers.add(); top.name = "Top";

        var itemA = rect(top.pathItems, a, 20, 20, 40, 30); itemA.name = "ItemA";
        fill(itemA, rgb(255, 0, 0)); noStroke(itemA);
        var sub1 = top.layers.add(); sub1.name = "Sub1";
        var inSub = ellipse(sub1.pathItems, a, 80, 20, 40, 40); inSub.name = "InSub1";
        fill(inSub, rgb(0, 255, 0)); noStroke(inSub);
        var itemB = rect(top.pathItems, a, 20, 60, 40, 30); itemB.name = "ItemB";
        fill(itemB, rgb(0, 0, 255)); noStroke(itemB);
        var sub2 = top.layers.add(); sub2.name = "Sub2";

        var bridge = rect(nested.pathItems, a, 250, 160, 150 + 70, 30); bridge.name = "Bridge";
        fill(bridge, rgb(255, 0, 255)); noStroke(bridge);
        var outerG = nested.groupItems.add(); outerG.name = "Outer";
        var innerG = outerG.groupItems.add(); innerG.name = "Inner";
        var beside = rect(innerG.pathItems, a, 150, 150, 100, 30); beside.name = "Beside";
        fill(beside, rgb(0, 128, 128)); noStroke(beside);
        var clip = innerG.groupItems.add();
        var disc = ellipse(clip.pathItems, a, 130, 20, 140, 140);
        fill(disc, rgb(255, 200, 0)); noStroke(disc);
        var mask = rect(clip.pathItems, a, 150, 40, 100, 100);
        noFill(mask); noStroke(mask);
        clip.clipped = true;

        var c = P(b, 60, 120);
        var star = symbols.pathItems.star(c[0], c[1], 40, 16, 5, false);
        fill(star, rgb(255, 180, 0)); stroke(star, rgb(120, 60, 0), 2);
        var sym = doc.symbols.add(star);
        sym.name = "Star";
        star.remove();
        var inst = symbols.symbolItems.add(sym);
        inst.name = "StarInstance";
        inst.position = [c[0] - inst.width / 2, c[1] + inst.height / 2];

        var g = poly(guides.pathItems, a, [[0, 100], [300, 100]], false);
        g.guides = true;
        var far = rect(guides.pathItems, a, -500, -500, 50, 50); far.name = "Far";
        fill(far, rgb(0, 0, 0)); noStroke(far);

        tmpl.printable = false;
        tmpl.preview = false;
        tmpl.locked = true;
        return doc;
    });

    // ------------------------------------------------------------------ text
    // Artboard "Text" 400x300, layer "Text" (topmost first):
    //   "Mixed"    point text "Big small\rcentred line" (MyriadPro 30/14 pt, second
    //              paragraph centred, "small" red), baseline (20, 50)
    //   "Rotated"  point text "Turn" 24 pt rotated 30 degrees counter-clockwise, (200, 120)
    //   "Chinese"  point text "\u4F60\u597D\uFF0C\u4E16\u754C" ("Hello, world") SourceHanSansSC 24 pt, 1 pt blue stroke, (20, 200)
    //   "Area"     area text in rect (220,180,160,90), 12 pt
    //   "OnPath"   text on an open curve
    build("text", function () {
        var doc = app.documents.add(DocumentColorSpace.RGB, 400, 300);
        var ab = doc.artboards[0];
        ab.name = "Text";
        var L = doc.layers[0];
        L.name = "Text";

        var curve = bezier(L.pathItems, ab, [[20, 280, 0, 280, 60, 240], [180, 280, 140, 240, 200, 290]], false);
        var onPath = L.textFrames.pathText(curve);
        onPath.contents = "along the path";
        onPath.textRange.characterAttributes.textFont = app.textFonts.getByName("MyriadPro-Regular");
        onPath.textRange.characterAttributes.size = 14;
        onPath.name = "OnPath";

        var box = rect(L.pathItems, ab, 220, 180, 160, 90);
        var area = L.textFrames.areaText(box);
        area.contents = "Area text wraps inside its frame. Second sentence.";
        area.textRange.characterAttributes.textFont = app.textFonts.getByName("MyriadPro-Regular");
        area.textRange.characterAttributes.size = 12;
        area.name = "Area";

        var zh = pointText(L, ab, 20, 200, "\u4F60\u597D\uFF0C\u4E16\u754C", "SourceHanSansSC-Regular", 24, rgb(20, 20, 20));
        zh.name = "Chinese";
        zh.textRange.characterAttributes.strokeColor = rgb(0, 0, 255);
        zh.textRange.characterAttributes.strokeWeight = 1;

        var rot = pointText(L, ab, 200, 120, "Turn", "MyriadPro-Regular", 24, rgb(0, 100, 0));
        rot.name = "Rotated";
        var anchor = rot.anchor;
        rot.rotate(30, true, true, true, true, Transformation.BOTTOMLEFT);
        rot.translate(anchor[0] - rot.anchor[0], anchor[1] - rot.anchor[1]);

        var mixed = pointText(L, ab, 20, 50, "Big small\rcentred line", "MyriadPro-Regular", 30, rgb(0, 0, 0));
        mixed.name = "Mixed";
        var small = mixed.textRange.characters;
        for (var i = 4; i < 9; i++) {
            small[i].characterAttributes.size = 14;
            small[i].characterAttributes.fillColor = rgb(220, 0, 0);
        }
        mixed.paragraphs[1].characterAttributes.size = 14;
        mixed.paragraphs[1].paragraphAttributes.justification = Justification.CENTER;
        return doc;
    });

    return built.join(",");
}());
