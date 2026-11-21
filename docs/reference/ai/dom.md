# Adobe Illustrator 文档对象模型（DOM）参考

> 来源：Adobe Illustrator 2026 (30.1.0, build 136R) | 语言：zh_CN | 平台：Microsoft Windows 11 专业版 Build 26200 | 提取日期：2026-11-21

通过 ExtendScript 在临时文档中实际创建各对象并反射读取其属性名、访问权限、声明类型、运行时类型与示例值。共覆盖 34 个对象实例/类型。

## 索引

| 键名 | ExtendScript `typename` | 属性数 | 方法数 |
|---|---|---:|---:|
| `Document` | `Document` | 85 | 70 |
| `Artboard` | `Artboard` | 9 | 3 |
| `Layer` | `Layer` | 34 | 3 |
| `PathItem` | `PathItem` | 54 | 17 |
| `PathPoint` | `PathPoint` | 7 | 1 |
| `CompoundPathItem` | `CompoundPathItem` | 33 | 12 |
| `GroupItem` | `GroupItem` | 49 | 13 |
| `GroupItem_ClippingMask` | `GroupItem` | 49 | 13 |
| `TextFrame_Point` | `TextFrame` | 62 | 16 |
| `TextFrame_Area` | `TextFrame` | 62 | 16 |
| `TextFrame_Path` | `TextFrame` | 62 | 16 |
| `TextRange` | `TextRange` | 18 | 8 |
| `CharacterAttributes` | `CharacterAttributes` | 61 | 0 |
| `ParagraphAttributes` | `ParagraphAttributes` | 42 | 0 |
| `Gradient` | `Gradient` | 5 | 1 |
| `GradientStop` | `GradientStop` | 6 | 1 |
| `GradientColor_Linear` | `GradientColor` | 8 | 0 |
| `GradientColor_Radial` | `GradientColor` | 8 | 0 |
| `RGBColor` | `RGBColor` | 4 | 0 |
| `CMYKColor` | `CMYKColor` | 5 | 0 |
| `GrayColor` | `GrayColor` | 2 | 0 |
| `Spot` | `Spot` | 6 | 2 |
| `SpotColor` | `SpotColor` | 3 | 0 |
| `NoColor` | `NoColor` | 1 | 0 |
| `PatternColor` | `PatternColor` | 0 | 0 |
| `Pattern` | `Pattern` | 3 | 1 |
| `Symbol` | `Symbol` | 3 | 2 |
| `SymbolItem` | `SymbolItem` | 33 | 13 |
| `Swatch` | `Swatch` | 4 | 1 |
| `SwatchGroup` | `SwatchGroup` | 3 | 4 |
| `GraphicStyle` | `GraphicStyle` | 3 | 3 |
| `Brush` | `Brush` | 3 | 1 |
| `RasterItem` | `RasterItem` | 45 | 14 |
| `Matrix` | `Matrix` | 7 | 0 |

## Document (`Document`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `XMPString` | readwrite | `string` | `string` | `<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?> <x:xmpme...` |
| `activeDataSet` | readwrite | `any` | `unreadable` | *(不可用: `no_active_dataset`)* |
| `activeLayer` | readwrite | `Layer` | `Layer` | `<Layer>` |
| `activeView` | readonly | `View` | `View` | `<View>` |
| `artboards` | readonly | `Artboards` | `Artboards` | `<Artboards>` |
| `assets` | readonly | `Assets` | `Assets` | `<Assets>` |
| `brushes` | readonly | `Brushes` | `Brushes` | `<Brushes>` |
| `characterStyles` | readonly | `CharacterStyles` | `CharacterStyles` | `<CharacterStyles>` |
| `cloudPath` | readonly | `any` | `unreadable` | *(不可用: `error_2072`)* |
| `colorProfileName` | readonly | `string` | `string` | `sRGB IEC61966-2.1` |
| `compoundPathItems` | readonly | `CompoundPathItems` | `CompoundPathItems` | `<CompoundPathItems>` |
| `cropBox` | readwrite | `rect` | `Array(4)` | `[850, 600, 1250, 200]` |
| `cropStyle` | readwrite | `Object` | `CropOptions.Standard` | `<CropOptions.Standard>` |
| `dataSets` | readonly | `DataSets` | `DataSets` | `<DataSets>` |
| `defaultFillColor` | readwrite | `RGBColor` | `RGBColor` | `<RGBColor>` |
| `defaultFillOverprint` | readwrite | `boolean` | `boolean` | `false` |
| `defaultFilled` | readwrite | `boolean` | `boolean` | `true` |
| `defaultStrokeCap` | readwrite | `Object` | `StrokeCap.BUTTENDCAP` | `<StrokeCap.BUTTENDCAP>` |
| `defaultStrokeColor` | readwrite | `RGBColor` | `RGBColor` | `<RGBColor>` |
| `defaultStrokeDashOffset` | readwrite | `number` | `number` | `0` |
| `defaultStrokeDashes` | readwrite | `array` | `Array(0)` | `[]` |
| `defaultStrokeJoin` | readwrite | `Object` | `StrokeJoin.MITERENDJOIN` | `<StrokeJoin.MITERENDJOIN>` |
| `defaultStrokeMiterLimit` | readwrite | `number` | `number` | `10` |
| `defaultStrokeOverprint` | readwrite | `boolean` | `boolean` | `false` |
| `defaultStrokeWidth` | readwrite | `number` | `number` | `1` |
| `defaultStroked` | readwrite | `boolean` | `boolean` | `true` |
| `documentColorSpace` | readonly | `Object` | `DocumentColorSpace.RGB` | `<DocumentColorSpace.RGB>` |
| `embeddedItems` | readonly | `EmbeddedItems` | `EmbeddedItems` | `<EmbeddedItems>` |
| `fullName` | readonly | `filespec` | `` | `<redacted:undefined>` |
| `geometricBounds` | readonly | `rect` | `Array(4)` | `[50, 550, 660, 73.904]` |
| `gradients` | readonly | `Gradients` | `Gradients` | `<Gradients>` |
| `graphItems` | readonly | `GraphItems` | `GraphItems` | `<GraphItems>` |
| `graphicStyles` | readonly | `GraphicStyles` | `GraphicStyles` | `<GraphicStyles>` |
| `gridRepeatItems` | readonly | `GridRepeatItems` | `GridRepeatItems` | `<GridRepeatItems>` |
| `groupItems` | readonly | `GroupItems` | `GroupItems` | `<GroupItems>` |
| `height` | readonly | `number` | `number` | `400` |
| `inkList` | readonly | `array` | `Array(4)` | `["Ink", "Ink", "Ink", "Ink"]` |
| `isCloudDocument` | readonly | `boolean` | `boolean` | `false` |
| `kinsokuSet` | readonly | `array` | `Array(2)` | `["Hard", "Soft"]` |
| `layers` | readonly | `Layers` | `Layers` | `<Layers>` |
| `legacyTextItems` | readonly | `LegacyTextItems` | `LegacyTextItems` | `<LegacyTextItems>` |
| `listStyles` | readonly | `ListStyles` | `ListStyles` | `<ListStyles>` |
| `meshItems` | readonly | `MeshItems` | `MeshItems` | `<MeshItems>` |
| `mojikumiSet` | readonly | `array` | `Array(5)` | `["GyomatsuYakumonoZenkakuOrHankaku", "YakumonoHankaku", "GyomatsuYakumonoHankaku", "GyomatsuYakumonoZenkaku", "YakumonoZenkaku"]` |
| `name` | readonly | `string` | `string` | `未标题-1` |
| `nonNativeItems` | readonly | `NonNativeItems` | `NonNativeItems` | `<NonNativeItems>` |
| `outputResolution` | readonly | `number` | `number` | `800` |
| `pageItems` | readonly | `PageItems` | `PageItems` | `<PageItems>` |
| `pageOrigin` | readwrite | `point` | `Array(2)` | `[0, 0]` |
| `paragraphStyles` | readonly | `ParagraphStyles` | `ParagraphStyles` | `<ParagraphStyles>` |
| `parent` | readonly | `Application` | `Application` | `<Application>` |
| `path` | readonly | `filespec` | `` | `<redacted:undefined>` |
| `pathItems` | readonly | `PathItems` | `PathItems` | `<PathItems>` |
| `patterns` | readonly | `Patterns` | `Patterns` | `<Patterns>` |
| `placedItems` | readonly | `PlacedItems` | `PlacedItems` | `<PlacedItems>` |
| `pluginItems` | readonly | `PluginItems` | `PluginItems` | `<PluginItems>` |
| `printTiles` | readonly | `boolean` | `boolean` | `false` |
| `radialRepeatItems` | readonly | `RadialRepeatItems` | `RadialRepeatItems` | `<RadialRepeatItems>` |
| `rasterEffectSettings` | readwrite | `RasterEffectOptions` | `RasterEffectOptions` | `<RasterEffectOptions>` |
| `rasterItems` | readonly | `RasterItems` | `RasterItems` | `<RasterItems>` |
| `rulerOrigin` | readwrite | `point` | `Array(2)` | `[-850, -200]` |
| `rulerUnits` | readonly | `Object` | `RulerUnits.Points` | `<RulerUnits.Points>` |
| `saved` | readwrite | `boolean` | `boolean` | `false` |
| `scaleFactor` | readonly | `number` | `number` | `1` |
| `selection` | readwrite | `array` | `Array(0)` | `[]` |
| `showPlacedImages` | readonly | `boolean` | `boolean` | `false` |
| `splitLongPaths` | readonly | `boolean` | `boolean` | `false` |
| `spots` | readonly | `Spots` | `Spots` | `<Spots>` |
| `stationery` | readonly | `boolean` | `boolean` | `false` |
| `stories` | readonly | `Stories` | `Stories` | `<Stories>` |
| `swatchGroups` | readonly | `SwatchGroups` | `SwatchGroups` | `<SwatchGroups>` |
| `swatches` | readonly | `Swatches` | `Swatches` | `<Swatches>` |
| `symbolItems` | readonly | `SymbolItems` | `SymbolItems` | `<SymbolItems>` |
| `symbols` | readonly | `Symbols` | `Symbols` | `<Symbols>` |
| `symmetryRepeatItems` | readonly | `SymmetryRepeatItems` | `SymmetryRepeatItems` | `<SymmetryRepeatItems>` |
| `tags` | readonly | `Tags` | `Tags` | `<Tags>` |
| `textFrames` | readonly | `TextFrames` | `TextFrames` | `<TextFrames>` |
| `tileFullPages` | readonly | `boolean` | `boolean` | `false` |
| `typename` | readonly | `string` | `string` | `Document` |
| `useDefaultScreen` | readonly | `boolean` | `boolean` | `true` |
| `variables` | readonly | `Variables` | `Variables` | `<Variables>` |
| `variablesLocked` | readwrite | `boolean` | `boolean` | `false` |
| `views` | readonly | `Views` | `Views` | `<Views>` |
| `visibleBounds` | readonly | `rect` | `Array(4)` | `[48.5, 550.5, 660, 73.904]` |
| `width` | readonly | `number` | `number` | `400` |

方法：`activate()`, `close()`, `convertCoordinate()`, `convertToLargeCanvas()`, `exitIsolationMode()`, `exitSymbolIsolationMode()`, `exportAsFormat()`, `exportFile()`, `exportForScreens()`, `exportPDFPreset()`, `exportPerspectiveGridPreset()`, `exportPrintPreset()`, `exportSelectionAsAi()`, `exportSelectionAsPNG()`, `exportVariables()`, `fitArtboardToSelectedArt()`, `generateMockup()`, `getAllBrushes()`, `getDeHighlightedObjectsCount()`, `getDocumentInfoAsText()`, `getHighlightedObjectsCount()`, `getPageItemFromUuid()`, `getPerspectiveActivePlane()`, `getPreviewMode()`, `getScreenMode()`, `getSelSymbolSetProps()`, `getSelectedBrushes()`, `getSymbol9SliceGrid()`, `getViewMode()`, `hidePerspectiveGrid()`, `imageCapture()`, `importCharacterStyles()`, `importFile()`, `importPDFPreset()`, `importParagraphStyles()`, `importPerspectiveGridPreset()`, `importPrintPreset()`, `importVariables()`, `isGridVisible()`, `isGuideVisible()`, `isOutlineMode()`, `isPixelGridVisible()`, `isRulerVisible()`, `isSmartEditMode()`, `isSnapToGridEnabled()`, `isSymbolGuidesEnabled()`, `isSymbolIsolationModeActive()`, `isTransparencyGridVisible()`, `isTrimViewEnabled()`, `isViewClippedToArtboards()`, `isolateSelectedArt()`, `numSelectedSymbol()`, `openSymbolInIsolationMode()`, `print()`, `processGesture()`, `rasterize()`, `rearrangeArtboards()`, `requestPlaceCloudPSD()`, `save()`, `saveAs()`, `saveToCloud()`, `selectGraphicStyle()`, `selectObjectsOnActiveArtboard()`, `selectPerspectivePreset()`, `selectSymbol()`, `setPerspectiveActivePlane()`, `setSymbol9SliceGrid()`, `showPerspectiveGrid()`, `windowCapture()`, `writeAsLibrary()`

## Artboard (`Artboard`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `artboardRect` | readwrite | `rect` | `Array(4)` | `[0, 600, 800, 0]` |
| `name` | readwrite | `string` | `string` | `Main Artboard` |
| `parent` | readonly | `Document` | `Document` | `<Document>` |
| `rulerOrigin` | readwrite | `point` | `Array(2)` | `[0, 0]` |
| `rulerPAR` | readwrite | `number` | `number` | `1` |
| `showCenter` | readwrite | `boolean` | `boolean` | `false` |
| `showCrossHairs` | readwrite | `boolean` | `boolean` | `false` |
| `showSafeAreas` | readwrite | `boolean` | `boolean` | `false` |
| `typename` | readonly | `string` | `string` | `Artboard` |

方法：`canChangeRulerCoordinateSystem()`, `isVideoRulerVisible()`, `remove()`

## Layer (`Layer`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `absoluteZOrderPosition` | readonly | `number` | `number` | `19` |
| `artworkKnockout` | readwrite | `Object` | `KnockoutState.INHERITED` | `<KnockoutState.INHERITED>` |
| `blendingMode` | readwrite | `Object` | `BlendModes.NORMAL` | `<BlendModes.NORMAL>` |
| `color` | readwrite | `RGBColor` | `RGBColor` | `<RGBColor>` |
| `compoundPathItems` | readonly | `CompoundPathItems` | `CompoundPathItems` | `<CompoundPathItems>` |
| `dimPlacedImages` | readwrite | `boolean` | `boolean` | `false` |
| `graphItems` | readonly | `GraphItems` | `GraphItems` | `<GraphItems>` |
| `gridRepeatItems` | readonly | `GridRepeatItems` | `GridRepeatItems` | `<GridRepeatItems>` |
| `groupItems` | readonly | `GroupItems` | `GroupItems` | `<GroupItems>` |
| `hasSelectedArtwork` | readwrite | `boolean` | `boolean` | `false` |
| `isIsolated` | readwrite | `boolean` | `boolean` | `false` |
| `layers` | readonly | `Layers` | `Layers` | `<Layers>` |
| `legacyTextItems` | readonly | `LegacyTextItems` | `LegacyTextItems` | `<LegacyTextItems>` |
| `locked` | readwrite | `boolean` | `boolean` | `false` |
| `meshItems` | readonly | `MeshItems` | `MeshItems` | `<MeshItems>` |
| `name` | readwrite | `string` | `string` | `Vector Layer` |
| `nonNativeItems` | readonly | `NonNativeItems` | `NonNativeItems` | `<NonNativeItems>` |
| `opacity` | readwrite | `number` | `number` | `100` |
| `pageItems` | readonly | `PageItems` | `PageItems` | `<PageItems>` |
| `parent` | readonly | `Document` | `Document` | `<Document>` |
| `pathItems` | readonly | `PathItems` | `PathItems` | `<PathItems>` |
| `placedItems` | readonly | `PlacedItems` | `PlacedItems` | `<PlacedItems>` |
| `pluginItems` | readonly | `PluginItems` | `PluginItems` | `<PluginItems>` |
| `preview` | readwrite | `boolean` | `boolean` | `true` |
| `printable` | readwrite | `boolean` | `boolean` | `true` |
| `radialRepeatItems` | readonly | `RadialRepeatItems` | `RadialRepeatItems` | `<RadialRepeatItems>` |
| `rasterItems` | readonly | `RasterItems` | `RasterItems` | `<RasterItems>` |
| `sliced` | readwrite | `boolean` | `boolean` | `false` |
| `symbolItems` | readonly | `SymbolItems` | `SymbolItems` | `<SymbolItems>` |
| `symmetryRepeatItems` | readonly | `SymmetryRepeatItems` | `SymmetryRepeatItems` | `<SymmetryRepeatItems>` |
| `textFrames` | readonly | `TextFrames` | `TextFrames` | `<TextFrames>` |
| `typename` | readonly | `string` | `string` | `Layer` |
| `visible` | readwrite | `boolean` | `boolean` | `true` |
| `zOrderPosition` | readonly | `number` | `number` | `1` |

方法：`move()`, `remove()`, `zOrder()`

## PathItem (`PathItem`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `absoluteZOrderPosition` | readonly | `number` | `number` | `2` |
| `area` | readonly | `number` | `number` | `24000` |
| `artworkKnockout` | readwrite | `Object` | `KnockoutState.DISABLED` | `<KnockoutState.DISABLED>` |
| `blendingMode` | readwrite | `Object` | `BlendModes.MULTIPLY` | `<BlendModes.MULTIPLY>` |
| `clipping` | readwrite | `boolean` | `boolean` | `false` |
| `closed` | readwrite | `boolean` | `boolean` | `true` |
| `controlBounds` | readonly | `rect` | `Array(4)` | `[48.5, 501.5, 251.5, 378.5]` |
| `editable` | readonly | `boolean` | `boolean` | `true` |
| `evenodd` | readwrite | `boolean` | `boolean` | `false` |
| `fillColor` | readwrite | `RGBColor` | `RGBColor` | `<RGBColor>` |
| `fillOverprint` | readwrite | `boolean` | `boolean` | `false` |
| `filled` | readwrite | `boolean` | `boolean` | `true` |
| `geometricBounds` | readonly | `rect` | `Array(4)` | `[50, 500, 250, 380]` |
| `guides` | readwrite | `boolean` | `boolean` | `false` |
| `height` | readwrite | `number` | `number` | `120` |
| `hidden` | readwrite | `boolean` | `boolean` | `false` |
| `isIsolated` | readwrite | `boolean` | `boolean` | `false` |
| `layer` | readonly | `Layer` | `Layer` | `<Layer>` |
| `left` | readwrite | `number` | `number` | `48.5` |
| `length` | readonly | `number` | `number` | `640` |
| `locked` | readwrite | `boolean` | `boolean` | `false` |
| `name` | readwrite | `string` | `string` | `SampleRect` |
| `note` | readwrite | `string` | `string` | `` |
| `opacity` | readwrite | `number` | `number` | `85` |
| `parent` | readonly | `Layer` | `Layer` | `<Layer>` |
| `pathPoints` | readonly | `PathPoints` | `PathPoints` | `<PathPoints>` |
| `pixelAligned` | readwrite | `boolean` | `boolean` | `false` |
| `polarity` | readwrite | `Object` | `PolarityValues.POSITIVE` | `<PolarityValues.POSITIVE>` |
| `position` | readwrite | `point` | `Array(2)` | `[50, 500]` |
| `resolution` | readwrite | `number` | `number` | `800` |
| `selected` | readwrite | `boolean` | `boolean` | `false` |
| `selectedPathPoints` | readonly | `array` | `Array(0)` | `[]` |
| `sliced` | readwrite | `boolean` | `boolean` | `false` |
| `strokeCap` | readwrite | `Object` | `StrokeCap.ROUNDENDCAP` | `<StrokeCap.ROUNDENDCAP>` |
| `strokeColor` | readwrite | `RGBColor` | `RGBColor` | `<RGBColor>` |
| `strokeDashOffset` | readwrite | `number` | `number` | `2` |
| `strokeDashes` | readwrite | `array` | `Array(2)` | `[8, 4]` |
| `strokeJoin` | readwrite | `Object` | `StrokeJoin.ROUNDENDJOIN` | `<StrokeJoin.ROUNDENDJOIN>` |
| `strokeMiterLimit` | readwrite | `number` | `number` | `4` |
| `strokeOverprint` | readwrite | `boolean` | `boolean` | `false` |
| `strokeWidth` | readwrite | `number` | `number` | `3` |
| `stroked` | readwrite | `boolean` | `boolean` | `true` |
| `tags` | readonly | `Tags` | `Tags` | `<Tags>` |
| `top` | readwrite | `number` | `number` | `501.5` |
| `typename` | readonly | `string` | `string` | `PathItem` |
| `uRL` | readwrite | `string` | `string` | `` |
| `uuid` | readonly | `string` | `string` | `513` |
| `visibilityVariable` | readwrite | `null` | `null` | `null` |
| `visibleBounds` | readonly | `rect` | `Array(4)` | `[48.5, 501.5, 251.5, 378.5]` |
| `width` | readwrite | `number` | `number` | `200` |
| `wrapInside` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapOffset` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapped` | readwrite | `boolean` | `boolean` | `false` |
| `zOrderPosition` | readonly | `number` | `number` | `4` |

方法：`applyEffect()`, `bringInPerspective()`, `canParallelExecutionRun()`, `convertAnchorToCorner()`, `convertAnchorToSmooth()`, `duplicate()`, `isAnchorPointCorner()`, `move()`, `remove()`, `resize()`, `rotate()`, `sendScriptMessage()`, `setEntirePath()`, `setSmootheningPercentageValue()`, `transform()`, `translate()`, `zOrder()`

## PathPoint (`PathPoint`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `anchor` | readwrite | `point` | `Array(2)` | `[100, 300]` |
| `leftDirection` | readwrite | `point` | `Array(2)` | `[100, 300]` |
| `parent` | readonly | `PathItem` | `PathItem` | `<PathItem>` |
| `pointType` | readwrite | `Object` | `PointType.SMOOTH` | `<PointType.SMOOTH>` |
| `rightDirection` | readwrite | `point` | `Array(2)` | `[140, 340]` |
| `selected` | readwrite | `Object` | `PathPointSelection.NOSELECTION` | `<PathPointSelection.NOSELECTION>` |
| `typename` | readonly | `string` | `string` | `PathPoint` |

方法：`remove()`

## CompoundPathItem (`CompoundPathItem`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `absoluteZOrderPosition` | readonly | `number` | `number` | `6` |
| `artworkKnockout` | readwrite | `Object` | `KnockoutState.DISABLED` | `<KnockoutState.DISABLED>` |
| `blendingMode` | readwrite | `Object` | `BlendModes.NORMAL` | `<BlendModes.NORMAL>` |
| `controlBounds` | readonly | `rect` | `Array(4)` | `[299.5, 450.5, 440.5, 309.5]` |
| `editable` | readonly | `boolean` | `boolean` | `true` |
| `geometricBounds` | readonly | `rect` | `Array(4)` | `[300, 450, 440, 310]` |
| `height` | readwrite | `number` | `number` | `140` |
| `hidden` | readwrite | `boolean` | `boolean` | `false` |
| `isIsolated` | readwrite | `boolean` | `boolean` | `false` |
| `layer` | readonly | `Layer` | `Layer` | `<Layer>` |
| `left` | readwrite | `number` | `number` | `299.5` |
| `locked` | readwrite | `boolean` | `boolean` | `false` |
| `name` | readwrite | `string` | `string` | `SampleCompound` |
| `note` | readwrite | `string` | `string` | `` |
| `opacity` | readwrite | `number` | `number` | `100` |
| `parent` | readonly | `Layer` | `Layer` | `<Layer>` |
| `pathItems` | readonly | `PathItems` | `PathItems` | `<PathItems>` |
| `pixelAligned` | readwrite | `boolean` | `boolean` | `false` |
| `position` | readwrite | `point` | `Array(2)` | `[300, 450]` |
| `selected` | readwrite | `boolean` | `boolean` | `false` |
| `sliced` | readwrite | `boolean` | `boolean` | `false` |
| `tags` | readonly | `Tags` | `Tags` | `<Tags>` |
| `top` | readwrite | `number` | `number` | `450.5` |
| `typename` | readonly | `string` | `string` | `CompoundPathItem` |
| `uRL` | readwrite | `string` | `string` | `` |
| `uuid` | readonly | `string` | `string` | `515` |
| `visibilityVariable` | readwrite | `null` | `null` | `null` |
| `visibleBounds` | readonly | `rect` | `Array(4)` | `[299.5, 450.5, 440.5, 309.5]` |
| `width` | readwrite | `number` | `number` | `140` |
| `wrapInside` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapOffset` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapped` | readwrite | `boolean` | `boolean` | `false` |
| `zOrderPosition` | readonly | `number` | `number` | `12` |

方法：`applyEffect()`, `bringInPerspective()`, `canParallelExecutionRun()`, `duplicate()`, `move()`, `remove()`, `resize()`, `rotate()`, `sendScriptMessage()`, `transform()`, `translate()`, `zOrder()`

## GroupItem (`GroupItem`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `absoluteZOrderPosition` | readonly | `number` | `number` | `8` |
| `artworkKnockout` | readwrite | `Object` | `KnockoutState.INHERITED` | `<KnockoutState.INHERITED>` |
| `blendingMode` | readwrite | `Object` | `BlendModes.SCREEN` | `<BlendModes.SCREEN>` |
| `clipped` | readwrite | `boolean` | `boolean` | `false` |
| `compoundPathItems` | readonly | `CompoundPathItems` | `CompoundPathItems` | `<CompoundPathItems>` |
| `controlBounds` | readonly | `rect` | `Array(4)` | `[299.5, 300.5, 380.5, 219.5]` |
| `editable` | readonly | `boolean` | `boolean` | `true` |
| `geometricBounds` | readonly | `rect` | `Array(4)` | `[300, 300, 380, 220]` |
| `graphItems` | readonly | `GraphItems` | `GraphItems` | `<GraphItems>` |
| `gridRepeatItems` | readonly | `GridRepeatItems` | `GridRepeatItems` | `<GridRepeatItems>` |
| `groupItems` | readonly | `GroupItems` | `GroupItems` | `<GroupItems>` |
| `height` | readwrite | `number` | `number` | `80` |
| `hidden` | readwrite | `boolean` | `boolean` | `false` |
| `isIsolated` | readwrite | `boolean` | `boolean` | `false` |
| `layer` | readonly | `Layer` | `Layer` | `<Layer>` |
| `left` | readwrite | `number` | `number` | `299.5` |
| `legacyTextItems` | readonly | `LegacyTextItems` | `LegacyTextItems` | `<LegacyTextItems>` |
| `locked` | readwrite | `boolean` | `boolean` | `false` |
| `meshItems` | readonly | `MeshItems` | `MeshItems` | `<MeshItems>` |
| `name` | readwrite | `string` | `string` | `SampleGroup` |
| `nonNativeItems` | readonly | `NonNativeItems` | `NonNativeItems` | `<NonNativeItems>` |
| `note` | readwrite | `string` | `string` | `` |
| `opacity` | readwrite | `number` | `number` | `90` |
| `pageItems` | readonly | `PageItems` | `PageItems` | `<PageItems>` |
| `parent` | readonly | `Layer` | `Layer` | `<Layer>` |
| `pathItems` | readonly | `PathItems` | `PathItems` | `<PathItems>` |
| `pixelAligned` | readwrite | `boolean` | `boolean` | `true` |
| `placedItems` | readonly | `PlacedItems` | `PlacedItems` | `<PlacedItems>` |
| `pluginItems` | readonly | `PluginItems` | `PluginItems` | `<PluginItems>` |
| `position` | readwrite | `point` | `Array(2)` | `[300, 300]` |
| `radialRepeatItems` | readonly | `RadialRepeatItems` | `RadialRepeatItems` | `<RadialRepeatItems>` |
| `rasterItems` | readonly | `RasterItems` | `RasterItems` | `<RasterItems>` |
| `selected` | readwrite | `boolean` | `boolean` | `false` |
| `sliced` | readwrite | `boolean` | `boolean` | `false` |
| `symbolItems` | readonly | `SymbolItems` | `SymbolItems` | `<SymbolItems>` |
| `symmetryRepeatItems` | readonly | `SymmetryRepeatItems` | `SymmetryRepeatItems` | `<SymmetryRepeatItems>` |
| `tags` | readonly | `Tags` | `Tags` | `<Tags>` |
| `textFrames` | readonly | `TextFrames` | `TextFrames` | `<TextFrames>` |
| `top` | readwrite | `number` | `number` | `300.5` |
| `typename` | readonly | `string` | `string` | `GroupItem` |
| `uRL` | readwrite | `string` | `string` | `` |
| `uuid` | readonly | `string` | `string` | `518` |
| `visibilityVariable` | readwrite | `null` | `null` | `null` |
| `visibleBounds` | readonly | `rect` | `Array(4)` | `[299.5, 300.5, 380.5, 219.5]` |
| `width` | readwrite | `number` | `number` | `80` |
| `wrapInside` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapOffset` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapped` | readwrite | `boolean` | `boolean` | `false` |
| `zOrderPosition` | readonly | `number` | `number` | `11` |

方法：`applyEffect()`, `bringInPerspective()`, `canParallelExecutionRun()`, `duplicate()`, `isDimensionArt()`, `move()`, `remove()`, `resize()`, `rotate()`, `sendScriptMessage()`, `transform()`, `translate()`, `zOrder()`

## GroupItem (clipping mask) (`GroupItem`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `absoluteZOrderPosition` | readonly | `number` | `number` | `11` |
| `artworkKnockout` | readwrite | `Object` | `KnockoutState.INHERITED` | `<KnockoutState.INHERITED>` |
| `blendingMode` | readwrite | `Object` | `BlendModes.NORMAL` | `<BlendModes.NORMAL>` |
| `clipped` | readwrite | `boolean` | `boolean` | `true` |
| `compoundPathItems` | readonly | `CompoundPathItems` | `CompoundPathItems` | `<CompoundPathItems>` |
| `controlBounds` | readonly | `rect` | `Array(4)` | `[419.5, 260.5, 520.5, 159.5]` |
| `editable` | readonly | `boolean` | `boolean` | `true` |
| `geometricBounds` | readonly | `rect` | `Array(4)` | `[420, 260, 520, 160]` |
| `graphItems` | readonly | `GraphItems` | `GraphItems` | `<GraphItems>` |
| `gridRepeatItems` | readonly | `GridRepeatItems` | `GridRepeatItems` | `<GridRepeatItems>` |
| `groupItems` | readonly | `GroupItems` | `GroupItems` | `<GroupItems>` |
| `height` | readwrite | `number` | `number` | `100` |
| `hidden` | readwrite | `boolean` | `boolean` | `false` |
| `isIsolated` | readwrite | `boolean` | `boolean` | `false` |
| `layer` | readonly | `Layer` | `Layer` | `<Layer>` |
| `left` | readwrite | `number` | `number` | `419.5` |
| `legacyTextItems` | readonly | `LegacyTextItems` | `LegacyTextItems` | `<LegacyTextItems>` |
| `locked` | readwrite | `boolean` | `boolean` | `false` |
| `meshItems` | readonly | `MeshItems` | `MeshItems` | `<MeshItems>` |
| `name` | readwrite | `string` | `string` | `SampleClippingGroup` |
| `nonNativeItems` | readonly | `NonNativeItems` | `NonNativeItems` | `<NonNativeItems>` |
| `note` | readwrite | `string` | `string` | `` |
| `opacity` | readwrite | `number` | `number` | `100` |
| `pageItems` | readonly | `PageItems` | `PageItems` | `<PageItems>` |
| `parent` | readonly | `Layer` | `Layer` | `<Layer>` |
| `pathItems` | readonly | `PathItems` | `PathItems` | `<PathItems>` |
| `pixelAligned` | readwrite | `boolean` | `boolean` | `false` |
| `placedItems` | readonly | `PlacedItems` | `PlacedItems` | `<PlacedItems>` |
| `pluginItems` | readonly | `PluginItems` | `PluginItems` | `<PluginItems>` |
| `position` | readwrite | `point` | `Array(2)` | `[420, 260]` |
| `radialRepeatItems` | readonly | `RadialRepeatItems` | `RadialRepeatItems` | `<RadialRepeatItems>` |
| `rasterItems` | readonly | `RasterItems` | `RasterItems` | `<RasterItems>` |
| `selected` | readwrite | `boolean` | `boolean` | `false` |
| `sliced` | readwrite | `boolean` | `boolean` | `false` |
| `symbolItems` | readonly | `SymbolItems` | `SymbolItems` | `<SymbolItems>` |
| `symmetryRepeatItems` | readonly | `SymmetryRepeatItems` | `SymmetryRepeatItems` | `<SymmetryRepeatItems>` |
| `tags` | readonly | `Tags` | `Tags` | `<Tags>` |
| `textFrames` | readonly | `TextFrames` | `TextFrames` | `<TextFrames>` |
| `top` | readwrite | `number` | `number` | `260.5` |
| `typename` | readonly | `string` | `string` | `GroupItem` |
| `uRL` | readwrite | `string` | `string` | `` |
| `uuid` | readonly | `string` | `string` | `520` |
| `visibilityVariable` | readwrite | `null` | `null` | `null` |
| `visibleBounds` | readonly | `rect` | `Array(4)` | `[419.5, 260.5, 520.5, 159.5]` |
| `width` | readwrite | `number` | `number` | `100` |
| `wrapInside` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapOffset` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapped` | readwrite | `boolean` | `boolean` | `false` |
| `zOrderPosition` | readonly | `number` | `number` | `12` |

方法：`applyEffect()`, `bringInPerspective()`, `canParallelExecutionRun()`, `duplicate()`, `isDimensionArt()`, `move()`, `remove()`, `resize()`, `rotate()`, `sendScriptMessage()`, `transform()`, `translate()`, `zOrder()`

## TextFrame (point text) (`TextFrame`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `absoluteZOrderPosition` | readonly | `number` | `number` | `13` |
| `anchor` | readwrite | `point` | `Array(2)` | `[50, 80]` |
| `antialias` | readwrite | `Object` | `TextAntialias.SHARP` | `<TextAntialias.SHARP>` |
| `artworkKnockout` | readwrite | `Object` | `KnockoutState.DISABLED` | `<KnockoutState.DISABLED>` |
| `blendingMode` | readwrite | `Object` | `BlendModes.NORMAL` | `<BlendModes.NORMAL>` |
| `characters` | readonly | `Characters` | `Characters` | `<Characters>` |
| `columnCount` | readwrite | `number` | `number` | `1` |
| `columnGutter` | readwrite | `number` | `number` | `0` |
| `contentVariable` | readwrite | `undefined` | `undefined` | `null` |
| `contents` | readwrite | `string` | `string` | `Hello BAE` |
| `controlBounds` | readonly | `rect` | `Array(4)` | `[49, 102.12, 163.223, 72.904]` |
| `editable` | readonly | `boolean` | `boolean` | `true` |
| `endTValue` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `firstBaseline` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `firstBaselineMin` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `flowLinksHorizontally` | readwrite | `boolean` | `boolean` | `true` |
| `geometricBounds` | readonly | `rect` | `Array(4)` | `[50, 101.12, 150.847, 73.904]` |
| `height` | readwrite | `number` | `number` | `27.2158203125` |
| `hidden` | readwrite | `boolean` | `boolean` | `false` |
| `insertionPoints` | readonly | `InsertionPoints` | `InsertionPoints` | `<InsertionPoints>` |
| `isIsolated` | readwrite | `boolean` | `boolean` | `false` |
| `kind` | readonly | `Object` | `TextType.POINTTEXT` | `<TextType.POINTTEXT>` |
| `layer` | readonly | `Layer` | `Layer` | `<Layer>` |
| `left` | readwrite | `number` | `number` | `50` |
| `lines` | readonly | `Lines` | `Lines` | `<Lines>` |
| `locked` | readwrite | `boolean` | `boolean` | `false` |
| `matrix` | readonly | `Matrix` | `Matrix` | `<Matrix>` |
| `name` | readwrite | `string` | `string` | `SamplePointText` |
| `nextFrame` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `note` | readwrite | `string` | `string` | `` |
| `opacity` | readwrite | `number` | `number` | `100` |
| `opticalAlignment` | readwrite | `boolean` | `boolean` | `false` |
| `orientation` | readwrite | `Object` | `TextOrientation.HORIZONTAL` | `<TextOrientation.HORIZONTAL>` |
| `paragraphs` | readonly | `Paragraphs` | `Paragraphs` | `<Paragraphs>` |
| `parent` | readonly | `Layer` | `Layer` | `<Layer>` |
| `pixelAligned` | readwrite | `boolean` | `boolean` | `true` |
| `position` | readwrite | `point` | `Array(2)` | `[50, 101.12]` |
| `previousFrame` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `rowCount` | readwrite | `number` | `number` | `1` |
| `rowGutter` | readwrite | `number` | `number` | `0` |
| `selected` | readwrite | `boolean` | `boolean` | `false` |
| `sliced` | readwrite | `boolean` | `boolean` | `false` |
| `spacing` | readwrite | `number` | `number` | `0` |
| `startTValue` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `story` | readonly | `Story` | `Story` | `<Story>` |
| `tags` | readonly | `Tags` | `Tags` | `<Tags>` |
| `textPath` | readonly | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `textRange` | readonly | `TextRange` | `TextRange` | `<TextRange>` |
| `textRanges` | readonly | `TextRanges` | `TextRanges` | `<TextRanges>` |
| `textSelection` | readonly | `array` | `Array(0)` | `[]` |
| `top` | readwrite | `number` | `number` | `101.1201171875` |
| `typename` | readonly | `string` | `string` | `TextFrame` |
| `uRL` | readwrite | `string` | `string` | `` |
| `uuid` | readonly | `string` | `string` | `524` |
| `visibilityVariable` | readwrite | `null` | `null` | `null` |
| `visibleBounds` | readonly | `rect` | `Array(4)` | `[50, 101.12, 150.847, 73.904]` |
| `width` | readwrite | `number` | `number` | `100.8466796875` |
| `words` | readonly | `Words` | `Words` | `<Words>` |
| `wrapInside` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapOffset` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapped` | readwrite | `boolean` | `boolean` | `false` |
| `zOrderPosition` | readonly | `number` | `number` | `10` |

方法：`applyEffect()`, `bringInPerspective()`, `canParallelExecutionRun()`, `convertAreaObjectToPointObject()`, `convertPointObjectToAreaObject()`, `createOutline()`, `duplicate()`, `generateThumbnailWithTextFrameProperties()`, `move()`, `remove()`, `resize()`, `rotate()`, `sendScriptMessage()`, `transform()`, `translate()`, `zOrder()`

## TextFrame (area text) (`TextFrame`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `absoluteZOrderPosition` | readonly | `number` | `number` | `14` |
| `anchor` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `antialias` | readwrite | `Object` | `TextAntialias.SHARP` | `<TextAntialias.SHARP>` |
| `artworkKnockout` | readwrite | `Object` | `KnockoutState.DISABLED` | `<KnockoutState.DISABLED>` |
| `blendingMode` | readwrite | `Object` | `BlendModes.NORMAL` | `<BlendModes.NORMAL>` |
| `characters` | readonly | `Characters` | `Characters` | `<Characters>` |
| `columnCount` | readwrite | `number` | `number` | `1` |
| `columnGutter` | readwrite | `number` | `number` | `18` |
| `contentVariable` | readwrite | `undefined` | `undefined` | `null` |
| `contents` | readwrite | `string` | `string` | `Area text sample` |
| `controlBounds` | readonly | `rect` | `Array(4)` | `[294, 186, 461, 99]` |
| `editable` | readonly | `boolean` | `boolean` | `true` |
| `endTValue` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `firstBaseline` | readwrite | `Object` | `FirstBaselineType.BASELINEEMBOXHEIGHT` | `<FirstBaselineType.BASELINEEMBOXHEIGHT>` |
| `firstBaselineMin` | readwrite | `number` | `number` | `0` |
| `flowLinksHorizontally` | readwrite | `boolean` | `boolean` | `true` |
| `geometricBounds` | readonly | `rect` | `Array(4)` | `[300, 180, 460, 100]` |
| `height` | readwrite | `number` | `number` | `80` |
| `hidden` | readwrite | `boolean` | `boolean` | `false` |
| `insertionPoints` | readonly | `InsertionPoints` | `InsertionPoints` | `<InsertionPoints>` |
| `isIsolated` | readwrite | `boolean` | `boolean` | `false` |
| `kind` | readonly | `Object` | `TextType.AREATEXT` | `<TextType.AREATEXT>` |
| `layer` | readonly | `Layer` | `Layer` | `<Layer>` |
| `left` | readwrite | `number` | `number` | `300` |
| `lines` | readonly | `Lines` | `Lines` | `<Lines>` |
| `locked` | readwrite | `boolean` | `boolean` | `false` |
| `matrix` | readonly | `Matrix` | `Matrix` | `<Matrix>` |
| `name` | readwrite | `string` | `string` | `SampleAreaText` |
| `nextFrame` | readwrite | `TextFrame` | `TextFrame` | `<TextFrame>` |
| `note` | readwrite | `string` | `string` | `` |
| `opacity` | readwrite | `number` | `number` | `100` |
| `opticalAlignment` | readwrite | `boolean` | `boolean` | `false` |
| `orientation` | readwrite | `Object` | `TextOrientation.HORIZONTAL` | `<TextOrientation.HORIZONTAL>` |
| `paragraphs` | readonly | `Paragraphs` | `Paragraphs` | `<Paragraphs>` |
| `parent` | readonly | `Layer` | `Layer` | `<Layer>` |
| `pixelAligned` | readwrite | `boolean` | `boolean` | `true` |
| `position` | readwrite | `point` | `Array(2)` | `[300, 180]` |
| `previousFrame` | readwrite | `null` | `null` | `null` |
| `rowCount` | readwrite | `number` | `number` | `1` |
| `rowGutter` | readwrite | `number` | `number` | `18` |
| `selected` | readwrite | `boolean` | `boolean` | `false` |
| `sliced` | readwrite | `boolean` | `boolean` | `false` |
| `spacing` | readwrite | `number` | `number` | `0` |
| `startTValue` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `story` | readonly | `Story` | `Story` | `<Story>` |
| `tags` | readonly | `Tags` | `Tags` | `<Tags>` |
| `textPath` | readonly | `TextPath` | `TextPath` | `<TextPath>` |
| `textRange` | readonly | `TextRange` | `TextRange` | `<TextRange>` |
| `textRanges` | readonly | `TextRanges` | `TextRanges` | `<TextRanges>` |
| `textSelection` | readonly | `array` | `Array(0)` | `[]` |
| `top` | readwrite | `number` | `number` | `180` |
| `typename` | readonly | `string` | `string` | `TextFrame` |
| `uRL` | readwrite | `string` | `string` | `` |
| `uuid` | readonly | `string` | `string` | `526` |
| `visibilityVariable` | readwrite | `null` | `null` | `null` |
| `visibleBounds` | readonly | `rect` | `Array(4)` | `[300, 180, 460, 100]` |
| `width` | readwrite | `number` | `number` | `160` |
| `words` | readonly | `Words` | `Words` | `<Words>` |
| `wrapInside` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapOffset` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapped` | readwrite | `boolean` | `boolean` | `false` |
| `zOrderPosition` | readonly | `number` | `number` | `11` |

方法：`applyEffect()`, `bringInPerspective()`, `canParallelExecutionRun()`, `convertAreaObjectToPointObject()`, `convertPointObjectToAreaObject()`, `createOutline()`, `duplicate()`, `generateThumbnailWithTextFrameProperties()`, `move()`, `remove()`, `resize()`, `rotate()`, `sendScriptMessage()`, `transform()`, `translate()`, `zOrder()`

## TextFrame (path text) (`TextFrame`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `absoluteZOrderPosition` | readonly | `number` | `number` | `15` |
| `anchor` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `antialias` | readwrite | `Object` | `TextAntialias.SHARP` | `<TextAntialias.SHARP>` |
| `artworkKnockout` | readwrite | `Object` | `KnockoutState.DISABLED` | `<KnockoutState.DISABLED>` |
| `blendingMode` | readwrite | `Object` | `BlendModes.NORMAL` | `<BlendModes.NORMAL>` |
| `characters` | readonly | `Characters` | `Characters` | `<Characters>` |
| `columnCount` | readwrite | `number` | `number` | `1` |
| `columnGutter` | readwrite | `number` | `number` | `0` |
| `contentVariable` | readwrite | `undefined` | `undefined` | `null` |
| `contents` | readwrite | `string` | `string` | `Path text sample` |
| `controlBounds` | readonly | `rect` | `Array(4)` | `[486.905, 161.582, 660.885, 86.521]` |
| `editable` | readonly | `boolean` | `boolean` | `true` |
| `endTValue` | readwrite | `number` | `number` | `0.99000000953674` |
| `firstBaseline` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `firstBaselineMin` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `flowLinksHorizontally` | readwrite | `boolean` | `boolean` | `true` |
| `geometricBounds` | readonly | `rect` | `Array(4)` | `[500, 140, 650, 100]` |
| `height` | readwrite | `number` | `number` | `40` |
| `hidden` | readwrite | `boolean` | `boolean` | `false` |
| `insertionPoints` | readonly | `InsertionPoints` | `InsertionPoints` | `<InsertionPoints>` |
| `isIsolated` | readwrite | `boolean` | `boolean` | `false` |
| `kind` | readonly | `Object` | `TextType.PATHTEXT` | `<TextType.PATHTEXT>` |
| `layer` | readonly | `Layer` | `Layer` | `<Layer>` |
| `left` | readwrite | `number` | `number` | `497.283203125` |
| `lines` | readonly | `Lines` | `Lines` | `<Lines>` |
| `locked` | readwrite | `boolean` | `boolean` | `false` |
| `matrix` | readonly | `Matrix` | `Matrix` | `<Matrix>` |
| `name` | readwrite | `string` | `string` | `SamplePathText` |
| `nextFrame` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `note` | readwrite | `string` | `string` | `` |
| `opacity` | readwrite | `number` | `number` | `100` |
| `opticalAlignment` | readwrite | `boolean` | `boolean` | `false` |
| `orientation` | readwrite | `Object` | `TextOrientation.HORIZONTAL` | `<TextOrientation.HORIZONTAL>` |
| `paragraphs` | readonly | `Paragraphs` | `Paragraphs` | `<Paragraphs>` |
| `parent` | readonly | `Layer` | `Layer` | `<Layer>` |
| `pixelAligned` | readwrite | `boolean` | `boolean` | `true` |
| `position` | readwrite | `point` | `Array(2)` | `[500, 140]` |
| `previousFrame` | readwrite | `any` | `unreadable` | *(不可用: `text_frame_kind`)* |
| `rowCount` | readwrite | `number` | `number` | `1` |
| `rowGutter` | readwrite | `number` | `number` | `0` |
| `selected` | readwrite | `boolean` | `boolean` | `false` |
| `sliced` | readwrite | `boolean` | `boolean` | `false` |
| `spacing` | readwrite | `number` | `number` | `0` |
| `startTValue` | readwrite | `number` | `number` | `0` |
| `story` | readonly | `Story` | `Story` | `<Story>` |
| `tags` | readonly | `Tags` | `Tags` | `<Tags>` |
| `textPath` | readonly | `TextPath` | `TextPath` | `<TextPath>` |
| `textRange` | readonly | `TextRange` | `TextRange` | `<TextRange>` |
| `textRanges` | readonly | `TextRanges` | `TextRanges` | `<TextRanges>` |
| `textSelection` | readonly | `array` | `Array(0)` | `[]` |
| `top` | readwrite | `number` | `number` | `140` |
| `typename` | readonly | `string` | `string` | `TextFrame` |
| `uRL` | readwrite | `string` | `string` | `` |
| `uuid` | readonly | `string` | `string` | `528` |
| `visibilityVariable` | readwrite | `null` | `null` | `null` |
| `visibleBounds` | readonly | `rect` | `Array(4)` | `[497.283, 140, 650, 97.057]` |
| `width` | readwrite | `number` | `number` | `150` |
| `words` | readonly | `Words` | `Words` | `<Words>` |
| `wrapInside` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapOffset` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapped` | readwrite | `boolean` | `boolean` | `false` |
| `zOrderPosition` | readonly | `number` | `number` | `12` |

方法：`applyEffect()`, `bringInPerspective()`, `canParallelExecutionRun()`, `convertAreaObjectToPointObject()`, `convertPointObjectToAreaObject()`, `createOutline()`, `duplicate()`, `generateThumbnailWithTextFrameProperties()`, `move()`, `remove()`, `resize()`, `rotate()`, `sendScriptMessage()`, `transform()`, `translate()`, `zOrder()`

## TextRange (`TextRange`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `characterAttributes` | readonly | `CharacterAttributes` | `CharacterAttributes` | `<CharacterAttributes>` |
| `characterOffset` | readwrite | `number` | `number` | `1` |
| `characterStyles` | readonly | `CharacterStyles` | `CharacterStyles` | `<CharacterStyles>` |
| `characters` | readonly | `Characters` | `Characters` | `<Characters>` |
| `contents` | readwrite | `string` | `string` | `Hello BAE` |
| `end` | readwrite | `number` | `number` | `9` |
| `insertionPoints` | readonly | `InsertionPoints` | `InsertionPoints` | `<InsertionPoints>` |
| `kerning` | readwrite | `any` | `unreadable` | *(不可用: `no_manual_kerning`)* |
| `length` | readwrite | `number` | `number` | `9` |
| `lines` | readonly | `Lines` | `Lines` | `<Lines>` |
| `paragraphAttributes` | readonly | `ParagraphAttributes` | `ParagraphAttributes` | `<ParagraphAttributes>` |
| `paragraphStyles` | readonly | `ParagraphStyles` | `ParagraphStyles` | `<ParagraphStyles>` |
| `paragraphs` | readonly | `Paragraphs` | `Paragraphs` | `<Paragraphs>` |
| `start` | readwrite | `number` | `number` | `0` |
| `story` | readonly | `Story` | `Story` | `<Story>` |
| `textRanges` | readonly | `TextRanges` | `TextRanges` | `<TextRanges>` |
| `textSelection` | readonly | `array` | `Array(0)` | `[]` |
| `words` | readonly | `Words` | `Words` | `<Words>` |

方法：`changeCaseTo()`, `deSelect()`, `duplicate()`, `getParagraphLength()`, `getTextRunLength()`, `move()`, `remove()`, `select()`

## CharacterAttributes (`CharacterAttributes`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `Tsume` | readwrite | `number` | `number` | `0` |
| `akiLeft` | readwrite | `number` | `number` | `-1` |
| `akiRight` | readwrite | `number` | `number` | `-1` |
| `alignment` | readwrite | `Object` | `StyleRunAlignmentType.ROMANBASELINE` | `<StyleRunAlignmentType.ROMANBASELINE>` |
| `alternateGlyphs` | readwrite | `Object` | `AlternateGlyphsForm.DEFAULTFORM` | `<AlternateGlyphsForm.DEFAULTFORM>` |
| `autoLeading` | readwrite | `boolean` | `boolean` | `true` |
| `baselineDirection` | readwrite | `Object` | `BaselineDirectionType.Standard` | `<BaselineDirectionType.Standard>` |
| `baselinePosition` | readwrite | `Object` | `FontBaselineOption.NORMALBASELINE` | `<FontBaselineOption.NORMALBASELINE>` |
| `baselineShift` | readwrite | `number` | `number` | `0` |
| `capitalization` | readwrite | `Object` | `FontCapsOption.NORMALCAPS` | `<FontCapsOption.NORMALCAPS>` |
| `connectionForms` | readwrite | `boolean` | `boolean` | `true` |
| `contextualLigature` | readwrite | `boolean` | `boolean` | `true` |
| `diacVPos` | readwrite | `Object` | `DiacVPosType.DEFAULT_POSITION` | `<DiacVPosType.DEFAULT_POSITION>` |
| `diacXOffset` | readwrite | `number` | `number` | `0` |
| `diacYOffset` | readwrite | `number` | `number` | `0` |
| `digitSet` | readwrite | `Object` | `DigitSetType.DEFAULT_DIGITS` | `<DigitSetType.DEFAULT_DIGITS>` |
| `dirOverride` | readwrite | `Object` | `DirOverrideType.DEFAULT_DIRECTION` | `<DirOverrideType.DEFAULT_DIRECTION>` |
| `discretionaryLigature` | readwrite | `boolean` | `boolean` | `false` |
| `figureStyle` | readwrite | `Object` | `FigureStyleType.DEFAULTFIGURESTYLE` | `<FigureStyleType.DEFAULTFIGURESTYLE>` |
| `fillColor` | readwrite | `RGBColor` | `RGBColor` | `<RGBColor>` |
| `fractions` | readwrite | `boolean` | `boolean` | `false` |
| `horizontalScale` | readwrite | `number` | `number` | `100` |
| `italics` | readwrite | `boolean` | `boolean` | `false` |
| `justificationAlternates` | readwrite | `boolean` | `boolean` | `false` |
| `kana` | readwrite | `boolean` | `boolean` | `false` |
| `kashidas` | readwrite | `Object` | `KashidasType.DEFAULT_KASHIDAS` | `<KashidasType.DEFAULT_KASHIDAS>` |
| `kerningMethod` | readwrite | `Object` | `AutoKernType.AUTO` | `<AutoKernType.AUTO>` |
| `language` | readwrite | `Object` | `LanguageType.ENGLISH` | `<LanguageType.ENGLISH>` |
| `leading` | readwrite | `number` | `number` | `28.8000011444092` |
| `ligature` | readwrite | `boolean` | `boolean` | `true` |
| `noBreak` | readwrite | `boolean` | `boolean` | `false` |
| `openTypePosition` | readwrite | `Object` | `FontOpenTypePositionOption.OPENTYPEDEFAULT` | `<FontOpenTypePositionOption.OPENTYPEDEFAULT>` |
| `ordinals` | readwrite | `boolean` | `boolean` | `false` |
| `ornaments` | readwrite | `boolean` | `boolean` | `false` |
| `overprintFill` | readwrite | `boolean` | `boolean` | `false` |
| `overprintStroke` | readwrite | `boolean` | `boolean` | `false` |
| `parent` | readonly | `Story` | `Story` | `<Story>` |
| `proportionalMetrics` | readwrite | `boolean` | `boolean` | `false` |
| `rotation` | readwrite | `number` | `number` | `0` |
| `size` | readwrite | `number` | `number` | `24` |
| `strikeThrough` | readwrite | `boolean` | `boolean` | `false` |
| `strokeColor` | readwrite | `NoColor` | `NoColor` | `<NoColor>` |
| `strokeWeight` | readwrite | `number` | `number` | `1` |
| `stylisticAlternates` | readwrite | `boolean` | `boolean` | `false` |
| `stylisticSets` | readwrite | `number` | `number` | `0` |
| `swash` | readwrite | `boolean` | `boolean` | `false` |
| `tateChuYokoHorizontal` | readwrite | `number` | `number` | `0` |
| `tateChuYokoVertical` | readwrite | `number` | `number` | `0` |
| `textFont` | readwrite | `TextFont` | `TextFont` | `<redacted:TextFont>` |
| `titling` | readwrite | `boolean` | `boolean` | `false` |
| `tracking` | readwrite | `number` | `number` | `0` |
| `typename` | readonly | `string` | `string` | `CharacterAttributes` |
| `underline` | readwrite | `boolean` | `boolean` | `false` |
| `verticalScale` | readwrite | `number` | `number` | `100` |
| `wariChuCharactersAfterBreak` | readwrite | `number` | `number` | `2` |
| `wariChuCharactersBeforeBreak` | readwrite | `number` | `number` | `2` |
| `wariChuEnabled` | readwrite | `boolean` | `boolean` | `false` |
| `wariChuJustification` | readwrite | `Object` | `WariChuJustificationType.WARICHUAUTOJUSTIFY` | `<WariChuJustificationType.WARICHUAUTOJUSTIFY>` |
| `wariChuLineGap` | readwrite | `number` | `number` | `0` |
| `wariChuLines` | readwrite | `number` | `number` | `2` |
| `wariChuScale` | readwrite | `number` | `number` | `50` |

## ParagraphAttributes (`ParagraphAttributes`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `autoLeadingAmount` | readwrite | `number` | `number` | `120.000004768372` |
| `bunriKinshi` | readwrite | `boolean` | `boolean` | `false` |
| `burasagariType` | readwrite | `Object` | `BurasagariTypeEnum.None` | `<BurasagariTypeEnum.None>` |
| `composerEngine` | readwrite | `Object` | `ComposerEngineType.latinCJKComposer` | `<ComposerEngineType.latinCJKComposer>` |
| `desiredGlyphScaling` | readwrite | `number` | `number` | `100` |
| `desiredLetterSpacing` | readwrite | `number` | `number` | `0` |
| `desiredWordSpacing` | readwrite | `number` | `number` | `100` |
| `everyLineComposer` | readwrite | `boolean` | `boolean` | `false` |
| `firstLineIndent` | readwrite | `number` | `number` | `0` |
| `hyphenateCapitalizedWords` | readwrite | `boolean` | `boolean` | `true` |
| `hyphenation` | readwrite | `boolean` | `boolean` | `false` |
| `hyphenationPreference` | readwrite | `number` | `number` | `0.5` |
| `hyphenationZone` | readwrite | `number` | `number` | `36` |
| `justification` | readwrite | `Object` | `Justification.LEFT` | `<Justification.LEFT>` |
| `kashidaWidth` | readwrite | `Object` | `KashidaWidthType.kashidaMedium` | `<KashidaWidthType.kashidaMedium>` |
| `kinsoku` | readwrite | `any` | `unreadable` | *(不可用: `undefined_for_text_range`)* |
| `kinsokuOrder` | readwrite | `Object` | `KinsokuOrderEnum.PUSHIN` | `<KinsokuOrderEnum.PUSHIN>` |
| `kurikaeshiMojiShori` | readwrite | `boolean` | `boolean` | `false` |
| `leadingType` | readwrite | `Object` | `AutoLeadingType.BOTTOMTOBOTTOM` | `<AutoLeadingType.BOTTOMTOBOTTOM>` |
| `leftIndent` | readwrite | `number` | `number` | `0` |
| `listStyle` | readwrite | `ListStyle` | `ListStyle` | `<ListStyle>` |
| `listStyleTier` | readwrite | `number` | `number` | `0` |
| `maximumConsecutiveHyphens` | readwrite | `number` | `number` | `0` |
| `maximumGlyphScaling` | readwrite | `number` | `number` | `100` |
| `maximumLetterSpacing` | readwrite | `number` | `number` | `0` |
| `maximumWordSpacing` | readwrite | `number` | `number` | `133.000004291534` |
| `minimumAfterHyphen` | readwrite | `number` | `number` | `2` |
| `minimumBeforeHyphen` | readwrite | `number` | `number` | `2` |
| `minimumGlyphScaling` | readwrite | `number` | `number` | `100` |
| `minimumHyphenatedWordSize` | readwrite | `number` | `number` | `6` |
| `minimumLetterSpacing` | readwrite | `number` | `number` | `0` |
| `minimumWordSpacing` | readwrite | `number` | `number` | `80.0000011920929` |
| `mojikumi` | readwrite | `any` | `unreadable` | *(不可用: `undefined_for_text_range`)* |
| `paragraphDirection` | readwrite | `Object` | `ParagraphDirectionType.LEFT_TO_RIGHT_DIRECTION` | `<ParagraphDirectionType.LEFT_TO_RIGHT_DIRECTION>` |
| `parent` | readonly | `Story` | `Story` | `<Story>` |
| `rightIndent` | readwrite | `number` | `number` | `0` |
| `romanHanging` | readwrite | `boolean` | `boolean` | `false` |
| `singleWordJustification` | readwrite | `Object` | `Justification.FULLJUSTIFY` | `<Justification.FULLJUSTIFY>` |
| `spaceAfter` | readwrite | `number` | `number` | `0` |
| `spaceBefore` | readwrite | `number` | `number` | `0` |
| `tabStops` | readwrite | `array` | `Array(0)` | `[]` |
| `typename` | readonly | `string` | `string` | `ParagraphAttributes` |

## Gradient (`Gradient`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `gradientStops` | readonly | `GradientStops` | `GradientStops` | `<GradientStops>` |
| `name` | readwrite | `string` | `string` | `SampleLinearGradient` |
| `parent` | readonly | `Document` | `Document` | `<Document>` |
| `type` | readwrite | `Object` | `GradientType.LINEAR` | `<GradientType.LINEAR>` |
| `typename` | readonly | `string` | `string` | `Gradient` |

方法：`remove()`

## GradientStop (`GradientStop`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `color` | readwrite | `RGBColor` | `RGBColor` | `<RGBColor>` |
| `midPoint` | readwrite | `number` | `number` | `50` |
| `opacity` | readwrite | `number` | `number` | `100` |
| `parent` | readonly | `Gradient` | `Gradient` | `<Gradient>` |
| `rampPoint` | readwrite | `number` | `number` | `0` |
| `typename` | readonly | `string` | `string` | `GradientStop` |

方法：`remove()`

## GradientColor (linear) (`GradientColor`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `angle` | readwrite | `number` | `number` | `45` |
| `gradient` | readwrite | `Gradient` | `Gradient` | `<Gradient>` |
| `hiliteAngle` | readwrite | `number` | `number` | `0` |
| `hiliteLength` | readwrite | `number` | `number` | `0` |
| `length` | readwrite | `number` | `number` | `120` |
| `matrix` | readwrite | `Matrix` | `Matrix` | `<Matrix>` |
| `origin` | readwrite | `50,150` | `Array(2)` | `[50, 150]` |
| `typename` | readonly | `string` | `string` | `GradientColor` |

## GradientColor (radial) (`GradientColor`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `angle` | readwrite | `number` | `number` | `0` |
| `gradient` | readwrite | `Gradient` | `Gradient` | `<Gradient>` |
| `hiliteAngle` | readwrite | `number` | `number` | `30` |
| `hiliteLength` | readwrite | `number` | `number` | `25` |
| `length` | readwrite | `number` | `number` | `60` |
| `matrix` | readwrite | `Matrix` | `Matrix` | `<Matrix>` |
| `origin` | readwrite | `240,160` | `Array(2)` | `[240, 160]` |
| `typename` | readonly | `string` | `string` | `GradientColor` |

## RGBColor (`RGBColor`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `blue` | readwrite | `number` | `number` | `220` |
| `green` | readwrite | `number` | `number` | `128` |
| `red` | readwrite | `number` | `number` | `42` |
| `typename` | readonly | `string` | `string` | `RGBColor` |

## CMYKColor (`CMYKColor`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `black` | readwrite | `number` | `number` | `5` |
| `cyan` | readwrite | `number` | `number` | `10` |
| `magenta` | readwrite | `number` | `number` | `40` |
| `typename` | readonly | `string` | `string` | `CMYKColor` |
| `yellow` | readwrite | `number` | `number` | `80` |

## GrayColor (`GrayColor`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `gray` | readwrite | `number` | `number` | `50` |
| `typename` | readonly | `string` | `string` | `GrayColor` |

## Spot (`Spot`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `color` | readwrite | `RGBColor` | `RGBColor` | `<RGBColor>` |
| `colorType` | readwrite | `Object` | `ColorModel.SPOT` | `<ColorModel.SPOT>` |
| `name` | readwrite | `string` | `string` | `Sample Spot` |
| `parent` | readonly | `Document` | `Document` | `<Document>` |
| `spotKind` | readonly | `Object` | `SpotColorKind.SPOTRGB` | `<SpotColorKind.SPOTRGB>` |
| `typename` | readonly | `string` | `string` | `Spot` |

方法：`getInternalColor()`, `remove()`

## SpotColor (`SpotColor`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `spot` | readwrite | `Spot` | `Spot` | `<Spot>` |
| `tint` | readwrite | `number` | `number` | `75` |
| `typename` | readonly | `string` | `string` | `SpotColor` |

## NoColor (`NoColor`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `typename` | readonly | `string` | `string` | `NoColor` |

## PatternColor (`PatternColor`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|

## Pattern (`Pattern`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `name` | readwrite | `string` | `string` | `小白花` |
| `parent` | readonly | `Document` | `Document` | `<Document>` |
| `typename` | readonly | `string` | `string` | `Pattern` |

方法：`remove()`

## Symbol (`Symbol`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `name` | readwrite | `string` | `string` | `SampleSymbol` |
| `parent` | readonly | `Document` | `Document` | `<Document>` |
| `typename` | readonly | `string` | `string` | `Symbol` |

方法：`duplicate()`, `remove()`

## SymbolItem (`SymbolItem`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `absoluteZOrderPosition` | readonly | `number` | `number` | `17` |
| `artworkKnockout` | readwrite | `Object` | `KnockoutState.INHERITED` | `<KnockoutState.INHERITED>` |
| `blendingMode` | readwrite | `Object` | `BlendModes.NORMAL` | `<BlendModes.NORMAL>` |
| `controlBounds` | readonly | `rect` | `Array(4)` | `[378.211, 321.628, 421.211, 278.628]` |
| `editable` | readonly | `boolean` | `boolean` | `true` |
| `geometricBounds` | readonly | `rect` | `Array(4)` | `[379.211, 320.628, 420.211, 279.628]` |
| `height` | readwrite | `number` | `number` | `41` |
| `hidden` | readwrite | `boolean` | `boolean` | `false` |
| `isIsolated` | readwrite | `boolean` | `boolean` | `false` |
| `layer` | readonly | `Layer` | `Layer` | `<Layer>` |
| `left` | readwrite | `number` | `number` | `379.210622710622` |
| `locked` | readwrite | `boolean` | `boolean` | `false` |
| `name` | readwrite | `string` | `string` | `SampleSymbolInstance` |
| `note` | readwrite | `string` | `string` | `` |
| `opacity` | readwrite | `number` | `number` | `100` |
| `parent` | readonly | `Layer` | `Layer` | `<Layer>` |
| `pixelAligned` | readwrite | `boolean` | `boolean` | `false` |
| `position` | readwrite | `point` | `Array(2)` | `[379.211, 320.628]` |
| `selected` | readwrite | `boolean` | `boolean` | `false` |
| `sliced` | readwrite | `boolean` | `boolean` | `false` |
| `symbol` | readwrite | `Symbol` | `Symbol` | `<Symbol>` |
| `tags` | readonly | `Tags` | `Tags` | `<Tags>` |
| `top` | readwrite | `number` | `number` | `320.628205128205` |
| `typename` | readonly | `string` | `string` | `SymbolItem` |
| `uRL` | readwrite | `string` | `string` | `` |
| `uuid` | readonly | `string` | `string` | `535` |
| `visibilityVariable` | readwrite | `null` | `null` | `null` |
| `visibleBounds` | readonly | `rect` | `Array(4)` | `[379.211, 320.628, 420.211, 279.628]` |
| `width` | readwrite | `number` | `number` | `41.0000000000009` |
| `wrapInside` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapOffset` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapped` | readwrite | `boolean` | `boolean` | `false` |
| `zOrderPosition` | readonly | `number` | `number` | `12` |

方法：`applyEffect()`, `breakLink()`, `bringInPerspective()`, `canParallelExecutionRun()`, `duplicate()`, `move()`, `remove()`, `resize()`, `rotate()`, `sendScriptMessage()`, `transform()`, `translate()`, `zOrder()`

## Swatch (`Swatch`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `color` | readwrite | `NoColor` | `NoColor` | `<NoColor>` |
| `name` | readwrite | `string` | `string` | `[无]` |
| `parent` | readonly | `Document` | `Document` | `<Document>` |
| `typename` | readonly | `string` | `string` | `Swatch` |

方法：`remove()`

## SwatchGroup (`SwatchGroup`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `name` | readwrite | `string` | `string` | `` |
| `parent` | readonly | `Document` | `Document` | `<Document>` |
| `typename` | readonly | `string` | `string` | `SwatchGroup` |

方法：`addSpot()`, `addSwatch()`, `getAllSwatches()`, `remove()`

## GraphicStyle (`GraphicStyle`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `name` | readwrite | `string` | `string` | `[默认]` |
| `parent` | readonly | `Document` | `Document` | `<Document>` |
| `typename` | readonly | `string` | `string` | `GraphicStyle` |

方法：`applyTo()`, `mergeTo()`, `remove()`

## Brush (`Brush`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `name` | readwrite | `string` | `string` | `3 点圆形` |
| `parent` | readonly | `Document` | `Document` | `<Document>` |
| `typename` | readonly | `string` | `string` | `Brush` |

方法：`applyTo()`

## RasterItem (`RasterItem`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `absoluteZOrderPosition` | readonly | `number` | `number` | `18` |
| `artworkKnockout` | readwrite | `Object` | `KnockoutState.DISABLED` | `<KnockoutState.DISABLED>` |
| `bitsPerChannel` | readonly | `number` | `number` | `8` |
| `blendingMode` | readwrite | `Object` | `BlendModes.NORMAL` | `<BlendModes.NORMAL>` |
| `boundingBox` | readwrite | `rect` | `Array(4)` | `[0, 40, 40, 0]` |
| `channels` | readonly | `number` | `number` | `4` |
| `colorants` | readonly | `array` | `Array(3)` | `["Red", "Green", "Blue"]` |
| `colorizedGrayscale` | readonly | `boolean` | `boolean` | `false` |
| `contentVariable` | readwrite | `undefined` | `undefined` | `null` |
| `controlBounds` | readonly | `rect` | `Array(4)` | `[619, 551, 661, 509]` |
| `editable` | readonly | `boolean` | `boolean` | `true` |
| `embedded` | readwrite | `boolean` | `boolean` | `true` |
| `file` | readonly | `any` | `unreadable` | *(不可用: `no_associated_file`)* |
| `geometricBounds` | readonly | `rect` | `Array(4)` | `[620, 550, 660, 510]` |
| `height` | readwrite | `number` | `number` | `40` |
| `hidden` | readwrite | `boolean` | `boolean` | `false` |
| `imageColorSpace` | readonly | `Object` | `ImageColorSpace.RGB` | `<ImageColorSpace.RGB>` |
| `isIsolated` | readwrite | `boolean` | `boolean` | `false` |
| `layer` | readonly | `Layer` | `Layer` | `<Layer>` |
| `left` | readwrite | `number` | `number` | `620` |
| `locked` | readwrite | `boolean` | `boolean` | `false` |
| `matrix` | readwrite | `Matrix` | `Matrix` | `<Matrix>` |
| `name` | readwrite | `string` | `string` | `` |
| `note` | readwrite | `string` | `string` | `` |
| `opacity` | readwrite | `number` | `number` | `100` |
| `overprint` | readwrite | `boolean` | `boolean` | `false` |
| `parent` | readonly | `Layer` | `Layer` | `<Layer>` |
| `pixelAligned` | readwrite | `boolean` | `boolean` | `true` |
| `position` | readwrite | `point` | `Array(2)` | `[620, 550]` |
| `selected` | readwrite | `boolean` | `boolean` | `false` |
| `sliced` | readwrite | `boolean` | `boolean` | `false` |
| `status` | readonly | `Object` | `RasterLinkState.DATAFROMFILE` | `<RasterLinkState.DATAFROMFILE>` |
| `tags` | readonly | `Tags` | `Tags` | `<Tags>` |
| `top` | readwrite | `number` | `number` | `550` |
| `transparent` | readonly | `boolean` | `boolean` | `true` |
| `typename` | readonly | `string` | `string` | `RasterItem` |
| `uRL` | readwrite | `string` | `string` | `` |
| `uuid` | readonly | `string` | `string` | `537` |
| `visibilityVariable` | readwrite | `null` | `null` | `null` |
| `visibleBounds` | readonly | `rect` | `Array(4)` | `[620, 550, 660, 510]` |
| `width` | readwrite | `number` | `number` | `40` |
| `wrapInside` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapOffset` | readwrite | `any` | `unreadable` | *(不可用: `wrapped_items_only`)* |
| `wrapped` | readwrite | `boolean` | `boolean` | `false` |
| `zOrderPosition` | readonly | `number` | `number` | `12` |

方法：`applyEffect()`, `bringInPerspective()`, `canParallelExecutionRun()`, `colorize()`, `duplicate()`, `move()`, `remove()`, `resize()`, `rotate()`, `sendScriptMessage()`, `trace()`, `transform()`, `translate()`, `zOrder()`

## Matrix (`Matrix`)

| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |
|---|---|---|---|---|
| `mValueA` | readwrite | `number` | `number` | `1` |
| `mValueB` | readwrite | `number` | `number` | `0` |
| `mValueC` | readwrite | `number` | `number` | `0` |
| `mValueD` | readwrite | `number` | `number` | `1` |
| `mValueTX` | readwrite | `number` | `number` | `0` |
| `mValueTY` | readwrite | `number` | `number` | `0` |
| `typename` | readonly | `string` | `string` | `Matrix` |
