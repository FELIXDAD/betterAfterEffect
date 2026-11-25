import { app, CompItem } from "bae";

export function fade({ frames = 12 } = {}) {
  const comp = app.project.activeItem;
  if (!(comp instanceof CompItem)) throw new Error("Open a composition first.");
  if (!Number.isInteger(frames) || frames < 1) throw new Error("Frames must be a positive integer.");
  const layers = comp.selectedLayers;
  if (!layers.length) throw new Error("Select at least one layer.");
  const start = comp.time;
  const end = start + frames * comp.frameDuration;
  if (end >= comp.duration) throw new Error("Move the playhead earlier or shorten the fade.");
  for (const layer of layers) {
    const opacity = layer.opacity;
    const target = opacity.valueAtTime(end, true);
    opacity.setValueAtTime(start, 0);
    opacity.setValueAtTime(end, target);
  }
  return { animated: layers.length };
}
