var dir = Folder.current;
var outsideDenied = false;
try { new File(dir + "/../outside.txt").open("r"); } catch (error) {outsideDenied = true;}
if (!outsideDenied) throw Error("folder scope");
var reports = new Folder(dir + "/reports");
reports.create();
var file = new File(reports + "/caption.txt");
file.open("w");
file.writeln("Alpha");
file.write("Beta");
file.close();
if (reports.getFiles("*.txt").length !== 1 || !file.open("r")) throw Error("file listing");
if (file.readln() !== "Alpha" || file.read() !== "Beta" || !file.eof) throw Error("read text");
file.close();
app.settings.saveSetting("Caption tool", "Last title", "Beta");
if (!app.settings.haveSetting("Caption tool","Last title") ||
    app.settings.getSetting("Caption tool","Last title") !== "Beta") throw Error("settings");
