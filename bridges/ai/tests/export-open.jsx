// export-open.jsx — export every open document saved in BAE_EXPORT_DIR with
// bae-ai-export.jsx (stable output) next to its .ai file, then close it without saving.
// Used by regen.sh. Globals: BAE_EXPORT_DIR, BAE_AI_EXPORT_SCRIPT (path of bae-ai-export.jsx).
(function () {
    var BAE_AI_EXPORT_NO_AUTORUN_SAVED = $.global.BAE_AI_EXPORT_NO_AUTORUN;
    $.global.BAE_AI_EXPORT_NO_AUTORUN = true;
    $.evalFile(new File(BAE_AI_EXPORT_SCRIPT));
    $.global.BAE_AI_EXPORT_NO_AUTORUN = BAE_AI_EXPORT_NO_AUTORUN_SAVED;
    var dir = new Folder(BAE_EXPORT_DIR).fsName;
    var done = [];
    for (var i = app.documents.length - 1; i >= 0; i--) {
        var doc = app.documents[i];
        var path = "";
        try { path = doc.fullName.fsName; } catch (e) { continue; }
        if (new File(path).parent.fsName !== dir) { continue; }
        var base = String(doc.name).replace(/\.ai$/, "");
        BAE_AI_EXPORT.run({ document: doc, outputPath: dir + "/" + base + ".bae-ai.json", stable: true });
        doc.close(SaveOptions.DONOTSAVECHANGES);
        done.push(base);
    }
    return done.reverse().join(",");
}());
