// AE expression library for bae-anim. Evaluated once per QuickJS runtime.
//
// Host functions (implemented in Rust, see expr/mod.rs) live on `__h`:
//   __h.prop(c, l, prim, attr, t, pre) -> number[]   property value at comp time t
//   __h.vel(c, l, prim, attr, t, pre) -> number[]    AE velocity (see velocityAtTime)
//   __h.keys(c, l, prim, attr) -> number[]           [t0, v0..., t1, v1..., ...] (comp time)
//   __h.dims(c, l, prim, attr) -> number             value dimensions
//   __h.layerIndex(c, nameOrIndex) -> number         0-based layer index or -1
//   __h.layerInfo(c, l) -> object                    name, inPoint, outPoint, ...
//   __h.compInfo(c) -> object                        name, width, height, ...
//   __h.compByName(name) -> number                   comp id or -1
//   __h.effect(c, l, nameOrIndex) -> object|null     effect path, active and layerControl
//   __h.param(c, l, effectPath, nameOrIndex) -> string|null   param attr name
//   __h.mask(c, l, nameOrIndex) -> string|null       mask prim path
//   __h.xform(c, l, t, kind, point) -> number[]      0 toComp 1 fromComp 2 toCompVec 3 fromCompVec
//                                                    4 toWorld 5 fromWorld 6 toWorldVec 7 fromWorldVec
//   __h.seed(c, l, prim, attr) -> number             stable per-property random seed
// `__now` is the expression's current comp time (seconds); posterizeTime changes it.

"use strict";
var __now = 0;

// ---- vector arithmetic (operator rewriting targets, AE semantics) ----
// Property objects stand for their value in arithmetic and library calls (AE).
function __val(a) {
  if (a === undefined) throw new Error("Undefined value used in expression (could be an out of range array subscript?)");
  if (a instanceof Property) return a.value;
  // `text.sourceText` (a String carrying the property's methods) is its text.
  if (a instanceof String) return a.valueOf();
  return a;
}
function __isArr(a) { return Array.isArray(a); }
function __isNum(a) { return typeof a === "number" || typeof a === "boolean"; }
function $bm_sum(a, b) {
  a = __val(a); b = __val(b);
  if (__isNum(a) && __isNum(b)) return a + b;
  if (typeof a === "string" || typeof b === "string") return a + b;
  if (__isArr(a) && __isNum(b)) { a = a.slice(0); a[0] += b; return a; }
  if (__isNum(a) && __isArr(b)) { b = b.slice(0); b[0] = a + b[0]; return b; }
  if (__isArr(a) && __isArr(b)) {
    var n = Math.max(a.length, b.length), out = new Array(n);
    for (var i = 0; i < n; i++) out[i] = (i < a.length ? a[i] : 0) + (i < b.length ? b[i] : 0);
    return out;
  }
  return a + b;
}
function $bm_sub(a, b) {
  a = __val(a); b = __val(b);
  if (__isNum(a) && __isNum(b)) return a - b;
  if (__isArr(a) && __isNum(b)) { a = a.slice(0); a[0] -= b; return a; }
  if (__isNum(a) && __isArr(b)) { b = b.slice(0); b[0] = a - b[0]; return b; }
  if (__isArr(a) && __isArr(b)) {
    var n = Math.max(a.length, b.length), out = new Array(n);
    for (var i = 0; i < n; i++) out[i] = (i < a.length ? a[i] : 0) - (i < b.length ? b[i] : 0);
    return out;
  }
  return a - b;
}
function $bm_mul(a, b) {
  a = __val(a); b = __val(b);
  if (__isNum(a) && __isNum(b)) return a * b;
  if (__isArr(a) && __isNum(b)) return a.map(function (x) { return x * b; });
  if (__isNum(a) && __isArr(b)) return b.map(function (x) { return a * x; });
  if (__isArr(a) && __isArr(b)) {
    var n = Math.min(a.length, b.length), out = new Array(n);
    for (var i = 0; i < n; i++) out[i] = a[i] * b[i];
    return out;
  }
  return a * b;
}
function $bm_div(a, b) {
  a = __val(a); b = __val(b);
  if (__isNum(a) && __isNum(b)) return a / b;
  if (__isArr(a) && __isNum(b)) return a.map(function (x) { return x / b; });
  if (__isNum(a) && __isArr(b)) return b.map(function (x) { return a / x; });
  if (__isArr(a) && __isArr(b)) {
    var n = Math.min(a.length, b.length), out = new Array(n);
    for (var i = 0; i < n; i++) out[i] = a[i] / b[i];
    return out;
  }
  return a / b;
}
function $bm_mod(a, b) {
  a = __val(a); b = __val(b);
  if (__isArr(a) && __isNum(b)) return a.map(function (x) { return x % b; });
  return a % b;
}
function $bm_neg(a) {
  a = __val(a);
  if (__isArr(a)) return a.map(function (x) { return -x; });
  return -a;
}

// ---- vector math ----
function add(a, b) { return $bm_sum(a, b); }
function sub(a, b) { return $bm_sub(a, b); }
function mul(a, b) { return $bm_mul(a, b); }
function div(a, b) { return $bm_div(a, b); }
function clamp(v, lo, hi) {
  v = __val(v); lo = __val(lo); hi = __val(hi);
  if (__isArr(v)) {
    return v.map(function (x, i) {
      var l = __isArr(lo) ? lo[i] : lo, h = __isArr(hi) ? hi[i] : hi;
      return Math.min(Math.max(x, Math.min(l, h)), Math.max(l, h));
    });
  }
  return Math.min(Math.max(v, Math.min(lo, hi)), Math.max(lo, hi));
}
function dot(a, b) {
  a = __val(a); b = __val(b);
  var s = 0;
  for (var i = 0; i < Math.min(a.length, b.length); i++) s += a[i] * b[i];
  return s;
}
function cross(a, b) {
  a = __val(a); b = __val(b);
  return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
}
function length(a, b) {
  a = __val(a);
  if (b !== undefined) a = $bm_sub(a, b);
  if (__isNum(a)) return Math.abs(a);
  var s = 0;
  for (var i = 0; i < a.length; i++) s += a[i] * a[i];
  return Math.sqrt(s);
}
function normalize(a) {
  a = __val(a);
  var l = length(a);
  return l > 0 ? $bm_div(a, l) : a.map(function () { return 0; });
}
function lookAt(from, to) {
  from = __val(from); to = __val(to);
  var d = normalize($bm_sub(to, from));
  var yaw = Math.atan2(d[0], d[2]), pitch = Math.atan2(-d[1], Math.sqrt(d[0] * d[0] + d[2] * d[2]));
  return [radiansToDegrees(pitch), radiansToDegrees(yaw), 0];
}
function degreesToRadians(d) { return d * Math.PI / 180; }
function radiansToDegrees(r) { return r * 180 / Math.PI; }

// ---- interpolation (AE: ease = smoothstep, easeIn/easeOut = Hermite with one unit tangent) ----
function __interp(shape, args) {
  var t, tMin, tMax, v1, v2;
  if (args.length <= 3) { t = args[0]; tMin = 0; tMax = 1; v1 = args[1]; v2 = args[2]; }
  else { t = args[0]; tMin = args[1]; tMax = args[2]; v1 = args[3]; v2 = args[4]; }
  t = __val(t); tMin = __val(tMin); tMax = __val(tMax); v1 = __val(v1); v2 = __val(v2);
  if (tMin > tMax) { var x = tMin; tMin = tMax; tMax = x; x = v1; v1 = v2; v2 = x; }
  var f = tMax === tMin ? (t < tMin ? 0 : 1) : (t - tMin) / (tMax - tMin);
  f = Math.min(Math.max(f, 0), 1);
  var p = shape(f);
  return $bm_sum(v1, $bm_mul($bm_sub(v2, v1), p));
}
function linear() { return __interp(function (f) { return f; }, arguments); }
function ease() { return __interp(function (f) { return f * f * (3 - 2 * f); }, arguments); }
function easeIn() { return __interp(function (f) { return f * f * (2 - f); }, arguments); }
function easeOut() { return __interp(function (f) { return f * (1 + f - f * f); }, arguments); }

// ---- color ----
function rgbToHsl(c) {
  c = __val(c);
  var r = c[0], g = c[1], b = c[2], a = c.length > 3 ? c[3] : 1;
  var max = Math.max(r, g, b), min = Math.min(r, g, b), h = 0, s = 0, l = (max + min) / 2;
  if (max !== min) {
    var d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    if (max === r) h = (g - b) / d + (g < b ? 6 : 0);
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h /= 6;
  }
  return [h, s, l, a];
}
function hslToRgb(c) {
  c = __val(c);
  var h = c[0], s = c[1], l = c[2], a = c.length > 3 ? c[3] : 1;
  if (s === 0) return [l, l, l, a];
  function hue(p, q, t) {
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  }
  var q = l < 0.5 ? l * (1 + s) : l + s - l * s, p = 2 * l - q;
  return [hue(p, q, h + 1 / 3), hue(p, q, h), hue(p, q, h - 1 / 3), a];
}
function hexToRgb(hex) {
  hex = String(hex).replace(/^#/, "");
  if (hex.length === 3) hex = hex.replace(/(.)/g, "$1$1");
  var n = parseInt(hex.slice(0, 6), 16);
  var a = hex.length >= 8 ? parseInt(hex.slice(6, 8), 16) / 255 : 1;
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255, a];
}

// ---- deterministic random (per property, AE-like API; numbers differ from AE) ----
function __Rng(seed) { this.s = seed >>> 0; }
__Rng.prototype.next = function () {
  // AE 26 uniform draws are successive unsigned LCG states divided by UINT32_MAX.
  this.s = (Math.imul(this.s, 1664525) + 1013904223) >>> 0;
  return this.s / 4294967295;
};
function __hash(a, b) {
  var h = (a | 0) ^ Math.imul((b | 0) + 0x9e3779b9, 0x85ebca6b);
  h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
  return (h ^ (h >>> 16)) >>> 0;
}
// The generator is seeded on the first random() of an evaluation (most
// expressions draw none): null until then.
var __rng = null, __rngBase = 1, __offset = 0, __timeless = false;
var __seedBits = new DataView(new ArrayBuffer(8));
function __numberSeed(x) {
  if ((x | 0) === x) return x;
  __seedBits.setFloat64(0, x);
  return __hash(__seedBits.getUint32(0), __seedBits.getUint32(4));
}
function __offsetSeed(seed) { return __offset === 0 ? seed >>> 0 : __hash(seed, __offset); }
function __resetRandom(seed) { __rngBase = seed >>> 0; __offset = 0; __timeless = false; __rng = null; }
function __R() { if (__rng === null) __reseed(); return __rng; }
function __reseed() {
  var tick = __timeless ? 0 : __numberSeed(__now);
  __rng = new __Rng(__hash(__offsetSeed(__rngBase), tick));
}
function seedRandom(seed, timeless) {
  __offset = __numberSeed(Number(__val(seed)));
  __timeless = !!timeless;
  __reseed();
}
function __drawRange(draw, a, b) {
  if (a === undefined) return draw();
  a = __val(a);
  if (b === undefined) { b = a; a = 0; } else b = __val(b);
  function component(v, i) { return __isArr(v) ? (i < v.length ? v[i] : 0) : v; }
  if (!__isArr(a) && !__isArr(b)) return a + draw() * (b - a);
  var result = new Array(Math.max(__isArr(a) ? a.length : 0, __isArr(b) ? b.length : 0));
  for (var i = 0; i < result.length; i++) {
    var lo = component(a, i);
    result[i] = lo + draw() * (component(b, i) - lo);
  }
  return result;
}
function __uniform() { return __R().next(); }
function __gaussian() {
  // The central 90% of a standard normal lies within 1.64485 standard deviations.
  return 0.5 + Math.sqrt(-2 * Math.log((__R().next() * 4294967295 + 1) / 4294967296)) * Math.cos(2 * Math.PI * __uniform()) / 3.2897072539;
}
function random(a, b) { return __drawRange(__uniform, a, b); }
function gaussRandom(a, b) { return __drawRange(__gaussian, a, b); }
// 1-D gradient noise in [-1, 1].
function __grad(seed, i) { return (__hash(seed, i) / 4294967296) * 2 - 1; }
function __noise1(seed, x) {
  var i = Math.floor(x), f = x - i, u = f * f * f * (f * (f * 6 - 15) + 10);
  var a = __grad(seed, i) * f, b = __grad(seed, i + 1) * (f - 1);
  return (a + (b - a) * u) * 2;
}
function noise(v) {
  v = __val(v);
  if (!__isArr(v)) return __noise1(12345, v);
  if (v.length === 1) return __noise1(12345, v[0]);
  if (v.length === 4) v = v.slice(0, 3);
  // Interpolate the gradient at each lattice corner along each input axis.
  var dims = v.length, cell = v.map(Math.floor);
  if (dims !== 2 && dims !== 3) throw new Error("noise expects a number or an array of one to four components");
  var f = v.map(function (x, i) { return x - cell[i]; });
  var u = f.map(function (x) { return x * x * x * (x * (x * 6 - 15) + 10); });
  var sum = 0;
  for (var corner = 0; corner < (1 << dims); corner++) {
    var h = 12345, weight = 1;
    for (var i = 0; i < dims; i++) {
      var high = (corner >> i) & 1;
      h = __hash(h, cell[i] + high);
      weight *= high ? u[i] : 1 - u[i];
    }
    var axis = (h >>> 1) % dims;
    sum += weight * (h & 1 ? -1 : 1) * (f[axis] - ((corner >> axis) & 1));
  }
  return 2 * sum;
}

// ---- time ----
function __compId() { return __env_c; }
var __env_c = 0;
function timeToFrames(t, fps, isDuration) {
  if (t === undefined || fps === undefined) {
    var c = __h.compInfo(__env_c);
    if (t === undefined) t = __now + c.displayStartTime;
    if (fps === undefined) fps = 1 / c.frameDuration;
  }
  var f = __val(t) * __val(fps);
  var near = Math.round(f);
  if (Math.abs(f - near) <= 4 * Number.EPSILON * Math.max(1, Math.abs(f))) f = near;
  return isDuration && f > 0 ? Math.ceil(f) : Math.floor(f);
}
function framesToTime(frames, fps) {
  if (fps === undefined) fps = 1 / __h.compInfo(__env_c).frameDuration;
  return __val(frames) / __val(fps);
}
function __pad(n, w) { var s = String(Math.floor(Math.abs(n))); while (s.length < w) s = "0" + s; return s; }
function __timecode(frames, base, drop, hourDigits) {
  var total = Math.abs(frames);
  // Skip two frame labels per minute at 29.97, four at 59.94, except each tenth minute.
  var skip = drop && (base === 30 || base === 60) ? base / 15 : 0;
  if (skip) {
    var minute = 60 * base - skip, ten = 600 * base - 9 * skip;
    total += 9 * skip * Math.floor(total / ten) + skip * Math.max(0, Math.floor((total % ten - skip) / minute));
  }
  var sep = skip ? ";" : ":", seconds = Math.floor(total / base);
  return (frames < 0 ? "-" : "") + __pad(Math.floor(seconds / 3600), hourDigits) + sep + __pad(Math.floor(seconds / 60) % 60, 2) +
    sep + __pad(seconds % 60, 2) + sep + __pad(total % base, Math.max(2, String(base - 1).length));
}
function timeToTimecode(t, timecodeBase, isDuration) {
  if (timecodeBase === undefined) timecodeBase = 30;
  return __timecode(timeToFrames(t, timecodeBase, isDuration), timecodeBase, false, 2);
}
function timeToCurrentFormat(t, fps, isDuration, ntscDropFrame) {
  if (fps === undefined || ntscDropFrame === undefined) {
    var c = __h.compInfo(__env_c);
    if (fps === undefined) fps = 1 / c.frameDuration;
    if (ntscDropFrame === undefined) ntscDropFrame = c.ntscDropFrame;
  }
  var frames = timeToFrames(t, fps, isDuration);
  if (__h.projectInfo().displayFrames) return String(frames);
  var ntsc = Math.abs(fps - 29.97) < 0.01 || Math.abs(fps - 59.94) < 0.01;
  return __timecode(frames, Math.round(fps), ntsc && ntscDropFrame, 1);
}
function timeToNTSCTimecode(t, ntscDropFrame, isDuration) {
  return __timecode(timeToFrames(t, 30000 / 1001, isDuration), 30, ntscDropFrame, 2);
}
function timeToFeetAndFrames(t, fps, framesPerFoot, isDuration) {
  if (framesPerFoot === undefined) framesPerFoot = 16;
  var total = timeToFrames(t, fps, isDuration);
  var magnitude = Math.abs(total);
  return (total < 0 ? "-" : "") + __pad(Math.floor(magnitude / framesPerFoot), 4) + "+" +
    __pad(magnitude % framesPerFoot, String(framesPerFoot - 1).length);
}

// ---- project and footage (AE: thisProject, colorDepth, footage(name)) ----
function __project() {
  var p = __h.projectInfo();
  return { fullPath: p.fullPath, bitsPerChannel: p.bitsPerChannel, linearBlending: p.linearBlending };
}
function Footage(f) {
  this.name = f.name; this.width = f.width; this.height = f.height; this.duration = f.duration;
  this.frameDuration = f.frameDuration; this.pixelAspect = f.pixelAspect; this.ntscDropFrame = f.ntscDropFrame;
}
Footage.prototype = {
  // Data-driven footage (JSON / CSV) is not imported yet.
  get sourceText() { throw new Error("Footage is not a text or data file"); },
  get sourceData() { throw new Error("Footage is not a data file"); },
  dataValue: function () { throw new Error("Footage is not a data file"); },
  dataKeyCount: function () { throw new Error("Footage is not a data file"); },
  dataKeyTimes: function () { throw new Error("Footage is not a data file"); },
  dataKeyValues: function () { throw new Error("Footage is not a data file"); },
};
function footage(name) {
  var f = __h.footage(String(name));
  if (!f) throw new Error("Footage " + JSON.stringify(name) + " does not exist");
  return new Footage(f);
}

// ---- evaluation entry (called from Rust) ----
function __call(fn, c, l, prim, attr, t, seed) {
  var saveNow = __now, saveC = __env_c, saveRng = __rng, saveBase = __rngBase, saveOffset = __offset, saveTl = __timeless;
  __now = t;
  __env_c = c;
  __resetRandom(seed);
  try {
    var r = fn(c, l, prim, attr, t);
    // A Source Text expression may end with a text style (AE 17.0+).
    if (r instanceof TextStyle) return { __style: true, changes: r.__c };
    return __val(r);
  } finally {
    __now = saveNow; __env_c = saveC; __rng = saveRng; __rngBase = saveBase; __offset = saveOffset; __timeless = saveTl;
  }
}

// ---- object model ----
function Property(c, l, p, a) { this.__c = c; this.__l = l; this.__p = p; this.__a = a; }
function __prop(c, l, p, a) { return new Property(c, l, p, a); }
// Time grid of AE's valueAtTime: 1/1024 of a frame (measured in AE 26 at 30 fps).
function __tick() { return __h.compInfo(__env_c).frameDuration / 1024; }
function __q(t) { var k = __tick(); return Math.round(t / k) * k; }
function __unwrap(r) { return typeof r === "string" || r.length !== 1 ? r : r[0]; }
Property.prototype = {
  get value() { return __unwrap(__h.prop(this.__c, this.__l, this.__p, this.__a, __now, false)); },
  valueAtTime: function (t) { return __unwrap(__h.prop(this.__c, this.__l, this.__p, this.__a, t, false)); },
  __pre: function (t) { return __unwrap(__h.prop(this.__c, this.__l, this.__p, this.__a, t, true)); },
  // AE: forward difference over one time-grid step from the quantized time.
  velocityAtTime: function (t) {
    return __unwrap(__h.vel(this.__c, this.__l, this.__p, this.__a, t, false));
  },
  valueOf: function () { return this.value; },
  get velocity() { return this.velocityAtTime(__now); },
  speedAtTime: function (t) { return length(this.velocityAtTime(t)); },
  get speed() { return this.speedAtTime(__now); },
  get numKeys() { return this.__keys().length; },
  __keys: function () {
    var raw = __h.keys(this.__c, this.__l, this.__p, this.__a), d = __h.dims(this.__c, this.__l, this.__p, this.__a);
    var out = [];
    for (var i = 0; i + d < raw.length; i += d + 1) {
      out.push({ time: raw[i], value: d === 1 ? raw[i + 1] : raw.slice(i + 1, i + 1 + d) });
    }
    return out;
  },
  key: function (i) {
    var ks = this.__keys();
    if (i < 1 || i > ks.length) throw new Error("Key index " + i + " out of range");
    var k = ks[i - 1];
    k.index = i;
    return k;
  },
  nearestKey: function (t) {
    var ks = this.__keys(), best = -1, bd = Infinity;
    for (var i = 0; i < ks.length; i++) {
      var d = Math.abs(ks[i].time - t);
      if (d < bd) { bd = d; best = i; }
    }
    if (best < 0) throw new Error("property has no keyframes");
    var k = ks[best];
    k.index = best + 1;
    return k;
  },
  wiggle: function (freq, amp, octaves, ampMult, t) {
    if (octaves === undefined) octaves = 1;
    if (ampMult === undefined) ampMult = 0.5;
    if (t === undefined) t = __now;
    var v = this.valueAtTime(t), seed = __h.seed(this.__c, this.__l, this.__p, this.__a);
    function w(dim) { return __h.wiggleNoise(__offsetSeed(seed), dim, t * freq, octaves, ampMult) * amp; }
    if (__isArr(v)) return v.map(function (x, i) { return x + w(i); });
    return v + w(0);
  },
  temporalWiggle: function (freq, amp, octaves, ampMult, t) {
    if (t === undefined) t = __now;
    var seed = __offsetSeed(__h.seed(this.__c, this.__l, this.__p, this.__a));
    var delta = __h.wiggleNoise(seed, 0, t * freq, octaves === undefined ? 1 : octaves,
                                ampMult === undefined ? 0.5 : ampMult);
    return this.valueAtTime(t + delta * amp);
  },
  smooth: function (width, samples, t) {
    if (width === undefined) width = 0.2;
    if (samples === undefined) samples = 5;
    if (t === undefined) t = __now;
    if (samples < 2) return this.valueAtTime(t);
    var acc = null;
    for (var i = 0; i < samples; i++) {
      var v = this.valueAtTime(t - width / 2 + width * i / (samples - 1));
      acc = acc === null ? v : $bm_sum(acc, v);
    }
    return $bm_div(acc, samples);
  },
  __loop: function (out, type, numKeyframes, duration) {
    if (type === undefined) type = "cycle";
    var ks = this.__keys(), t = __now;
    if (ks.length < 2) return this.__pre(t);
    var first, last;
    if (out) {
      last = ks[ks.length - 1].time;
      first = duration !== undefined && duration > 0 ? last - duration
        : ks[numKeyframes > 0 && numKeyframes < ks.length ? ks.length - 1 - numKeyframes : 0].time;
      if (t <= last) return this.__pre(t);
    } else {
      first = ks[0].time;
      last = duration !== undefined && duration > 0 ? first + duration
        : ks[numKeyframes > 0 && numKeyframes < ks.length ? numKeyframes : ks.length - 1].time;
      if (t >= first) return this.__pre(t);
    }
    var span = last - first;
    if (span <= 0) return this.__pre(t);
    var vFirst = this.__pre(first), vLast = this.__pre(last);
    if (type === "continue") {
      // AE: the velocity 1 ms inside the keyframe range.
      if (out) return $bm_sum(vLast, $bm_mul(this.__preVel(last - 0.001), t - last));
      return $bm_sub(vFirst, $bm_mul(this.__preVel(first + 0.001), first - t));
    }
    var rel = out ? t - first : last - t;
    var n = Math.floor(rel / span), r = rel - n * span;
    var local;
    if (type === "pingpong") {
      var forward = n % 2 === 0;
      local = out ? (forward ? first + r : last - r) : (forward ? last - r : first + r);
      return this.__pre(local);
    }
    local = out ? first + r : last - r;
    var v = this.__pre(local);
    if (type === "offset") {
      var delta = $bm_sub(vLast, vFirst);
      return out ? $bm_sum(v, $bm_mul(delta, n)) : $bm_sub(v, $bm_mul(delta, n));
    }
    return v; // cycle
  },
  __preVel: function (t) {
    return __unwrap(__h.vel(this.__c, this.__l, this.__p, this.__a, t, true));
  },
  loopOut: function (type, numKeyframes) { return this.__loop(true, type, numKeyframes || 0); },
  loopIn: function (type, numKeyframes) { return this.__loop(false, type, numKeyframes || 0); },
  loopOutDuration: function (type, duration) { return this.__loop(true, type, 0, duration || 0); },
  loopInDuration: function (type, duration) { return this.__loop(false, type, 0, duration || 0); },
  get propertyIndex() { return 1; },
};
// ---- property names and groups (AE: thisProperty.name, propertyGroup(n)) ----
Property.prototype.__chain = function () {
  if (!this.__ch) this.__ch = __h.propertyChain(this.__c, this.__l, this.__p, this.__a);
  return this.__ch;
};
Object.defineProperty(Property.prototype, "name", { get: function () { return this.__chain().name; } });
Object.defineProperty(Property.prototype, "active", { get: function () { return true; } });
Object.defineProperty(Property.prototype, "enabled", { get: function () { return true; } });
Property.prototype.propertyGroup = function (countUp) {
  var n = countUp === undefined ? 1 : countUp;
  var g = this.__chain().groups[n - 1];
  if (!g) throw new Error("Property group " + n + " is out of range");
  var c = this.__c, l = this.__l, layer = new Layer(c, l);
  switch (g.kind) {
    case "layer": return layer;
    case "transform": var t = layer.transform; t.name = g.name; return t;
    case "mask": var m = layer.mask(g.name); m.name = g.name; return m;
    case "effect": var e = layer.effect(g.name); if (!e.name) e.name = g.name; return e;
    case "cameraOption": return layer.cameraOption;
    case "lightOption": return layer.lightOption;
    case "materialOption": return layer.materialOption;
    case "shape":
      if (typeof __ShapeItem === "function") {
        return new __ShapeItem(c, l, { path: g.path, type: g.type, op: g.op, name: g.name });
      }
      return { name: g.name };
    default: return { name: g.name };
  }
};

// ---- shape layer contents (AE: content("Shape 1").content("Path 1").path) ----
function __content(c, l, parent, which) {
  var info = __h.content(c, l, parent, which);
  if (info === null || info === undefined) {
    throw new Error("Property " + JSON.stringify(which) + " is missing or does not exist");
  }
  return new __ShapeItem(c, l, info);
}
function __ShapeItem(c, l, info) {
  this.__c = c; this.__l = l; this.__p = info.path; this.__t = info.type; this.__o = info.op;
  this.name = info.name;
}
__ShapeItem.prototype = {
  content: function (which) { return __content(this.__c, this.__l, this.__p, which); },
  get transform() {
    if (this.__t !== "ShapeGroup") return undefined;
    var c = this.__c, l = this.__l, p = this.__p;
    var t = {};
    [["anchorPoint", "anchor_point"], ["position", "position"], ["scale", "scale"], ["skew", "skew"],
     ["skewAxis", "skew_axis"], ["rotation", "rotation"], ["opacity", "opacity"]].forEach(function (n) {
      Object.defineProperty(t, n[0], { get: function () { return __prop(c, l, p, "transform:" + n[1]); } });
    });
    return t;
  },
};
// AE's property names by item type (and by operator).
var __SHAPE_PROPS = {
  Rect: { size: "size", position: "position", roundness: "roundness" },
  Ellipse: { size: "size", position: "position" },
  Star: { points: "points", position: "position", rotation: "rotation", innerRadius: "inner_radius",
          outerRadius: "outer_radius", innerRoundness: "inner_roundness", outerRoundness: "outer_roundness" },
  Path: { path: "path" },
  Fill: { color: "color", opacity: "opacity" },
  Stroke: { color: "color", opacity: "opacity", strokeWidth: "width", miterLimit: "miter_limit" },
  GradientFill: { startPoint: "start_point", endPoint: "end_point", highlightLength: "highlight_length",
                  highlightAngle: "highlight_angle", opacity: "opacity" },
  GradientStroke: { startPoint: "start_point", endPoint: "end_point", highlightLength: "highlight_length",
                    highlightAngle: "highlight_angle", opacity: "opacity", strokeWidth: "width",
                    miterLimit: "miter_limit" },
};
var __OPERATOR_PROPS = {
  trim: { start: "trim:start", end: "trim:end", offset: "trim:offset" },
  repeater: { copies: "repeater:copies", offset: "repeater:offset" },
};
(function () {
  var names = {};
  [__SHAPE_PROPS, __OPERATOR_PROPS].forEach(function (group) {
    for (var t in group) for (var n in group[t]) names[n] = true;
  });
  Object.keys(names).forEach(function (n) {
    Object.defineProperty(__ShapeItem.prototype, n, {
      get: function () {
        var map = this.__t === "ShapeOperator" ? __OPERATOR_PROPS[this.__o] : __SHAPE_PROPS[this.__t];
        var attr = map && map[n];
        return attr ? __prop(this.__c, this.__l, this.__p, attr) : undefined;
      },
    });
  });
})();

// ---- paths (AE: mask paths, shape paths) ----
Property.prototype.__path = function (t) {
  var p = __h.prop(this.__c, this.__l, this.__p, this.__a, t === undefined ? __now : t, false);
  if (!p || !p.__path) throw new Error("Property is not a path");
  return p;
};
Property.prototype.points = function (t) { return this.__path(t).points; };
Property.prototype.inTangents = function (t) { return this.__path(t).inTangents; };
Property.prototype.outTangents = function (t) { return this.__path(t).outTangents; };
Property.prototype.isClosed = function () { return this.__path(__now).closed; };
[["pointOnPath", 0], ["tangentOnPath", 1], ["normalOnPath", 2]].forEach(function (m) {
  Property.prototype[m[0]] = function (percentage, t) {
    return __h.pathAt(this.__c, this.__l, this.__p, this.__a,
      [percentage === undefined ? 0.5 : percentage, t === undefined ? __now : t, m[1]]);
  };
});
// createPath(points = [[0,0], [100,0], [100,100], [0,100]], inTangents = [],
// outTangents = [], is_closed = true): a path a path expression can return.
Property.prototype.createPath = function (points, inTangents, outTangents, isClosed) {
  if (points === undefined) points = [[0, 0], [100, 0], [100, 100], [0, 100]];
  points = __val(points);
  if (!__isArr(points) || points.length < 1) throw new Error("createPath needs at least one point");
  function tangents(v) {
    v = v === undefined ? [] : __val(v);
    if (v.length !== 0 && v.length !== points.length) {
      throw new Error("Tangent array length must equal the number of points");
    }
    return v.length ? v : points.map(function () { return [0, 0]; });
  }
  return { __path: true, points: points, inTangents: tangents(inTangents),
           outTangents: tangents(outTangents), closed: isClosed === undefined ? true : !!isClosed };
};

// ---- text (AE: thisLayer.text.sourceText and the 17.0+ style API) ----
Property.prototype.getStyleAt = function (charIndex, t) {
  return new TextStyle(__h.textStyle(this.__c, this.__l, this.__p, this.__a, t === undefined ? __now : t), {});
};
Property.prototype.createStyle = function () { return this.getStyleAt(0, __now); };
Object.defineProperty(Property.prototype, "style", { get: function () { return this.getStyleAt(0, __now); } });

function __TextGroup(c, l) { this.__c = c; this.__l = l; }
__TextGroup.prototype = {
  // The text as a string (string methods, concatenation, length) that also has
  // the property's methods, as AE's sourceText behaves.
  get sourceText() {
    var p = __prop(this.__c, this.__l, "", "text:source");
    var s = new String(p.value);
    ["valueAtTime", "velocityAtTime", "key", "nearestKey", "getStyleAt", "createStyle"].forEach(function (k) {
      s[k] = function () { return p[k].apply(p, arguments); };
    });
    ["value", "style", "numKeys"].forEach(function (k) {
      Object.defineProperty(s, k, { get: function () { return p[k]; } });
    });
    return s;
  },
};

// A text style: what the text uses (`__f`, from the host) plus the `set…`
// calls made on it (`__c`), each returning a new style.
function TextStyle(fields, changes) { this.__f = fields; this.__c = changes; }
TextStyle.prototype = {
  get horizontalScale() { return "horizontalScale" in this.__c ? this.__c.horizontalScale : 100; },
  get verticalScale() { return "verticalScale" in this.__c ? this.__c.verticalScale : 100; },
};
[
  ["font", "font"], ["fontSize", "fontSize"], ["fillColor", "fillColor"], ["strokeColor", "strokeColor"],
  ["strokeWidth", "strokeWidth"], ["tracking", "tracking"], ["leading", "leading"], ["autoLeading", "autoLeading"],
  ["baselineShift", "baselineShift"], ["isFauxBold", "fauxBold"], ["isFauxItalic", "fauxItalic"],
  ["isAllCaps", "allCaps"], ["isSmallCaps", "smallCaps"], ["applyFill", "applyFill"], ["applyStroke", "applyStroke"],
].forEach(function (p) {
  Object.defineProperty(TextStyle.prototype, p[0], {
    get: function () { return p[1] in this.__c ? this.__c[p[1]] : this.__f[p[1]]; },
  });
});
[
  ["setText", "text"], ["setFont", "font"], ["setFontSize", "fontSize"], ["setFillColor", "fillColor"],
  ["setStrokeColor", "strokeColor"], ["setStrokeWidth", "strokeWidth"], ["setTracking", "tracking"],
  ["setLeading", "leading"], ["setAutoLeading", "autoLeading"], ["setBaselineShift", "baselineShift"],
  ["setFauxBold", "fauxBold"], ["setFauxItalic", "fauxItalic"], ["setAllCaps", "allCaps"],
  ["setSmallCaps", "smallCaps"], ["setApplyFill", "applyFill"], ["setApplyStroke", "applyStroke"],
  ["setHorizontalScale", "horizontalScale"], ["setVerticalScale", "verticalScale"],
].forEach(function (p) {
  TextStyle.prototype[p[0]] = function (v) {
    var c = {};
    for (var k in this.__c) c[k] = this.__c[k];
    c[p[1]] = __val(v);
    return new TextStyle(this.__f, c);
  };
});

// `transform.position[0]` and `.length` read the value, as in AE: getters on
// the prototype (a Proxy would trap every method and field access).
(function () {
  function index(i) {
    return function () { var v = this.value; return __isArr(v) || typeof v === "string" ? v[i] : undefined; };
  }
  for (var i = 0; i < 16; i++) Object.defineProperty(Property.prototype, String(i), { get: index(i) });
  Object.defineProperty(Property.prototype, "length", {
    get: function () { var v = this.value; return __isArr(v) || typeof v === "string" ? v.length : 1; },
  });
})();

var __TRANSFORM = {
  anchorPoint: "transform:anchor_point", position: "transform:position", scale: "transform:scale",
  rotation: "transform:rotation", zRotation: "transform:rotation", xRotation: "transform:rotation_x",
  yRotation: "transform:rotation_y", orientation: "transform:orientation", opacity: "transform:opacity",
  pointOfInterest: "transform:point_of_interest", xPosition: "transform:position_x",
  yPosition: "transform:position_y", zPosition: "transform:position_z",
};
// The layer's own properties use the empty prim path (the host resolves it).
function __Transform(layer) { this.__c = layer.__c; this.__l = layer.__l; this.__path = ""; }
Object.keys(__TRANSFORM).forEach(function (k) {
  var attr = __TRANSFORM[k];
  Object.defineProperty(__Transform.prototype, k, {
    get: function () { return __prop(this.__c, this.__l, this.__path, attr); },
  });
});

function Layer(c, l) {
  this.__c = c;
  this.__l = l;
  this.__i = null;
}
Layer.prototype = {
  // Layer facts are fetched on first use (most expressions need none).
  get __info() { return this.__i || (this.__i = __h.layerInfo(this.__c, this.__l)); },
  get __path() { return this.__info.path; },
  get name() { return this.__info.name; },
  get index() { return this.__l + 1; },
  get inPoint() { return this.__info.inPoint; },
  get outPoint() { return this.__info.outPoint; },
  get startTime() { return this.__info.startTime; },
  get stretch() { return this.__info.stretch; },
  get hasParent() { return this.__info.parent >= 0; },
  get parent() {
    if (this.__info.parent < 0) throw new Error("Layer has no parent");
    return new Layer(this.__c, this.__info.parent);
  },
  get enabled() { return this.__info.enabled; },
  get active() { return this.__info.enabled && __now >= Math.min(this.inPoint, this.outPoint) && __now < Math.max(this.inPoint, this.outPoint); },
  get hasVideo() { return this.__info.hasVideo; },
  get hasAudio() { return this.__info.hasAudio; },
  get audioActive() { return this.hasAudio && this.active; },
  get width() { return this.__info.width; },
  get height() { return this.__info.height; },
  // A precomp's comp, else the footage item (solids, footage).
  get source() {
    if (this.__info.sourceComp >= 0) return new Comp(this.__info.sourceComp);
    var f = __h.layerFootage(this.__c, this.__l);
    if (!f) throw new Error("Layer has no source");
    return new Footage(f);
  },
  get transform() { return new __Transform(this); },
  get anchorPoint() { return this.transform.anchorPoint; },
  get position() { return this.transform.position; },
  get scale() { return this.transform.scale; },
  get rotation() { return this.transform.rotation; },
  get zRotation() { return this.transform.rotation; },
  get xRotation() { return this.transform.xRotation; },
  get yRotation() { return this.transform.yRotation; },
  get orientation() { return this.transform.orientation; },
  get opacity() { return this.transform.opacity; },
  get pointOfInterest() { return this.transform.pointOfInterest; },
  get timeRemap() { return __prop(this.__c, this.__l, "", "time_remap"); },
  get marker() { return new MarkerProperty(this.__c, this.__l); },
  get text() { return new __TextGroup(this.__c, this.__l); },
  sourceRectAtTime: function (t, includeExtents) {
    var r = __h.sourceRect(this.__c, this.__l, t === undefined ? __now : t, !!includeExtents);
    return { top: r[1], left: r[0], width: r[2], height: r[3] };
  },
  content: function (which) { return __content(this.__c, this.__l, "", which); },
  get rotationX() { return this.transform.xRotation; },
  get rotationY() { return this.transform.yRotation; },
  get rotationZ() { return this.transform.rotation; },
  get cameraOption() {
    var g = __optionGroup(this, "Camera Options", __CAMERA_OPTIONS);
    for (var k in __CAMERA_DEFAULTS) g[k] = __CAMERA_DEFAULTS[k];
    return g;
  },
  get lightOption() { return __optionGroup(this, "Light Options", __LIGHT_OPTIONS); },
  get materialOption() { return __optionGroup(this, "Material Options", __MATERIAL_OPTIONS); },
  get audioLevels() {
    if (!this.hasAudio) {
      throw new Error("property or method named 'audioLevels' in Class 'Layer' is missing or does not exist. " +
        "It may have been renamed, moved, deleted, or the name may have been mistyped.");
    }
    return __prop(this.__c, this.__l, "", "audio:levels");
  },
  sourceTime: function (t) {
    if (t === undefined) t = __now;
    return (t - this.startTime) * 100 / this.stretch;
  },
  effect: function (nameOrIndex) {
    var effect = __h.effect(this.__c, this.__l, nameOrIndex);
    if (effect === null) throw new Error("Effect " + JSON.stringify(nameOrIndex) + " is missing or does not exist");
    var c = this.__c, l = this.__l, p = effect.path;
    var fx = function (param) {
      var a = __h.param(c, l, p, param);
      if (a === null) throw new Error("Effect parameter " + JSON.stringify(param) + " does not exist");
      if (effect.layerControl) {
        var index = __h.effectLayer(c, l, p, a, __now);
        return index < 0 ? null : new Layer(c, index);
      }
      return __prop(c, l, p, a);
    };
    fx.param = fx;
    fx.active = effect.active;
    return fx;
  },
  mask: function (nameOrIndex) {
    var p = __h.mask(this.__c, this.__l, nameOrIndex);
    if (p === null) throw new Error("Mask " + JSON.stringify(nameOrIndex) + " does not exist");
    var c = this.__c, l = this.__l;
    return {
      get maskPath() { return __prop(c, l, p, "path"); },
      get maskFeather() { return __prop(c, l, p, "feather"); },
      get maskOpacity() { return __prop(c, l, p, "opacity"); },
      get maskExpansion() { return __prop(c, l, p, "expansion"); },
    };
  },
  __xf: function (kind, point, t) {
    if (t === undefined) t = __now;
    var p = point.slice(0);
    while (p.length < 3) p.push(0);
    var r = __h.xform(this.__c, this.__l, t, kind, p);
    return point.length === 2 ? [r[0], r[1]] : r;
  },
  toComp: function (p, t) { return this.__xf(0, p, t); },
  fromComp: function (p, t) { return this.__xf(1, p, t); },
  toWorld: function (p, t) { return this.__xf(4, p, t); },
  fromWorld: function (p, t) { return this.__xf(5, p, t); },
  toCompVec: function (p, t) { return this.__xf(2, p, t); },
  fromCompVec: function (p, t) { return this.__xf(3, p, t); },
  toWorldVec: function (p, t) { return this.__xf(6, p, t); },
  fromCompToSurface: function (p, t) { return this.__xf(8, p, t); },
  fromWorldVec: function (p, t) { return this.__xf(7, p, t); },
};

// ---- camera, light and material options (AE: layer.zoom, cameraOption.zoom …) ----
var __CAMERA_OPTIONS = {
  zoom: "camera:zoom", depthOfField: "camera:depth_of_field", focusDistance: "camera:focus_distance",
  aperture: "camera:aperture", blurLevel: "camera:blur_level",
};
// Iris and highlight options are not modelled yet: AE's defaults.
var __CAMERA_DEFAULTS = {
  irisShape: 1, irisRotation: 0, irisRoundness: 0, irisAspectRatio: 1, irisDiffractionFringe: 0,
  highlightGain: 0, highlightThreshold: 255, highlightSaturation: 0,
};
var __LIGHT_OPTIONS = {
  intensity: "light:intensity", color: "light:color", coneAngle: "light:cone_angle",
  coneFeather: "light:cone_feather", radius: "light:radius", falloffDistance: "light:falloff_distance",
  shadowDarkness: "light:shadow_darkness", shadowDiffusion: "light:shadow_diffusion",
  castsShadows: "light:casts_shadows",
};
var __MATERIAL_OPTIONS = {
  castsShadows: "material:casts_shadows", lightTransmission: "material:light_transmission",
  acceptsShadows: "material:accepts_shadows", acceptsLights: "material:accepts_lights",
  ambient: "material:ambient", diffuse: "material:diffuse", specular: "material:specular_intensity",
  specularIntensity: "material:specular_intensity", shininess: "material:specular_shininess",
  specularShininess: "material:specular_shininess", metal: "material:metal",
};
function __optionGroup(layer, name, options) {
  var g = { name: name };
  for (var k in options) {
    (function (attr) {
      Object.defineProperty(g, k, { get: function () { return __prop(layer.__c, layer.__l, "", attr); } });
    })(options[k]);
  }
  return g;
}
// The same on the layer itself (AE: thisComp.layer("Camera 1").zoom).
(function () {
  function define(name, get) {
    if (!(name in Layer.prototype)) Object.defineProperty(Layer.prototype, name, { get: get });
  }
  function attrGetter(attr) { return function () { return __prop(this.__c, this.__l, "", attr); }; }
  [__CAMERA_OPTIONS, __LIGHT_OPTIONS, __MATERIAL_OPTIONS].forEach(function (options) {
    for (var k in options) if (k !== "castsShadows") define(k, attrGetter(options[k]));
  });
  Object.keys(__CAMERA_DEFAULTS).forEach(function (k) {
    define(k, function () { return __CAMERA_DEFAULTS[k]; });
  });
  // A light's own Casts Shadows; any other layer's Material Option.
  define("castsShadows", function () {
    return __prop(this.__c, this.__l, "", this.__info.kind === "light" ? "light:casts_shadows" : "material:casts_shadows");
  });
})();

// ---- markers (AE: thisLayer.marker, thisComp.marker, MarkerKey) ----
function MarkerProperty(c, l) { this.__c = c; this.__l = l; this.__m = null; }
MarkerProperty.prototype = {
  get __keys() {
    if (this.__m === null) {
      this.__m = __h.markers(this.__c, this.__l).map(function (m, i) { return new MarkerKey(m, i + 1); });
    }
    return this.__m;
  },
  get numKeys() { return this.__keys.length; },
  // key(index) or key(comment), as AE.
  key: function (k) {
    var ks = this.__keys;
    if (typeof k === "string") {
      for (var i = 0; i < ks.length; i++) if (ks[i].comment === k) return ks[i];
      throw new Error("Marker key " + JSON.stringify(k) + " does not exist");
    }
    if (k < 1 || k > ks.length) throw new Error("Key index " + k + " out of range");
    return ks[k - 1];
  },
  nearestKey: function (t) {
    var ks = this.__keys, best = -1, bd = Infinity;
    for (var i = 0; i < ks.length; i++) {
      var d = Math.abs(ks[i].time - t);
      if (d < bd) { bd = d; best = i; }
    }
    if (best < 0) throw new Error("property has no keyframes");
    return ks[best];
  },
};
function MarkerKey(m, index) {
  this.time = m.time;
  this.index = index;
  this.duration = m.duration;
  this.comment = m.comment;
  this.value = m.comment;
  this.chapter = m.chapter;
  this.url = m.url;
  this.frameTarget = m.frameTarget;
  this.eventCuePoint = false;
  this.cuePointName = m.cuePointName;
  this.parameters = {};
  this.protectedRegion = m.protectedRegion;
}

function Comp(c) { this.__c = c; this.__i = null; }
Comp.prototype = {
  get marker() { return new MarkerProperty(this.__c, -1); },
  get __info() { return this.__i || (this.__i = __h.compInfo(this.__c)); },
  get name() { return this.__info.name; },
  get width() { return this.__info.width; },
  get height() { return this.__info.height; },
  get duration() { return this.__info.duration; },
  get frameDuration() { return this.__info.frameDuration; },
  get pixelAspect() { return this.__info.pixelAspect; },
  get numLayers() { return this.__info.numLayers; },
  get ntscDropFrame() { return this.__info.ntscDropFrame; },
  get displayStartTime() { return this.__info.displayStartTime; },
  get bgColor() { return this.__info.bgColor; },
  get shutterAngle() { return this.__info.shutterAngle; },
  get shutterPhase() { return this.__info.shutterPhase; },
  get activeCamera() {
    if (this.__info.activeCamera < 0) throw new Error("Composition has no active camera");
    return new Layer(this.__c, this.__info.activeCamera);
  },
  layer: function (nameOrIndex, relIndex) {
    if (nameOrIndex instanceof Layer) {
      return this.layer(nameOrIndex.index + (relIndex || 0));
    }
    var l = __h.layerIndex(this.__c, nameOrIndex);
    if (l < 0) throw new Error("Layer " + JSON.stringify(nameOrIndex) + " does not exist");
    return new Layer(this.__c, l);
  },
};

function comp(name) {
  var c = __h.compByName(String(name));
  if (c < 0) throw new Error("Composition " + JSON.stringify(name) + " does not exist");
  return new Comp(c);
}
