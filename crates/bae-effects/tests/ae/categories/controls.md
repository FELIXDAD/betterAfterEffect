# 表达式控制与 AE 26 的像素对照

## 已有效果

| AE matchName | 名称 | 实现 |
|---|---|---|
| `ADBE Point3D Control` | 3D 点控制 | Rust |
| `ADBE Angle Control` | 角度控制 | Rust |
| `ADBE Checkbox Control` | 复选框控制 | Rust |
| `ADBE Color Control` | 颜色控制 | Rust |
| `ADBE Dropdown Control` | 下拉菜单控件 | Rust |
| `ADBE Layer Control` | 图层控制 | Rust |
| `ADBE Point Control` | 点控制 | Rust |
| `ADBE Slider Control` | 滑块控制 | Rust |

`cases-controls-channel.json` 的 39 个用例由一次 `aectl run` 批量生成。AE 的启动提示框由 `aectl` 恢复后，批次正常完成。输入采用不透明色块与羽化 alpha 圆盘，PNG 输出是 8 bpc。对照按预乘 RGBA 比较，使用每像素四通道中的最大差，再计算整图平均。

| 效果 | 最大差 / 平均差（8 位色阶） | 观察 |
|---|---:|---|
| 8 个表达式控制 | 0 / 0 | 像素不变。表达式命名、数字索引和图层对象另有宿主测试。 |

## 工作流进度

2026-11-24，`effects/controls-channel` 增加 8 个表达式控制和其他分类的 9 个效果。表达式可按参数名、matchName、序号取得控制值，图层控制返回图层对象。8 个控制项的参考图逐像素一致。下拉菜单的自定义项需效果控件工作流接入，中英文选项按序号提供。
