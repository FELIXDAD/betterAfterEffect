// fixture-project.js - builds a mock AE project that touches every branch of bae-export.jsx.
// The same scene is described in README.md (manual test T1) so it can be rebuilt by hand in AE.

function buildFixtureProject() {
    var P = PropertyValueType;
    var K = KeyframeInterpolationType;
    MOCK_FS = {
        "/Users/test/footage/clip.mov": {size: 1048576, modified: Date.UTC(2026, 10, 1, 8, 30, 0)},
        "/Users/test/footage/seq/frame_0001.png": {size: 2048, modified: Date.UTC(2026, 10, 2)},
        "/Users/test/footage/seq/frame_0002.png": {size: 2048, modified: Date.UTC(2026, 10, 2)},
        "/Users/test/footage/seq/frame_0003.png": {size: 2048, modified: Date.UTC(2026, 10, 2)},
        "/Users/test/footage/seq/other_0001.png": {size: 2048, modified: Date.UTC(2026, 10, 2)},
        "/Users/test/projects/demo.aep": {size: 4096, modified: Date.UTC(2026, 10, 3)}
    };
    var project = new Project();
    project.file = new File("/Users/test/projects/demo.aep");
    app.project = project;

    var footageFolder = project.addItem(new FolderItem(2, "Footage"));
    var solidsFolder = project.addItem(new FolderItem(3, "Solids"));
    var clip = project.addItem(new FootageItem(10, "clip.mov",
        new FileSource(new File("/Users/test/footage/clip.mov"), false),
        {duration: 5, frameRate: 25, hasAudio: true}), footageFolder);
    var seq = project.addItem(new FootageItem(11, "frame_[0001-0003].png",
        new FileSource(new File("/Users/test/footage/seq/frame_0001.png"), false),
        {duration: 0.1, frameRate: 30, width: 640, height: 360}), footageFolder);
    var missingSource = new FileSource(null, false);
    missingSource.missingFootagePath = "D:\\shots\\missing.mov";
    var missing = project.addItem(new FootageItem(12, "missing.mov", missingSource, {footageMissing: true}),
        footageFolder);
    var redSolid = project.addItem(new FootageItem(13, "Red Solid 1", new SolidSource([1, 0, 0])), solidsFolder);
    var nullSolid = project.addItem(new FootageItem(14, "Null 1", new SolidSource([1, 1, 1]),
        {width: 100, height: 100}), solidsFolder);

    var pre = project.addItem(new CompItem(20, "Pre comp", {frameRate: 30000 / 1001, duration: 5}));
    var main = project.addItem(new CompItem(21, "Main Comp", {
        frameRate: 30000 / 1001, duration: 10.01, bgColor: [0.1, 0.2, 0.3], comment: "hero shot", label: 9,
        markerProperty: markerProp([
            [1.001, new MarkerValue("intro", {duration: 0.5, chapter: "Ch1", label: 3})],
            [2.002, new MarkerValue("beat", {_params: {"cue type": "hit", "power": "2"}})]
        ])
    }));
    var dup = project.addItem(new CompItem(22, "Main Comp", {}));
    dup.addLayer(new Layer("ADBE AV Layer", "Clip", [transformGroup()], {source: clip, hasAudio: true}));

    // --- Pre comp: one text layer with an animator, a selector and keyframed source text.
    var sourceText = new Prop("ADBE Text Document", "Source Text", P.TEXT_DOCUMENT, null);
    var td1 = new TextDocument("Hello\rWorld \"quoted\"");
    td1.font = "SourceHanSansSC-Bold";
    td1.fontFamily = "Source Han Sans SC";
    td1.fontStyle = "Bold";
    td1.fontSize = 72;
    td1.applyFill = true;
    td1.applyStroke = false;
    td1.fillColor = [1, 1, 1];
    td1.justification = ParagraphJustification.CENTER_JUSTIFY;
    td1.tracking = 25;
    td1.leading = 86.4;
    td1.boxText = false;
    td1.pointText = true;
    var td2 = new TextDocument("\u4f60\u597d");
    td2.fontSize = 64;
    sourceText.addKey({t: 0, v: td1, inType: K.HOLD, outType: K.HOLD});
    sourceText.addKey({t: 1.001, v: td2, inType: K.HOLD, outType: K.HOLD});
    var selector = named("ADBE Text Selector", "Range Selector 1", [
        new Prop("ADBE Text Percent Start", "Start", P.OneD, 0).addKey({t: 0, v: 0}).addKey({t: 1, v: 100}),
        new Prop("ADBE Text Percent End", "End", P.OneD, 100),
        named("ADBE Text Range Advanced", "Advanced", [
            new Prop("ADBE Text Range Units", "Units", P.OneD, 1),
            new Prop("ADBE Text Range Shape", "Shape", P.OneD, 1)
        ])
    ]);
    var animator = named("ADBE Text Animator", "Animator 1", [
        indexed("ADBE Text Selectors", "Selectors", [selector]),
        indexed("ADBE Text Animator Properties", "Properties", [
            new Prop("ADBE Text Opacity", "Opacity", P.OneD, 0),
            new Prop("ADBE Text Fill Color", "Fill Color", P.COLOR, [1, 0.5, 0, 1]),
            // AE 26 reports PropertyValueType 161 for the variable-font axes.
            new Prop("ADBE Text VF Axis 1", "Axis 1", 161, 0),
            new Prop("ADBE Text VF Axis 2", "Axis 2", 161, 0)
        ])
    ]);
    pre.addLayer(new Layer("ADBE Text Layer", "Title \u6807\u9898", [
        named("ADBE Text Properties", "Text", [
            sourceText,
            named("ADBE Text Path Options", "Path Options", [
                new Prop("ADBE Text Path", "Path", P.MASK_INDEX, 1),
                new Prop("ADBE Text Reverse Path", "Reverse Path", P.OneD, 0)
            ]),
            indexed("ADBE Text Animators", "Animators", [animator])
        ]),
        indexed("ADBE Mask Parade", "Masks", [
            named("ADBE Mask Atom", "Path Mask", [
                new Prop("ADBE Mask Shape", "Mask Path", P.SHAPE,
                    new Shape([[0, 0], [100, 0]], [[0, 0], [-30, 0]], [[30, 0], [0, 0]], false))
            ], {maskMode: MaskMode.NONE, inverted: false, rotoBezier: false, maskMotionBlur: MaskMotionBlur.SAME_AS_LAYER,
                locked: false, color: [0.6, 0.6, 0.2], maskFeatherFalloff: MaskFeatherFalloff.FFO_SMOOTH})
        ]),
        indexed("ADBE Effect Parade", "Effects", []),
        transformGroup()
    ], {threeDPerChar: false}));

    // --- Main comp, top to bottom.
    // 1. Camera
    main.addLayer(new Layer("ADBE Camera Layer", "Camera 1", [
        transformGroup(null, [new Prop("ADBE Point of Interest", "Point of Interest", P.ThreeD_SPATIAL, [960, 540, 0])]),
        named("ADBE Camera Options Group", "Camera Options", [
            new Prop("ADBE Camera Zoom", "Zoom", P.OneD, 1866.667)
        ])
    ], {threeDLayer: true}));
    // 2. Light
    main.addLayer(new Layer("ADBE Light Layer", "Light 1", [
        transformGroup(),
        named("ADBE Light Options Group", "Light Options", [
            new Prop("ADBE Light Intensity", "Intensity", P.OneD, 100),
            new Prop("ADBE Light Color", "Color", P.COLOR, [1, 1, 1, 1])
        ])
    ], {threeDLayer: true, lightType: LightType.SPOT}));
    // 3. Null (parent of the shape layer)
    var nullLayer = main.addLayer(new Layer("ADBE AV Layer", "Controller", [transformGroup()],
        {nullLayer: true, source: nullSolid}));
    // 4. Shape layer with a group, rect, fill, gradient fill, trim paths and a repeater.
    var position = new Prop("ADBE Position", "Position", P.ThreeD_SPATIAL, [200, 540, 0]);
    position.addKey({t: 0, v: [200, 540, 0], inType: K.BEZIER, outType: K.BEZIER,
        inEase: [{speed: 0, influence: 33.333333}], outEase: [{speed: 0, influence: 33.333333}],
        inTan: [0, 0, 0], outTan: [100, -50, 0], sc: true, sab: false});
    position.addKey({t: 1.001, v: [1000, 300, 0], inType: K.BEZIER, outType: K.BEZIER,
        inEase: [{speed: 812.5, influence: 50}], outEase: [{speed: 812.5, influence: 50}],
        inTan: [-80, 0, 0], outTan: [80, 0, 0], sc: true, roving: true, tc: true});
    position.addKey({t: 2.002, v: [1720, 540, 0], inType: K.LINEAR, outType: K.HOLD, label: 4,
        inEase: [{speed: 0, influence: 16.666667}], outEase: [{speed: 0, influence: 16.666667}]});
    var scale = new Prop("ADBE Scale", "Scale", P.ThreeD, [100, 100, 100]);
    scale.addKey({t: 0, v: [0, 0, 100], inType: K.BEZIER, outType: K.BEZIER,
        inEase: [{speed: 0, influence: 10}, {speed: 0, influence: 20}, {speed: 0, influence: 30}],
        outEase: [{speed: 0, influence: 75}, {speed: 10, influence: 80}, {speed: 0, influence: 33.333333}], tab: true});
    scale.addKey({t: 0.5005, v: [120, 80, 100], inType: K.BEZIER, outType: K.BEZIER,
        inEase: [{speed: -5, influence: 40}, {speed: 0, influence: 33.333333}, {speed: 0, influence: 33.333333}],
        outEase: [{speed: 0, influence: 33.333333}, {speed: 0, influence: 33.333333}, {speed: 0, influence: 33.333333}]});
    var opacity = new Prop("ADBE Opacity", "Opacity", P.OneD, 100, {
        expression: "wiggle(2, 20)", expressionEnabled: true});
    var rotation = new Prop("ADBE Rotate Z", "Rotation", P.OneD, 0, {
        expression: "time * 36 +", expressionEnabled: false, expressionError: "SyntaxError: unexpected end"});
    var shapeTransform = named("ADBE Transform Group", "Transform", [
        new Prop("ADBE Anchor Point", "Anchor Point", P.ThreeD_SPATIAL, [0, 0, 0]),
        position, scale, rotation, opacity,
        new Prop("ADBE Position_0", "X Position", P.OneD, 200)
    ]);
    var rect = named("ADBE Vector Shape - Rect", "Rectangle Path 1", [
        new Prop("ADBE Vector Shape Direction", "Shape Direction", P.OneD, 1),
        new Prop("ADBE Vector Rect Size", "Size", P.TwoD, [200, 120]),
        new Prop("ADBE Vector Rect Position", "Position", P.TwoD_SPATIAL, [0, 0]),
        new Prop("ADBE Vector Rect Roundness", "Roundness", P.OneD, 12)
    ]);
    var fill = named("ADBE Vector Graphic - Fill", "Fill 1", [
        new Prop("ADBE Vector Blend Mode", "Blend Mode", P.OneD, 1),
        new Prop("ADBE Vector Fill Color", "Color", P.COLOR, [0.2, 0.6, 1, 1], {isSpatial: true})
            .addKey({t: 0, v: [0.2, 0.6, 1, 1], sc: true, sab: true})
            .addKey({t: 1, v: [1, 0, 0, 1], sc: true})
    ]);
    var gfill = named("ADBE Vector Graphic - G-Fill", "Gradient Fill 1", [
        new Prop("ADBE Vector Grad Type", "Type", P.OneD, 1),
        new Prop("ADBE Vector Grad Start Pt", "Start Point", P.TwoD_SPATIAL, [0, 0]),
        new Prop("ADBE Vector Grad Colors", "Colors", P.NO_VALUE, null)
    ]);
    var group = named("ADBE Vector Group", "Rectangle 1", [
        new Prop("ADBE Vector Blend Mode", "Blend Mode", P.OneD, 1),
        indexed("ADBE Vectors Group", "Contents", [rect, fill, gfill]),
        named("ADBE Vector Transform Group", "Transform", [
            new Prop("ADBE Vector Anchor", "Anchor Point", P.TwoD_SPATIAL, [0, 0]),
            new Prop("ADBE Vector Position", "Position", P.TwoD_SPATIAL, [0, 0]),
            new Prop("ADBE Vector Group Opacity", "Opacity", P.OneD, 100)
        ])
    ]);
    var path = named("ADBE Vector Shape - Group", "Path 1", [
        new Prop("ADBE Vector Shape", "Path", P.SHAPE,
            new Shape([[0, 0], [50, 50], [0, 100]], [[0, 0], [0, -20], [0, 0]], [[0, 0], [0, 20], [0, 0]], true))
    ]);
    var trim = named("ADBE Vector Filter - Trim", "Trim Paths 1", [
        new Prop("ADBE Vector Trim Start", "Start", P.OneD, 0),
        new Prop("ADBE Vector Trim End", "End", P.OneD, 100).addKey({t: 0, v: 0}).addKey({t: 2, v: 100})
    ]);
    var repeater = named("ADBE Vector Filter - Repeater", "Repeater 1", [
        new Prop("ADBE Vector Repeater Copies", "Copies", P.OneD, 3)
    ]);
    var shapeLayer = main.addLayer(new Layer("ADBE Vector Layer", "Box", [
        indexed("ADBE Root Vectors Group", "Contents", [group, path, trim, repeater]),
        indexed("ADBE Mask Parade", "Masks", []),
        indexed("ADBE Effect Parade", "Effects", []),
        shapeTransform,
        markerProp([[0.5, new MarkerValue("layer marker", {protectedRegion: true})]])
    ], {parent: nullLayer, blendingMode: BlendingMode.SILHOUETE_ALPHA, motionBlur: true}));
    // 5. Matte: the precomp, used as alpha matte for layer 6.
    main.addLayer(new Layer("ADBE AV Layer", "Pre comp", [transformGroup()],
        {source: pre, isTrackMatte: true, enabled: false, startTime: 1, inPoint: 1, outPoint: 6, stretch: 50}));
    // 6. Solid with two masks (same display name), effects (Gaussian blur, a layer parameter,
    // a pseudo effect, a curves-like CUSTOM_VALUE parameter) and time remapping disabled.
    var maskShape = new Shape([[0, 0], [100, 0], [100, 100]], [[0, 0], [0, 0], [0, 0]], [[0, 0], [0, 0], [0, 0]], true);
    maskShape.featherSegLocs = [0];
    var maskShapeProp = new Prop("ADBE Mask Shape", "Mask Path", P.SHAPE, maskShape);
    maskShapeProp.addKey({t: 0, v: maskShape});
    maskShapeProp.addKey({t: 1, v: maskShape});
    var solid = main.addLayer(new Layer("ADBE AV Layer", "Red Solid 1", [
        indexed("ADBE Mask Parade", "Masks", [
            named("ADBE Mask Atom", "Mask 1", [
                maskShapeProp,
                new Prop("ADBE Mask Feather", "Mask Feather", P.TwoD, [10, 10]),
                new Prop("ADBE Mask Opacity", "Mask Opacity", P.OneD, 100),
                new Prop("ADBE Mask Offset", "Mask Expansion", P.OneD, 0)
            ], {maskMode: MaskMode.ADD, inverted: true, rotoBezier: false, maskMotionBlur: MaskMotionBlur.SAME_AS_LAYER,
                locked: false, color: [1, 1, 0], maskFeatherFalloff: MaskFeatherFalloff.FFO_SMOOTH}),
            named("ADBE Mask Atom", "Mask 1", [
                new Prop("ADBE Mask Shape", "Mask Path", P.SHAPE, maskShape)
            ], {maskMode: 9999})
        ]),
        indexed("ADBE Effect Parade", "Effects", [
            named("ADBE Gaussian Blur 2", "Gaussian Blur", [
                new Prop("ADBE Gaussian Blur 2-0001", "Blurriness", P.OneD, 12.5),
                new Prop("ADBE Gaussian Blur 2-0002", "Blur Dimensions", P.OneD, 1),
                new Prop("ADBE Gaussian Blur 2-0003", "Repeat Edge Pixels", P.OneD, 0)
            ]),
            named("ADBE Set Matte3", "Set Matte", [
                new Prop("ADBE Set Matte3-0001", "Take Matte From Layer", P.LAYER_INDEX, 3),
                new Prop("ADBE Set Matte3-0002", "Use For Matte", P.OneD, 4)
            ]),
            named("Pseudo/my_ctrl", "My Controls", [
                new Prop("Pseudo/my_ctrl-0001", "Amount", P.OneD, 50),
                new Prop("Pseudo/my_ctrl-0002", "Target Mask", P.MASK_INDEX, 2),
                new Prop("Pseudo/my_ctrl-0003", "Nothing", P.LAYER_INDEX, 0)
            ]),
            named("ADBE CurvesCustom", "Curves", [
                new Prop("ADBE CurvesCustom-0001", "Curves", P.CUSTOM_VALUE, null),
                new Prop("ADBE CurvesCustom-0002", "", P.NO_VALUE, null)
            ])
        ]),
        transformGroup(),
        new Prop("ADBE Time Remapping", "Time Remap", P.OneD, 0)
    ], {source: redSolid, trackMatteType: TrackMatteType.ALPHA}));
    // 7. Footage + audio, two layers with the same name, an emoji name and a leading dash name.
    main.addLayer(new Layer("ADBE AV Layer", "clip.mov", [transformGroup(),
        named("ADBE Audio Group", "Audio", [new Prop("ADBE Audio Levels", "Audio Levels", P.TwoD, [0, 0])])],
        {source: clip, hasAudio: true}));
    main.addLayer(new Layer("ADBE AV Layer", "clip.mov", [transformGroup()], {source: seq}));
    main.addLayer(new Layer("ADBE AV Layer", "\ud83d\ude00", [transformGroup()], {source: missing}));
    main.addLayer(new Layer("ADBE AV Layer", "-dash", [transformGroup()], {source: redSolid, adjustmentLayer: true}));
    return project;
}
