var comp = app.project.items.addComp("Title", 640, 480, 1, 2, 25);
var doc = new TextDocument("Chapter One");
doc.fontSize = 64;
comp.layers.addText(doc);
comp.layers.addNull();
var child = comp.layers.precompose([1, 2], "Card", true);
if (!(child instanceof CompItem) || child.numLayers !== 2 || comp.numLayers !== 1 ||
    child.layer(2).text.sourceText.value.fontSize !== 64) throw Error("precompose");
