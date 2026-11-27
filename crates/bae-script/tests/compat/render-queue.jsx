var comp = app.project.items.addComp("Queued title", 256, 128, 1, 3, 24);
var q = app.project.renderQueue;
var job = q.items.add(comp);
job.outputModule(1).file = new File(Folder.current + "/titles_[####].png");
job.render = false;
if (q.numItems !== 1 || q.item(1).comp.id !== comp.id || q.items[1].render ||
    q.item(1).outputModule(1).file.name !== "titles_[####].png") throw Error("render queue");
