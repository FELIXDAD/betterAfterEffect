// BAE scripting: the After Effects scripting object model (app, Project, items,
// layers, properties) over one host function, `__host(method, json) -> json`.
// Collections are read once per host generation. Every project change is a
// registered BAE command run in the script's undo step. Indices are 1-based and
// times are seconds of comp time, as in AE. Written from the public AE scripting
// guide's object and method names; no Adobe code.
(function (g) {
  "use strict";
  var H = function (method, args) {
    return JSON.parse(__host(method, JSON.stringify(args === undefined ? null : args)));
  };
  var version = -1, snapshot;
  function reads() {
    var current = __generation();
    if (version !== current) {
      version = current;
      snapshot = {
        items: new Map(), layers: new Map(), layerRows: new Map(), properties: new Map(),
        folders: new Map(), itemRows: new Map(), effects: new Map(), effectInfo: new Map(),
        masks: new Map()
      };
    }
    return snapshot;
  }
  function projectItems() {
    var cache = reads();
    if (!cache.project) {
      cache.project = H("items");
      for (var row of cache.project) {
        cache.itemRows.set(row.path, row);
        var parent = row.parent || "/";
        if (!cache.folders.has(parent)) cache.folders.set(parent, []);
        cache.folders.get(parent).push(row);
      }
    }
    return cache.project;
  }
  function folderItems(path) {
    projectItems();
    return snapshot.folders.get(path) || [];
  }
  function itemInfo(path) {
    var cache = reads();
    if (!cache.items.has(path)) {
      var info = H("item", { path: path });
      cache.items.set(path, info);
      if (info.layers) info.layers.forEach(function (row, i) {
        row.index = i + 1;
        cache.layerRows.set(row.path, row);
      });
    }
    return cache.items.get(path);
  }
  function layerInfo(path) {
    var layers = reads().layers;
    if (!layers.has(path)) layers.set(path, H("layer", { path: path }));
    return layers.get(path);
  }
  function layerRow(path) {
    var rows = reads().layerRows;
    if (!rows.has(path)) itemInfo(layerInfo(path).comp);
    return rows.get(path);
  }
  function propInfo(p) {
    var props = reads().properties;
    if (!props.has(p._p)) props.set(p._p, new Map());
    var attrs = props.get(p._p);
    if (!attrs.has(p._a)) attrs.set(p._a, H("prop", { path: p._p, attr: p._a }));
    return attrs.get(p._a);
  }
  function cachedRead(map, key, method, args) {
    var cache = reads()[map];
    if (!cache.has(key)) cache.set(key, H(method, args));
    return cache.get(key);
  }
  function layerEffects(layer) { return cachedRead("effects", layer, "effects", { layer: layer }); }
  function effectInfo(path) { return cachedRead("effectInfo", path, "effect", { path: path }); }
  function layerMasks(layer) { return cachedRead("masks", layer, "masks", { layer: layer }); }
  function selection() {
    var cache = reads();
    if (!cache.selected) cache.selected = new Set(H("context").selected);
    return cache.selected;
  }
  var hide = function (obj, key, value) {
    Object.defineProperty(obj, key, { value: value, enumerable: false, writable: true });
  };
  var unsupported = function (what) {
    throw new Error(what + " is not supported by BAE scripting yet");
  };
  var text = function (args) {
    var out = [];
    for (var i = 0; i < args.length; i++) out.push(String(args[i]));
    return out.join(" ");
  };

  // ---- values ---------------------------------------------------------------

  function TextDocument(t) {
    if (!(this instanceof TextDocument)) return new TextDocument(t);
    this.text = t === undefined ? "" : String(t);
  }
  TextDocument.prototype.toString = function () { return this.text; };
  TextDocument.prototype.resetCharStyle = function () {};
  TextDocument.prototype.resetParagraphStyle = function () {};
  var fromHost = function (v) {
    if (v && typeof v === "object" && !Array.isArray(v) && typeof v.text === "string") {
      var d = new TextDocument(v.text);
      for (var k in v) if (k !== "text") d[k] = v[k];
      return d;
    }
    return v;
  };
  var toHost = function (v) {
    if (v instanceof Layer || v instanceof Item) return v._p;
    if (v instanceof TextDocument) {
      var o = {};
      for (var k in v) if (Object.prototype.hasOwnProperty.call(v, k)) o[k] = v[k];
      return o;
    }
    return v === undefined ? null : v;
  };

  // ---- collections (1-based, like AE) ---------------------------------------

  // `list()` returns the current members; `extra` adds methods.
  var collection = function (list, make, extra) {
    var base = { get length() { return list().length; } };
    for (var k in extra) Object.defineProperty(base, k, Object.getOwnPropertyDescriptor(extra, k));
    return new Proxy(base, {
      get: function (t, key) {
        if (typeof key === "string" && /^[0-9]+$/.test(key)) {
          var items = list();
          var i = Number(key);
          if (i < 1 || i > items.length) return undefined;
          return make(items[i - 1]);
        }
        return t[key];
      },
    });
  };

  // ---- project items --------------------------------------------------------

  var TYPE_NAMES = { comp: "Composition", folder: "Folder", footage: "Footage" };

  var mkItem = function (path, type) {
    if (path === null || path === undefined) return null;
    if (!type) type = (reads().itemRows.get(path) || itemInfo(path)).type;
    if (type === "comp") return new CompItem(path);
    if (type === "folder") return new FolderItem(path);
    return new FootageItem(path);
  };

  function itemFromRow(row) { return mkItem(row.path, row.type); }

  function Item(path) { hide(this, "_p", path); }
  Item.prototype = {
    constructor: Item,
    _i: function () {
      if (this._p === "/") return { type: "folder", name: "Root", parent: null, items: folderItems("/") };
      return itemInfo(this._p);
    },
    get id() { return this._p; },
    get name() { return (reads().itemRows.get(this._p) || this._i()).name; },
    set name(v) {
      if (this instanceof CompItem) H("setComp", { path: this._p, field: "name", value: String(v) });
      else H("rename", { path: this._p, name: String(v) });
    },
    get typeName() { return TYPE_NAMES[(reads().itemRows.get(this._p) || this._i()).type]; },
    get parentFolder() {
      var p = (reads().itemRows.get(this._p) || this._i()).parent;
      return p ? new FolderItem(p) : g.app.project.rootFolder;
    },
    get selected() { return false; },
    set selected(v) {},
    get comment() { return ""; },
    set comment(v) {},
    remove: function () { H("deleteItem", { path: this._p }); },
    toJSON: function () { return { item: this._p, name: this.name }; },
    toString: function () { return "[object " + this.constructor.name + "]"; },
  };

  function FolderItem(path) { Item.call(this, path); }
  FolderItem.prototype = Object.create(Item.prototype, {
    constructor: { value: FolderItem },
    items: {
      get: function () {
        var self = this;
        return collection(function () { return folderItems(self._p); }, itemFromRow, {
          addComp: function (n, w, h, pa, d, fr) {
            return g.app.project.items.addComp(n, w, h, pa, d, fr, self._p === "/" ? null : self._p);
          },
          addFolder: function (n) {
            return g.app.project.items.addFolder(n, self._p === "/" ? null : self._p);
          },
        });
      },
    },
    numItems: { get: function () { return this.items.length; } },
    item: { value: function (i) { return this.items[i]; } },
  });

  function AVItem(path) { Item.call(this, path); }
  AVItem.prototype = Object.create(Item.prototype, {
    constructor: { value: AVItem },
    width: { get: function () { return this._i().width; } },
    height: { get: function () { return this._i().height; } },
    pixelAspect: { get: function () { return this._i().pixelAspect; } },
    duration: { get: function () { return this._i().duration; } },
    frameRate: { get: function () { return this._i().frameRate; } },
    frameDuration: { get: function () { var r = this._i().frameRate; return r > 0 ? 1 / r : 0; } },
    hasVideo: { get: function () { return true; } },
  });

  function FootageItem(path) { AVItem.call(this, path); }
  FootageItem.prototype = Object.create(AVItem.prototype, {
    constructor: { value: FootageItem },
    file: { get: function () { var f = this._i().file; return f ? { fsName: f, name: f.split(/[\\/]/).pop() } : null; } },
    mainSource: {
      get: function () {
        var i = this._i();
        return { color: i.color && i.color.slice(), isStill: i.duration === 0, file: this.file };
      },
    },
  });

  function CompItem(path) { AVItem.call(this, path); }
  var compSetter = function (field) {
    return function (v) { H("setComp", { path: this._p, field: field, value: v }); };
  };
  CompItem.prototype = Object.create(AVItem.prototype, {
    constructor: { value: CompItem },
    width: { get: function () { return this._i().width; }, set: compSetter("width") },
    height: { get: function () { return this._i().height; }, set: compSetter("height") },
    pixelAspect: { get: function () { return this._i().pixelAspect; }, set: compSetter("pixelAspect") },
    duration: { get: function () { return this._i().duration; }, set: compSetter("duration") },
    frameRate: { get: function () { return this._i().frameRate; }, set: compSetter("frameRate") },
    frameDuration: { get: function () { return this._i().frameDuration; } },
    bgColor: { get: function () { return this._i().bgColor.slice(); }, set: compSetter("bgColor") },
    workAreaStart: { get: function () { return this._i().workAreaStart; } },
    workAreaDuration: { get: function () { return this._i().workAreaDuration; } },
    displayStartTime: { get: function () { return this._i().displayStartTime; } },
    time: {
      get: function () { return H("compTime", { path: this._p }); },
      set: function (v) { H("setComp", { path: this._p, field: "time", value: Number(v) }); },
    },
    numLayers: { get: function () { return this._i().layers.length; } },
    layers: {
      get: function () {
        var comp = this;
        var add = function (args) {
          args.comp = comp._p;
          return mkLayer(H("addLayer", args));
        };
        return collection(function () { return comp._i().layers; }, layerFromRow, {
          addSolid: function (color, name, w, h, pa, d) {
            return add({ kind: "solid", color: color, name: name, width: w, height: h, pixelAspect: pa, duration: d });
          },
          addText: function (t) {
            return add({ kind: "text", text: t === undefined ? undefined : String(t instanceof TextDocument ? t.text : t) });
          },
          addBoxText: function (size, t) { return this.addText(t); },
          addNull: function (d) { return add({ kind: "null", duration: d }); },
          addShape: function () { return add({ kind: "shape" }); },
          addCamera: function (name) { return add({ kind: "camera", name: name }); },
          addLight: function (name) { return add({ kind: "light", name: name }); },
          add: function (item, d) {
            var type = (reads().itemRows.get(item._p) || itemInfo(item._p)).type;
            return add({ kind: type === "comp" ? "pre_comp" : "footage", source: item._p, duration: d });
          },
          byName: function (n) { return comp.layer(n); },
          precompose: function () { unsupported("LayerCollection.precompose"); },
        });
      },
    },
    layer: {
      value: function (which) {
        var rows = this._i().layers;
        var row = typeof which === "number" ? rows[which - 1] :
          rows.find(function (r) { return r.name === String(which); });
        return row ? layerFromRow(row) : null;
      },
    },
    selectedLayers: {
      get: function () {
        var comp = this._p;
        return Array.from(selection()).filter(function (p) {
          return layerInfo(p).comp === comp;
        }).map(function (p) { return mkLayer(p); });
      },
    },
    selectedProperties: { get: function () { return []; } },
  });

  // ---- properties -----------------------------------------------------------

  // A property: `attr` on the prim at `path`.
  function Property(path, attr, name, matchName, parent) {
    hide(this, "_p", path);
    hide(this, "_a", attr);
    hide(this, "_n", name);
    hide(this, "_m", matchName || name);
    hide(this, "_parent", parent || null);
  }
  var keyTimes = function (p) { return propInfo(p).keys; };
  Property.prototype = {
    constructor: Property,
    get name() { return this._n; },
    get matchName() { return this._m; },
    get parentProperty() { return this._parent; },
    get propertyType() { return g.PropertyType.PROPERTY; },
    get isModified() { return true; },
    get canVaryOverTime() { return true; },
    get canSetExpression() { return true; },
    get enabled() { return true; },
    get value() { return fromHost(H("sample", { path: this._p, attr: this._a })); },
    valueAtTime: function (t, preExpression) { return fromHost(H("sample", { path: this._p, attr: this._a, time: t, preExpression: !!preExpression })); },
    setValue: function (v) { H("set", { path: this._p, attr: this._a, value: toHost(v) }); },
    setValueAtTime: function (t, v) {
      H("set", { path: this._p, attr: this._a, value: toHost(v), time: t, key: true });
    },
    setValuesAtTimes: function (times, values) {
      for (var i = 0; i < times.length; i++) this.setValueAtTime(times[i], values[i]);
    },
    setValueAtKey: function (i, v) { this.setValueAtTime(this.keyTime(i), v); },
    get numKeys() { return keyTimes(this).length; },
    get isTimeVarying() { return keyTimes(this).length > 0 || this.expressionEnabled; },
    keyTime: function (i) {
      var k = keyTimes(this)[i - 1];
      if (!k) throw new RangeError("keyframe index " + i + " is out of range");
      return k.time;
    },
    keyValue: function (i) {
      var k = keyTimes(this)[i - 1];
      if (!k) throw new RangeError("keyframe index " + i + " is out of range");
      return fromHost(k.value !== null && typeof k.value === "object" ? JSON.parse(JSON.stringify(k.value)) : k.value);
    },
    addKey: function (t) { return H("addKey", { path: this._p, attr: this._a, time: t }); },
    removeKey: function (i) { H("removeKey", { path: this._p, attr: this._a, index: i }); },
    nearestKeyIndex: function (t) {
      var keys = keyTimes(this), best = 0, dist = Infinity;
      for (var i = 0; i < keys.length; i++) {
        var d = Math.abs(keys[i].time - t);
        if (d < dist) { dist = d; best = i + 1; }
      }
      return best;
    },
    setInterpolationTypeAtKey: function (i, inType, outType) {
      var name = function (t) { return t === undefined ? null : INTERP[t] || null; };
      H("setInterpolation", {
        path: this._p, attr: this._a, index: i,
        in: name(inType), out: name(outType === undefined ? inType : outType),
      });
    },
    get expression() { return propInfo(this).expression; },
    set expression(src) {
      H("setExpression", { path: this._p, attr: this._a, source: String(src), enabled: true });
    },
    get expressionEnabled() { return propInfo(this).expressionEnabled; },
    set expressionEnabled(on) {
      H("setExpression", { path: this._p, attr: this._a, source: this.expression, enabled: !!on });
    },
    get expressionError() { return ""; },
    toJSON: function () { return { property: this._n, value: this.value }; },
  };

  var KeyframeInterpolationType = { LINEAR: 6612, BEZIER: 6613, HOLD: 6614 };
  var INTERP = {};
  INTERP[6612] = "linear";
  INTERP[6613] = "bezier";
  INTERP[6614] = "hold";

  // A property group that can be called like a function, as in AE
  // (`layer.effect("Blur")("Blurriness")`). `children()` returns its members.
  var group = function (name, matchName, parent, children, extra) {
    var f = function (which) { return f.property(which); };
    delete f.name;
    delete f.length;
    Object.setPrototypeOf(f, PropertyGroup.prototype);
    hide(f, "_n", name);
    hide(f, "_m", matchName);
    hide(f, "_parent", parent || null);
    hide(f, "_children", children);
    if (extra) {
      for (var k in extra) Object.defineProperty(f, k, Object.getOwnPropertyDescriptor(extra, k));
    }
    return f;
  };

  function PropertyGroup() {}
  PropertyGroup.prototype = {
    constructor: PropertyGroup,
    get name() { return this._n; },
    get matchName() { return this._m; },
    get parentProperty() { return this._parent; },
    get propertyType() { return g.PropertyType.INDEXED_GROUP; },
    get numProperties() { return this._children().length; },
    property: function (which) {
      var list = this._children();
      if (typeof which === "number") return list[which - 1] || null;
      var key = String(which).toLowerCase();
      for (var i = 0; i < list.length; i++) {
        var c = list[i];
        if (c.name.toLowerCase() === key || String(c.matchName).toLowerCase() === key ||
            (c._alias && c._alias.indexOf(key) >= 0)) return c;
      }
      return null;
    },
    canAddProperty: function () { return false; },
    addProperty: function () { unsupported("addProperty on " + this._n); },
  };

  // name, matchName, attr, attribute shortcut, aliases
  var TRANSFORM = [
    ["Anchor Point", "ADBE Anchor Point", "transform:anchor_point", "anchorPoint"],
    ["Position", "ADBE Position", "transform:position", "position"],
    ["X Position", "ADBE Position_0", "transform:position_x", "xPosition"],
    ["Y Position", "ADBE Position_1", "transform:position_y", "yPosition"],
    ["Z Position", "ADBE Position_2", "transform:position_z", "zPosition"],
    ["Scale", "ADBE Scale", "transform:scale", "scale"],
    ["Orientation", "ADBE Orientation", "transform:orientation", "orientation"],
    ["X Rotation", "ADBE Rotate X", "transform:rotation_x", "xRotation"],
    ["Y Rotation", "ADBE Rotate Y", "transform:rotation_y", "yRotation"],
    ["Rotation", "ADBE Rotate Z", "transform:rotation", "rotation", ["z rotation", "zrotation"]],
    ["Opacity", "ADBE Opacity", "transform:opacity", "opacity"],
    ["Point of Interest", "ADBE Point of Interest", "transform:point_of_interest", "pointOfInterest"],
  ];

  var transformGroup = function (layer) {
    var g0 = group("Transform", "ADBE Transform Group", layer, function () {
      return TRANSFORM.map(function (d) {
        var p = new Property(layer._p, d[2], d[0], d[1], g0);
        hide(p, "_alias", [d[3].toLowerCase()].concat(d[4] || []));
        return p;
      });
    });
    TRANSFORM.forEach(function (d) {
      Object.defineProperty(g0, d[3], {
        get: function () { return new Property(layer._p, d[2], d[0], d[1], g0); },
      });
    });
    return g0;
  };

  var effectGroup = function (layer, e) {
    var eg = group(e.name, e.matchName, null, function () {
      return effectInfo(e.path).params.map(function (p) {
        return new Property(e.path, p.attr, p.name, p.matchName, eg);
      });
    }, {
      get enabled() { return effectInfo(e.path).enabled; },
      set enabled(v) { H("setEffectEnabled", { path: e.path, value: !!v }); },
      get active() { return this.enabled; },
      get propertyType() { return g.PropertyType.NAMED_GROUP; },
      remove: function () { H("removeEffect", { path: e.path }); },
    });
    return eg;
  };

  var effectsGroup = function (layer) {
    var eg = group("Effects", "ADBE Effect Parade", layer, function () {
      return layerEffects(layer._p).map(function (e) { return effectGroup(layer, e); });
    }, {
      canAddProperty: function () { return true; },
      addProperty: function (matchName) {
        var path = H("applyEffect", { layer: layer._p, matchName: String(matchName) });
        var e = layerEffects(layer._p).filter(function (x) { return x.path === path; })[0];
        return effectGroup(layer, e);
      },
    });
    return eg;
  };

  var MASK_PROPS = [
    ["Mask Path", "ADBE Mask Shape", "path", "maskPath"],
    ["Mask Feather", "ADBE Mask Feather", "feather", "maskFeather"],
    ["Mask Opacity", "ADBE Mask Opacity", "opacity", "maskOpacity"],
    ["Mask Expansion", "ADBE Mask Offset", "expansion", "maskExpansion"],
  ];

  var masksGroup = function (layer) {
    return group("Masks", "ADBE Mask Parade", layer, function () {
      return layerMasks(layer._p).map(function (m) {
        var mg = group(m.name, "ADBE Mask Atom", null, function () {
          return MASK_PROPS.map(function (d) { return new Property(m.path, d[2], d[0], d[1], mg); });
        });
        MASK_PROPS.forEach(function (d) {
          Object.defineProperty(mg, d[3], { get: function () { return new Property(m.path, d[2], d[0], d[1], mg); } });
        });
        return mg;
      });
    });
  };

  var textGroup = function (layer) {
    var tg = group("Text", "ADBE Text Properties", layer, function () {
      return [new Property(layer._p, "text:source", "Source Text", "ADBE Text Document", tg)];
    });
    Object.defineProperty(tg, "sourceText", {
      get: function () { return new Property(layer._p, "text:source", "Source Text", "ADBE Text Document", tg); },
    });
    return tg;
  };

  // ---- layers ---------------------------------------------------------------

  function layerFromRow(row) { return mkLayer(row.path, row.kind); }
  var mkLayer = function (path, kind) {
    if (path === null || path === undefined) return null;
    if (kind === undefined) kind = (reads().layerRows.get(path) || layerInfo(path)).kind;
    if (kind === "text") return new TextLayer(path);
    if (kind === "shape") return new ShapeLayer(path);
    if (kind === "camera") return new CameraLayer(path);
    if (kind === "light") return new LightLayer(path);
    return new AVLayer(path);
  };

  var layerSetter = function (field) {
    return function (v) { H("setLayer", { path: this._p, field: field, value: toHost(v) }); };
  };

  function Layer(path) { hide(this, "_p", path); }
  Layer.prototype = {
    constructor: Layer,
    _i: function () { return layerInfo(this._p); },
    get name() { return (reads().layerRows.get(this._p) || this._i()).name; },
    set name(v) { H("setLayer", { path: this._p, field: "name", value: String(v) }); },
    get index() { return layerRow(this._p).index; },
    get id() { return this._p; },
    get inPoint() { return this._i().inPoint; },
    set inPoint(v) { layerSetter("inPoint").call(this, v); },
    get outPoint() { return this._i().outPoint; },
    set outPoint(v) { layerSetter("outPoint").call(this, v); },
    get startTime() { return this._i().startTime; },
    set startTime(v) { layerSetter("startTime").call(this, v); },
    get stretch() { return this._i().stretch; },
    get enabled() { return this._i().enabled; },
    set enabled(v) { layerSetter("enabled").call(this, !!v); },
    get solo() { return this._i().solo; },
    set solo(v) { layerSetter("solo").call(this, !!v); },
    get shy() { return this._i().shy; },
    set shy(v) { layerSetter("shy").call(this, !!v); },
    get locked() { return this._i().locked; },
    set locked(v) { layerSetter("locked").call(this, !!v); },
    get label() { return this._i().label; },
    set label(v) { layerSetter("label").call(this, v); },
    get selected() { return selection().has(this._p); },
    set selected(v) { layerSetter("selected").call(this, !!v); },
    get nullLayer() { return this._i().kind === "null"; },
    get hasVideo() { var k = this._i().kind; return k !== "null" && k !== "camera" && k !== "light" && k !== "audio"; },
    get isNameSet() { return true; },
    get comment() { return ""; },
    set comment(v) {},
    get containingComp() { return new CompItem(this._i().comp); },
    get parent() { return mkLayer(this._i().parent); },
    set parent(l) { layerSetter("parent").call(this, l ? l : null); },
    setParentWithJump: function (l) { layerSetter("parentJump").call(this, l ? l : null); },
    activeAtTime: function (t) { var i = this._i(); return i.enabled && t >= i.inPoint && t < i.outPoint; },
    remove: function () { H("deleteLayer", { path: this._p }); },
    duplicate: function () { return mkLayer(H("duplicateLayer", { path: this._p })); },
    moveToBeginning: function () { H("moveLayer", { path: this._p, index: 0 }); },
    moveToEnd: function () { H("moveLayer", { path: this._p, index: 1e9 }); },
    moveBefore: function (other) {
      var a = this.index, b = other.index;
      H("moveLayer", { path: this._p, index: a < b ? b - 2 : b - 1 });
    },
    moveAfter: function (other) {
      var a = this.index, b = other.index;
      H("moveLayer", { path: this._p, index: a < b ? b - 1 : b });
    },
    _groups: function () {
      var kind = this._i().kind, out = [];
      if (kind === "text") out.push(this.text);
      out.push(this.mask, this.effect, this.transform);
      return out;
    },
    get numProperties() { return this._groups().length; },
    property: function (which) {
      if (typeof which === "number") return this._groups()[which - 1] || null;
      var key = String(which).toLowerCase();
      var groups = this._groups();
      for (var i = 0; i < groups.length; i++) {
        var gr = groups[i];
        if (gr.name.toLowerCase() === key || gr.matchName.toLowerCase() === key) return gr;
      }
      // Lenient: transform properties and Source Text directly on the layer.
      for (var j = 0; j < groups.length; j++) {
        var p = groups[j].property(which);
        if (p && groups[j].matchName !== "ADBE Effect Parade" && groups[j].matchName !== "ADBE Mask Parade") return p;
      }
      return null;
    },
    get transform() { return transformGroup(this); },
    get effect() { return effectsGroup(this); },
    get Effects() { return effectsGroup(this); },
    get mask() { return masksGroup(this); },
    get Masks() { return masksGroup(this); },
    applyPreset: function () { unsupported("Layer.applyPreset"); },
    copyToComp: function () { unsupported("Layer.copyToComp"); },
    toJSON: function () { return { layer: this._p, name: this.name }; },
    toString: function () { return "[object " + this.constructor.name + "]"; },
  };
  // Transform properties directly on the layer (`layer.position`), as AE allows.
  TRANSFORM.forEach(function (d) {
    Object.defineProperty(Layer.prototype, d[3], {
      get: function () { return this.transform[d[3]]; },
    });
  });

  function AVLayer(path) { Layer.call(this, path); }
  AVLayer.prototype = Object.create(Layer.prototype, {
    constructor: { value: AVLayer },
    source: { get: function () { var s = this._i().source; return s ? mkItem(s) : null; } },
    threeDLayer: { get: function () { return this._i().threeD; }, set: function (v) { layerSetter("threeD").call(this, !!v); } },
    adjustmentLayer: { get: function () { return this._i().adjustment; }, set: function (v) { layerSetter("adjustment").call(this, !!v); } },
    motionBlur: { get: function () { return this._i().motionBlur; }, set: function (v) { layerSetter("motionBlur").call(this, !!v); } },
    blendingMode: { get: function () { return this._i().blendingMode; }, set: function (v) { layerSetter("blendingMode").call(this, String(v)); } },
    width: { get: function () { var s = this.source; return s ? s.width : this.containingComp.width; } },
    height: { get: function () { var s = this.source; return s ? s.height : this.containingComp.height; } },
  });

  function TextLayer(path) { AVLayer.call(this, path); }
  TextLayer.prototype = Object.create(AVLayer.prototype, {
    constructor: { value: TextLayer },
    text: { get: function () { return textGroup(this); } },
    sourceText: { get: function () { return textGroup(this).sourceText; } },
  });

  function ShapeLayer(path) { AVLayer.call(this, path); }
  ShapeLayer.prototype = Object.create(AVLayer.prototype, { constructor: { value: ShapeLayer } });
  function CameraLayer(path) { Layer.call(this, path); }
  CameraLayer.prototype = Object.create(Layer.prototype, { constructor: { value: CameraLayer } });
  function LightLayer(path) { Layer.call(this, path); }
  LightLayer.prototype = Object.create(Layer.prototype, { constructor: { value: LightLayer } });

  // ---- project and app ------------------------------------------------------

  var project = {
    get items() {
      return collection(projectItems, itemFromRow, {
        addComp: function (name, w, h, pa, d, fr, parent) {
          return new CompItem(H("addComp", {
            name: name, width: w, height: h, pixelAspect: pa, duration: d, frameRate: fr, parent: parent || null,
          }));
        },
        addFolder: function (name, parent) {
          return new FolderItem(H("addFolder", { name: name, parent: parent || null }));
        },
      });
    },
    get numItems() { return projectItems().length; },
    item: function (i) { return this.items[i]; },
    itemByID: function (id) { return mkItem(id); },
    layerByPath: function (path) { return mkLayer(path); },
    get rootFolder() { return new FolderItem("/"); },
    get activeItem() {
      var c = H("context").activeComp;
      return c ? new CompItem(c) : null;
    },
    get selection() { return []; },
    get file() { return null; },
    importFile: function () { unsupported("Project.importFile"); },
    save: function () { unsupported("Project.save"); },
  };

  var app = {
    get project() { return project; },
    get version() { return H("context").version; },
    get buildName() { return "BAE " + H("context").version; },
    get isWatchFolder() { return false; },
    get isRenderEngine() { return false; },
    beginUndoGroup: function (name) { H("undoGroup", { name: String(name) }); },
    endUndoGroup: function () {},
    beginSuppressDialogs: function () {},
    endSuppressDialogs: function () {},
    newProject: function () { unsupported("app.newProject"); },
    open: function () { unsupported("app.open"); },
    executeCommand: function () { unsupported("app.executeCommand"); },
    findMenuCommandId: function () { return 0; },
    scheduleTask: function () { unsupported("app.scheduleTask"); },
  };

  // ---- globals --------------------------------------------------------------

  g.app = app;
  g.TextDocument = TextDocument;
  g.PropertyType = Object.freeze({ PROPERTY: 6212, INDEXED_GROUP: 6213, NAMED_GROUP: 6214 });
  var modes = {};
  for (var mode of H("blendModes")) modes[mode.toUpperCase()] = mode;
  modes.SILHOUETE_ALPHA = modes.SILHOUETTE_ALPHA;
  g.BlendingMode = Object.freeze(modes);
  g.KeyframeInterpolationType = KeyframeInterpolationType;
  g.Item = Item;
  g.FolderItem = FolderItem;
  g.AVItem = AVItem;
  g.FootageItem = FootageItem;
  g.CompItem = CompItem;
  g.Layer = Layer;
  g.AVLayer = AVLayer;
  g.TextLayer = TextLayer;
  g.ShapeLayer = ShapeLayer;
  g.CameraLayer = CameraLayer;
  g.LightLayer = LightLayer;
  g.Property = Property;
  g.PropertyGroup = PropertyGroup;
  g.alert = function (msg) { H("alert", { text: String(msg) }); };
  // No modal questions during a run: confirm answers "no", prompt the default.
  g.confirm = function (msg) { H("log", { text: "confirm: " + String(msg) + " → false" }); return false; };
  g.prompt = function (msg, def) { return def === undefined ? null : def; };
  var log = function () { H("log", { text: text(arguments) }); };
  g.console = { log: log, info: log, warn: log, error: log, debug: log };
  g.$ = {
    writeln: log,
    write: log,
    sleep: function () {},
    global: g,
    get os() { return "BAE"; },
    get version() { return H("context").version; },
    get fileName() { return ""; },
  };
  g.bae = {
    pathProperty: function (path) {
      var info = H("pathInfo", { path: path });
      info.layer = mkLayer(info.layer);
      info.property = new Property(path, "path", info.name);
      return info;
    },
    commands: {
      // Runs any registered BAE command (the same ones MCP tools and menus run).
      invoke: function (id, params) { return H("invoke", { id: String(id), params: params || {} }); },
    },
  };
})(globalThis);

export const {
  app, bae, Item, FolderItem, AVItem, FootageItem, CompItem, Layer, AVLayer,
  TextLayer, ShapeLayer, CameraLayer, LightLayer, Property, PropertyGroup,
  TextDocument, KeyframeInterpolationType, PropertyType, BlendingMode,
} = globalThis;
