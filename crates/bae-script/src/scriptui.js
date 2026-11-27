// ScriptUI objects live in the script runtime. Only the visible control tree
// crosses the host boundary, so callbacks retain their original closures.
(function (g) {
  "use strict";
  var next = 0, palette = null;
  function Control(type, parent, text, options) {
    this.id = ++next;
    this.type = type.toLowerCase();
    this.parent = parent;
    this.text = typeof text === "string" ? text : "";
    this.name = options && options.name || "";
    this.children = [];
    this.enabled = true;
    this.visible = true;
    this.orientation = this.type === "group" ? "row" : "column";
    this.alignChildren = this.type === "group" ? ["left", "center"] : ["left", "top"];
    this.spacing = 6;
    this.margins = this.type === "group" ? 0 : 8;
    this.layout = { layout: function () {}, resize: function () {} };
    this.value = this.type === "checkbox" || this.type === "radiobutton" ? false : 0;
    this.items = [];
    this.minvalue = 0;
    this.maxvalue = 100;
    this._selection = null;
    if (Array.isArray(text)) text.forEach(function (s) { this.add("item", s); }, this);
  }
  Object.defineProperty(Control.prototype, "selection", {
    get: function () { return this._selection; },
    set: function (v) { this._selection = typeof v === "number" ? this.items[v] || null : v; }
  });
  Control.prototype.add = function (type, bounds, text, options, maximum) {
    type = String(type).toLowerCase();
    if (type === "item") {
      var item = { text: String(bounds), index: this.items.length };
      this.items.push(item);
      return item;
    }
    if (!/^(panel|group|button|statictext|edittext|checkbox|radiobutton|dropdownlist|listbox|slider|progressbar)$/.test(type))
      throw new Error("unknown ScriptUI control " + type);
    var child = type === "panel" ? new Panel(text, this) : new Control(type, this, text, options);
    if (type === "slider" || type === "progressbar") {
      child.value = Number(text) || 0;
      child.minvalue = Number(options) || 0;
      child.maxvalue = maximum === undefined ? 100 : Number(maximum);
    }
    this.children.push(child);
    if (child.name) this[child.name] = child;
    return child;
  };
  function Panel(text, parent) { Control.call(this, "panel", parent || null, text); }
  Panel.prototype = Object.create(Control.prototype);
  Panel.prototype.constructor = Panel;
  function Window(type, title) {
    Control.call(this, type, null, title);
    if (!/^(dialog|palette|window)$/.test(this.type)) throw new Error("unknown ScriptUI window type");
  }
  Window.prototype = Object.create(Control.prototype);
  Window.prototype.constructor = Window;
  Window.prototype.center = function () {};
  Window.prototype.close = Panel.prototype.close = function (result) {
    this._result = result === undefined ? 0 : Number(result);
    this.visible = false;
  };
  function snapshot(node, nodes) {
    nodes[node.id] = node;
    var margins = Array.isArray(node.margins) ? node.margins : Array(4).fill(node.margins);
    return {id: node.id, type: node.type, text: String(node.text), enabled: !!node.enabled,
      orientation: node.orientation, alignChildren: node.alignChildren, margins: margins, spacing: node.spacing,
      value: node.value, min: node.minvalue, max: node.maxvalue,
      items: node.items.map(function (i) { return i.text; }),
      selection: node.selection && node.selection.index, translate: !!node._translate,
      children: node.children.filter(function (c) { return c.visible; }).map(function (c) { return snapshot(c, nodes); })
    };
  }
  function describe(e) {
    if (!(e instanceof Error)) return "Error: " + String(e);
    var line = /script:(\d+)/.exec(e.stack || "");
    return (e.name || "Error") + (line ? " at line " + line[1] : "") + ": " + e.message;
  }
  function interact(root) {
    if (typeof __uiWait !== "function") throw new Error("ScriptUI needs an interactive script host");
    root.visible = true;
    var error = null;
    while (root.visible) {
      var nodes = {}, tree = snapshot(root, nodes);
      // A callback error is reported for that event; the window stays usable, as in AE.
      if (error !== null) { tree.error = error; error = null; }
      var event = JSON.parse(__uiWait(JSON.stringify(tree)));
      if (event.close) { root.close(2); continue; }
      var c = nodes[event.id];
      if (!c || !c.enabled) continue;
      if (c.type === "edittext") {
        // Typing is onChanging; onChange comes on Enter or focus loss after an edit.
        c.text = event.value;
        if (event.kind === "onChanging") c._edited = true;
        else if (event.kind === "onChange" && !c._edited) continue;
        else c._edited = false;
      }
      else if (c.type === "listbox" || c.type === "dropdownlist") c.selection = event.value;
      else if (c.type === "checkbox" || c.type === "radiobutton" || c.type === "slider") c.value = event.value;
      if (c.type === "radiobutton" && c.value)
        c.parent.children.forEach(function (other) { if (other !== c && other.type === c.type) other.value = false; });
      var fn = c[event.kind];
      if (typeof fn === "function") {
        try { fn.call(c); } catch (e) {
          if (!root.visible) throw e;
          error = describe(e);
        }
      }
      else if (event.kind === "onClick" && c.type === "button") {
        var name = (c.name || c.text).toLowerCase();
        if (name === "ok" || name === "cancel") root.close(name === "ok" ? 1 : 2);
      }
    }
    return root._result;
  }
  Window.prototype.show = function () {
    if (this.type === "dialog") return interact(this);
    palette = this;
    this.visible = true;
  };
  g.Window = Window;
  g.Panel = Panel;
  g.__finishUI = function (panel) {
    if (panel && panel.children.length) interact(panel);
    else if (palette && palette.visible) interact(palette);
  };
  // confirm/prompt windows; the host translates the default title and the buttons.
  g.__scriptQuestion = function (type, message, initial, title) {
    var dialog = new Window("dialog", title ? String(title) : type === "prompt" ? "Prompt" : "Confirm");
    dialog._translate = !title;
    dialog.add("statictext", undefined, String(message));
    var field = type === "prompt" ? dialog.add("edittext", undefined, String(initial || "")) : null;
    var row = dialog.add("group");
    row.add("button", undefined, "OK", {name: "ok"})._translate = true;
    row.add("button", undefined, "Cancel", {name: "cancel"})._translate = true;
    var ok = dialog.show() === 1;
    return field ? (ok ? field.text : null) : ok;
  };
})(globalThis);
