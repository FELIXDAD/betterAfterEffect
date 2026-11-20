// mock-ae.js - a minimal stand-in for the After Effects scripting DOM, enough to run
// bae-export.jsx outside AE. Behaviour follows the AE scripting guide
// (https://ae-scripting.docsforadobe.dev/); it is NOT a substitute for testing in AE.
// Written without ES5 library calls so it keeps working after es3-guard.js runs.

var PropertyType = {PROPERTY: 6212, INDEXED_GROUP: 6213, NAMED_GROUP: 6214};
var PropertyValueType = {
    NO_VALUE: 6412, ThreeD_SPATIAL: 6413, ThreeD: 6414, TwoD_SPATIAL: 6415, TwoD: 6416, OneD: 6417,
    COLOR: 6418, CUSTOM_VALUE: 6419, MARKER: 6420, LAYER_INDEX: 6421, MASK_INDEX: 6422, SHAPE: 6423,
    TEXT_DOCUMENT: 6424
};
var KeyframeInterpolationType = {LINEAR: 6612, BEZIER: 6613, HOLD: 6614};
var BlendingMode = {NORMAL: 5212, MULTIPLY: 5220, SCREEN: 5225, ADD: 5213, SILHOUETE_ALPHA: 5230};
var TrackMatteType = {NO_TRACK_MATTE: 5012, ALPHA: 5013, ALPHA_INVERTED: 5014, LUMA: 5015, LUMA_INVERTED: 5016};
var AutoOrientType = {NO_AUTO_ORIENT: 4212, ALONG_PATH: 4213, CAMERA_OR_POINT_OF_INTEREST: 4214,
    CHARACTERS_TOWARD_CAMERA: 4215};
var FrameBlendingType = {NO_FRAME_BLEND: 4012, FRAME_MIX: 4013, PIXEL_MOTION: 4014};
var LayerQuality = {BEST: 4612, DRAFT: 4613, WIREFRAME: 4614};
var LayerSamplingQuality = {BICUBIC: 4812, BILINEAR: 4813};
var LightType = {PARALLEL: 4412, SPOT: 4413, POINT: 4414, AMBIENT: 4415};
var MaskMode = {NONE: 6812, ADD: 6813, SUBTRACT: 6814, INTERSECT: 6815, LIGHTEN: 6816, DARKEN: 6817,
    DIFFERENCE: 6818};
var MaskMotionBlur = {SAME_AS_LAYER: 7012, ON: 7013, OFF: 7014};
var MaskFeatherFalloff = {FFO_LINEAR: 7213, FFO_SMOOTH: 7212};
var AlphaMode = {IGNORE: 5412, STRAIGHT: 5413, PREMULTIPLIED: 5414};
var FieldSeparationType = {OFF: 5612, UPPER_FIELD_FIRST: 5613, LOWER_FIELD_FIRST: 5614};
var PulldownPhase = {OFF: 5812};
var ParagraphJustification = {LEFT_JUSTIFY: 7413, RIGHT_JUSTIFY: 7414, CENTER_JUSTIFY: 7415};
var TimeDisplayType = {TIMECODE: 2012, FRAMES: 2013};
var FramesCountType = {FC_START_0: 2612, FC_START_1: 2613, FC_TIMECODE_CONVERSION: 2614};
var FeetFramesFilmType = {MM16: 2412, MM35: 2413};
var FootageTimecodeDisplayStartType = {FTCS_START_0: 2212, FTCS_USE_SOURCE_MEDIA: 2213};
var GpuAccelType = {CUDA: 1812, METAL: 1813, OPENCL: 1814, SOFTWARE: 1815};

var $ = {
    os: "Mock OS",
    written: [],
    writeln: function (s) {
        this.written.push(String(s));
    }
};

// ---------------------------------------------------------------- files

function File(path) {
    this.fsName = String(path);
    var slash = this.fsName.lastIndexOf("/");
    this.name = encodeURI(this.fsName.substr(slash + 1));
    this.parent = new Folder(this.fsName.substr(0, slash));
    var info = MOCK_FS[this.fsName];
    this.exists = !!info;
    this.length = info ? info.size : -1;
    this.modified = info ? new Date(info.modified) : null;
}

File.decode = function (s) {
    return decodeURI(s);
};

function Folder(path) {
    this.fsName = String(path);
    this.name = this.fsName.substr(this.fsName.lastIndexOf("/") + 1);
    this.exists = !!MOCK_DIRS[this.fsName];
}

Folder.prototype.getFiles = function (filter) {
    var out = [];
    for (var p in MOCK_FS) {
        if (p.substr(0, this.fsName.length + 1) === this.fsName + "/" && p.substr(this.fsName.length + 1).indexOf("/") < 0) {
            var f = new File(p);
            if (!filter || filter(f)) {
                out.push(f);
            }
        }
    }
    return out;
};

File.prototype.open = function (mode) {
    this._mode = mode;
    this._buf = [];
    return true;
};

File.prototype.write = function (s) {
    this._buf.push(String(s));
    return true;
};

File.prototype.close = function () {
    if (this._mode === "w") {
        MOCK_WRITES[this.fsName] = {text: this._buf.join(""), encoding: this.encoding, lineFeed: this.lineFeed};
        MOCK_FS[this.fsName] = {size: this._buf.join("").length, modified: Date.UTC(2026, 10, 20)};
    }
    return true;
};

File.prototype.copy = function (target) {
    if (!this.exists) {
        return false;
    }
    MOCK_FS[String(target)] = MOCK_FS[this.fsName];
    return true;
};

Folder.prototype.create = function () {
    MOCK_DIRS[this.fsName] = true;
    this.exists = true;
    return true;
};

/** path -> {size, modified (ms since epoch)} */
var MOCK_FS = {};
/** path -> {text, encoding, lineFeed} written through File.open("w") */
var MOCK_WRITES = {};
/** folders created through Folder.create */
var MOCK_DIRS = {};

// ---------------------------------------------------------------- properties

function Prop(matchName, name, vt, value, extra) {
    this.matchName = matchName;
    this.name = name;
    this.propertyType = PropertyType.PROPERTY;
    this.propertyValueType = vt;
    this._value = value;
    this.value = value;
    this._keys = [];
    this.numKeys = 0;
    this.expression = "";
    this.expressionEnabled = false;
    this.expressionError = "";
    this.canSetExpression = vt !== PropertyValueType.MARKER && vt !== PropertyValueType.NO_VALUE;
    this.isSpatial = vt === PropertyValueType.TwoD_SPATIAL || vt === PropertyValueType.ThreeD_SPATIAL;
    this.isModified = true;
    if (extra) {
        for (var k in extra) {
            this[k] = extra[k];
        }
    }
}

Prop.prototype.valueAtTime = function (t, pre) {
    if (this.propertyValueType === PropertyValueType.CUSTOM_VALUE ||
        this.propertyValueType === PropertyValueType.NO_VALUE) {
        throw new Error("Can not get value of " + this.matchName);
    }
    return this._value;
};

/** key: {t, v, inType, outType, inEase, outEase, inTan, outTan, tc, tab, sc, sab, roving, label} */
Prop.prototype.addKey = function (key) {
    this._keys.push(key);
    this.numKeys = this._keys.length;
    return this;
};

Prop.prototype._key = function (i) {
    if (i < 1 || i > this._keys.length) {
        throw new Error("keyIndex out of range");
    }
    return this._keys[i - 1];
};

Prop.prototype.keyTime = function (i) {
    return this._key(i).t;
};
Prop.prototype.keyValue = function (i) {
    return this._key(i).v;
};
Prop.prototype.keyInInterpolationType = function (i) {
    return this._key(i).inType || KeyframeInterpolationType.LINEAR;
};
Prop.prototype.keyOutInterpolationType = function (i) {
    return this._key(i).outType || KeyframeInterpolationType.LINEAR;
};
Prop.prototype.keyInTemporalEase = function (i) {
    if (this.propertyValueType === PropertyValueType.TEXT_DOCUMENT) {
        throw new Error("no temporal ease on text documents");
    }
    return this._key(i).inEase || [{speed: 0, influence: 16.666667}];
};
Prop.prototype.keyOutTemporalEase = function (i) {
    if (this.propertyValueType === PropertyValueType.TEXT_DOCUMENT) {
        throw new Error("no temporal ease on text documents");
    }
    return this._key(i).outEase || [{speed: 0, influence: 16.666667}];
};
Prop.prototype.keyTemporalContinuous = function (i) {
    return !!this._key(i).tc;
};
Prop.prototype.keyTemporalAutoBezier = function (i) {
    return !!this._key(i).tab;
};
Prop.prototype._spatialOnly = function () {
    if (!this.isSpatial) {
        throw new Error("not a spatial property");
    }
};
Prop.prototype.keyInSpatialTangent = function (i) {
    this._spatialOnly();
    return this._key(i).inTan || [0, 0, 0];
};
Prop.prototype.keyOutSpatialTangent = function (i) {
    this._spatialOnly();
    return this._key(i).outTan || [0, 0, 0];
};
Prop.prototype.keySpatialContinuous = function (i) {
    this._spatialOnly();
    return !!this._key(i).sc;
};
Prop.prototype.keySpatialAutoBezier = function (i) {
    this._spatialOnly();
    return !!this._key(i).sab;
};
Prop.prototype.keyRoving = function (i) {
    this._spatialOnly();
    return !!this._key(i).roving;
};
Prop.prototype.keyLabel = function (i) {
    return this._key(i).label || 0;
};

function Group(matchName, name, type, children, extra) {
    this.matchName = matchName;
    this.name = name;
    this.propertyType = type;
    this._children = children || [];
    this.numProperties = this._children.length;
    this.canSetEnabled = true;
    this.enabled = true;
    if (extra) {
        for (var k in extra) {
            this[k] = extra[k];
        }
    }
}

Group.prototype.property = function (key) {
    if (typeof key === "number") {
        if (key < 1 || key > this._children.length) {
            throw new Error("index out of range");
        }
        return this._children[key - 1];
    }
    for (var i = 0; i < this._children.length; i++) {
        if (this._children[i].matchName === key || this._children[i].name === key) {
            return this._children[i];
        }
    }
    return null;
};

Group.prototype.add = function (child) {
    this._children.push(child);
    this.numProperties = this._children.length;
    return child;
};

function named(matchName, name, children, extra) {
    return new Group(matchName, name, PropertyType.NAMED_GROUP, children, extra);
}

function indexed(matchName, name, children, extra) {
    return new Group(matchName, name, PropertyType.INDEXED_GROUP, children, extra);
}

function Shape(vertices, inTangents, outTangents, closed) {
    this.vertices = vertices;
    this.inTangents = inTangents;
    this.outTangents = outTangents;
    this.closed = closed;
    this.featherSegLocs = [];
}

function TextDocument(text) {
    this.text = text;
}

function MarkerValue(comment, extra) {
    this.comment = comment;
    this.duration = 0;
    this.chapter = "";
    this.url = "";
    this.frameTarget = "";
    this.cuePointName = "";
    this.eventCuePoint = false;
    this.label = 0;
    this.protectedRegion = false;
    this._params = {};
    if (extra) {
        for (var k in extra) {
            this[k] = extra[k];
        }
    }
}

MarkerValue.prototype.getParameters = function () {
    return this._params;
};

function markerProp(list) {
    var p = new Prop("ADBE Marker", "Marker", PropertyValueType.MARKER, null);
    for (var i = 0; i < list.length; i++) {
        p.addKey({t: list[i][0], v: list[i][1]});
    }
    return p;
}

// ---------------------------------------------------------------- items

function Item(id, name) {
    this.id = id;
    this.name = name;
    this.comment = "";
    this.label = 0;
    this.parentFolder = null;
}

function FolderItem(id, name) {
    Item.call(this, id, name);
}

function FootageSourceBase() {
    this.isStill = false;
    this.alphaMode = AlphaMode.STRAIGHT;
    this.premulColor = [0, 0, 0];
    this.invertAlpha = false;
    this.fieldSeparationType = FieldSeparationType.OFF;
    this.removePulldown = PulldownPhase.OFF;
    this.loop = 1;
    this.conformFrameRate = 0;
    this.nativeFrameRate = 0;
}

function FileSource(file, isStill) {
    FootageSourceBase.call(this);
    this.file = file;
    this.isStill = !!isStill;
    this.missingFootagePath = "";
}

function SolidSource(color) {
    FootageSourceBase.call(this);
    this.color = color;
    this.isStill = true;
}

function PlaceholderSource() {
    FootageSourceBase.call(this);
}

function FootageItem(id, name, source, opts) {
    Item.call(this, id, name);
    this.mainSource = source;
    this.file = source.file || null;
    this.footageMissing = false;
    this.width = 1920;
    this.height = 1080;
    this.pixelAspect = 1;
    this.duration = 0;
    this.frameRate = 0;
    this.hasVideo = true;
    this.hasAudio = false;
    if (opts) {
        for (var k in opts) {
            this[k] = opts[k];
        }
    }
}

function CompItem(id, name, opts) {
    Item.call(this, id, name);
    this.width = 1920;
    this.height = 1080;
    this.pixelAspect = 1;
    this.frameRate = 30;
    this.duration = 10;
    this.displayStartTime = 0;
    this.bgColor = [0, 0, 0];
    this.workAreaStart = 0;
    this.workAreaDuration = 10;
    this.shutterAngle = 180;
    this.shutterPhase = -90;
    this.motionBlurSamplesPerFrame = 16;
    this.motionBlurAdaptiveSampleLimit = 128;
    this.motionBlur = false;
    this.frameBlending = false;
    this.draft3d = false;
    this.hideShyLayers = false;
    this.preserveNestedFrameRate = false;
    this.preserveNestedResolution = false;
    this.renderer = "ADBE Advanced 3d";
    this.resolutionFactor = [1, 1];
    this.markerProperty = markerProp([]);
    this._layers = [];
    this.numLayers = 0;
    if (opts) {
        for (var k in opts) {
            this[k] = opts[k];
        }
    }
}

CompItem.prototype.layer = function (i) {
    return this._layers[i - 1];
};

/** Append a layer (AE index = position, 1 = top). */
CompItem.prototype.addLayer = function (layer) {
    this._layers.push(layer);
    this.numLayers = this._layers.length;
    layer.index = this.numLayers;
    layer.containingComp = this;
    return layer;
};

// ---------------------------------------------------------------- layers

function transformGroup(position, extra) {
    var children = [
        new Prop("ADBE Anchor Point", "Anchor Point", PropertyValueType.ThreeD_SPATIAL, [0, 0, 0]),
        position || new Prop("ADBE Position", "Position", PropertyValueType.ThreeD_SPATIAL, [960, 540, 0]),
        new Prop("ADBE Position_0", "X Position", PropertyValueType.OneD, 960),
        new Prop("ADBE Position_1", "Y Position", PropertyValueType.OneD, 540),
        new Prop("ADBE Position_2", "Z Position", PropertyValueType.OneD, 0),
        new Prop("ADBE Scale", "Scale", PropertyValueType.ThreeD, [100, 100, 100]),
        new Prop("ADBE Orientation", "Orientation", PropertyValueType.ThreeD_SPATIAL, [0, 0, 0]),
        new Prop("ADBE Rotate X", "X Rotation", PropertyValueType.OneD, 0),
        new Prop("ADBE Rotate Y", "Y Rotation", PropertyValueType.OneD, 0),
        new Prop("ADBE Rotate Z", "Rotation", PropertyValueType.OneD, 0),
        new Prop("ADBE Opacity", "Opacity", PropertyValueType.OneD, 100)
    ];
    if (extra) {
        for (var i = 0; i < extra.length; i++) {
            children.push(extra[i]);
        }
    }
    return named("ADBE Transform Group", "Transform", children);
}

/** A layer is a property group in AE; `groups` are its top-level property groups. */
function Layer(matchName, name, groups, opts) {
    Group.call(this, matchName, name, PropertyType.NAMED_GROUP, groups);
    this.index = 0;
    this.comment = "";
    this.label = 1;
    this.enabled = true;
    this.audioEnabled = true;
    this.solo = false;
    this.shy = false;
    this.locked = false;
    this.inPoint = 0;
    this.outPoint = 10;
    this.startTime = 0;
    this.stretch = 100;
    this.parent = null;
    this.hasVideo = true;
    this.hasAudio = false;
    this.threeDLayer = false;
    this.autoOrient = AutoOrientType.NO_AUTO_ORIENT;
    this.blendingMode = BlendingMode.NORMAL;
    this.preserveTransparency = false;
    this.trackMatteType = TrackMatteType.NO_TRACK_MATTE;
    this.isTrackMatte = false;
    this.motionBlur = false;
    this.effectsActive = true;
    this.adjustmentLayer = false;
    this.guideLayer = false;
    this.environmentLayer = false;
    this.collapseTransformation = false;
    this.timeRemapEnabled = false;
    this.frameBlendingType = FrameBlendingType.NO_FRAME_BLEND;
    this.quality = LayerQuality.BEST;
    this.samplingQuality = LayerSamplingQuality.BILINEAR;
    this.nullLayer = false;
    this.source = null;
    if (opts) {
        for (var k in opts) {
            this[k] = opts[k];
        }
    }
}

Layer.prototype = new Group("", "", PropertyType.NAMED_GROUP, []);
Layer.prototype.constructor = Layer;

var app = {
    version: "15.0x123 (mock)",
    buildName: "mock",
    isoLanguage: "en_US",
    project: null
};

function Project() {
    this._items = [];
    this.numItems = 0;
    this.rootFolder = new FolderItem(0, "Root");
    this.file = null;
    this.selection = [];
    this.bitsPerChannel = 8;
    this.linearBlending = false;
    this.expressionEngine = "javascript-1.0";
    this.displayStartFrame = 0;
    this.framesCountType = FramesCountType.FC_START_0;
    this.timeDisplayType = TimeDisplayType.TIMECODE;
    this.feetFramesFilmType = FeetFramesFilmType.MM35;
    this.footageTimecodeDisplayStartType = FootageTimecodeDisplayStartType.FTCS_USE_SOURCE_MEDIA;
    this.framesUseFeetFrames = false;
    this.gpuAccelType = GpuAccelType.METAL;
    this.items = new ItemCollection(this);
}

Project.prototype.item = function (i) {
    return this._items[i - 1];
};

Project.prototype.addItem = function (item, folder) {
    this._items.push(item);
    this.numItems = this._items.length;
    item.parentFolder = folder || this.rootFolder;
    return item;
};

// ---------------------------------------------------------------- write-side API (sample builder)

function KeyframeEase(speed, influence) {
    this.speed = speed;
    this.influence = influence;
}

app.beginUndoGroup = function () {
    app._undoDepth = (app._undoDepth || 0) + 1;
};
app.endUndoGroup = function () {
    app._undoDepth--;
};

/** text files readable through File.open("r") */
var MOCK_TEXT = {};

File.prototype.read = function () {
    if (!MOCK_TEXT.hasOwnProperty(this.fsName)) {
        throw new Error("mock: no text for " + this.fsName);
    }
    return MOCK_TEXT[this.fsName];
};

var _mockOpen = File.prototype.open;
File.prototype.open = function (mode) {
    if (mode === "r") {
        this._mode = "r";
        return MOCK_TEXT.hasOwnProperty(this.fsName);
    }
    return _mockOpen.call(this, mode);
};

function easeCountOf(prop) {
    if (prop.propertyValueType === PropertyValueType.TwoD) {
        return 2;
    }
    if (prop.propertyValueType === PropertyValueType.ThreeD) {
        return 3;
    }
    return 1;
}

Prop.prototype.setValue = function (v) {
    this._value = v;
    this.value = v;
};

Prop.prototype.setValueAtTime = function (t, v) {
    for (var i = 0; i < this._keys.length; i++) {
        if (Math.abs(this._keys[i].t - t) < 1e-9) {
            this._keys[i].v = v;
            return;
        }
    }
    var key = {t: t, v: v};
    var pos = this._keys.length;
    for (var j = 0; j < this._keys.length; j++) {
        if (this._keys[j].t > t) {
            pos = j;
            break;
        }
    }
    this._keys.splice(pos, 0, key);
    this.numKeys = this._keys.length;
};

Prop.prototype.removeKey = function (i) {
    this._key(i);
    this._keys.splice(i - 1, 1);
    this.numKeys = this._keys.length;
};

Prop.prototype.nearestKeyIndex = function (t) {
    var best = 1;
    for (var i = 0; i < this._keys.length; i++) {
        if (Math.abs(this._keys[i].t - t) < Math.abs(this._keys[best - 1].t - t)) {
            best = i + 1;
        }
    }
    return best;
};

Prop.prototype.setInterpolationTypeAtKey = function (i, inType, outType) {
    var k = this._key(i);
    k.inType = inType;
    k.outType = outType === undefined ? inType : outType;
};

Prop.prototype.setTemporalEaseAtKey = function (i, inEase, outEase) {
    var n = easeCountOf(this);
    if (inEase.length !== n || (outEase && outEase.length !== n)) {
        throw new Error("mock AE: " + this.matchName + " needs " + n + " KeyframeEase objects, got " + inEase.length);
    }
    for (var e = 0; e < inEase.length; e++) {
        if (!(inEase[e] instanceof KeyframeEase)) {
            throw new Error("mock AE: ease must be a KeyframeEase");
        }
    }
    var k = this._key(i);
    k.inEase = inEase;
    k.outEase = outEase || inEase;
};

Prop.prototype.setSpatialTangentsAtKey = function (i, inTan, outTan) {
    this._spatialOnly();
    var n = this.value.length;
    if (inTan.length !== n || outTan.length !== n) {
        throw new Error("mock AE: tangents need " + n + " values");
    }
    var k = this._key(i);
    k.inTan = inTan.length === 2 ? [inTan[0], inTan[1], 0] : inTan;
    k.outTan = outTan.length === 2 ? [outTan[0], outTan[1], 0] : outTan;
};

Prop.prototype.setTemporalContinuousAtKey = function (i, on) {
    this._key(i).tc = on;
};
Prop.prototype.setTemporalAutoBezierAtKey = function (i, on) {
    this._key(i).tab = on;
};
Prop.prototype.setSpatialContinuousAtKey = function (i, on) {
    this._spatialOnly();
    this._key(i).sc = on;
};
Prop.prototype.setSpatialAutoBezierAtKey = function (i, on) {
    this._spatialOnly();
    this._key(i).sab = on;
};
Prop.prototype.setRovingAtKey = function (i, on) {
    this._spatialOnly();
    if (i === 1 || i === this._keys.length) {
        throw new Error("mock AE: first and last keys cannot rove");
    }
    this._key(i).roving = on;
};
Prop.prototype.setLabelAtKey = function (i, label) {
    this._key(i).label = label;
};

/** Mock evaluation: linear between keys (hold where out type is HOLD); expressions are not run. */
Prop.prototype.valueAtTime = function (t, pre) {
    if (this.propertyValueType === PropertyValueType.CUSTOM_VALUE ||
        this.propertyValueType === PropertyValueType.NO_VALUE) {
        throw new Error("Can not get value of " + this.matchName);
    }
    var keys = this._keys;
    if (keys.length === 0) {
        return this._value;
    }
    if (t <= keys[0].t) {
        return keys[0].v;
    }
    for (var i = 0; i < keys.length - 1; i++) {
        var a = keys[i];
        var b = keys[i + 1];
        if (t < b.t) {
            if (a.outType === KeyframeInterpolationType.HOLD || typeof a.v !== typeof b.v) {
                return a.v;
            }
            var u = (t - a.t) / (b.t - a.t);
            if (typeof a.v === "number") {
                return a.v + (b.v - a.v) * u;
            }
            var out = [];
            for (var d = 0; d < a.v.length; d++) {
                out.push(a.v[d] + (b.v[d] - a.v[d]) * u);
            }
            return out;
        }
    }
    return keys[keys.length - 1].v;
};

var MOCK_EFFECTS = {
    "ADBE Color Control": [PropertyValueType.COLOR, [1, 0, 0, 1]],
    "ADBE Point Control": [PropertyValueType.TwoD_SPATIAL, [0, 0]],
    "ADBE Slider Control": [PropertyValueType.OneD, 0]
};

Group.prototype.addProperty = function (matchName) {
    var def = MOCK_EFFECTS[matchName];
    if (this.matchName !== "ADBE Effect Parade" || !def) {
        throw new Error("mock AE: cannot add " + matchName + " to " + this.matchName);
    }
    return this.add(named(matchName, matchName, [new Prop(matchName + "-0001", "Value", def[0], def[1])]));
};

function LayerCollection(comp) {
    this._comp = comp;
}

LayerCollection.prototype._add = function (matchName, name, opts) {
    var layer = new Layer(matchName, name, [
        indexed("ADBE Mask Parade", "Masks", []),
        indexed("ADBE Effect Parade", "Effects", []),
        transformGroup()
    ], opts);
    // AE inserts new layers at the top; the builder only uses one layer per comp.
    return this._comp.addLayer(layer);
};
LayerCollection.prototype.addNull = function () {
    return this._add("ADBE AV Layer", "Null 1", {nullLayer: true});
};
LayerCollection.prototype.addSolid = function (color, name) {
    return this._add("ADBE AV Layer", name, {});
};
LayerCollection.prototype.addShape = function () {
    return this._add("ADBE Vector Layer", "Shape Layer 1", {});
};

function ItemCollection(project) {
    this._project = project;
}

ItemCollection.prototype.addComp = function (name, width, height, pixelAspect, duration, frameRate) {
    var p = this._project;
    var comp = p.addItem(new CompItem(1000 + p.numItems, name, {width: width, height: height, pixelAspect: pixelAspect,
        duration: duration, frameRate: frameRate, frameDuration: 1 / frameRate}));
    comp.layers = new LayerCollection(comp);
    return comp;
};

ItemCollection.prototype.addFolder = function (name) {
    return this._project.addItem(new FolderItem(1000 + this._project.numItems, name));
};

Item.prototype.remove = function () {
    var items = app.project._items;
    for (var i = 0; i < items.length; i++) {
        if (items[i] === this) {
            items.splice(i, 1);
            app.project.numItems = items.length;
            return;
        }
    }
};
FolderItem.prototype.remove = Item.prototype.remove;
CompItem.prototype.remove = Item.prototype.remove;
