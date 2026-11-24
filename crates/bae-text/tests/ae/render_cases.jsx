// Run with tools/ae/aectl run. Only temporary comps created here are removed.
(function () {
    var here = File($.fileName).parent;
    var input = new File(here.fsName + "/cases.json");
    if (!input.open("r")) throw Error("Cannot open cases.json");
    var spec = eval("(" + input.read() + ")");
    input.close();
    var out = new Folder(Folder.temp.fsName + "/bae05-text-golden-" + (new Date()).getTime());
    if (!out.create()) throw Error("Cannot create output folder");
    for (var i = 0; i < spec.cases.length; ++i) {
        var c = spec.cases[i];
        var comp = app.project.items.addComp(c.name, spec.width, spec.height, 1, 1, 30);
        try {
            var layer = comp.layers.addText(spec.text);
            layer.property("ADBE Transform Group").property("ADBE Position").setValue([16, 50]);
            var text = layer.property("ADBE Text Properties");
            var source = text.property("ADBE Text Document");
            var doc = source.value;
            doc.resetCharStyle();
            doc.font = spec.font;
            doc.fontSize = spec.size;
            doc.tracking = 0;
            doc.applyFill = true;
            doc.fillColor = [1, 1, 1];
            doc.applyStroke = false;
            doc.justification = ParagraphJustification.LEFT_JUSTIFY;
            source.setValue(doc);
            if (c.position) {
                var group = text.property("ADBE Text Animators").addProperty("ADBE Text Animator");
                var s = group.property("ADBE Text Selectors").addProperty("ADBE Text Selector");
                s.property("ADBE Text Percent Start").setValue(c.start);
                s.property("ADBE Text Percent End").setValue(c.end);
                s.property("ADBE Text Percent Offset").setValue(c.offset);
                var props = group.property("ADBE Text Animator Properties");
                props.addProperty("ADBE Text Position 3D").setValue([c.position[0], c.position[1], 0]);
                props.addProperty("ADBE Text Opacity").setValue(c.opacity);
            }
            var path = out.fsName + "/" + c.name + ".png";
            comp.saveFrameToPng(0, new File(path));
            for (var n = 0; n < 100 && new File(path).length <= 0; ++n) $.sleep(100);
            if (new File(path).length <= 0) throw Error("Missing frame " + path);
        } finally {
            comp.remove();
        }
    }
    return out.fsName;
})();
