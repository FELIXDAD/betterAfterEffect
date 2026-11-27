var comp = app.project.items.addComp("Matte", 128, 128, 1, 2, 24);
var solid = comp.layers.addSolid([1, 1, 1], "Paper", 128, 128, 1);
var mask = solid.property("ADBE Mask Parade").addProperty("ADBE Mask Atom");
var triangle = new Shape();
triangle.vertices = [[0, 100], [50, 0], [100, 100]];
triangle.closed = true;
mask.maskPath.setValue(triangle);
var result = solid.mask(1).property("ADBE Mask Shape").value;
if (!(result instanceof Shape) || !result.closed || result.vertices.length !== 3 ||
    result.inTangents.length !== 3) throw Error("mask shape");
