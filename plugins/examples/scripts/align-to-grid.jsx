// Align to grid: snaps the position of the selected layers (or every layer)
// to a 10 px grid. Animated positions get a keyframe at the current time.
var GRID = 10;
app.beginUndoGroup("Align to Grid");

var comp = app.project.activeItem;
var moved = 0;
if (comp instanceof CompItem) {
  var layers = comp.selectedLayers;
  if (layers.length === 0) {
    layers = [];
    for (var i = 1; i <= comp.numLayers; i++) layers.push(comp.layer(i));
  }
  for (var n = 0; n < layers.length; n++) {
    var pos = layers[n].transform.position;
    var v = pos.value;
    var snapped = [Math.round(v[0] / GRID) * GRID, Math.round(v[1] / GRID) * GRID, v[2]];
    if (snapped[0] === v[0] && snapped[1] === v[1]) continue;
    if (pos.numKeys > 0) pos.setValueAtTime(comp.time, snapped);
    else pos.setValue(snapped);
    moved++;
  }
} else {
  alert("Open a composition first.");
}

app.endUndoGroup();
moved;
