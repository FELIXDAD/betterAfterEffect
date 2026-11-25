// Inspect contour knots by following Twist with a negligible corner Zig Zag.
// Run under tools/ae/aectl lock -- tools/ae/aectl run as for render.jsx.
(function () {
    var out = new Folder(Folder.temp.fsName + "/bae-vector-sampling-" + new Date().getTime());
    if (!out.create()) throw Error("Cannot create output folder");
    function sample(name, vertices, closed, filter, settings, knots) {
        var comp = app.project.items.addComp(name, 320, 240, 1, 1, 24);
        try {
            var root = comp.layers.addShape().property("ADBE Root Vectors Group");
            var shape = new Shape(), handles = [];
            for (var i = 0; i < vertices.length; ++i) handles.push([0, 0]);
            shape.vertices = vertices;
            shape.inTangents = handles;
            shape.outTangents = handles;
            shape.closed = closed;
            root.addProperty("ADBE Vector Shape - Group").property("ADBE Vector Shape").setValue(shape);
            var op = root.addProperty("ADBE Vector Filter - " + filter);
            for (var k in settings) op.property(k).setValue(settings[k]);
            if (knots) {
                var zig = root.addProperty("ADBE Vector Filter - Zigzag");
                zig.property("ADBE Vector Zigzag Size").setValue(0.001);
                zig.property("ADBE Vector Zigzag Detail").setValue(0);
            }
            if (closed) root.addProperty("ADBE Vector Graphic - Fill");
            else root.addProperty("ADBE Vector Graphic - Stroke").property("ADBE Vector Stroke Width").setValue(1);
            var path = out.fsName + "/" + name + ".png";
            comp.saveFrameToPng(0, new File(path));
            for (var n = 0; n < 100 && new File(path).length <= 0; ++n) $.sleep(100);
            if (new File(path).length <= 0) throw Error(path);
        } finally { comp.remove(); }
    }
    var rect = [[-40,-20],[40,-20],[40,20],[-40,20]];
    var angles = [10,30,40,45,50,52,60,90,100,105,120,180];
    var lengths = [20,40,41,60,80,120], details = [1,3,10,30];
    for (var i = 0; i < angles.length; ++i)
        sample("angle-" + angles[i],rect,true,"Twist",{"ADBE Vector Twist Angle":angles[i]},true);
    for (var j = 0; j < lengths.length; ++j) {
        var line = [[0,0],[lengths[j],0]];
        sample("line-" + lengths[j],line,false,"Twist",{"ADBE Vector Twist Angle":90},false);
        for (var k = 0; k < details.length; ++k)
            sample("detail-" + lengths[j] + "-" + details[k],line,false,"Roughen",
                {"ADBE Vector Roughen Detail":details[k],"ADBE Vector Correlation":0},false);
    }
    return out.fsName;
}())
