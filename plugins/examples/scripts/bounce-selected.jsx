// Bounce: a scale overshoot on every selected layer, starting at the current
// time (110% -> 95% -> 100% over 10 frames).
app.beginUndoGroup("Bounce Selected Layers");

var comp = app.project.activeItem;
if (!(comp instanceof CompItem)) {
  alert("Open a composition first.");
} else if (comp.selectedLayers.length === 0) {
  alert("Select one or more layers.");
} else {
  var f = comp.frameDuration;
  var t = comp.time;
  var steps = [[0, 100], [3, 110], [6, 95], [10, 100]];
  for (var i = 0; i < comp.selectedLayers.length; i++) {
    var scale = comp.selectedLayers[i].transform.scale;
    for (var s = 0; s < steps.length; s++) {
      var v = steps[s][1];
      scale.setValueAtTime(t + steps[s][0] * f, [v, v, 100]);
    }
  }
}

app.endUndoGroup();
