(function (panel) {
    var group = panel.add("group");
    var count = 0;
    var progress = group.add("progressbar", undefined, 0, 0, 5);
    group.add("checkbox", undefined, "Guide layer");
    group.add("radiobutton", undefined, "Use comp time");
    var list = panel.add("listbox", undefined, ["Light", "Camera"]);
    list.selection = 0;
    var slider = panel.add("slider", undefined, 10, 0, 100);
    slider.onChange = function () { progress.value = this.value / 20; };
    panel.add("button", undefined, "Add numbered comp").onClick = function () {
        app.project.items.addComp("Pass " + (++count), 320, 180, 1, 1, 24);
        progress.value = count;
    };
})(this);
