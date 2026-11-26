//! The expression language menu (the ▸ button of an expression row): AE's groups
//! in AE's order, the items inserted as written. AE shows this menu in English in
//! every language.

pub(crate) mod assist;

use egui::Ui;

use crate::common;

enum Group {
    Items(&'static [&'static str]),
    Sub(&'static [(&'static str, &'static [&'static str])]),
}

const MENU: &[(&str, Group)] = &[
    (
        "Global",
        Group::Items(&[
            "comp(name)",
            "footage(name)",
            "thisComp",
            "thisLayer",
            "thisProperty",
            "time",
            "colorDepth",
            "posterizeTime(updatesPerSecond)",
            "timeToFrames(t = time + thisComp.displayStartTime, fps = 1.0 / thisComp.frameDuration, isDuration = false)",
            "framesToTime(frames, fps = 1.0 / thisComp.frameDuration)",
            "timeToTimecode(t = time + thisComp.displayStartTime, timecodeBase = 30, isDuration = false)",
            "timeToNTSCTimecode(t = time + thisComp.displayStartTime, ntscDropFrame = false, isDuration = false)",
            "timeToFeetAndFrames(t = time + thisComp.displayStartTime, fps = 1.0 / thisComp.frameDuration, framesPerFoot = 16, isDuration = false)",
            "timeToCurrentFormat(t = time + thisComp.displayStartTime, fps = 1.0 / thisComp.frameDuration, isDuration = false)",
        ]),
    ),
    (
        "Vector Math",
        Group::Items(&[
            "add(vec1, vec2)",
            "sub(vec1, vec2)",
            "mul(vec, amount)",
            "div(vec, amount)",
            "clamp(value, limit1, limit2)",
            "dot(vec1, vec2)",
            "cross(vec1, vec2)",
            "normalize(vec)",
            "length(vec)",
            "length(point1, point2)",
            "lookAt(fromPoint, atPoint)",
        ]),
    ),
    (
        "Random Numbers",
        Group::Items(&[
            "seedRandom(offset, timeless = false)",
            "random()",
            "random(maxValOrArray)",
            "random(minValOrArray, maxValOrArray)",
            "gaussRandom()",
            "gaussRandom(maxValOrArray)",
            "gaussRandom(minValOrArray, maxValOrArray)",
            "noise(valOrArray)",
        ]),
    ),
    (
        "Interpolation",
        Group::Items(&[
            "linear(t, tMin, tMax, value1, value2)",
            "linear(t, value1, value2)",
            "ease(t, value1, value2)",
            "ease(t, tMin, tMax, value1, value2)",
            "easeIn(t, value1, value2)",
            "easeIn(t, tMin, tMax, value1, value2)",
            "easeOut(t, value1, value2)",
            "easeOut(t, tMin, tMax, value1, value2)",
        ]),
    ),
    (
        "Color Conversion",
        Group::Items(&[
            "rgbToHsl(rgbaArray)",
            "hslToRgb(hslaArray)",
            "hexToRgb(hexString)",
        ]),
    ),
    (
        "Other Math",
        Group::Items(&["degreesToRadians(degrees)", "radiansToDegrees(radians)"]),
    ),
    (
        "JavaScript Math",
        Group::Items(&[
            "Math.cos(value)",
            "Math.acos(value)",
            "Math.tan(value)",
            "Math.atan(value)",
            "Math.atan2(y, x)",
            "Math.sin(value)",
            "Math.sqrt(value)",
            "Math.exp(value)",
            "Math.pow(value, exponent)",
            "Math.log(value)",
            "Math.abs(value)",
            "Math.round(value)",
            "Math.ceil(value)",
            "Math.floor(value)",
            "Math.min(value1, value2)",
            "Math.max(value1, value2)",
            "Math.PI",
            "Math.E",
            "Math.LOG2E",
            "Math.LOG10E",
            "Math.LN2",
            "Math.LN10",
            "Math.SQRT2",
            "Math.SQRT1_2",
        ]),
    ),
    (
        "Comp",
        Group::Items(&[
            "layer(index)",
            "layer(name)",
            "layer(otherLayer, relIndex)",
            "marker",
            "numLayers",
            "activeCamera",
            "width",
            "height",
            "duration",
            "ntscDropFrame",
            "displayStartTime",
            "frameDuration",
            "shutterAngle",
            "shutterPhase",
            "bgColor",
            "pixelAspect",
            "name",
        ]),
    ),
    (
        "Footage",
        Group::Items(&[
            "width",
            "height",
            "duration",
            "frameDuration",
            "ntscDropFrame",
            "pixelAspect",
            "name",
            "sourceText",
            "sourceData",
            "dataValue(dataPath)",
            "dataKeyCount(dataPath)",
            "dataKeyTimes(dataPath, t0 = startTime, t1 = endTime)",
            "dataKeyValues(dataPath, t0 = startTime, t1 = endTime)",
        ]),
    ),
    (
        "Layer",
        Group::Sub(&[
            (
                "Sub-objects",
                &[
                    "source",
                    "sourceRectAtTime(t = time, includeExtents = false)",
                    "effect(name)",
                    "effect(index)",
                    "mask(name)",
                    "mask(index)",
                ],
            ),
            (
                "General",
                &[
                    "width",
                    "height",
                    "index",
                    "parent",
                    "hasParent",
                    "inPoint",
                    "outPoint",
                    "startTime",
                    "hasVideo",
                    "hasAudio",
                    "active",
                    "enabled",
                    "audioActive",
                    "sampleImage(point, radius = [.5, .5], postEffect = true, t = time)",
                ],
            ),
            (
                "Properties",
                &[
                    "anchorPoint",
                    "position",
                    "scale",
                    "rotation",
                    "opacity",
                    "audioLevels",
                    "timeRemap",
                    "marker",
                    "name",
                ],
            ),
            (
                "3D",
                &[
                    "orientation",
                    "rotationX",
                    "rotationY",
                    "rotationZ",
                    "lightTransmission",
                    "castsShadows",
                    "acceptsShadows",
                    "acceptsLights",
                    "ambient",
                    "diffuse",
                    "specular",
                    "specularIntensity",
                    "shininess",
                    "specularShininess",
                    "metal",
                ],
            ),
            (
                "Space Transforms",
                &[
                    "toComp(point, t = time)",
                    "fromComp(point, t = time)",
                    "toWorld(point, t = time)",
                    "fromWorld(point, t = time)",
                    "toCompVec(vec, t = time)",
                    "fromCompVec(vec, t = time)",
                    "toWorldVec(vec, t = time)",
                    "fromWorldVec(vec, t = time)",
                    "fromCompToSurface(point, t = time)",
                ],
            ),
        ]),
    ),
    (
        "Camera",
        Group::Items(&[
            "pointOfInterest",
            "zoom",
            "depthOfField",
            "focusDistance",
            "aperture",
            "blurLevel",
            "irisShape",
            "irisRotation",
            "irisRoundness",
            "irisAspectRatio",
            "irisDiffractionFringe",
            "highlightGain",
            "highlightThreshold",
            "highlightSaturation",
            "active",
        ]),
    ),
    (
        "Light",
        Group::Items(&[
            "pointOfInterest",
            "intensity",
            "color",
            "coneAngle",
            "coneFeather",
            "shadowDarkness",
            "shadowDiffusion",
        ]),
    ),
    (
        "Effect",
        Group::Items(&["active", "param(name)", "param(index)", "name"]),
    ),
    (
        "Path Property",
        Group::Items(&[
            "points(t = time)",
            "inTangents(t = time)",
            "outTangents(t = time)",
            "isClosed()",
            "pointOnPath(percentage = 0.5, t = time)",
            "tangentOnPath(percentage = 0.5, t = time)",
            "normalOnPath(percentage = 0.5, t = time)",
            "createPath(points = [[0,0], [100,0], [100,100], [0,100]], inTangents = [], outTangents = [], is_closed = true)",
        ]),
    ),
    (
        "Property",
        Group::Items(&[
            "value",
            "valueAtTime(t)",
            "velocity",
            "velocityAtTime(t)",
            "speed",
            "speedAtTime(t)",
            "wiggle(freq, amp, octaves = 1, amp_mult = .5, t = time)",
            "temporalWiggle(freq, amp, octaves = 1, amp_mult = .5, t = time)",
            "smooth(width = .2, samples = 5, t = time)",
            "loopIn(type = \"cycle\", numKeyframes = 0)",
            "loopOut(type = \"cycle\", numKeyframes = 0)",
            "loopInDuration(type = \"cycle\", duration = 0)",
            "loopOutDuration(type = \"cycle\", duration = 0)",
            "key(index)",
            "key(markerName)",
            "nearestKey(t)",
            "numKeys",
            "name",
            "active",
            "enabled",
            "propertyGroup(countUp = 1)",
            "propertyIndex",
        ]),
    ),
    ("Key", Group::Items(&["value", "time", "index"])),
    (
        "MarkerKey",
        Group::Items(&[
            "duration",
            "comment",
            "chapter",
            "url",
            "frameTarget",
            "eventCuePoint",
            "cuePointName",
            "parameters",
            "protectedRegion",
        ]),
    ),
    (
        "Project",
        Group::Items(&["fullPath", "bitsPerChannel", "linearBlending"]),
    ),
    (
        "Text",
        Group::Items(&[
            "sourceText",
            "sourceText.style",
            "sourceText.getStyleAt(charIndex, t = time)",
            "sourceText.createStyle()",
            "style.setFont(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setFontSize(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setFillColor(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setStrokeColor(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setStrokeWidth(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setTracking(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setLeading(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setBaselineShift(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setFauxBold(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setFauxItalic(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setAllCaps(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setSmallCaps(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setBaselineOption(value, startIndex = 0, numOfCharacters = text.sourceText.length - startIndex)",
            "style.setText(value)",
            "style.setJustification(value)",
            "style.setApplyFill(value)",
            "style.setApplyStroke(value)",
            "style.setAutoLeading(value)",
        ]),
    ),
];

fn items(ui: &mut Ui, list: &'static [&'static str], chosen: &mut Option<&'static str>) {
    for item in list {
        if ui.button(*item).clicked() {
            *chosen = Some(item);
            ui.close();
        }
    }
}

/// The menu's contents; `chosen` gets the item clicked.
pub(crate) fn ui(ui: &mut Ui, chosen: &mut Option<&'static str>) {
    for (name, group) in MENU {
        match group {
            Group::Items(list) => common::submenu(ui, name, &mut |ui| items(ui, list, chosen)),
            Group::Sub(subs) => common::submenu(ui, name, &mut |ui| {
                for (sub, list) in subs.iter() {
                    common::submenu(ui, sub, &mut |ui| items(ui, list, chosen));
                }
            }),
        }
    }
}

#[cfg(test)]
pub(crate) fn groups() -> Vec<(&'static str, usize)> {
    MENU.iter()
        .map(|(n, g)| {
            let count = match g {
                Group::Items(l) => l.len(),
                Group::Sub(s) => s.iter().map(|(_, l)| l.len()).sum(),
            };
            (*n, count)
        })
        .collect()
}
