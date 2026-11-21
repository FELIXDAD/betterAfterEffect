// Close the current After Effects project without saving and quit cleanly.
(function () {
    app.beginSuppressDialogs();
    try {
        if (app.project) {
            app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES);
        }
    } catch (e) {}
    try {
        app.quit();
    } catch (e) {}
    app.endSuppressDialogs(false);
})();
