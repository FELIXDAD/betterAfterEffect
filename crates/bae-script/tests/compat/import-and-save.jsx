var options = new ImportOptions(new File(Folder.current + "/input.png"));
var image = app.project.importFile(options);
if (!(image instanceof FootageItem) || image.width !== 2) throw Error("footage");
var comp = app.project.items.addComp("Artwork", 320, 180, 1, 2, 24);
comp.layers.add(image);
app.project.save(new File(Folder.current + "/demo.baez"));
if (app.project.file.name !== "demo.baez") throw Error("saved project location");
