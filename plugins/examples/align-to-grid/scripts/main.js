// Script plugin example (docs/spec/plugin-architecture.md §17).
//
// The host imports this ES module into the plugin's own QuickJS runtime and calls the
// exported functions named by `entry` in bae-plugin.toml. The object model follows AE
// scripting names (app.project, CompItem, Layer, Property). Every mutation is a command;
// all mutations of one call form one undo step attributed to the invoking user or agent.

import { app, CompItem } from "bae";

function targetLayers(comp, paths) {
  if (paths && paths.length > 0) {
    return paths.map((p) => app.project.layerByPath(p));
  }
  return comp.selectedLayers;
}

function snap(v, size) {
  return Math.round(v / size) * size;
}

/** Command `plugin.com.example.align-to-grid.align-to-grid`. */
export function alignToGrid(args) {
  const size = args.size ?? 10;
  if (!Number.isFinite(size) || size <= 0) throw new Error("Grid size must be positive.");
  const comp = app.project.activeItem;
  if (!args.layers?.length && !(comp instanceof CompItem)) {
    throw new Error("Open a composition first.");
  }
  let moved = 0;
  for (const layer of targetLayers(comp, args.layers)) {
    const position = layer.transform.position;
    const [x, y, ...rest] = position.value;
    const snapped = [snap(x, size), snap(y, size), ...rest];
    if (snapped[0] !== x || snapped[1] !== y) {
      if (position.numKeys) position.setValueAtTime(layer.containingComp.time, snapped);
      else position.setValue(snapped);
      moved += 1;
    }
  }
  return { moved };
}

/** Script `script.com.example.align-to-grid.report-off-grid`: read-only. */
export function reportOffGrid() {
  const comp = app.project.activeItem;
  if (!(comp instanceof CompItem)) throw new Error("Open a composition first.");
  const off = [];
  for (let i = 1; i <= comp.numLayers; i++) {
    const layer = comp.layer(i);
    const [x, y] = layer.transform.position.value;
    if (x % 10 !== 0 || y % 10 !== 0) off.push(layer);
  }
  console.log(`${off.length} layer(s) are off the 10 px grid`);
  return { layers: off.map((l) => l.name) };
}
