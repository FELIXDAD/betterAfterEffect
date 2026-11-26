import { app, bae } from "bae";

const invoke = bae.commands.invoke;
const quote = JSON.stringify;

function setAttr(path, attr, type, value) {
    invoke("attr.set", {path, attr, value: {type, value}});
}

function addControl(layer, matchName, title) {
    const path = invoke("effect.apply", {layers: [layer.id], match_name: matchName}).effects[0];
    setAttr(path, "name", "string", title);
    return path;
}

function link(layer, title, target) {
    const path = addControl(layer, "ADBE Layer Control", title);
    setAttr(path, "param:0001", "prim_ref", target.id);
}

// Shape group transforms are local to the Contents hierarchy, inside layer space.
// Map a basis through the group stack once; its inverse places driven vertices.
function space(info) {
    let group = "source";
    const groups = [];
    if (!info.mask) {
        for (const index of info.indices.slice(0, -1)) {
            group += ".content(" + index + ")";
            groups.unshift(group + ".transform");
        }
    }
    const index = info.indices[info.indices.length - 1];
    const path = info.mask ? "source.mask(" + index + ").maskPath" : group + ".content(" + index + ").path";
    if (!groups.length) return {path, setup: "function local(p){return p;} function inverse(p){return p;}\n"};
    const setup = `
function rotate(p,a) {
    var r = a*Math.PI/180, c = Math.cos(r), s = Math.sin(r);
    return [c*p[0]-s*p[1], s*p[0]+c*p[1]];
}
function local(p) {
    var groups = [${groups.join(",")}];
    for (var j=0; j<groups.length; j++) {
        var t=groups[j], a=t.anchorPoint.value, s=t.scale.value, pos=t.position.value;
        p=[(p[0]-a[0])*s[0]/100,(p[1]-a[1])*s[1]/100];
        var axis=t.skewAxis.value;
        p=rotate(p,axis);
        p=[p[0]-Math.tan(t.skew.value*Math.PI/180)*p[1],p[1]];
        p=rotate(p,t.rotation.value-axis);
        p=[p[0]+pos[0],p[1]+pos[1]];
    }
    return p;
}
function inverse(p) {
    var o=local([0,0]), x=local([1,0]), y=local([0,1]);
    var a=x[0]-o[0], b=x[1]-o[1], c=y[0]-o[0], d=y[1]-o[1], det=a*d-b*c;
    return [(d*(p[0]-o[0])-c*(p[1]-o[1]))/det, (a*(p[1]-o[1])-b*(p[0]-o[0]))/det];
}
`;
    return {path, setup};
}

function trace(comp, info, name, path, setup) {
    const tracer = comp.layers.addNull();
    tracer.name = "Trace " + name;
    link(tracer, "Path Layer", info.layer);
    addControl(tracer, "ADBE Slider Control", "Progress");
    addControl(tracer, "ADBE Checkbox Control", "Loop");
    tracer.effect("Loop")(1).setValue(true);
    const progress = tracer.effect("Progress")(1);
    progress.setValueAtTime(comp.time, 0);
    progress.setValueAtTime(comp.time + 1, 100);
    progress.expression = 'effect("Loop")(1).value && numKeys > 1 ? loopOut("cycle") : value';
    const sample = 'var source=effect("Path Layer")(1);\n' + setup + "var curve=" + path +
        '; var u=effect("Progress")(1).value/100; var p=curve.pointOnPath(u);\n';
    tracer.position.expression = sample + "source.toComp(local(p))";
    tracer.rotation.expression = sample + `
var t=curve.tangentOnPath(u), a=source.toComp(local(p)), b=source.toComp(local([p[0]+t[0],p[1]+t[1]]));
Math.atan2(b[1]-a[1],b[0]-a[0])*180/Math.PI`;
}

export function bind({mode, paths}) {
    if (!["controllers", "followers", "trace"].includes(mode) || !paths?.length) {
        throw new Error("Select a path and a binding mode.");
    }
    for (const id of new Set(paths)) {
        const info = bae.pathProperty(id);
        const comp = info.layer.containingComp;
        const count = info.property.value.vertices.length;
        if (!count || info.layer.locked) throw new Error("Select an unlocked path with vertices.");
        const {path, setup} = space(info);
        const name = info.layer.name + ": " + info.name;
        if (mode === "trace") {
            trace(comp, info, name, path, setup);
            continue;
        }
        const controls = [];
        for (let i = 0; i < count; i++) {
            const node = comp.layers.addNull();
            node.name = name + " [" + i + "]";
            const drives = mode === "controllers";
            const source = drives ? "thisComp.layer(" + info.layer.index + ")" : 'effect("Path Layer")(1)';
            if (!drives) link(node, "Path Layer", info.layer);
            node.position.expression = "var source=" + source + ";\n" + setup +
                "source.toComp(local(" + path + ".points()[" + i + "]))";
            if (drives) {
                const position = node.position.value;
                node.position.expression = "";
                node.position.setValue(position);
                // A controller stays reassignable even when layers are renamed or reordered.
                let title = node.name;
                let suffix = 2;
                while (info.layer.effect(title)) title = node.name + " " + suffix++;
                link(info.layer, title, node);
                controls.push(title);
            }
        }
        if (controls.length) {
            info.property.expression = "var source=thisLayer;\n" + setup + "var names=" + quote(controls) + `;
var vertices=points();
for (var i=0; i<Math.min(names.length,vertices.length); i++) {
    var node=effect(names[i])(1);
    if (node && node.index!==thisLayer.index) vertices[i]=inverse(fromCompToSurface(node.toComp(node.anchorPoint.value)));
}
createPath(vertices,inTangents(),outTangents(),isClosed())`;
        }
    }
}
