import { app, bae, CameraLayer, CompItem, Property } from "bae";

const call = bae.commands.invoke;
const ratios = [0, 1, 4 / 3, 16 / 9, 16 / 10];
const faces = [
  ["Right", 90, 0, 2, 1],
  ["Left", -90, 0, 0, 1],
  ["Top", 0, -90, 1, 0],
  ["Bottom", 0, 90, 1, 2],
  ["Front", 0, 0, 1, 1],
  ["Back", 180, 0, 3, 1],
];

function attr(obj, key, type, value) {
  call("attr.set", { path: obj.id, attr: key, value: { type, value } });
}
function read(obj, key) {
  return new Property(obj.id, key, key).valueAtTime(0, true);
}
function tag(obj, role, master) {
  attr(obj, "vr:role", "string", role);
  if (master) attr(obj, "vr:master", "prim_ref", master.id);
}
function members(master, role) {
  const found = [];
  for (let i = 1; i <= app.project.numItems; i++) {
    const item = app.project.item(i);
    if (item instanceof CompItem && read(item, "vr:master") === master.id && read(item, "vr:role") === role)
      found.push(item);
  }
  return found;
}
function newComp(master, name, role, w, h) {
  const comp = app.project.items.addComp(name, w, h, 1, master.duration, master.frameRate);
  tag(comp, role, master);
  return comp;
}
function place(comp, source, x = 0, y = 0) {
  const layer = comp.layers.add(source);
  layer.anchorPoint.setValue([0, 0, 0]);
  layer.position.setValue([x, y, 0]);
  return layer;
}
function effect(layer, match, values) {
  const fx = layer.effect.addProperty(match);
  for (const [name, value] of Object.entries(values)) fx(name).setValue(value);
  return fx;
}
function planar(layer, width, aspect, pan, tilt, input = 3) {
  return effect(layer, "Mettle SkyBox Viewer", {
    "Input Format": input,
    "Output Frame Width": width,
    "Output Frame Aspect Ratio": aspect,
    "Projection Pan (Y Axis)": pan,
    "Projection Tilt (X Axis)": tilt,
  });
}
function convert(layer, input, output, width, pan = 0, tilt = 0) {
  return effect(layer, "Mettle SkyBox Converter", {
    Input: input,
    Output: output,
    "Output Frame Width": width,
    "Field of View": 90,
    "Pan (Y Axis)": pan,
    "Tilt (X Axis)": tilt,
    "Invert Rotation": true,
  });
}
function camera(comp, title, width, position, orientation, twoNode = false) {
  const cam = comp.layers.addCamera(title);
  attr(cam, "camera:type", "enum", twoNode ? "two_node" : "one_node");
  attr(cam, "camera:zoom", "float", width / 2);
  cam.position.setValue(position);
  if (twoNode) cam.pointOfInterest.setValue([position[0], position[1], position[2] + width / 2]);
  cam.orientation.setValue(orientation);
  return cam;
}
function editComp(master, role, args) {
  const aspect = args.aspect ?? 3;
  const width = args.width;
  const edits = members(master, "edit2d").concat(members(master, "edit3d"));
  const seq = edits.reduce((last, edit) => Math.max(last, read(edit, "vr:order")), 0) + 1;
  const title = master.name + " - Edit " + seq + (role === "edit2d" ? " (2D)" : " (3D)");
  const edit = newComp(master, title, role, width, Math.round(width / ratios[aspect]));
  attr(edit, "vr:aspect", "int", aspect);
  attr(edit, "vr:order", "int", seq);
  const spatial = read(master, "vr:role") === "master3d";
  const guide = place(
    edit,
    spatial ? cube(master, master, Math.max(256, Math.ceil(master.width / 4)), false) : master,
  );
  attr(guide, "guide", "bool", true);
  tag(guide, "guide");
  planar(guide, width, aspect, args.pan ?? 0, args.tilt ?? 0, spatial ? 1 : 3);
  attr(edit, "vr:pan", "float", args.pan ?? 0);
  attr(edit, "vr:tilt", "float", args.tilt ?? 0);
  if (role === "edit3d") {
    camera(edit, "VR Camera", width, [width / 2, edit.height / 2, -width / 2], [0, 0, 0]);
  }
  return edit;
}

// Cross layout matches the VR Converter's 4:3 cubemap. Each face remains an editable comp.
function cube(master, scene, width, extract) {
  const role = extract ? "cubemap" : "sceneCube";
  let atlas = members(master, role).find((c) => read(c, "vr:scene") === scene.id);
  if (!atlas) {
    atlas = newComp(master, scene.name + " - Cubemap", role, width * 4, width * 3);
    attr(atlas, "vr:scene", "prim_ref", scene.id);
  }
  atlas.width = width * 4;
  atlas.height = width * 3;
  const existing = members(master, "face").filter((c) => read(c, "vr:atlas") === atlas.id);
  const original = [];
  if (!extract) {
    for (let i = 1; i <= scene.numLayers; i++) {
      const layer = scene.layer(i);
      if (!(layer instanceof CameraLayer) && !read(layer, "guide")) original.push(layer.id);
    }
  }
  let rig = atlas.layer("VR Master Camera");
  if (!rig) {
    rig = camera(
      atlas,
      "VR Master Camera",
      width,
      [scene.width / 2, scene.height / 2, -scene.width / 2],
      [0, 0, 0],
    );
    if (!extract) {
      const src = "comp(" + JSON.stringify(scene.name) + ").activeCamera";
      rig.position.expression = src + ".toWorld([0,0,0])";
      // Direction and roll include the source camera's animated parents.
      rig.orientation.expression =
        "var c=" +
        src +
        "; var z=c.toWorldVec([0,0,1]); var x=c.toWorldVec([1,0,0]); var y=c.toWorldVec([0,1,0]); var deg=180/Math.PI; [Math.atan2(-z[1],z[2])*deg,Math.asin(Math.max(-1,Math.min(1,z[0])))*deg,Math.atan2(-y[0],x[0])*deg]";
    }
  }
  for (const [name, pan, tilt, col, row] of faces) {
    let face = existing.find((c) => read(c, "vr:face") === name);
    if (!face) {
      face = newComp(master, scene.name + " - " + name, "face", width, width);
      attr(face, "vr:atlas", "prim_ref", atlas.id);
      attr(face, "vr:face", "string", name);
      const tile = place(atlas, face, width * col, width * row);
      tag(tile, name);
    }
    face.width = width;
    face.height = width;
    for (let i = atlas.numLayers; i >= 1; i--) {
      const tile = atlas.layer(i);
      if (read(tile, "vr:role") === name) tile.position.setValue([width * col, width * row, 0]);
    }
    // Refresh only the generated scene copies. User layers on an extracted face survive.
    for (let i = face.numLayers; i >= 1; i--) {
      const layer = face.layer(i);
      if (read(layer, "vr:role") === "snapshot") layer.remove();
    }
    let cam = face.layer("VR " + name + " Camera");
    if (!cam)
      cam = camera(
        face,
        "VR " + name + " Camera",
        width,
        [scene.width / 2, scene.height / 2, -scene.width / 2],
        [-tilt, pan, 0],
      );
    attr(cam, "camera:zoom", "float", width / 2);
    const rigRef = "comp(" + JSON.stringify(atlas.name) + ").layer(" + JSON.stringify(rig.name) + ")";
    cam.position.expression = rigRef + ".position.value";
    if (extract) {
      cam.orientation.expression =
        "var a=" + rigRef + ".orientation.value; [a[0]+" + -tilt + ",a[1]+" + pan + ",a[2]]";
    } else {
      let mount = face.layer("VR Camera Mount");
      if (!mount) {
        mount = face.layers.addNull();
        mount.name = "VR Camera Mount";
        mount.threeDLayer = true;
      }
      mount.position.expression = rigRef + ".position.value";
      mount.orientation.expression = rigRef + ".orientation.value";
      cam.setParentWithJump(mount);
      cam.position.expression = "";
      cam.position.setValue([0, 0, 0]);
      cam.orientation.setValue([-tilt, pan, 0]);
    }
    if (extract) {
      let background = face.layer("VR Panorama");
      if (!background) {
        background = place(face, scene);
        background.name = "VR Panorama";
        planar(background, width, 1, pan, tilt);
      }
      const fx = background.effect("VR Sphere to Plane");
      fx("Output Frame Width").setValue(width);
      fx("Projection Pan (Y Axis)").expression =
        "thisComp.layer(" + JSON.stringify(cam.name) + ").orientation[1]";
      fx("Projection Tilt (X Axis)").expression =
        "-thisComp.layer(" + JSON.stringify(cam.name) + ").orientation[0]";
    } else if (original.length) {
      const copies = bae.copyLayersToComp(original, face.id);
      for (let i = 0; i < copies.length; i++) {
        const layer = app.project.layerByPath(copies[i]);
        layer.name = app.project.layerByPath(original[i]).name;
        tag(layer, "snapshot");
      }
    }
  }
  return atlas;
}
function output(master, args) {
  const out =
    members(master, "output")[0] ??
    newComp(master, master.name + " - Output", "output", master.width, master.height);
  const width = args.width ?? read(out, "vr:width") ?? Math.max(256, master.width);
  const format = args.format ?? read(out, "vr:format") ?? 3;
  const h = { 1: 3 / 4, 2: 1, 3: 1 / 2, 4: 1, 5: 2 / 3, 6: 2 / 3, 7: 1 / 6, 8: 9 / 16 }[format];
  out.width = width;
  out.height = Math.round(width * h);
  attr(out, "vr:width", "int", width);
  attr(out, "vr:format", "int", format);
  for (let i = out.numLayers; i >= 1; i--) {
    if (read(out.layer(i), "vr:role") === "projection") out.layer(i).remove();
  }
  const project = (source, input, pan, tilt) => {
    const layer = place(out, source);
    tag(layer, "projection");
    convert(layer, input, format, width, pan, tilt);
  };
  if (read(master, "vr:role") === "master3d") {
    project(cube(master, master, Math.max(256, Math.ceil(width / 4)), false), 2, 0, 0);
  } else project(master, 4, 0, 0);
  const edits = members(master, "edit2d").concat(members(master, "edit3d"));
  edits.sort((a, b) => read(a, "vr:order") - read(b, "vr:order"));
  for (const edit of edits) {
    if (read(edit, "vr:role") === "edit2d") project(edit, 1, read(edit, "vr:pan"), read(edit, "vr:tilt"));
    else project(cube(master, edit, Math.max(256, Math.ceil(width / 4)), false), 2, read(edit, "vr:pan"), read(edit, "vr:tilt"));
  }
  // Extracted faces read one shared equirectangular panorama, in every output layout.
  const panorama = members(master, "panorama")[0];
  if (panorama) updatePanorama(panorama, out, width, format);
  return out;
}

function updatePanorama(panorama, out, width, format) {
  panorama.width = width;
  panorama.height = Math.round(width / 2);
  for (let i = 1; i <= panorama.numLayers; i++) {
    const projection = panorama.layer(i);
    if (read(projection, "vr:role") !== "projection") continue;
    const fx = projection.effect("VR Converter");
    fx("Input").setValue(format + 1);
    fx("Output Frame Width").setValue(width);
    return;
  }
  const projection = place(panorama, out);
  tag(projection, "projection");
  convert(projection, format + 1, 3, width);
}

export function edit(args) {
  const width = args.width ?? 1024;
  if (!Number.isInteger(width) || width < 256 || width > 8192) throw new Error("Invalid VR width.");
  if (args.action === "master") {
    const comp = app.project.items.addComp(
      "VR Master " + (app.project.numItems + 1),
      width,
      width,
      1,
      args.duration ?? 30,
      args.fps ?? 30,
    );
    tag(comp, "master3d");
    const pos = args.center === false ? [0, 0, -width / 2] : [width / 2, width / 2, -width / 2];
    const cam = camera(comp, "VR Camera", width, pos, [0, 0, 0], args.twoNode !== false);
    if (args.nullControl) {
      const ctrl = comp.layers.addNull();
      ctrl.name = "VR Camera Control";
      ctrl.threeDLayer = true;
      ctrl.position.setValue(pos);
      cam.parent = ctrl;
    }
    return comp.id;
  }
  const master = app.project.itemByID(args.master);
  if (!(master instanceof CompItem)) throw new Error("Choose a composition.");
  if (args.action === "cube") {
    const out = output(master, {});
    let panorama = members(master, "panorama")[0];
    if (!panorama) {
      panorama = newComp(master, master.name + " - Panorama", "panorama", out.width, Math.round(out.width / 2));
      updatePanorama(panorama, out, out.width, read(out, "vr:format"));
    }
    return cube(master, panorama, width, true).id;
  }
  if (args.action === "output") return output(master, args).id;
  if (args.action === "delete") {
    const edit = app.project.itemByID(args.edit);
    if (!["edit2d", "edit3d"].includes(read(edit, "vr:role")) || read(edit, "vr:master") !== master.id)
      throw new Error("Choose a VR edit.");
    const remove = [edit.id];
    for (const atlas of members(master, "sceneCube").filter((c) => read(c, "vr:scene") === edit.id)) {
      remove.push(
        atlas.id,
        ...members(master, "face")
          .filter((c) => read(c, "vr:atlas") === atlas.id)
          .map((c) => c.id),
      );
    }
    call("item.delete", { paths: remove });
    return output(master, {}).id;
  }
  if (args.action === "properties") {
    const edit = app.project.itemByID(args.edit);
    if (read(edit, "vr:master") !== master.id) throw new Error("Edit belongs to another VR environment.");
    const aspect = args.aspect ?? 3;
    edit.width = width;
    edit.height = Math.round(width / ratios[aspect]);
    attr(edit, "vr:aspect", "int", aspect);
    attr(edit, "vr:pan", "float", args.pan);
    attr(edit, "vr:tilt", "float", args.tilt);
    for (let i = 1; i <= edit.numLayers; i++) {
      const layer = edit.layer(i);
      if (read(layer, "vr:role") !== "guide") continue;
      const fx = layer.effect("VR Sphere to Plane");
      fx("Output Frame Width").setValue(width);
      fx("Output Frame Aspect Ratio").setValue(aspect);
      fx("Projection Pan (Y Axis)").setValue(args.pan);
      fx("Projection Tilt (X Axis)").setValue(args.tilt);
    }
    output(master, {});
    return edit.id;
  }
  if (!["edit2d", "edit3d"].includes(args.action)) throw new Error("Unknown VR edit.");
  const result = editComp(master, args.action, { ...args, width });
  output(master, {});
  return result.id;
}
