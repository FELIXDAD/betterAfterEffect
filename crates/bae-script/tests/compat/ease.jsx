var comp = app.project.items.addComp("Fade", 320, 180, 1, 2, 30);
var opacity = comp.layers.addNull().property("ADBE Transform Group").property("ADBE Opacity");
opacity.setValuesAtTimes([0, 1], [0, 100]);
opacity.setTemporalEaseAtKey(1, [new KeyframeEase(15, 65)]);
var ease = opacity.keyOutTemporalEase(1)[0];
if (!(ease instanceof KeyframeEase) || ease.influence !== 65 || ease.speed !== 15 ||
    opacity.keyInInterpolationType(1) !== KeyframeInterpolationType.BEZIER) throw Error("ease");
opacity.expression = "value * 0.5";
if (opacity.valueAtTime(1, false) !== 50) throw Error("expression");
