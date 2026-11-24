//! The expression language menu (the ▸ button of an expression row): AE's groups
//! in AE's order, the items inserted as written. AE shows this menu in English in
//! every language. Groups without items yet are listed disabled.

use egui::Ui;

use crate::common;

enum Group {
    Items(&'static [&'static str]),
    Sub(&'static [(&'static str, &'static [&'static str])]),
    Later,
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
    ("Vector Math", Group::Later),
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
    ("Color Conversion", Group::Later),
    ("Other Math", Group::Later),
    ("JavaScript Math", Group::Later),
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
    ("Footage", Group::Later),
    (
        "Layer",
        Group::Sub(&[
            (
                "Sub-objects",
                &[
                    "source",
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
    ("Camera", Group::Later),
    ("Light", Group::Later),
    ("Effect", Group::Later),
    ("Path Property", Group::Later),
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
    ("Key", Group::Later),
    ("MarkerKey", Group::Later),
    ("Project", Group::Later),
    ("Text", Group::Later),
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
            Group::Later => {
                ui.add_enabled(false, egui::Button::new(*name));
            }
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
                Group::Later => 0,
            };
            (*n, count)
        })
        .collect()
}
