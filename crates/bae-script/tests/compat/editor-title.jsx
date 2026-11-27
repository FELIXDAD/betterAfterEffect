app.beginUndoGroup("Build caption");
var comp = app.project.items.addComp("Caption", 640, 360, 1, 3, 24);
comp.layers.addText("Open captions");
app.endUndoGroup();
