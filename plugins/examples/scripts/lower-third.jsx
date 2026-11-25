// Lower third: a new 1920x1080 comp with a color bar, a title and a controller
// null, and a 0.5 s slide-in on the controller.
// Run it with File > Scripts > Run Script File... (one undo step).
app.beginUndoGroup("Create Lower Third");

var comp = app.project.items.addComp("Lower Third", 1920, 1080, 1, 5, 30);

var bar = comp.layers.addSolid([0.05, 0.35, 0.85], "Bar", 900, 120, 1);
bar.position.setValue([560, 900]);

var title = comp.layers.addText("Name Surname");
title.position.setValue([180, 915]);

var ctrl = comp.layers.addNull();
ctrl.name = "Controller";
bar.parent = ctrl;
title.parent = ctrl;

// Slide in from the left: keyframes at 0 s and 0.5 s, held at the end.
var pos = ctrl.position;
var end = pos.value;
pos.setValueAtTime(0, [end[0] - 1200, end[1]]);
pos.setValueAtTime(0.5, end);
pos.setInterpolationTypeAtKey(2, KeyframeInterpolationType.BEZIER);

app.endUndoGroup();
comp.name;
