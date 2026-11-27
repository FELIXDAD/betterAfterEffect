// ExtendScript file and project services supplied by the application.
(function (g) {
  "use strict";
  function host(op, args) { return JSON.parse(__host("application", JSON.stringify({op:op,args:args || {}}))); }
  function desktop(op, args) {
    if (typeof __uiWait !== "function") throw new Error(op + " requires the application window");
    var result = JSON.parse(__uiWait(JSON.stringify({type:"application", op:op, args:args || {}})));
    if (result.error) throw new Error(result.error);
    return result.value;
  }
  function File(path) {
    if (!(this instanceof File)) return new File(path);
    this.fsName = absolute(path);
    this.encoding = "UTF-8";
    this.lineFeed = "Unix";
    this._mode = null;
    this._pos = 0;
  }
  function Folder(path) {
    if (!(this instanceof Folder)) return new Folder(path);
    this.fsName = absolute(path);
  }
  function absolute(path) {
    var s = String(path);
    return /^(?:[A-Za-z]:[\\/]|[\\/])/.test(s) ? s : host("absolute", {path:s});
  }
  function join(base, name) {return base.replace(/[\\/]$/, "") + "/" + name;}
  [File, Folder].forEach(function (ctor) {
    var proto = ctor.prototype;
    proto.toString = function () {return this.fsName;};
    Object.defineProperties(proto, {
      fullName: {get:function () {return this.fsName.replace(/\\/g,"/");}},
      name: {get:function () {return this.fullName.split("/").pop();}},
      parent: {get:function () {var p = host("parent",{path:this.fsName}); return p ? new Folder(p) : null;}},
      exists: {get:function () {return host("exists",{path:this.fsName,folder:ctor === Folder});}}
    });
  });
  Object.defineProperty(Folder,"current",{get:function () {return new Folder(host("root"));}});
  Folder.selectDialog = function (title) {var p = desktop("folder",{title:title});return p ? new Folder(p) : null;};
  File.openDialog = function (title) {var p = desktop("pickFile",{title:title});return p ? new File(p) : null;};
  Folder.prototype.create = function () {return host("mkdir",{path:this.fsName});};
  Folder.prototype.getFiles = function (filter) {
    var files = host("list",{path:this.fsName}).map(function (r) {
      return r.folder ? new Folder(join(this.fsName,r.name)) : new File(join(this.fsName,r.name));
    },this);
    if (typeof filter === "function") return files.filter(filter);
    if (typeof filter !== "string") return files;
    var pattern = filter.split("*").map(function (s) {
      return Array.from(s).map(function (c) {return c === "?" ? "." : "\\u" + c.charCodeAt(0).toString(16).padStart(4,"0");}).join("");
    }).join(".*");
    var matcher = new RegExp("^" + pattern + "$","i");
    return files.filter(function (f) {return matcher.test(f.name);});
  };
  File.prototype.open = function (mode) {
    this.close();
    if (!/^[rwa]$/.test(mode)) throw new Error("unknown file mode");
    var value = host("open",{path:this.fsName,mode:mode});
    if (value === null) return false;
    this._mode = mode;
    this._text = mode === "r" ? value : "";
    this._pos = 0;
    return true;
  };
  File.prototype.close = function () { this._mode = null; this._text = ""; return true; };
  File.prototype.read = function (n) {
    if (this._mode !== "r") throw new Error("file is not open for reading");
    var text = this._text.slice(this._pos,n === undefined ? undefined : this._pos + Math.max(0,n));
    this._pos += text.length; return text;
  };
  File.prototype.readln = function () {
    if (this._mode !== "r") throw new Error("file is not open for reading");
    var m = /^[^\r\n]*(?:\r\n|\r|\n)?/.exec(this._text.slice(this._pos))[0];
    this._pos += m.length;return m.replace(/[\r\n]+$/,"");
  };
  Object.defineProperty(File.prototype,"eof",{get:function () {return this._mode !== "r" || this._pos >= this._text.length;}});
  File.prototype.write = function () {
    if (this._mode !== "w" && this._mode !== "a") throw new Error("file is not open for writing");
    return host("append",{path:this.fsName,text:Array.prototype.join.call(arguments,"")});
  };
  File.prototype.writeln = function () {
    return this.write(Array.prototype.join.call(arguments,""),this.lineFeed === "Windows" ? "\r\n" : this.lineFeed === "Macintosh" ? "\r" : "\n");
  };
  function ImportOptions(file) {this.file = file || null; this.sequence = false;}
  g.File = File; g.Folder = Folder; g.ImportOptions = ImportOptions;
  var project = g.app.project;
  Object.defineProperty(g.app,"activeItem",{get:function () {return project.activeItem;}});
  project.importFile = function (options) {
    return new FootageItem(host("import",{path:options.file.fsName,sequence:!!options.sequence}));
  };
  Object.defineProperty(project,"file",{get:function () {
    var path = host("projectFile");return path ? new File(path) : null;
  }});
  project.save = function (file) { desktop("save",{path:file ? file.fsName : null}); };
  g.app.settings = {
    haveSetting:function(s,k) {return host("setting",{section:String(s),key:String(k)}) !== null;},
    getSetting:function(s,k) {var v=host("setting",{section:String(s),key:String(k)});if(v === null) throw Error("unknown setting");return v;},
    saveSetting:function(s,k,v) {host("saveSetting",{section:String(s),key:String(k),value:String(v)});}
  };
  g.app.findMenuCommandId = function (name) {return host("menuId",{name:String(name)});};
  g.app.executeCommand = function (id) { desktop("menu",{id:Number(id)}); };
  function row(id) {
    var data=host("queueItems").find(function (r) {return r.id === id;});
    if (!data) throw Error("render queue item was removed");return data;
  }
  function queueItem(id) {
    return {
      get comp() {return new CompItem(row(id).comp);},
      get render() {return row(id).render;},
      set render(v) {host("queueSet",{id:id,render:!!v});},
      get numOutputModules() {return row(id).outputs.length;},
      outputModule:function (i) {
        if (!Number.isInteger(i) || i < 1 || i > this.numOutputModules) throw RangeError("invalid output module index");
        return {get file() {var p=row(id).outputs[i-1];return p ? new File(p) : null;},
          set file(v) {host("queueSet",{id:id,module:i-1,path:v.fsName});}};
      },
      remove:function () {host("queueRemove",{id:id});}
    };
  }
  var queue = {
    get rendering() {return host("queueRunning");},
    render:function () {desktop("render");},
    get numItems() {return host("queueItems").length;},
    item:function (i) {var r=host("queueItems")[i-1];return r ? queueItem(r.id) : null;},
    get items() {
      return new Proxy({add:function (comp) {return queueItem(host("queueAdd",{comp:comp.id}));},
        get length() {return queue.numItems;}},{get:function (t,k) {return typeof k === "string" && /^[0-9]+$/.test(k) ? queue.item(Number(k)) : t[k];}});
    }
  };
  project.renderQueue = queue;
})(globalThis);
