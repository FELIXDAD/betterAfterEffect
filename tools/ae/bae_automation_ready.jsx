// BAE automation (tools/ae/aectl): tell aectl that ExtendScript is available, so it
// only sends DoScript once After Effects can run scripts. Installed by `aectl install`
// into the user's Scripts/Startup folder; safe to delete. Never shows a dialog.
(function () {
    try {
        var dir = new Folder("__STATE__");
        if (!dir.exists) { dir.create(); }
        var f = new File(dir.fsName + "/ready.json");
        f.encoding = "UTF-8";
        if (f.open("w")) {
            f.write('{"ready_ms":' + new Date().getTime() + ',"version":"' + app.version + '"}');
            f.close();
        }
    } catch (e) {}
})();
