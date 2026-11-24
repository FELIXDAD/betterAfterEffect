// Times one AE render of B1 inside the running (licensed) After Effects.
// Globals set by the caller: BAE_MODE = "tiff" | "png", BAE_FRAMES = 300 | 1, BAE_OUT = dir.
(function () {
    app.beginSuppressDialogs();
    var tOpen0 = new Date().getTime();
    app.open(new File("/private/tmp/bae-speed/B1.aep"));
    var comp = null;
    for (var i = 1; i <= app.project.numItems; i++) if (app.project.item(i).name === "b1") comp = app.project.item(i);
    var tOpen = new Date().getTime() - tOpen0, t0, t1;
    // Cold caches for every run: AE reuses rendered frames from its RAM / disk cache.
    app.purge(PurgeTarget.ALL_CACHES);
    if (BAE_MODE === "tiff") {
        var rq = app.project.renderQueue.items.add(comp);
        rq.applyTemplate("Best Settings");
        var om = rq.outputModule(1);
        om.applyTemplate("TIFF Sequence with Alpha");
        om.file = new File(BAE_OUT + "/b1_[####].tif");
        rq.timeSpanStart = 0;
        rq.timeSpanDuration = BAE_FRAMES * comp.frameDuration;
        t0 = new Date().getTime();
        app.project.renderQueue.render();
        t1 = new Date().getTime();
    } else {
        t0 = new Date().getTime();
        var names = [];
        for (var f = 0; f < BAE_FRAMES; f++) {
            var n = "000" + f; n = BAE_OUT + "/b1_" + n.substr(n.length - 4) + ".png";
            names.push(n);
            comp.saveFrameToPng(f * comp.frameDuration, new File(n));
        }
        // saveFrameToPng returns before the file is complete: wait for every file
        // (a fresh File object each time; a reused one keeps reporting length 0).
        for (var k = 0; k < names.length; k++) {
            var guard = 0;
            while (!(new File(names[k]).exists && new File(names[k]).length > 0) && guard++ < 20000) $.sleep(5);
        }
        t1 = new Date().getTime();
    }
    app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES);
    app.endSuppressDialogs(false);
    return (t1 - t0) + "," + tOpen;
})();
