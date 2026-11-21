// Copied from bridges/ae/tests/es3-guard.js (workstream 15) so the two bridges can evolve separately.
// es3-guard.js - remove post-ES3 library functions so that code relying on them fails
// in tests the same way it would fail in ExtendScript (which is ES3 plus a few extras).
// Load AFTER anything that needs these functions; the harness keeps its own JSON reference.
(function (g) {
    function strip(obj, names) {
        for (var i = 0; i < names.length; i++) {
            try {
                delete obj[names[i]];
            } catch (e) {
                obj[names[i]] = undefined;
            }
        }
    }
    strip(Array.prototype, ["indexOf", "lastIndexOf", "forEach", "map", "filter", "reduce", "reduceRight", "some",
        "every", "includes", "find", "findIndex", "fill", "flat", "flatMap", "entries", "keys", "values", "at"]);
    strip(Array, ["isArray", "from", "of"]);
    strip(Object, ["keys", "create", "assign", "entries", "values", "getPrototypeOf", "freeze", "getOwnPropertyNames"]);
    strip(String.prototype, ["trim", "trimStart", "trimEnd", "includes", "startsWith", "endsWith", "repeat",
        "padStart", "padEnd", "at", "replaceAll"]);
    strip(Date.prototype, ["toISOString", "toJSON"]);
    strip(Date, ["now"]);
    strip(Function.prototype, ["bind"]);
    strip(Number, ["isFinite", "isNaN", "isInteger", "parseFloat", "parseInt"]);
    strip(Math, ["sign", "trunc", "log10", "log2", "hypot", "cbrt"]);
    strip(g, ["JSON", "Map", "Set", "WeakMap", "Promise", "Symbol", "Reflect", "Proxy"]);
}(this));
