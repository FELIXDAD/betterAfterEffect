// Rename layers: numbers the selected layers (or every layer of the active
// comp when nothing is selected) from the top: "Shot 01", "Shot 02", ...
app.beginUndoGroup("Rename Layers");

var comp = app.project.activeItem;
if (!(comp instanceof CompItem)) {
  alert("Open a composition first.");
} else {
  var layers = comp.selectedLayers;
  if (layers.length === 0) {
    layers = [];
    for (var i = 1; i <= comp.numLayers; i++) layers.push(comp.layer(i));
  }
  layers.sort(function (a, b) { return a.index - b.index; });
  for (var n = 0; n < layers.length; n++) {
    var num = n + 1 < 10 ? "0" + (n + 1) : String(n + 1);
    layers[n].name = "Shot " + num;
  }
  $.writeln("Renamed " + layers.length + " layer(s)");
}

app.endUndoGroup();
