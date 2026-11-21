# After Effects 效果目录总表

> 来源：Adobe After Effects 26.0x67 (zh_CN) | 平台：Windows 11 Pro (10.0.26200, x64) | 提取日期：2026-11-21

## 统计摘要

- `app.effects` 总条目：749
- Adobe 内置效果（`ADBE` / `APC` / `PSL` / `VISINF` / `Mettle` / `Pseudo/ADBE`）：362
- Adobe 随附效果（CycoreFX HD `CC`/`CS`、Keylight、ProEXR、Cineware、Mocha AE）：83
- 成功展开参数的效果数：439（共 5637 个叶子参数，其中 940 个含下拉选项列表）
- 因模态框或图层类型限制跳过的自带效果数：6
- 本机另有 304 个第三方插件，不在本数据范围内

## 分类文件索引

| 分类（AE 界面名） | 文件 | 效果数 |
|---|---|---:|
| 3D 通道 | `effects/3d-channel.json` | 8 |
| 音频 | `effects/audio.json` | 13 |
| 模糊和锐化 | `effects/blur-and-sharpen.json` | 16 |
| 通道 | `effects/channel.json` | 13 |
| Cinema 4D | `effects/cinema-4d.json` | 1 |
| 颜色校正 | `effects/color-correction.json` | 40 |
| 扭曲 | `effects/distort.json` | 37 |
| 表达式控制 | `effects/expression-controls.json` | 8 |
| 生成 | `effects/generate.json` | 26 |
| (Hidden / Animation Preset Pseudo-Effects) | `effects/hidden-presets.json` | 107 |
| 沉浸式视频 | `effects/immersive-video.json` | 12 |
| 抠像 | `effects/keying.json` | 11 |
| 遮罩 | `effects/matte.json` | 4 |
| Boris FX Mocha | `effects/mocha-ae.json` | 1 |
| 杂色和颗粒 | `effects/noise-and-grain.json` | 12 |
| 过时 | `effects/obsolete.json` | 10 |
| _Obsolete | `effects/obsolete-hidden.json` | 39 |
| 透视 | `effects/perspective.json` | 10 |
| 模拟 | `effects/simulation.json` | 18 |
| 风格化 | `effects/stylize.json` | 25 |
| 文本 | `effects/text.json` | 2 |
| 时间 | `effects/time.json` | 8 |
| 过渡 | `effects/transition.json` | 17 |
| 实用工具 | `effects/utility.json` | 7 |

## Adobe 自带及随附效果总表

| 分类 | 名称 | matchName | 来源 | 参数数 |
|---|---|---|---|---:|
| 3D 通道 | 3D 通道提取 | `ADBE AUX CHANNEL EXTRACT` | adobe-builtin | 8 |
| 3D 通道 | 场深度 | `ADBE DEPTH FIELD` | adobe-builtin | 6 |
| 3D 通道 | 深度遮罩 | `ADBE DEPTH MATTE` | adobe-builtin | 5 |
| 3D 通道 | 雾 3D | `ADBE FOG_3D` | adobe-builtin | 10 |
| 3D 通道 | ID 遮罩 | `ADBE ID MATTE` | adobe-builtin | 7 |
| 3D 通道 | Cryptomatte | `Cryptomatte` | adobe-bundled | 5 |
| 3D 通道 | EXtractoR | `EXtractoR` | adobe-bundled | 8 |
| 3D 通道 | IDentifier | `IDentifier` | adobe-bundled | 5 |
| 音频 | 低音和高音 | `ADBE Aud BT` | adobe-builtin | 4 |
| 音频 | 压缩程序 | `ADBE Aud Compressor` | adobe-builtin | 10 |
| 音频 | 延迟 | `ADBE Aud Delay` | adobe-builtin | 7 |
| 音频 | 扭曲 | `ADBE Aud Distortion` | adobe-builtin | 13 |
| 音频 | 门 | `ADBE Aud Gate` | adobe-builtin | 6 |
| 音频 | 高通/低通 | `ADBE Aud HiLo` | adobe-builtin | 6 |
| 音频 | 调制器 | `ADBE Aud Modulator` | adobe-builtin | 6 |
| 音频 | 混响 | `ADBE Aud Reverb` | adobe-builtin | 8 |
| 音频 | 倒放 | `ADBE Aud Reverse` | adobe-builtin | 3 |
| 音频 | 立体声混合器 | `ADBE Aud Stereo Mixer` | adobe-builtin | 7 |
| 音频 | 音调 | `ADBE Aud Tone` | adobe-builtin | 9 |
| 音频 | 变调与合声 | `ADBE Aud_Flange` | adobe-builtin | 11 |
| 音频 | 参数均衡 | `ADBE Param EQ` | adobe-builtin | 15 |
| 模糊和锐化 | 双向模糊 | `ADBE Bilateral` | adobe-builtin | 5 |
| 模糊和锐化 | 快速方框模糊 | `ADBE Box Blur2` | adobe-builtin | 6 |
| 模糊和锐化 | 摄像机镜头模糊 | `ADBE Camera Lens Blur` | adobe-builtin | 24 |
| 模糊和锐化 | 摄像机抖动去模糊 | `ADBE CameraShakeDeblur` | adobe-builtin | 6 |
| 模糊和锐化 | 通道模糊 | `ADBE Channel Blur` | adobe-builtin | 8 |
| 模糊和锐化 | 复合模糊 | `ADBE Compound Blur` | adobe-builtin | 6 |
| 模糊和锐化 | 高斯模糊 | `ADBE Gaussian Blur 2` | adobe-builtin | 5 |
| 模糊和锐化 | 定向模糊 | `ADBE Motion Blur` | adobe-builtin | 4 |
| 模糊和锐化 | 径向模糊 | `ADBE Radial Blur` | adobe-builtin | 8 |
| 模糊和锐化 | 锐化 | `ADBE Sharpen` | adobe-builtin | 3 |
| 模糊和锐化 | 智能模糊 | `ADBE Smart Blur` | adobe-builtin | 5 |
| 模糊和锐化 | 钝化蒙版 | `ADBE Unsharp Mask2` | adobe-builtin | 6 |
| 模糊和锐化 | CC Radial Blur | `CC Radial Blur` | adobe-bundled | 6 |
| 模糊和锐化 | CC Radial Fast Blur | `CC Radial Fast Blur` | adobe-bundled | 5 |
| 模糊和锐化 | CC Vector Blur | `CC Vector Blur` | adobe-bundled | 9 |
| 模糊和锐化 | CC Cross Blur | `CS CrossBlur` | adobe-bundled | 6 |
| 通道 | 算术 | `ADBE Arithmetic` | adobe-builtin | 7 |
| 通道 | 混合 | `ADBE Blend` | adobe-builtin | 6 |
| 通道 | 计算 | `ADBE Calculations` | adobe-builtin | 15 |
| 通道 | 通道合成器 | `ADBE Channel Combiner` | adobe-builtin | 10 |
| 通道 | 复合运算 | `ADBE Compound Arithmetic` | adobe-builtin | 8 |
| 通道 | 反转 | `ADBE Invert` | adobe-builtin | 4 |
| 通道 | 最小/最大 | `ADBE Minimax` | adobe-builtin | 7 |
| 通道 | 移除颜色遮罩 | `ADBE Remove Color Matting` | adobe-builtin | 4 |
| 通道 | 设置通道 | `ADBE Set Channels` | adobe-builtin | 11 |
| 通道 | 设置遮罩 | `ADBE Set Matte3` | adobe-builtin | 8 |
| 通道 | 转换通道 | `ADBE Shift Channels` | adobe-builtin | 6 |
| 通道 | 固态层合成 | `ADBE Solid Composite` | adobe-builtin | 6 |
| 通道 | CC Composite | `CS Composite` | adobe-bundled | 6 |
| Cinema 4D | Cineware | `CINEMA 4D Effect` | adobe-bundled | *(跳过)* |
| 颜色校正 | 自动颜色 | `ADBE AutoColor` | adobe-builtin | 8 |
| 颜色校正 | 自动对比度 | `ADBE AutoContrast` | adobe-builtin | 7 |
| 颜色校正 | 自动色阶 | `ADBE AutoLevels` | adobe-builtin | 7 |
| 颜色校正 | 黑色和白色 | `ADBE Black&White` | adobe-builtin | 10 |
| 颜色校正 | 亮度和对比度 | `ADBE Brightness & Contrast 2` | adobe-builtin | 5 |
| 颜色校正 | 广播颜色 | `ADBE Broadcast Colors` | adobe-builtin | 5 |
| 颜色校正 | 通道混合器 | `ADBE CHANNEL MIXER` | adobe-builtin | 15 |
| 颜色校正 | 更改颜色 | `ADBE Change Color` | adobe-builtin | 11 |
| 颜色校正 | 更改为颜色 | `ADBE Change To Color` | adobe-builtin | 13 |
| 颜色校正 | 颜色平衡 (HLS) | `ADBE Color Balance (HLS)` | adobe-builtin | 5 |
| 颜色校正 | 颜色平衡 | `ADBE Color Balance 2` | adobe-builtin | 12 |
| 颜色校正 | 颜色链接 | `ADBE Color Link` | adobe-builtin | 8 |
| 颜色校正 | 曲线 | `ADBE CurvesCustom` | adobe-builtin | 4 |
| 颜色校正 | 颜色稳定器 | `ADBE Deflicker` | adobe-builtin | 7 |
| 颜色校正 | 视频限幅器 | `ADBE DigitalVideoLimiter` | adobe-builtin | 6 |
| 颜色校正 | 色阶 | `ADBE Easy Levels2` | adobe-builtin | 11 |
| 颜色校正 | 色调均化 | `ADBE Equalize` | adobe-builtin | 4 |
| 颜色校正 | 曝光度 | `ADBE Exposure2` | adobe-builtin | 24 |
| 颜色校正 | 灰度系数/基值/增益 | `ADBE Gamma/Pedestal/Gain2` | adobe-builtin | 12 |
| 颜色校正 | 色相/饱和度 | `ADBE HUE SATURATION` | adobe-builtin | 11 |
| 颜色校正 | 保留颜色 | `ADBE Leave Color` | adobe-builtin | 7 |
| 颜色校正 | Lumetri 颜色 | `ADBE Lumetri` | adobe-builtin | 132 |
| 颜色校正 | OCIO CDL 变换 | `ADBE OCIO CDL Transform` | adobe-builtin | 8 |
| 颜色校正 | OCIO 颜色空间变换 | `ADBE OCIO Color Space Transform` | adobe-builtin | 6 |
| 颜色校正 | OCIO 显示变换 | `ADBE OCIO Display Transform` | adobe-builtin | 8 |
| 颜色校正 | OCIO 文件转换 | `ADBE OCIO FILE Transform` | adobe-builtin | 7 |
| 颜色校正 | OCIO 外观变换 | `ADBE OCIO Look Transform` | adobe-builtin | 8 |
| 颜色校正 | PS 任意映射 | `ADBE PS Arbitrary Map` | adobe-builtin | 4 |
| 颜色校正 | 照片滤镜 | `ADBE PhotoFilterPS` | adobe-builtin | 6 |
| 颜色校正 | 色阶（单独控件） | `ADBE Pro Levels2` | adobe-builtin | 41 |
| 颜色校正 | 可选颜色 | `ADBE SelectiveColor` | adobe-builtin | 64 |
| 颜色校正 | 阴影/高光 | `ADBE ShadowHighlight` | adobe-builtin | 18 |
| 颜色校正 | 色调 | `ADBE Tint` | adobe-builtin | 6 |
| 颜色校正 | 三色调 | `ADBE Tritone` | adobe-builtin | 6 |
| 颜色校正 | 自然饱和度 | `ADBE Vibrance` | adobe-builtin | 4 |
| 颜色校正 | 色光 | `APC Colorama` | adobe-builtin | 32 |
| 颜色校正 | CC Color Offset | `CC Color Offset` | adobe-bundled | 6 |
| 颜色校正 | CC Toner | `CC Toner` | adobe-bundled | 9 |
| 颜色校正 | CC Color Neutralizer | `CS Color Neutralizer` | adobe-bundled | 27 |
| 颜色校正 | CC Kernel | `CS Kernel` | adobe-bundled | 20 |
| 扭曲 | 贝塞尔曲线变形 | `ADBE BEZMESH` | adobe-builtin | 15 |
| 扭曲 | 凸出 | `ADBE Bulge` | adobe-builtin | 9 |
| 扭曲 | 边角定位 | `ADBE Corner Pin` | adobe-builtin | 6 |
| 扭曲 | 置换图 | `ADBE Displacement Map` | adobe-builtin | 10 |
| 扭曲 | 变换 | `ADBE Geometry2` | adobe-builtin | 14 |
| 扭曲 | 液化 | `ADBE LIQUIFY` | adobe-builtin | 19 |
| 扭曲 | 网格变形 | `ADBE MESH WARP` | adobe-builtin | 6 |
| 扭曲 | 放大 | `ADBE Magnify` | adobe-builtin | 12 |
| 扭曲 | 镜像 | `ADBE Mirror` | adobe-builtin | 4 |
| 扭曲 | 偏移 | `ADBE Offset` | adobe-builtin | 4 |
| 扭曲 | 光学补偿 | `ADBE Optics Compensation` | adobe-builtin | 8 |
| 扭曲 | 极坐标 | `ADBE Polar Coordinates` | adobe-builtin | 4 |
| 扭曲 | 改变形状 | `ADBE RESHAPE` | adobe-builtin | 9 |
| 扭曲 | 波纹 | `ADBE Ripple` | adobe-builtin | 9 |
| 扭曲 | 果冻效应修复 | `ADBE Rolling Shutter` | adobe-builtin | 9 |
| 扭曲 | 漩涡条纹 | `ADBE SCHMEAR` | adobe-builtin | 10 |
| 扭曲 | 球面化 | `ADBE Spherize` | adobe-builtin | 4 |
| 扭曲 | 变形稳定器 | `ADBE SubspaceStabilizer` | adobe-builtin | 39 |
| 扭曲 | 湍流置换 | `ADBE Turbulent Displace` | adobe-builtin | 16 |
| 扭曲 | 旋转扭曲 | `ADBE Twirl` | adobe-builtin | 5 |
| 扭曲 | 保留细节放大 | `ADBE Upscale` | adobe-builtin | 8 |
| 扭曲 | 变形 | `ADBE WRPMESH` | adobe-builtin | 7 |
| 扭曲 | 波形变形 | `ADBE Wave Warp` | adobe-builtin | 10 |
| 扭曲 | CC Bend It | `CC Bend It` | adobe-bundled | 7 |
| 扭曲 | CC Bender | `CC Bender` | adobe-bundled | 7 |
| 扭曲 | CC Blobbylize | `CC Blobbylize` | adobe-bundled | 24 |
| 扭曲 | CC Flo Motion | `CC Flo Motion` | adobe-bundled | 10 |
| 扭曲 | CC Griddler | `CC Griddler` | adobe-bundled | 7 |
| 扭曲 | CC Lens | `CC Lens` | adobe-bundled | 5 |
| 扭曲 | CC Page Turn | `CC Page Turn` | adobe-bundled | 11 |
| 扭曲 | CC Power Pin | `CC Power Pin` | adobe-bundled | 14 |
| 扭曲 | CC Ripple Pulse | `CC Ripple Pulse` | adobe-bundled | 7 |
| 扭曲 | CC Slant | `CC Slant` | adobe-bundled | 8 |
| 扭曲 | CC Smear | `CC Smear` | adobe-bundled | 7 |
| 扭曲 | CC Split | `CC Split` | adobe-bundled | 5 |
| 扭曲 | CC Split 2 | `CC Split 2` | adobe-bundled | 6 |
| 扭曲 | CC Tiler | `CC Tiler` | adobe-bundled | 5 |
| 表达式控制 | 角度控制 | `ADBE Angle Control` | adobe-builtin | 3 |
| 表达式控制 | 复选框控制 | `ADBE Checkbox Control` | adobe-builtin | 3 |
| 表达式控制 | 颜色控制 | `ADBE Color Control` | adobe-builtin | 3 |
| 表达式控制 | 下拉菜单控件 | `ADBE Dropdown Control` | adobe-builtin | 3 |
| 表达式控制 | 图层控制 | `ADBE Layer Control` | adobe-builtin | 3 |
| 表达式控制 | 点控制 | `ADBE Point Control` | adobe-builtin | 3 |
| 表达式控制 | 3D 点控制 | `ADBE Point3D Control` | adobe-builtin | 3 |
| 表达式控制 | 滑块控制 | `ADBE Slider Control` | adobe-builtin | 3 |
| 生成 | 四色渐变 | `ADBE 4ColorGradient` | adobe-builtin | 16 |
| 生成 | 音频频谱 | `ADBE AudSpect` | adobe-builtin | 25 |
| 生成 | 音频波形 | `ADBE AudWave` | adobe-builtin | 18 |
| 生成 | 单元格图案 | `ADBE Cell Pattern` | adobe-builtin | 20 |
| 生成 | 棋盘 | `ADBE Checkerboard` | adobe-builtin | 14 |
| 生成 | 圆形 | `ADBE Circle` | adobe-builtin | 14 |
| 生成 | 椭圆 | `ADBE ELLIPSE` | adobe-builtin | 10 |
| 生成 | 吸管填充 | `ADBE Eyedropper Fill` | adobe-builtin | 7 |
| 生成 | 填充 | `ADBE Fill` | adobe-builtin | 9 |
| 生成 | 分形 | `ADBE Fractal` | adobe-builtin | 33 |
| 生成 | 网格 | `ADBE Grid` | adobe-builtin | 16 |
| 生成 | 光束 | `ADBE Laser` | adobe-builtin | 13 |
| 生成 | 镜头光晕 | `ADBE Lens Flare` | adobe-builtin | 6 |
| 生成 | 高级闪电 | `ADBE Lightning 2` | adobe-builtin | 32 |
| 生成 | 油漆桶 | `ADBE Paint Bucket` | adobe-builtin | 12 |
| 生成 | 梯度渐变 | `ADBE Ramp` | adobe-builtin | 10 |
| 生成 | 涂写 | `ADBE Scribble Fill` | adobe-builtin | 31 |
| 生成 | 描边 | `ADBE Stroke` | adobe-builtin | 13 |
| 生成 | 写入 | `ADBE Write-on` | adobe-builtin | 12 |
| 生成 | 无线电波 | `APC Radio Waves` | adobe-builtin | 45 |
| 生成 | 勾画 | `APC Vegas` | adobe-builtin | 36 |
| 生成 | CC Glue Gun | `CC Glue Gun` | adobe-bundled | 30 |
| 生成 | CC Light Burst 2.5 | `CC Light Burst 2.5` | adobe-bundled | 9 |
| 生成 | CC Light Rays | `CC Light Rays` | adobe-bundled | 12 |
| 生成 | CC Light Sweep | `CC Light Sweep` | adobe-bundled | 11 |
| 生成 | CC Threads | `CS Threads` | adobe-bundled | 10 |
| *(隐藏/预设)* | 追踪控制 | `ADBE CM Animated Shape 2` | adobe-builtin | 6 |
| *(隐藏/预设)* | 动画形状控制 | `ADBE CM Animated Shape 3` | adobe-builtin | 4 |
| *(隐藏/预设)* | 动画形状控制 | `ADBE CM Animated Shape Control` | adobe-builtin | 7 |
| *(隐藏/预设)* | 自动滚动 - 水平 | `ADBE CM AutoscrollHorizontal` | adobe-builtin | 3 |
| *(隐藏/预设)* | 自动滚动 - 垂直 | `ADBE CM AutoscrollVertical` | adobe-builtin | 3 |
| *(隐藏/预设)* | 边角显示 | `ADBE CM CornerReveal` | adobe-builtin | 6 |
| *(隐藏/预设)* | 碎砖块拼贴 | `ADBE CM CrackedTiles` | adobe-builtin | 4 |
| *(隐藏/预设)* | 裁切边缘 | `ADBE CM CropEdges` | adobe-builtin | 4 |
| *(隐藏/预设)* | 溶解 - 凝固 | `ADBE CM DissolveUnmelt` | adobe-builtin | 4 |
| *(隐藏/预设)* | 淡入淡出（帧） | `ADBE CM FadeInOutFrames` | adobe-builtin | 4 |
| *(隐藏/预设)* | 淡入淡出（毫秒） | `ADBE CM FadeInOutmsec` | adobe-builtin | 4 |
| *(隐藏/预设)* | 飞动嵌入 | `ADBE CM FlyToInset` | adobe-builtin | 7 |
| *(隐藏/预设)* | 网格擦除 | `ADBE CM GridWipe` | adobe-builtin | 6 |
| *(隐藏/预设)* | 插入视频 - 加框 | `ADBE CM InsetVideoFramed` | adobe-builtin | 5 |
| *(隐藏/预设)* | 插入视频 - 撕边 | `ADBE CM InsetVideoTorn` | adobe-builtin | 3 |
| *(隐藏/预设)* | 漏光 - 图层标记 | `ADBE CM LightLeaksMarkers` | adobe-builtin | 3 |
| *(隐藏/预设)* | 漏光 - 随机 | `ADBE CM LightLeaksRandom` | adobe-builtin | 4 |
| *(隐藏/预设)* | 情绪照明 - 无定形 | `ADBE CM MoodLightAmorph` | adobe-builtin | 5 |
| *(隐藏/预设)* | 情绪照明 - 数字化 | `ADBE CM MoodLightDigital` | adobe-builtin | 5 |
| *(隐藏/预设)* | 情绪照明 - 窄条光线 | `ADBE CM MoodLightStreaks` | adobe-builtin | 5 |
| *(隐藏/预设)* | 不透明闪烁 - 图层标记 | `ADBE CM OpacityFlashMarkers` | adobe-builtin | 3 |
| *(隐藏/预设)* | 不透明闪烁 - 随机 | `ADBE CM OpacityFlashRandom` | adobe-builtin | 4 |
| *(隐藏/预设)* | 缩放回弹 - 图层标记 | `ADBE CM ScaleBounceMarkers` | adobe-builtin | 4 |
| *(隐藏/预设)* | 缩放回弹 - 随机 | `ADBE CM ScaleBounceRandom` | adobe-builtin | 5 |
| *(隐藏/预设)* | 滑动 - 变量 | `ADBE CM SlideVariable` | adobe-builtin | 5 |
| *(隐藏/预设)* | 随时间旋转 | `ADBE CM Spin` | adobe-builtin | 3 |
| *(隐藏/预设)* | 随时间漂移 | `ADBE CM Throw` | adobe-builtin | 4 |
| *(隐藏/预设)* | 卡片擦除主控 | `ADBE CM TransCard` | adobe-builtin | 4 |
| *(隐藏/预设)* | 过渡主控 | `ADBE CM TransComplete` | adobe-builtin | 3 |
| *(隐藏/预设)* | 伸缩主控（边角） | `ADBE CM TransCorner` | adobe-builtin | 4 |
| *(隐藏/预设)* | 伸缩主控（边缘） | `ADBE CM TransDirection` | adobe-builtin | 4 |
| *(隐藏/预设)* | 溶解主控 | `ADBE CM TransDissolve` | adobe-builtin | 3 |
| *(隐藏/预设)* | 淡入淡出主控 | `ADBE CM TransFade` | adobe-builtin | 3 |
| *(隐藏/预设)* | 蒙版淡化控制 | `ADBE CM TransFadeMask` | adobe-builtin | 4 |
| *(隐藏/预设)* | 光圈擦除主控 | `ADBE CM TransIris` | adobe-builtin | 5 |
| *(隐藏/预设)* | 径向擦除主控 | `ADBE CM TransRadial` | adobe-builtin | 6 |
| *(隐藏/预设)* | 滑动主控 | `ADBE CM TransSlide` | adobe-builtin | 4 |
| *(隐藏/预设)* | 伸缩主控 | `ADBE CM TransStretch` | adobe-builtin | 3 |
| *(隐藏/预设)* | 擦除主控 | `ADBE CM TransWipe` | adobe-builtin | 3 |
| *(隐藏/预设)* | 擦除主控 | `ADBE CM TransWipeFeath` | adobe-builtin | 4 |
| *(隐藏/预设)* | 摆动 - 凝结 | `ADBE CM WiggleGelatin` | adobe-builtin | 4 |
| *(隐藏/预设)* | 摆动 - 位置 | `ADBE CM WigglePosition` | adobe-builtin | 4 |
| *(隐藏/预设)* | 摆动 - 旋转 | `ADBE CM WiggleRotation` | adobe-builtin | 4 |
| *(隐藏/预设)* | 摆动 - 缩放 | `ADBE CM WiggleScale` | adobe-builtin | 6 |
| *(隐藏/预设)* | 摆动 - 修剪 | `ADBE CM WiggleShear` | adobe-builtin | 4 |
| *(隐藏/预设)* | 摆动的罗摩 | `ADBE CM Wigglerama` | adobe-builtin | 9 |
| *(隐藏/预设)* | 缩放 - 2D 旋转 | `ADBE CM Zoom2DSpin` | adobe-builtin | 4 |
| *(隐藏/预设)* | 缩放 - 3D 翻滚 | `ADBE CM Zoom3DTumble` | adobe-builtin | 5 |
| *(隐藏/预设)* | 缩放 - 气泡 | `ADBE CM ZoomBubble` | adobe-builtin | 3 |
| *(隐藏/预设)* | 缩放 - 螺旋 | `ADBE CM ZoomSpiral` | adobe-builtin | 6 |
| *(隐藏/预设)* | 缩放 - 摇摆 | `ADBE CM ZoomWobble` | adobe-builtin | 4 |
| *(隐藏/预设)* | 彩色旋绕 | `ADBE Color Swirl` | adobe-builtin | 4 |
| *(隐藏/预设)* | 回弹 | `ADBE DE Bounce` | adobe-builtin | 7 |
| *(隐藏/预设)* | 回弹（标记） | `ADBE DE Bounce At Marker` | adobe-builtin | 5 |
| *(隐藏/预设)* | 回弹（节拍） | `ADBE DE Bounce On Beat` | adobe-builtin | 7 |
| *(隐藏/预设)* | 回弹（随机） | `ADBE DE Bounce Random` | adobe-builtin | 7 |
| *(隐藏/预设)* | 跟随 | `ADBE DE Follow` | adobe-builtin | 9 |
| *(隐藏/预设)* | 微动 | `ADBE DE Jiggle` | adobe-builtin | 7 |
| *(隐藏/预设)* | 微动（标记） | `ADBE DE Jiggle At Marker` | adobe-builtin | 5 |
| *(隐藏/预设)* | 微动（节拍） | `ADBE DE Jiggle On Beat` | adobe-builtin | 7 |
| *(隐藏/预设)* | 微动（随机） | `ADBE DE Jiggle Random` | adobe-builtin | 7 |
| *(隐藏/预设)* | 不透明脉冲 | `ADBE DE Opacity Pulse` | adobe-builtin | 8 |
| *(隐藏/预设)* | 不透明脉冲（标记） | `ADBE DE Opacity Pulse At Marker` | adobe-builtin | 6 |
| *(隐藏/预设)* | 不透明脉冲（节拍） | `ADBE DE Opacity Pulse On Beat` | adobe-builtin | 8 |
| *(隐藏/预设)* | 不透明脉冲（随机） | `ADBE DE Opacity Pulse Random` | adobe-builtin | 8 |
| *(隐藏/预设)* | 轨道 | `ADBE DE Orbit` | adobe-builtin | 6 |
| *(隐藏/预设)* | 轨道 3D | `ADBE DE Orbit 3D` | adobe-builtin | 9 |
| *(隐藏/预设)* | 振动 | `ADBE DE Oscillate` | adobe-builtin | 8 |
| *(隐藏/预设)* | 振动（标记） | `ADBE DE Oscillate At Marker` | adobe-builtin | 6 |
| *(隐藏/预设)* | 振动（节拍） | `ADBE DE Oscillate On Beat` | adobe-builtin | 8 |
| *(隐藏/预设)* | 振动（随机） | `ADBE DE Oscillate Random` | adobe-builtin | 9 |
| *(隐藏/预设)* | 钟摆 | `ADBE DE Pendulum` | adobe-builtin | 7 |
| *(隐藏/预设)* | 钟摆（标记） | `ADBE DE Pendulum At Marker` | adobe-builtin | 5 |
| *(隐藏/预设)* | 钟摆（节拍） | `ADBE DE Pendulum On Beat` | adobe-builtin | 7 |
| *(隐藏/预设)* | 钟摆（随机） | `ADBE DE Pendulum Random` | adobe-builtin | 7 |
| *(隐藏/预设)* | 脉冲 | `ADBE DE Pulse` | adobe-builtin | 6 |
| *(隐藏/预设)* | 脉冲（标记） | `ADBE DE Pulse At Marker` | adobe-builtin | 4 |
| *(隐藏/预设)* | 脉冲（节拍） | `ADBE DE Pulse On Beat` | adobe-builtin | 6 |
| *(隐藏/预设)* | 脉冲（随机） | `ADBE DE Pulse Random` | adobe-builtin | 6 |
| *(隐藏/预设)* | 随机填充颜色 | `ADBE DE Random Fill Color` | adobe-builtin | 10 |
| *(隐藏/预设)* | 随机运动 | `ADBE DE Random Motion` | adobe-builtin | 8 |
| *(隐藏/预设)* | 随机运动 1D | `ADBE DE Random Motion 1D` | adobe-builtin | 8 |
| *(隐藏/预设)* | 随机不透明度 | `ADBE DE Random Opacity` | adobe-builtin | 8 |
| *(隐藏/预设)* | 随机旋转 | `ADBE DE Random Rotation` | adobe-builtin | 8 |
| *(隐藏/预设)* | 随机旋转 3D | `ADBE DE Random Rotation 3D` | adobe-builtin | 12 |
| *(隐藏/预设)* | 随机缩放 | `ADBE DE Random Scale` | adobe-builtin | 9 |
| *(隐藏/预设)* | 云集 | `ADBE DE Swarm` | adobe-builtin | 5 |
| *(隐藏/预设)* | 摇摆回弹 | `ADBE DE Wobble Bounce` | adobe-builtin | 10 |
| *(隐藏/预设)* | 摇摆回弹（标记） | `ADBE DE Wobble Bounce At Marker` | adobe-builtin | 8 |
| *(隐藏/预设)* | 摇摆回弹（节拍） | `ADBE DE Wobble Bounce On Beat` | adobe-builtin | 10 |
| *(隐藏/预设)* | 摇摆回弹（随机） | `ADBE DE Wobble Bounce Random` | adobe-builtin | 11 |
| *(隐藏/预设)* | Z 反弹 | `ADBE DE Z Spring` | adobe-builtin | 10 |
| *(隐藏/预设)* | Z 反弹（标记） | `ADBE DE Z Spring At Marker` | adobe-builtin | 8 |
| *(隐藏/预设)* | 开始活跃 | `ADBE Getting Jiggy` | adobe-builtin | 18 |
| *(隐藏/预设)* | 采样图像 | `ADBE Sample Image` | adobe-builtin | 5 |
| *(隐藏/预设)* | 分离 XYZ 位置 | `ADBE Separate XYZ Position` | adobe-builtin | 5 |
| *(隐藏/预设)* | 分离 XYZ 比例 | `ADBE Separate XYZ Scale` | adobe-builtin | 5 |
| *(隐藏/预设)* | 立体 3D 控件 | `ADBE Stereo 3D Controls` | adobe-builtin | 11 |
| *(隐藏/预设)* | 2D 文本框 | `Pseudo/ADBE 2D Text Box` | adobe-builtin | 35 |
| *(隐藏/预设)* | 脸部测量值 | `Pseudo/ADBE Animal Head14` | adobe-builtin | 31 |
| *(隐藏/预设)* | 脸部跟踪点 | `Pseudo/ADBE Animal Head66` | adobe-builtin | 167 |
| *(隐藏/预设)* | Counter Controls | `Pseudo/ADBE Counter Controls` | adobe-builtin | 16 |
| *(隐藏/预设)* | 货币控件 | `Pseudo/ADBE Currency Controls` | adobe-builtin | 19 |
| *(隐藏/预设)* | 图案模板 | `Pseudo/ADBE Pattern Template` | adobe-builtin | 7 |
| *(隐藏/预设)* | 百分比控件 | `Pseudo/ADBE Percentage Controls` | adobe-builtin | 17 |
| *(隐藏/预设)* | 定时器控件 | `Pseudo/ADBE Timer Controls` | adobe-builtin | 14 |
| *(隐藏/预设)* | 追踪路径 | `Pseudo/ADBE Trace Path` | adobe-builtin | 4 |
| 沉浸式视频 | VR 模糊 | `Mettle SkyBox Blur` | adobe-builtin | 7 |
| 沉浸式视频 | VR 色差 | `Mettle SkyBox Chromatic Aberrat` | adobe-builtin | 12 |
| 沉浸式视频 | VR 颜色渐变 | `Mettle SkyBox Color Gradients` | adobe-builtin | 29 |
| 沉浸式视频 | VR 转换器 | `Mettle SkyBox Converter` | adobe-builtin | 12 |
| 沉浸式视频 | VR 降噪 | `Mettle SkyBox Denoise` | adobe-builtin | 8 |
| 沉浸式视频 | VR 数字故障 | `Mettle SkyBox Digital Glitch` | adobe-builtin | 40 |
| 沉浸式视频 | VR 分形杂色 | `Mettle SkyBox Fractal Noise` | adobe-builtin | 28 |
| 沉浸式视频 | VR 发光 | `Mettle SkyBox Glow` | adobe-builtin | 12 |
| 沉浸式视频 | VR 平面到球面 | `Mettle SkyBox Project 2D` | adobe-builtin | 19 |
| 沉浸式视频 | VR 旋转球面 | `Mettle SkyBox Rotate Sphere` | adobe-builtin | 10 |
| 沉浸式视频 | VR 锐化 | `Mettle SkyBox Sharpen` | adobe-builtin | 7 |
| 沉浸式视频 | VR 球面到平面 | `Mettle SkyBox Viewer` | adobe-builtin | 11 |
| 抠像 | 内部/外部键 | `ADBE ATG Extract` | adobe-builtin | 116 |
| 抠像 | 颜色差值键 | `ADBE Color Difference Key` | adobe-builtin | 19 |
| 抠像 | 颜色范围 | `ADBE Color Range` | adobe-builtin | 11 |
| 抠像 | 差值遮罩 | `ADBE Difference Matte2` | adobe-builtin | 8 |
| 抠像 | 提取 | `ADBE Extract` | adobe-builtin | 9 |
| 抠像 | Key Cleaner | `ADBE KeyCleaner` | adobe-builtin | 6 |
| 抠像 | 线性颜色键 | `ADBE Linear Color Key2` | adobe-builtin | 9 |
| 抠像 | Advanced Spill Suppressor | `ADBE Spill2` | adobe-builtin | 12 |
| 抠像 | Unmult | `ADBE Unmult` | adobe-builtin | 7 |
| 抠像 | CC Simple Wire Removal | `CC Simple Wire Removal` | adobe-bundled | 9 |
| 抠像 | Keylight (1.2) | `Keylight 906` | adobe-bundled | 81 |
| 遮罩 | 遮罩阻塞工具 | `ADBE Matte Choker` | adobe-builtin | 9 |
| 遮罩 | 调整柔和遮罩 | `ADBE RefineMatte2` | adobe-builtin | 24 |
| 遮罩 | 调整实边遮罩 | `ADBE RefineRBMatte` | adobe-builtin | 19 |
| 遮罩 | 简单阻塞工具 | `ADBE Simple Choker` | adobe-builtin | 4 |
| Boris FX Mocha | Mocha AE | `mochaAECC` | adobe-bundled | 59 |
| 杂色和颗粒 | 湍流杂色 | `ADBE AIF Perlin Noise 3D` | adobe-builtin | 29 |
| 杂色和颗粒 | 蒙尘与划痕 | `ADBE Dust & Scratches` | adobe-builtin | 5 |
| 杂色和颗粒 | 分形杂色 | `ADBE Fractal Noise` | adobe-builtin | 33 |
| 杂色和颗粒 | 中间值（旧版） | `ADBE Median` | adobe-builtin | 4 |
| 杂色和颗粒 | 杂色 Alpha | `ADBE Noise Alpha2` | adobe-builtin | 11 |
| 杂色和颗粒 | 杂色 HLS 自动 | `ADBE Noise HLS Auto2` | adobe-builtin | 8 |
| 杂色和颗粒 | 杂色 HLS | `ADBE Noise HLS2` | adobe-builtin | 8 |
| 杂色和颗粒 | 杂色 | `ADBE Noise2` | adobe-builtin | 5 |
| 杂色和颗粒 | 中间值 | `ADBE PS Median` | adobe-builtin | 4 |
| 杂色和颗粒 | 匹配颗粒 | `VISINF Grain Duplication` | adobe-builtin | 105 |
| 杂色和颗粒 | 添加颗粒 | `VISINF Grain Implant` | adobe-builtin | 73 |
| 杂色和颗粒 | 移除颗粒 | `VISINF Grain Removal` | adobe-builtin | 72 |
| 过时 | 基本 3D | `ADBE Basic 3D` | adobe-builtin | 7 |
| 过时 | 基本文字 | `ADBE Basic Text2` | adobe-builtin | *(跳过)* |
| 过时 | 颜色键 | `ADBE Color Key` | adobe-builtin | 6 |
| 过时 | 高斯模糊（旧版） | `ADBE Gaussian Blur` | adobe-builtin | 4 |
| 过时 | 闪光 | `ADBE Lightning` | adobe-builtin | 27 |
| 过时 | 亮度键 | `ADBE Luma Key` | adobe-builtin | 7 |
| 过时 | 路径文本 | `ADBE Path Text` | adobe-builtin | *(跳过)* |
| 过时 | 减少交错闪烁 | `ADBE Reduce Interlace Flicker` | adobe-builtin | 3 |
| 过时 | 溢出抑制 | `ADBE Spill Suppressor` | adobe-builtin | 4 |
| 过时 | mocha shape | `ISL MochaShapeImporter` | adobe-bundled | 20 |
| _Obsolete | 3D 眼镜（旧版） | `ADBE 3D Glasses` | adobe-builtin | 10 |
| _Obsolete | Alpha 色阶 | `ADBE Alpha Levels2` | adobe-builtin | 7 |
| _Obsolete | Alpha 色阶 | `ADBE Alpha Levels3` | adobe-builtin | 7 |
| _Obsolete | 应用颜色 LUT | `ADBE Apply Color LUT` | adobe-builtin | *(跳过)* |
| _Obsolete | 方框模糊 | `ADBE Box Blur` | adobe-builtin | 6 |
| _Obsolete | 亮度和对比度 | `ADBE Brightness & Contrast` | adobe-builtin | 4 |
| _Obsolete | Cineon 转换器 | `ADBE Cineon Converter` | adobe-builtin | 9 |
| _Obsolete | 颜色平衡 | `ADBE Color Balance` | adobe-builtin | 5 |
| _Obsolete | 色阶 | `ADBE Easy Levels` | adobe-builtin | 11 |
| _Obsolete | 曝光度 | `ADBE Exposure` | adobe-builtin | 24 |
| _Obsolete | 快速模糊（旧版） | `ADBE Fast Blur` | adobe-builtin | 5 |
| _Obsolete | 操控 | `ADBE FreePin3` | adobe-builtin | 6 |
| _Obsolete | 变换 | `ADBE Geometry` | adobe-builtin | 14 |
| _Obsolete | 杂色 | `ADBE Noise` | adobe-builtin | 5 |
| _Obsolete | 杂色 Alpha | `ADBE Noise Alpha` | adobe-builtin | 11 |
| _Obsolete | 杂色 HLS | `ADBE Noise HLS` | adobe-builtin | 8 |
| _Obsolete | 杂色 HLS 自动 | `ADBE Noise HLS Auto` | adobe-builtin | 8 |
| _Obsolete | Photoshop 斜面和浮雕 | `ADBE PSL Bevel Emboss` | adobe-builtin | 9 |
| _Obsolete | Photoshop 投影 | `ADBE PSL Drop Shadow` | adobe-builtin | 7 |
| _Obsolete | Photoshop 内发光 | `ADBE PSL Inner Glow` | adobe-builtin | 6 |
| _Obsolete | Photoshop 内阴影 | `ADBE PSL Inner Shadow` | adobe-builtin | 7 |
| _Obsolete | Photoshop 外发光 | `ADBE PSL Outer Glow` | adobe-builtin | 5 |
| _Obsolete | Photoshop 纯色填充 | `ADBE PSL Solid Fill` | adobe-builtin | 3 |
| _Obsolete | 绘画 | `ADBE Paint` | adobe-builtin | 3 |
| _Obsolete | 照片滤镜 | `ADBE Photo Filter` | adobe-builtin | 6 |
| _Obsolete | 色阶（单独控件） | `ADBE Pro Levels` | adobe-builtin | 41 |
| _Obsolete | 调整遮罩 | `ADBE RefineMatte` | adobe-builtin | 19 |
| _Obsolete | Roto 笔刷和调整边缘 | `ADBE Samurai` | adobe-builtin | 45 |
| _Obsolete | 设置遮罩 | `ADBE Set Matte2` | adobe-builtin | 8 |
| _Obsolete | 三向颜色校正器 | `ADBE Three-Way Color Corrector` | adobe-builtin | 83 |
| _Obsolete | 阈值 | `ADBE Threshold` | adobe-builtin | 3 |
| _Obsolete | 钝化蒙版 | `ADBE Unsharp Mask` | adobe-builtin | 6 |
| _Obsolete | CC Composite (obsolete) | `CC Composite` | adobe-bundled | 5 |
| _Obsolete | CC PS Classic (obsolete) | `CC PS Classic` | adobe-bundled | 11 |
| _Obsolete | CC PS LE Classic (obsolete) | `CC PS LE Classic` | adobe-bundled | 14 |
| _Obsolete | CC Rain (obsolete) | `CC Rain` | adobe-bundled | 9 |
| _Obsolete | CC Snow (obsolete) | `CC Snow` | adobe-bundled | 9 |
| _Obsolete | CC Time Blend | `CC Time Blend` | adobe-bundled | 5 |
| _Obsolete | CC Time Blend FX | `CC Time Blend FX` | adobe-bundled | 6 |
| 透视 | 3D 眼镜 | `ADBE 3D Glasses2` | adobe-builtin | 10 |
| 透视 | 3D 摄像机跟踪器 | `ADBE 3D Tracker` | adobe-builtin | 22 |
| 透视 | 斜面 Alpha | `ADBE Bevel Alpha` | adobe-builtin | 6 |
| 透视 | 边缘斜面 | `ADBE Bevel Edges` | adobe-builtin | 6 |
| 透视 | 投影 | `ADBE Drop Shadow` | adobe-builtin | 8 |
| 透视 | 径向阴影 | `ADBE Radial Shadow` | adobe-builtin | 11 |
| 透视 | CC Cylinder | `CC Cylinder` | adobe-bundled | 28 |
| 透视 | CC Environment | `CC Environment` | adobe-bundled | 7 |
| 透视 | CC Sphere | `CC Sphere` | adobe-bundled | 28 |
| 透视 | CC Spotlight | `CC Spotlight` | adobe-bundled | 11 |
| 模拟 | 粒子运动场 | `ADBE Playgnd` | adobe-builtin | 127 |
| 模拟 | 卡片动画 | `APC CardDanceCam` | adobe-builtin | 81 |
| 模拟 | 焦散 | `APC Caustics` | adobe-builtin | 40 |
| 模拟 | 泡沫 | `APC Foam` | adobe-builtin | 48 |
| 模拟 | 碎片 | `APC Shatter` | adobe-builtin | 81 |
| 模拟 | 波形环境 | `APC Wave World` | adobe-builtin | 49 |
| 模拟 | CC Ball Action | `CC Ball Action` | adobe-bundled | 14 |
| 模拟 | CC Bubbles | `CC Bubbles` | adobe-bundled | 9 |
| 模拟 | CC Drizzle | `CC Drizzle` | adobe-bundled | 24 |
| 模拟 | CC Hair | `CC Hair` | adobe-bundled | 29 |
| 模拟 | CC Mr. Mercury | `CC Mr. Mercury` | adobe-bundled | 34 |
| 模拟 | CC Particle Systems II | `CC Particle Systems II` | adobe-bundled | 32 |
| 模拟 | CC Particle World | `CC Particle World` | adobe-bundled | 105 |
| 模拟 | CC Pixel Polly | `CC Pixel Polly` | adobe-bundled | 11 |
| 模拟 | CC Scatterize | `CC Scatterize` | adobe-bundled | 6 |
| 模拟 | CC Star Burst | `CC Star Burst` | adobe-bundled | 9 |
| 模拟 | CC Rainfall | `CSRainfall` | adobe-bundled | 25 |
| 模拟 | CC Snowfall | `CSSnowfall` | adobe-bundled | 34 |
| 风格化 | 画笔描边 | `ADBE Brush Strokes` | adobe-builtin | 9 |
| 风格化 | 卡通 | `ADBE Cartoonify` | adobe-builtin | 20 |
| 风格化 | 彩色浮雕 | `ADBE Color Emboss` | adobe-builtin | 6 |
| 风格化 | 浮雕 | `ADBE Emboss` | adobe-builtin | 6 |
| 风格化 | 查找边缘 | `ADBE Find Edges` | adobe-builtin | 4 |
| 风格化 | 发光 | `ADBE Glo2` | adobe-builtin | 16 |
| 风格化 | 马赛克 | `ADBE Mosaic` | adobe-builtin | 5 |
| 风格化 | 色调分离 | `ADBE Posterize` | adobe-builtin | 3 |
| 风格化 | 毛边 | `ADBE Roughen Edges` | adobe-builtin | 17 |
| 风格化 | 散布 | `ADBE Scatter` | adobe-builtin | 5 |
| 风格化 | 闪光灯 | `ADBE Strobe` | adobe-builtin | 10 |
| 风格化 | 纹理化 | `ADBE Texturize` | adobe-builtin | 6 |
| 风格化 | 阈值 | `ADBE Threshold2` | adobe-builtin | 3 |
| 风格化 | 动态拼贴 | `ADBE Tile` | adobe-builtin | 10 |
| 风格化 | CC Burn Film | `CC Burn Film` | adobe-bundled | 5 |
| 风格化 | CC Glass | `CC Glass` | adobe-bundled | 25 |
| 风格化 | CC Kaleida | `CC Kaleida` | adobe-bundled | 7 |
| 风格化 | CC Mr. Smoothie | `CC Mr. Smoothie` | adobe-bundled | 9 |
| 风格化 | CC Plastic | `CC Plastic` | adobe-bundled | 28 |
| 风格化 | CC RepeTile | `CC RepeTile` | adobe-bundled | 8 |
| 风格化 | CC Threshold | `CC Threshold` | adobe-bundled | 6 |
| 风格化 | CC Threshold RGB | `CC Threshold RGB` | adobe-bundled | 9 |
| 风格化 | CC Block Load | `CS BlockLoad` | adobe-bundled | 6 |
| 风格化 | CC HexTile | `CS HexTile` | adobe-bundled | 8 |
| 风格化 | CC Vignette | `CS Vignette` | adobe-bundled | 6 |
| 文本 | 编号 | `ADBE Numbers2` | adobe-builtin | *(跳过)* |
| 文本 | 时间码 | `ADBE Timecode` | adobe-builtin | 16 |
| 时间 | 时差 | `ADBE Difference` | adobe-builtin | 7 |
| 时间 | 残影 | `ADBE Echo` | adobe-builtin | 7 |
| 时间 | 像素运动模糊 | `ADBE OFMotionBlur` | adobe-builtin | 6 |
| 时间 | 抽帧 | `ADBE Posterize Time` | adobe-builtin | 3 |
| 时间 | 时间置换 | `ADBE Time Displacement` | adobe-builtin | 6 |
| 时间 | 时间扭曲 | `ADBE Timewarp` | adobe-builtin | 40 |
| 时间 | CC Force Motion Blur | `CC Force Motion Blur` | adobe-bundled | 7 |
| 时间 | CC Wide Time | `CC Wide Time` | adobe-bundled | 5 |
| 过渡 | 块溶解 | `ADBE Block Dissolve` | adobe-builtin | 7 |
| 过渡 | 渐变擦除 | `ADBE Gradient Wipe` | adobe-builtin | 7 |
| 过渡 | 光圈擦除 | `ADBE IRIS_WIPE` | adobe-builtin | 9 |
| 过渡 | 线性擦除 | `ADBE Linear Wipe` | adobe-builtin | 5 |
| 过渡 | 径向擦除 | `ADBE Radial Wipe` | adobe-builtin | 7 |
| 过渡 | 百叶窗 | `ADBE Venetian Blinds` | adobe-builtin | 6 |
| 过渡 | 卡片擦除 | `APC CardWipeCam` | adobe-builtin | 62 |
| 过渡 | CC Glass Wipe | `CC Glass Wipe` | adobe-bundled | 7 |
| 过渡 | CC Grid Wipe | `CC Grid Wipe` | adobe-bundled | 9 |
| 过渡 | CC Image Wipe | `CC Image Wipe` | adobe-bundled | 11 |
| 过渡 | CC Jaws | `CC Jaws` | adobe-bundled | 8 |
| 过渡 | CC Light Wipe | `CC Light Wipe` | adobe-bundled | 10 |
| 过渡 | CC Radial ScaleWipe | `CC Radial ScaleWipe` | adobe-bundled | 5 |
| 过渡 | CC Scale Wipe | `CC Scale Wipe` | adobe-bundled | 5 |
| 过渡 | CC Twister | `CC Twister` | adobe-bundled | 7 |
| 过渡 | CC WarpoMatic | `CC WarpoMatic` | adobe-bundled | 9 |
| 过渡 | CC Line Sweep | `CS LineSweep` | adobe-bundled | 7 |
| 实用工具 | 应用颜色 LUT | `ADBE Apply Color LUT2` | adobe-builtin | *(跳过)* |
| 实用工具 | Cineon 转换器 | `ADBE Cineon Converter2` | adobe-builtin | 9 |
| 实用工具 | HDR 压缩扩展器 | `ADBE Compander` | adobe-builtin | 5 |
| 实用工具 | 范围扩散 | `ADBE GROW BOUNDS` | adobe-builtin | 3 |
| 实用工具 | HDR 高光压缩 | `ADBE HDR ToneMap` | adobe-builtin | 3 |
| 实用工具 | 颜色配置文件转换器 | `ADBE ProfileToProfile` | adobe-builtin | 9 |
| 实用工具 | CC Overbrights | `CC Overbrights` | adobe-bundled | 4 |

## 跳过的自带效果

| 分类 | 名称 | matchName | 原因 |
|---|---|---|---|
| Cinema 4D | Cineware | `CINEMA 4D Effect` | requires Cinema 4D footage layer; aborts when added to a solid layer |
| 实用工具 | 应用颜色 LUT | `ADBE Apply Color LUT2` | opens native OS file picker modal dialog immediately upon apply |
| _Obsolete | 应用颜色 LUT | `ADBE Apply Color LUT` | obsolete LUT effect; opens native OS file picker modal dialog immediately upon apply |
| 过时 | 基本文字 | `ADBE Basic Text2` | obsolete text effect; opens modal text/font dialog immediately upon apply |
| 文本 | 编号 | `ADBE Numbers2` | opens modal font/format dialog immediately upon apply |
| 过时 | 路径文本 | `ADBE Path Text` | obsolete path text effect; opens modal text/font dialog immediately upon apply |
