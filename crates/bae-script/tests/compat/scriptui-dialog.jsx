var box = new Window("dialog", "Caption maker");
box.orientation = "column";
box.alignChildren = "fill";
var options = box.add("group");
options.add("statictext", undefined, "Color");
var color = options.add("dropdownlist", undefined, ["Red", "Blue"]);
color.selection = 1;
var title = box.add("edittext", undefined, "First draft");
var create = box.add("button", undefined, "Create caption");
var count = 0;
create.onClick = function () { count++; box.close(1); };
if (box.show() === 1) {
    var comp = app.project.items.addComp("Caption " + count, 640, 360, 1, 2, 24);
    comp.layers.addText(title.text + " " + color.selection.text);
}
