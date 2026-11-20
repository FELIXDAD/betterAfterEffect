// Renders reference movies for bae-media (tests/ae_reference.rs). Run with
// `tools/ae/aectl run crates/bae-media/tests/ae/render_refs.jsx`; edit the comp
// size for the HD variant (1920×1080, files ae_hd_*).
var outDir = new Folder("/private/tmp/bae-08-ae");
if (!outDir.exists && !outDir.create()) { throw new Error("cannot create " + outDir.fsName); }
if (app.project) app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES);
app.newProject();
var comp = app.project.items.addComp("ref", 320, 180, 1, 1, 24);
// Left half: opaque solid (0.75, 0.25, 0.5). Right half: 50 % opacity solid (0.2, 0.6, 0.9).
var left = comp.layers.addSolid([0.75, 0.25, 0.5], "left", 160, 180, 1);
left.property("Position").setValue([80, 90]);
var right = comp.layers.addSolid([0.2, 0.6, 0.9], "right", 160, 180, 1);
right.property("Position").setValue([240, 90]);
right.property("Opacity").setValue(50);
var jobs = [
  ["H.264 - Match Render Settings - 15 Mbps", "ae_h264"],
  ["High Quality", "ae_hq"],
  ["High Quality with Alpha", "ae_hq_alpha"]
];
var log = [];
for (var i = 0; i < jobs.length; i++) {
  var rq = app.project.renderQueue.items.add(comp);
  var om = rq.outputModule(1);
  om.applyTemplate(jobs[i][0]);
  var ext = jobs[i][0].indexOf("H.264") === 0 ? ".mp4" : ".mov";
  var f = new File(outDir.fsName + "/" + jobs[i][1] + ext);
  if (f.exists) f.remove();
  om.file = f;
  log.push(jobs[i][1] + ext + ": " + om.getSettings(GetSettingsFormat.STRING)["Format"] + " / " + om.getSettings(GetSettingsFormat.STRING)["Channels"] + " / " + om.getSettings(GetSettingsFormat.STRING)["Color"]);
}
app.project.renderQueue.render();
log.join("\n");
