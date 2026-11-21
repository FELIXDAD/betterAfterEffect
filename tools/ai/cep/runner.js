// aictl runner: an invisible CEP extension that executes queued ExtendScript files in
// Illustrator. It exists because driving Illustrator with AppleEvents (`do javascript`)
// needs a macOS Automation permission that only a human can grant; CEP's evalScript needs
// nothing beyond PlayerDebugMode (set by `aictl install`).
//
// Protocol (all files under STATE, written by tools/ai/aictl):
//   queue/<id>.req    JSON {"script": "/abs/path.jsx"}; renamed to <id>.taken when picked up
//   queue/<id>.ret    what evalScript returned (the wrapper writes the real result itself)
//   runner.json       heartbeat {pid, started, beat, busy, done}, rewritten every second
//   ready.json        written once Illustrator answered the first evalScript
//   runner.log        one line per event
// Jobs run one at a time, in name order. evalScript runs on Illustrator's main thread; while
// a job runs this page keeps beating, so aictl can tell "Illustrator busy" from "runner gone".
(function () {
    'use strict';
    var fs = require('fs');
    var path = require('path');
    var STATE = '__STATE__';
    var QUEUE = path.join(STATE, 'queue');
    var started = Date.now();
    var busy = null;
    var done = 0;

    function writeAtomic(file, text) {
        var tmp = file + '.tmp-' + process.pid;
        fs.writeFileSync(tmp, text);
        fs.renameSync(tmp, file);
    }

    function log(msg) {
        try {
            fs.appendFileSync(path.join(STATE, 'runner.log'), new Date().toISOString() + ' ' + msg + '\n');
        } catch (e) { /* the state dir may be gone while aictl cleans up */ }
    }

    function beat() {
        try {
            writeAtomic(path.join(STATE, 'runner.json'), JSON.stringify({
                pid: process.pid, started: started, beat: Date.now(), busy: busy, done: done
            }));
        } catch (e) { /* ignore */ }
    }

    function evalScript(src, cb) {
        // The native binding behind CEP's evalScript (no Adobe library needed).
        window.__adobe_cep__.evalScript(src, cb);
    }

    function poll() {
        if (busy) { return; }
        var names;
        try {
            names = fs.readdirSync(QUEUE).filter(function (n) { return /\.req$/.test(n); }).sort();
        } catch (e) {
            return;
        }
        if (!names.length) { return; }
        var id = names[0].replace(/\.req$/, '');
        var req = path.join(QUEUE, names[0]);
        var taken = path.join(QUEUE, id + '.taken');
        var job;
        try {
            fs.renameSync(req, taken);
            job = JSON.parse(fs.readFileSync(taken, 'utf8'));
        } catch (e) {
            log('bad request ' + id + ': ' + e);
            return;
        }
        busy = id;
        beat();
        log('run ' + id);
        evalScript('$.evalFile(new File(' + JSON.stringify(job.script) + '))', function (ret) {
            try { writeAtomic(path.join(QUEUE, id + '.ret'), String(ret)); } catch (e) { /* ignore */ }
            log('end ' + id + ' (' + String(ret).slice(0, 80) + ')');
            busy = null;
            done += 1;
            beat();
        });
    }

    try { fs.mkdirSync(QUEUE, { recursive: true }); } catch (e) { /* exists */ }
    log('runner loaded (pid ' + process.pid + ')');
    beat();
    setInterval(beat, 1000);
    setInterval(poll, 100);
    evalScript('app.name + "|" + app.version + "|" + app.locale', function (ret) {
        var parts = String(ret).split("|");
        writeAtomic(path.join(STATE, 'ready.json'), JSON.stringify({
            app: parts[0], version: parts[1], locale: parts[2], runner_pid: process.pid, at: Date.now()
        }));
        log('ready: ' + ret);
    });
}());
