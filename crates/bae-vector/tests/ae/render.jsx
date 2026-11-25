// Render isolated 80 x 40 rectangles. Merge also gets a translated rectangle.
(function () {
    var out = new Folder(Folder.temp.fsName + "/bae-vector-" + new Date().getTime());
    if (!out.create()) throw Error("Cannot create output folder");
    function render(name, filter, settings, time) {
        var comp = app.project.items.addComp(name, 256, 160, 1, 2, 24);
        try {
            var layer = comp.layers.addShape();
            var root = layer.property("ADBE Root Vectors Group");
            function rect(x) {
                var r = root.addProperty("ADBE Vector Shape - Rect");
                r.property("ADBE Vector Rect Size").setValue([80, 40]);
                r.property("ADBE Vector Rect Position").setValue([x, 0]);
            }
            if (name == "zigCurve") {
                root.addProperty("ADBE Vector Shape - Ellipse").property("ADBE Vector Ellipse Size").setValue([80,40]);
            } else rect(0);
            if (name == "repeaterPaint") root.addProperty("ADBE Vector Graphic - Fill");
            if (filter == "Merge") rect(24);
            var op = root.addProperty("ADBE Vector Filter - " + filter);
            for (var k in settings) {
                var owner = op.property(k) ? op : op.property("ADBE Vector Repeater Transform");
                owner.property(k).setValue(settings[k]);
            }
            if (name != "repeaterPaint") root.addProperty("ADBE Vector Graphic - Fill");
            var filename = out.fsName + "/" + name + ".png";
            comp.saveFrameToPng(time || 0, new File(filename));
            for (var n = 0; n < 100 && new File(filename).length <= 0; ++n) $.sleep(100);
            if (new File(filename).length <= 0) throw Error(filename);
        } finally { comp.remove(); }
    }
    for (var i = 1; i <= 5; ++i) render("merge" + i, "Merge", {"ADBE Vector Merge Type":i});
    render("offset", "Offset", {});
    render("offsetCopies", "Offset", {"ADBE Vector Offset Copies":3, "ADBE Vector Offset Copy Offset":0.5});
    render("pucker", "PB", {"ADBE Vector PuckerBloat Amount":40});
    render("round", "RC", {});
    render("repeater", "Repeater", {});
    render("zigzag", "Zigzag", {});
    render("twist", "Twist", {"ADBE Vector Twist Angle":90});
    render("wiggle", "Roughen", {}, 0.5);
    render("offsetBevel", "Offset", {"ADBE Vector Offset Amount":16,"ADBE Vector Offset Line Join":3});
    render("offsetRound", "Offset", {"ADBE Vector Offset Amount":16,"ADBE Vector Offset Line Join":2});
    render("offsetMiterLimit", "Offset", {"ADBE Vector Offset Amount":16,"ADBE Vector Offset Miter Limit":1});
    render("inset", "Offset", {"ADBE Vector Offset Amount":-12});
    render("zigSmooth", "Zigzag", {"ADBE Vector Zigzag Detail":3,"ADBE Vector Zigzag Points":2});
    render("zigCurve", "Zigzag", {"ADBE Vector Zigzag Detail":3});
    render("twist180", "Twist", {"ADBE Vector Twist Angle":180});
    render("repeaterPaint", "Repeater", {"ADBE Vector Repeater Copies":3.5,
        "ADBE Vector Repeater Offset":0.5,"ADBE Vector Repeater Position":[24,1],
        "ADBE Vector Repeater Rotation":30,"ADBE Vector Repeater Scale":[110,80],"ADBE Vector Repeater Anchor":[4,2],
        "ADBE Vector Repeater Opacity 1":80,"ADBE Vector Repeater Opacity 2":20});
    return out.fsName;
}())
