# AE 桥接脚本

负责：15 `interop/ae`。在 After Effects 里运行的 ExtendScript 脚本（ES3），支持 AE CC 2018（15.0）及以上。

> ⚠ **未在 AE 中验证。** 开发机上没有 After Effects。脚本只在模拟的 AE 对象模型上测过（`tests/`），模拟按 AE 脚本文档编写，不能代替真机。第一次在 AE 里运行请按第 4 节的步骤做，把结果（包括报错和 AE 版本）记到 `docs/workstreams/15-interop-ae.md` 的「进度」一节。

| 文件 | 作用 |
|---|---|
| `bae-export.jsx` | 把当前 AE 工程导出成 BAE 交换文件（格式：`docs/interop/bae-interchange.md`） |
| `bae-anim-samples.jsx` | 在 AE 里搭建关键帧/表达式用例并记录 AE 的求值结果，给 03 做对照（格式：`docs/interop/ae-anim-samples.md`） |
| `samples/gen_anim_cases.py`、`samples/anim-cases.json` | 对照用例的生成器和生成结果 |
| `examples/mock-project.bae-interchange.json` | 模拟工程的导出结果（不是 AE 的真实输出），也是测试的金样 |
| `tests/` | 不依赖 AE 的测试：模拟 DOM、ES3 检查、金样对比、Schema 校验 |

## 1. 准备

1. AE 首选项 → 脚本和表达式 → 勾选「允许脚本写入文件和访问网络」（英文版：Preferences > Scripting & Expressions > Allow Scripts to Write Files and Access Network）。
2. 脚本源码全是 ASCII（中文提示用 `\uXXXX` 转义），任何系统语言下都能直接运行。

## 2. 导出工程（`bae-export.jsx`）

文件 → 脚本 → 运行脚本文件…，选择 `bae-export.jsx`。对话框选项：

| 选项 | 默认 | 说明 |
|---|---|---|
| 只导出选中的合成 | 关 | 在项目面板里选中合成；会一并导出它们用到的预合成和素材 |
| 收集素材 | 关 | 把素材复制到 `<文件名>.assets/`，清单里记录 `collected` |
| 缩进 JSON | 开 | 便于阅读和对比；关掉文件更小 |
| 只导出修改过的属性 | 关 | 用 AE 的 `isModified` 省略默认值，文件更小，但导入时要依赖 BAE 的默认值 |

然后选保存位置，默认文件名 `<工程名>.bae-interchange.json`。完成后弹出统计：合成数、图层数、关键帧数、警告数、不支持项数。警告和不支持项的明细在文件的 `report` 里。

### 无界面运行

先定义 `BAE_EXPORT_OPTIONS`，再执行脚本。写一个包装脚本 `export-wrapper.jsx`：

```js
var BAE_EXPORT_OPTIONS = {
    outputPath: "D:/exports/demo.bae-interchange.json",
    selectedCompsOnly: false,
    collectAssets: true,
    pretty: true
};
$.evalFile("D:/bae/bridges/ae/bae-export.jsx");
```

- Windows：`"C:\Program Files\Adobe\Adobe After Effects 2024\Support Files\AfterFX.exe" -r D:\bae\export-wrapper.jsx`
- macOS：`osascript -e 'tell application "Adobe After Effects 2024" to DoScriptFile "/Users/me/bae/export-wrapper.jsx"'`

结果一行写到 ExtendScript 控制台（`$.writeln`）。`BAE_EXPORT_OPTIONS` 用完会被清空，之后从菜单运行仍会弹对话框。

## 3. 生成对照数据（`bae-anim-samples.jsx`）

文件 → 脚本 → 运行脚本文件…，选择 `bae-anim-samples.jsx`，先选 `samples/anim-cases.json`，再选输出位置（默认 `anim-samples.ae.json`）。脚本在当前工程里新建一个文件夹，每个用例建一个合成，采样后全部删除（整个过程是一个撤销步骤）。完成后提示「N cases, M failed」。

无界面运行时定义 `BAE_ANIM_SAMPLES_OPTIONS = {casesPath, outputPath, keepComps: false, only: "<用例 id>"}`；`keepComps: true` 保留合成便于在 AE 里查看，`only` 只跑一个用例。

## 4. 手动测试（在 AE 里）

每项写明 AE 版本（`app.version`）和系统。建议至少覆盖 CC 2018（15.x）、2020（17.x）、2023（23.x）和当前版本。

### T1 冒烟：导出一个覆盖面广的工程

搭建（与 `tests/fixture-project.js` 的模拟工程对应）：

1. 新建合成「Main Comp」：1920×1080，29.97 fps，10 秒，背景色 (0.1, 0.2, 0.3)。
2. 空对象「Controller」。
3. 形状图层「Box」：父级设为 Controller；内容里加矩形（大小 200×120、圆度 12）、填充、渐变填充、修剪路径（结束点 0→100 两个关键帧）、中继器。位置在 0、1.001、2.002 秒打三个关键帧：第 1、2 个用缓动并拖出空间切线，第 2 个设为漂浮，第 3 个出点改成定格。不透明度加表达式 `wiggle(2, 20)`。
4. 新建合成「Pre comp」，里面一个文字图层「Title 标题」：源文本在 0 秒和 1.001 秒各一个关键帧（第二个改成中文），加一个不透明度动画器，路径选项指向一个开放遮罩。
5. 把 Pre comp 拖进 Main Comp，开始时间 1 秒，伸缩 50%。
6. 纯色「Red Solid 1」放在 Pre comp 下面，轨道遮罩选「Alpha 遮罩」；画两个遮罩（同名「Mask 1」），第一个设为相加、反转，并在遮罩路径上加一个羽化点；加效果：高斯模糊（模糊度 12.5）、设置遮罩（从图层取：Controller）、曲线。
7. 合成标记：1.001 秒「intro」（时长 0.5 秒、章节 Ch1），2.002 秒「beat」。

运行导出（勾选「缩进 JSON」）。预期：

- 弹窗：`已导出 2 个合成…`（或英文 `Exported 2 comps…`），没有报错。
- `python3 bridges/ae/tests/validate_schema.py <导出文件>` 输出 `OK`（需要 `pip install jsonschema`）。
- 文件里能找到（路径是 Prim 路径）：
  - `/comps/Main_Comp` 的 `frame_rate` ≈ 29.97002997、`duration` = 7063056000。
  - `/comps/Main_Comp/layers/Box` 的 `parent` = `/comps/Main_Comp/layers/Controller`；`ae:ADBE_Transform_Group:ADBE_Position` 有 3 个关键帧，键是 `0`、`706305600`、`1412611200`；第 2 个 `flags` 含 `roving`；第 3 个 `interp_out` = `hold`。
  - `ae:ADBE_Transform_Group:ADBE_Opacity` 有 `expression.source` = `wiggle(2, 20)`。
  - `/comps/Main_Comp/layers/Box/contents/…` 下有 `ShapeGroup`、`Rect`、`Fill`、`GradientFill`、`ShapeOperator`（`operator` = `trim` / `repeater`）。
  - `/comps/Pre_comp/layers/Title_标题` 的源文本关键帧值类型是 `text_document`，文字和字体正确；`animators/…` 下有 `TextAnimator` 和 `TextSelector`。
  - 纯色图层的 `track_matte` = `alpha`，`track_matte_layer` 指向 Pre comp 图层；两个遮罩是 `Mask_1` 和 `Mask_1_2`；效果参数 `ae:ADBE_Gaussian_Blur_2--0001` = 12.5。
  - `report.unsupported` 里有 `gradient_colors`、`custom_value`（曲线）、`mask_feather_points`。
- 把结果文件和 `examples/mock-project.bae-interchange.json` 对比结构（值会不同）：同样的路径、同样的属性名。**两者结构不一致的地方就是模拟和真实 AE 的差别，请记下来。**

### T2 关键帧数值

在 T1 的工程里，选中 Box 位置的第 2 个关键帧，打开「关键帧速度」，记下传入/传出的速度和影响，与文件里 `ease_in` / `ease_out` 比较（应一致，速度单位 像素/秒）。缩放属性用「单独尺寸」以外的普通关键帧，确认 `ease_in` 有 3 项。

### T3 语言无关

在中文版和英文版 AE（或用 `app.isoLanguage` 不同的安装）各导出一次 T1 的工程。预期：除 `name` 属性、`host` 和 `exported_at` 外，两个文件的属性名（`ae:` 开头）完全相同。

### T4 关键帧时间基准（重要）

新合成 30 fps；空对象开始时间设为 1 秒；在合成时间 2 秒处给旋转打关键帧（图层时间 1 秒）。导出后看这个关键帧的键：

- 预期 `1411200000`（合成时间 2 秒，对应交换格式 `key_time_base = "comp"`）。
- 如果是 `705600000`，说明 `keyTime()` 返回的是图层时间：请记下 AE 版本，交换格式要改成 `layer`。
- 再把图层伸缩改成 200% 重复一次，记下结果。

### T5 素材

工程里放一个视频、一个图片序列（`frame_0001.png`…）、一个缺失的素材，导出时勾选「收集素材」。预期：`<名字>.assets/` 里有视频文件和一个放序列帧的子文件夹；清单里视频有 `size`、`modified`、`path_relative`；序列有 `sequence.count`；缺失素材 `missing: true`，`report.warnings` 有 `missing_footage`。

### T6 选中导出与无界面

1. 项目面板只选 Main Comp，勾选「只导出选中的合成」：预期 Pre comp 和用到的素材也在，没用到的合成不在。
2. 按第 2 节用包装脚本无界面运行：预期文件生成，控制台一行统计；之后再从菜单运行，仍然弹出对话框。

### T7 大工程

拿一个真实工程（几百个图层）导出，记录 `report.stats.seconds` 和文件大小。

### T8 对照数据（给 03）

运行 `bae-anim-samples.jsx`（第 3 节）。预期：

- 提示 `62 cases, 0 failed`；`validate_schema.py` 校验输出为 `OK`。
- 抽查：`linear_two_keys` 在 `times` = 705600000（1 秒）处 `pre_expression` = `[180]`；`hold_out` 在 0.5 秒处是 `[0]`；`expr_time` 在 1 秒处 `post_expression` = `[100]`；`comp_29_97` 的 `attr` 关键帧键是 `0`、`706305600`、`1412611200`。
- 失败的用例看 `error` 字段（多半是某个属性在该版本里叫法不同），记到任务书。
- 把输出改名为 `anim-samples.ae-<主版本号>.json` 放进 `bridges/ae/samples/` 提交。

## 5. 开发

```sh
bridges/ae/tests/run.sh             # 模拟 DOM 测试（node，或 macOS 自带的 jsc）
bridges/ae/tests/run.sh --update    # 改了导出逻辑后重写金样 examples/mock-project.bae-interchange.json
python3 bridges/ae/samples/gen_anim_cases.py          # 重新生成用例
python3 bridges/ae/tests/validate_schema.py FILE.json # Schema 校验（需要 jsonschema）
```

测试会做的检查：

- ES3 语法和库函数：源码里不能有 `let`/`const`/箭头函数/模板字符串/尾逗号/非 ASCII 字符；运行前删掉 `Array.prototype.indexOf`、`JSON`、`Object.keys` 等 ES5+ 函数，用到就会失败。
- 导出结果：Prim 名、属性名符合 02 的规则；所有 `prim_ref` / `asset_ref` 都指向存在的 Prim；关键帧、表达式、遮罩、形状、文字、效果、标记、素材清单的具体值；与金样逐字节一致。
- 对照脚本：62 个用例都能搭建；用例的 `AttrSpec` 经「写入 AE → 读回」后不变；采样数量和时间正确。

写 ExtendScript 的注意事项：只用 ES3；不要比较两个 AE 对象是否 `===`（同一个对象两次取出来的包装可能不同），用 `id` 或 `index` 比较；读属性一律包 `try`（很多访问器在不适用时直接抛异常）；AE 的全局变量在多次运行之间保留，脚本用到的开关（`BAE_EXPORT_OPTIONS` 等）用完要清空。
