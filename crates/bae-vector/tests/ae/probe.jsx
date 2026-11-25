// Run with tools/ae/aectl lock -- tools/ae/aectl run <this file>.
(function () {
    var comp = app.project.items.addComp("BAE operator probe", 400, 400, 1, 2, 24);
    var result = [];
    function properties(group) {
        for (var j = 1; j <= group.numProperties; ++j) {
            var p = group.property(j);
            if (p.numProperties) properties(p);
            else if (p.propertyType == PropertyType.PROPERTY && p.propertyValueType != PropertyValueType.NO_VALUE) result.push(p.matchName + " = " + String(p.value));
        }
    }
    try {
        var layer = comp.layers.addShape();
        var root = layer.property("ADBE Root Vectors Group");
        var rect = root.addProperty("ADBE Vector Shape - Rect");
        rect.property("ADBE Vector Rect Size").setValue([80, 40]);
        var names = ["Merge", "Offset", "PB", "Repeater", "RC", "Twist", "Roughen", "Zigzag"];
        for (var i = 0; i < names.length; ++i) {
            var op = root.addProperty("ADBE Vector Filter - " + names[i]);
            result.push(op.matchName);
            properties(op);
            op.remove();
        }
        return result.join("\n");
    } finally { comp.remove(); }
}())
