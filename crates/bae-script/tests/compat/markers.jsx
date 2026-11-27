var comp = app.project.items.addComp("Cues", 320, 180, 1, 5, 24);
var layer = comp.layers.addNull();
layer.startTime = 1;
var cue = new MarkerValue("Intro", "Chapter 1");
cue.duration = 0.5;
layer.property("ADBE Marker").setValueAtTime(2, cue);
cue.comment = "Opening";
layer.marker.setValueAtTime(2, cue);
comp.markerProperty.setValueAtTime(1, cue);
if (layer.marker.numKeys !== 1 || layer.marker.keyTime(1) !== 2 ||
    layer.marker.keyValue(1).comment !== "Opening" ||
    comp.markerProperty.keyValue(1).duration !== 0.5) throw Error("markers");
layer.marker.removeKey(1);
if (layer.marker.numKeys !== 0) throw Error("remove marker");
