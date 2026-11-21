// Copied from bridges/ae/tests/node-run.js (workstream 15).
// node-run.js - run harness.js under node by providing the jsc shell functions it expects.
"use strict";
const fs = require("fs");
const path = require("path");
const vm = require("vm");

process.chdir(__dirname);
// es3-guard.js deletes post-ES3 builtins such as Function.prototype.bind, which node's
// own stream code still calls (node 22 creates process.stdout lazily, on first use).
// Create the stdio streams now and print with plain synchronous writes.
void process.stdout;
void process.stderr;
global.print = (s) => fs.writeSync(1, String(s) + "\n");
global.readFile = (p) => fs.readFileSync(p, "utf8");
global.writeFile = (p, s) => fs.writeFileSync(p, s);
global.load = (p) => vm.runInThisContext(fs.readFileSync(p, "utf8"), { filename: path.resolve(p) });
global.quit = (code) => process.exit(code);
// Parse without running, like jsc's checkSyntax.
global.checkSyntax = (p) => new vm.Script(fs.readFileSync(p, "utf8"), { filename: path.resolve(p) });
const [file, ...rest] = process.argv.slice(2);
global.HARNESS_ARGS = rest;
global.load(file || "harness.js");
