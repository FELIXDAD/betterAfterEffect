// node-run.js - run harness.js under node by providing the jsc shell functions it expects.
"use strict";
const fs = require("fs");
const path = require("path");
const vm = require("vm");

process.chdir(__dirname);
global.print = (s) => console.log(String(s));
global.readFile = (p) => fs.readFileSync(p, "utf8");
global.writeFile = (p, s) => fs.writeFileSync(p, s);
global.load = (p) => vm.runInThisContext(fs.readFileSync(p, "utf8"), { filename: path.resolve(p) });
global.quit = (code) => process.exit(code);
// Parse without running, like jsc's checkSyntax.
global.checkSyntax = (p) => new vm.Script(fs.readFileSync(p, "utf8"), { filename: path.resolve(p) });
const [file, ...rest] = process.argv.slice(2);
global.HARNESS_ARGS = rest;
global.load(file || "harness.js");
