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
//   __h.effect(c, l, nameOrIndex) -> string|null     effect prim path
//   __h.param(c, l, effectPath, nameOrIndex) -> string|null   param attr name
//   __h.mask(c, l, nameOrIndex) -> string|null       mask prim path
//   __h.xform(c, l, t, kind, point) -> number[]      toComp/fromComp/toWorld/... (kind 0..7)
//   __h.seed(c, l, prim, attr) -> number             stable per-property random seed
// `__now` is the expression's current comp time (seconds); posterizeTime changes it.

"use strict";
var __now = 0;

// ---- vector arithmetic (operator rewriting targets, AE semantics) ----
// Property objects stand for their value in arithmetic and library calls (AE).
function __val(a) {
  if (a === undefined) throw new Error("Undefined value used in expression (could be an out of range array subscript?)");
  return a instanceof Property ? a.value : a;
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
function __Rng(seed) { this.s = (seed >>> 0) || 0x9e3779b9; }
__Rng.prototype.next = function () {
  // mulberry32
  var t = (this.s += 0x6d2b79f5);
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
function __hash(a, b) {
  var h = (a | 0) ^ Math.imul((b | 0) + 0x9e3779b9, 0x85ebca6b);
  h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
  return (h ^ (h >>> 16)) >>> 0;
}
var __rng = new __Rng(1), __rngBase = 1, __timeless = false;
function __resetRandom(seed) { __rngBase = seed >>> 0; __timeless = false; __reseed(); }
function __reseed() {
  var tick = __timeless ? 0 : Math.floor(__now * 1000 + 1e-6);
  __rng = new __Rng(__hash(__rngBase, tick));
}
function seedRandom(seed, timeless) {
  __rngBase = __hash(__rngBase, Math.floor(seed));
  __timeless = !!timeless;
  __reseed();
}
function random(a, b) {
  var r = __rng.next();
  if (a === undefined) return r;
  if (b === undefined) {
    if (__isArr(a)) return a.map(function (x) { return __rng.next() * x; });
    return r * a;
  }
  if (__isArr(a)) return a.map(function (x, i) { return x + __rng.next() * (b[i] - x); });
  return a + r * (b - a);
}
function gaussRandom(a, b) {
  function g() {
    var u = 1 - __rng.next(), v = __rng.next();
    return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v) / 6 + 0.5;
  }
  if (a === undefined) return g();
  if (b === undefined) return __isArr(a) ? a.map(function (x) { return g() * x; }) : g() * a;
  if (__isArr(a)) return a.map(function (x, i) { return x + g() * (b[i] - x); });
  return a + g() * (b - a);
}
// 1-D gradient noise in [-1, 1].
function __grad(seed, i) { return (__hash(seed, i) / 4294967296) * 2 - 1; }
function __noise1(seed, x) {
  var i = Math.floor(x), f = x - i, u = f * f * f * (f * (f * 6 - 15) + 10);
  var a = __grad(seed, i) * f, b = __grad(seed, i + 1) * (f - 1);
  return (a + (b - a) * u) * 2;
}
function noise(v) {
  var x = __isArr(v) ? v[0] + 31.7 * (v[1] || 0) + 57.3 * (v[2] || 0) : v;
  return __noise1(12345, x);
}

// ---- time ----
function __compId() { return __env_c; }
var __env_c = 0;
function timeToFrames(t, fps, isDuration) {
  var c = __h.compInfo(__env_c);
  if (t === undefined) t = __now + c.displayStartTime;
  if (fps === undefined) fps = 1 / c.frameDuration;
  var f = t * fps;
  return isDuration ? Math.round(f) : Math.floor(f + 1e-9);
}
function framesToTime(frames, fps) {
  if (fps === undefined) fps = 1 / __h.compInfo(__env_c).frameDuration;
  return frames / fps;
}
function __pad(n, w) { var s = String(Math.floor(Math.abs(n))); while (s.length < w) s = "0" + s; return s; }
function timeToTimecode(t, timecodeBase, isDuration) {
  if (t === undefined) t = __now + __h.compInfo(__env_c).displayStartTime;
  if (timecodeBase === undefined) timecodeBase = 30;
  var neg = t < 0, total = Math.floor(Math.abs(t) * timecodeBase + 1e-9);
  var fr = total % timecodeBase, s = Math.floor(total / timecodeBase);
  return (neg ? "-" : "") + __pad(Math.floor(s / 3600), 1) + ":" + __pad(Math.floor(s / 60) % 60, 2) + ":" +
    __pad(s % 60, 2) + ":" + __pad(fr, String(timecodeBase - 1).length);
}
function timeToCurrentFormat(t, fps, isDuration) { return timeToTimecode(t, fps, isDuration); }

// ---- evaluation entry (called from Rust) ----
function __call(fn, c, l, prim, attr, t, seed) {
  var saveNow = __now, saveC = __env_c, saveRng = __rng, saveBase = __rngBase, saveTl = __timeless;
  __now = t;
  __env_c = c;
  __resetRandom(seed);
  try {
    var env = { thisComp: new Comp(c), thisLayer: new Layer(c, l), thisProperty: __prop(c, l, prim, attr), time: t };
    return __val(fn(env));
  } finally {
    __now = saveNow; __env_c = saveC; __rng = saveRng; __rngBase = saveBase; __timeless = saveTl;
  }
}

// ---- object model ----
function Property(c, l, p, a) { this.__c = c; this.__l = l; this.__p = p; this.__a = a; }
// `transform.position[0]` and `.length` read the value, as in AE.
var __propHandler = {
  get: function (target, key, receiver) {
    if (typeof key === "string" && (key === "length" || /^[0-9]+$/.test(key))) {
      var v = target.value;
      return __isArr(v) ? v[key] : (key === "length" ? 1 : undefined);
    }
    return Reflect.get(target, key, receiver);
  },
};
function __prop(c, l, p, a) { return new Proxy(new Property(c, l, p, a), __propHandler); }
// Time grid of AE's valueAtTime: 1/1024 of a frame (measured in AE 26 at 30 fps).
function __tick() { return __h.compInfo(__env_c).frameDuration / 1024; }
function __q(t) { var k = __tick(); return Math.round(t / k) * k; }
function __unwrap(r) { return r.length === 1 ? r[0] : r; }
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
    function w(dim) {
      var s = 0, a = 1, f = freq, norm = 0;
      for (var o = 0; o < Math.max(1, Math.floor(octaves)); o++) {
        s += __noise1(__hash(seed, dim * 131 + o), t * f) * a;
        norm += a;
        a *= ampMult;
        f *= 2;
      }
      return s * amp;
    }
    if (__isArr(v)) return v.map(function (x, i) { return x + w(i); });
    return v + w(0);
  },
  temporalWiggle: function (freq, amp, octaves, ampMult, t) {
    if (t === undefined) t = __now;
    var seed = __h.seed(this.__c, this.__l, this.__p, this.__a);
    return this.valueAtTime(t + __noise1(seed, t * freq) * amp);
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
  get hasVideo() { return true; },
  get hasAudio() { return false; },
  get width() { return this.__info.width; },
  get height() { return this.__info.height; },
  get source() {
    if (this.__info.sourceComp < 0) throw new Error("Layer source is not a composition");
    return new Comp(this.__info.sourceComp);
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
  sourceTime: function (t) {
    if (t === undefined) t = __now;
    return (t - this.startTime) * 100 / this.stretch;
  },
  effect: function (nameOrIndex) {
    var p = __h.effect(this.__c, this.__l, nameOrIndex);
    if (p === null) throw new Error("Effect " + JSON.stringify(nameOrIndex) + " is missing or does not exist");
    var c = this.__c, l = this.__l;
    var fx = function (param) {
      var a = __h.param(c, l, p, param);
      if (a === null) throw new Error("Effect parameter " + JSON.stringify(param) + " does not exist");
      return __prop(c, l, p, a);
    };
    fx.param = fx;
    fx.active = true;
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
  toWorld: function (p, t) { return this.__xf(0, p, t); },
  fromWorld: function (p, t) { return this.__xf(1, p, t); },
  toCompVec: function (p, t) { return this.__xf(2, p, t); },
  fromCompVec: function (p, t) { return this.__xf(3, p, t); },
  toWorldVec: function (p, t) { return this.__xf(2, p, t); },
  fromWorldVec: function (p, t) { return this.__xf(3, p, t); },
};

function Comp(c) { this.__c = c; this.__i = null; }
Comp.prototype = {
  get __info() { return this.__i || (this.__i = __h.compInfo(this.__c)); },
  get name() { return this.__info.name; },
  get width() { return this.__info.width; },
  get height() { return this.__info.height; },
  get duration() { return this.__info.duration; },
  get frameDuration() { return this.__info.frameDuration; },
  get pixelAspect() { return this.__info.pixelAspect; },
  get numLayers() { return this.__info.numLayers; },
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
