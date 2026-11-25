import { app, CompItem } from "bae";

export function summary() {
  const comp = app.project.activeItem;
  if (!(comp instanceof CompItem)) throw new Error("Open a composition first.");
  const report = { name: comp.name, layers: comp.numLayers, duration: comp.duration, frameRate: comp.frameRate };
  console.log(JSON.stringify(report));
  return report;
}
