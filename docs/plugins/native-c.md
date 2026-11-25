# 原生插件：C ABI 入门

原生插件是一个动态库（macOS `.dylib`，Windows `.dll`），通过 [`sdk/c/bae_plugin.h`](../../sdk/c/bae_plugin.h) 定义的稳定 C ABI 接入。任何能导出 C 函数的语言都可以写（C、C++、Rust、Zig……）；用 Rust 写更省事，见 [Rust 原生插件](native-rust.md)。规范见规格 §8、§9、§12–§15。

> **现状（ABI 1.0，M1 起稳定）**：宿主用 libloading 加载原生效果，CPU 渲染路径可用（见下文“宿主怎么运行原生插件”）。在主版本 1 里只做追加：按 1.0 编译的插件在之后所有 1.x 宿主上照常运行。按 0.1 预发布版编译的插件仍然接受（布局相同）。头文件里宿主还没有提供的套件（`get_suite` 返回 `NULL` 的那些）是“暂定”的，宿主第一次提供时才冻结。

## 设计要点

- **一个导出符号**：`bae_plugin_entry`。宿主调用它，插件填写 `BaePluginExports`：插件 ID、版本、需要的最低 ABI、贡献列表（每个贡献一个 vtable）。
- **套件（suite）+ 版本 + 能力协商**：宿主的服务按（名字，版本）获取，例如 `bae.core` 1、`bae.render` 1。一个已发布的套件版本**永不改变**，新功能用新套件或新版本——这和 AE 的 PICA 套件、OpenFX 的思路一样。所以旧插件在新宿主上不用重新编译。
- **可扩展结构体**：每个会增长的结构体都以 `uint32_t struct_size` 开头。读字段前用 `BAE_HAS_FIELD(ptr, Type, field)` 检查；宿主给的输出结构体，插件不能写超过 `struct_size` 的部分。
- **效果生命周期和 AE 一一对应**：将来的 AE 插件兼容层（20）只做翻译。

## ABI 规则（摘要）

| 规则 | 内容 |
|---|---|
| R1 | 只支持 64 位（aarch64 / x86_64），默认 C 调用约定 |
| R2 | 只用定宽整数；枚举是 `uint32_t` / `int32_t` 字段加 `#define` 常量，结构体里不用 C `enum` |
| R3 | 可扩展结构体以 `struct_size` 开头，新字段只追加；缺失的字段按 0 处理 |
| R4 | 套件按（名字，版本）获取，发布后冻结 |
| R5 | 插件 vtable 也是可扩展结构体；`NULL` 回调表示"用文档规定的默认行为" |
| R6 | 字符串是 UTF-8 |
| R7 | 谁分配谁释放；跨边界的数据要么只在调用期间借用，要么通过宿主的 `BaeWriter` 写出 |
| R8 | 不能让 C++ 异常或 Rust panic 穿过边界，要在插件里捕获并返回 `BAE_ERR_PANIC` |
| R9 | 遇到不认识的枚举值和标志位要容忍，不能拒绝 |
| R10 | 传给回调的指针只在回调返回前有效（另有说明的除外） |

完整说明在头文件开头的注释里。

## 入口与版本协商

```c
#include "bae_plugin.h"

BAE_PLUGIN_ENTRY
{
    /* host 是宿主写的可扩展结构体：先检查字段在不在（R3）。 */
    if (!BAE_HAS_FIELD(host, BaeHost, get_suite) || host->get_suite == NULL)
        return BAE_ERR_VERSION_MISMATCH;
    /* 主版本不同、或宿主次版本比我需要的旧：拒绝。 */
    if (BAE_VERSION_MAJOR(host->abi_version) != 1 || BAE_VERSION_MINOR(host->abi_version) < 0)
        return BAE_ERR_VERSION_MISMATCH;

    const BaeCoreSuite1 *core = host->get_suite(host->handle, BAE_SUITE_CORE, BAE_SUITE_CORE_VERSION);
    /* ……获取需要的其他套件…… */

    out_exports->abi_built = BAE_ABI_VERSION;              /* 编译时的 ABI */
    out_exports->abi_required = BAE_MAKE_VERSION(1, 0);   /* 能运行的最低宿主 ABI，= 清单的 api */
    out_exports->plugin_id = "com.example.invert-c";      /* 必须和清单一致 */
    out_exports->plugin_version = "0.1.0";                /* 必须和清单一致 */
    out_exports->contribution_count = 1;
    out_exports->contributions = k_contributions;          /* 静态生命周期 */
    return BAE_OK;
}
```

宿主会核对：导出的插件 ID、版本和清单一致；`abi_required` 等于清单的 `api`，且宿主支持它；清单里每个 `implementation = "native"` 的效果都导出了，导出的每个效果也都在清单里声明过。不一致时插件加载失败，状态栏提示原因，`bae plugin list` 里显示为 `failed`。

## 宿主怎么运行原生插件

**崩溃不能拖垮 BAE。** 原生插件默认在单独的进程里运行（进程外模式，规格 §15）：BAE 启动一个 `bae plugin runner` 子进程加载动态库，控制消息走管道，像素走共享内存。插件崩溃、`abort()` 或卡死（渲染 10 秒、加载 30 秒没有响应）只会结束这个子进程，下一帧自动重启；一分钟内崩溃三次，本次运行中停用该插件。

只有可信的插件才在 BAE 进程内运行（省掉一次内存拷贝和进程间往返）：

| 情况 | 运行方式 |
|---|---|
| 清单 `isolation = "out-of-process"` | 进程外 |
| 清单 `isolation = "prefer-out-of-process"` | 进程外，除非用户把它设为进程内 |
| 清单 `isolation = "in-process"`（默认），随 BAE 安装或系统级插件 | 进程内 |
| 同上，但是用户级或工程内插件 | 进程外，除非用户把它设为进程内 |

开发时可以用环境变量 `BAE_PLUGIN_IN_PROCESS=<插件 ID>,<插件 ID>`（`*` 表示全部）让插件进程内运行，方便调试器直接附加。`bae plugin list` 会显示每个原生插件实际的运行方式。

**失败时停用并提示。** 加载失败（找不到库、导出和清单不一致、`global_setup` 或 `define_params` 出错）：插件不注册，它的效果在工程里显示为缺失占位，数据完整保留。渲染出错：这一帧该效果直接输出输入（整帧不会失败），状态栏提示一次。进程内插件返回 `BAE_ERR_PANIC`（Rust SDK 把 panic 转成它）时，宿主停用该插件。

**ABI 1.0 宿主目前提供的部分**（其余回调和套件在后续版本陆续提供，插件用 `NULL` 检查即可）：

| 项 | 状态 |
|---|---|
| 效果（`BAE_KIND_EFFECT`，滤镜类） | `global_setup`、`define_params`、`pre_render`、`render_cpu`、`release_frame_data`、`global_setdown`、`shutdown` |
| 参数类型 | 浮点、整数、角度、复选框、颜色、点、3D 点、弹出菜单、分组、按钮 |
| CPU 像素格式 | RGBA F32 预乘、RGBA U8 预乘（按插件列出的顺序选第一个支持的） |
| `bae.core` 1 | `log`（进入 BAE 日志）、`set_error`、`alloc` / `free`、`get_path`（插件目录、临时目录）；`has_permission` 目前总是返回假 |
| `bae.param.define` 1 | `add` |
| `bae.render` 1 | `checkout_input`（自身输入；图层参数和转场输入为空）、`checkout_param`（当前时间）、`get_input_cpu`、`alloc_cpu_image`、`parallel_for`、`is_aborted`、`report_progress` |
| 暂不提供 | GPU 渲染（`render_gpu`、`bae.gpu.*`）、实例状态、参数监视、其他贡献种类（导入器、命令……）；只有 GPU 路径的效果加载时会报错 |

## 宿主套件

| 套件 | 用途 | ABI 1.0 宿主 |
|---|---|---|
| `bae.core` | 日志、错误信息（`set_error`）、内存分配、权限查询、插件数据/缓存目录 | 提供 |
| `bae.param.define` | 在 `define_params` 里定义参数 | 提供 |
| `bae.param.edit` | 在参数变化回调里修改参数（进入同一个撤销步骤） | 暂定 |
| `bae.render` | 渲染时取输入（checkout，可以是其他图层、其他时间）、分配 CPU 图像、并行执行、进度、中止检查 | 提供 |
| `bae.gpu.compute` | 由宿主编码的可移植 GPU 计算（推荐） | 暂定 |
| `bae.gpu.native` | 原生 Metal / D3D12 设备和纹理句柄（零拷贝互操作） | 暂定 |
| `bae.path` | 形状操作器输出路径 | 暂定 |
| `bae.command` | 调用命令（修改工程只能通过命令）、只读查询（和 MCP 共用） | 暂定 |

## 效果 vtable 与 AE 命令

`BaeEffectVTable1` 的回调（括号里是调用线程：L 加载，M 模型线程，R 渲染线程，G 设备）：

| 回调 | 对应 AE 命令选择器 | 说明 |
|---|---|---|
| `global_setup`（L） | `GLOBAL_SETUP` | 填写效果描述（标志、线程安全、设备、像素格式） |
| `global_setdown`（L） | `GLOBAL_SETDOWN` | |
| `define_params`（L） | `PARAMS_SETUP` | 用 `bae.param.define` 定义参数，结果会缓存 |
| `gpu_device_setup` / `gpu_device_setdown`（G） | `GPU_DEVICE_SETUP` / `SETDOWN` | 每个设备一次，在这里创建管线 |
| `instance_create` / `instance_serialize` / `instance_destroy`（M/R） | `SEQUENCE_SETUP` / `RESETUP` / `FLATTEN` / `SETDOWN` | 实例状态（AE 的 sequence data），序列化后存进工程层并参与协同 |
| `param_changed`（M） | `USER_CHANGED_PARAM` | 监视参数变化 |
| `update_params_ui`（M） | `UPDATE_PARAMS_UI` | 隐藏、禁用、改名参数 |
| `query_dynamic_flags`（M） | `QUERY_DYNAMIC_FLAGS` | |
| `pre_render`（R） | `SMART_PRE_RENDER` | 声明需要的输入区域（ROI）和结果区域；`NULL` 表示输入区域等于输出区域 |
| `render_cpu`（R） | `SMART_RENDER` / `RENDER` | CPU 渲染 |
| `render_gpu`（R） | `SMART_RENDER_GPU` | GPU 渲染，可以返回 `BAE_STATUS_FALLBACK` 改走 CPU |
| `release_frame_data`（R） | `FRAME_SETDOWN` | 释放 `pre_render` 产生的数据 |

不线程安全的效果由宿主串行调用（`BAE_THREADING_SERIAL_GLOBAL` / `SERIAL_INSTANCE`）；`CONCURRENT` 的效果可以多帧并行渲染（对应 AE 的多帧渲染）。

## CPU 渲染的像素格式

`render_cpu` 拿到的是 CPU 图像：矩形、行跨度（字节）和像素格式。像素格式由通道顺序（RGBA / BGRA / ARGB）、分量类型（U8、U16、AE 的 0–32768 U16、F16、F32）和是否预乘组合而成，插件在描述里列出能接受的格式。宿主把这一帧的输入从 GPU 读回、转换成插件要的格式，渲染完再上传回 GPU；只有这个效果节点付出往返代价，其他节点仍然全在 GPU 上。行跨度按 64 字节对齐。

## 示例与测试

[`sdk/c/examples/invert`](../../sdk/c/examples/invert) 是一个完整的纯 C 效果（反转颜色，按"数量"混合），演示入口、套件、可扩展结构体、`define_params`、`render_cpu`、F32 和 U8 两种格式、中止检查和错误报告。构建方法见它的 README。Rust 示例是 [`plugins/examples/duotone-rs`](../../plugins/examples/duotone-rs)。

`cargo test -p bae-plugin --test native` 用宿主的真实代码测试原生插件：编译 invert 示例并在进程内加载、渲染、核对像素；构建 Rust 示例并在进程外渲染；用一个故意出错的测试插件验证段错误、`abort()` 和错误返回都不会影响宿主（子进程重启、三次后停用）；Rust SDK 把 panic 转成 `BAE_ERR_PANIC`；按 0.1 预发布版编译的插件能在 1.0 宿主上运行；GPU → CPU → GPU 渲染节点的结果和参考值一致。

```sh
sdk/c/tests/check.sh
```

这个脚本：

1. 用 C11 和 C17 编译 `abi_check.c`：检查每个结构体的大小、对齐、字段偏移，每个常量的数值，以及每个回调和套件函数的签名——ABI 一旦被无意改动就编译失败；
2. 用 C++17 编译头文件；
3. 把 invert 示例编译成动态库，用 `mini_host.c` 加载：版本协商的正反例、导出核对、参数定义，再用 RGBA F32 / U8 / BGRA U8 渲染并核对像素，同时检查插件不会写出宿主给的结构体范围。

## 清单

原生插件的清单要有 `[native]` 和每个平台的库路径，效果写 `implementation = "native"` 和 `devices`。清单里的 `[[effect.param]]` 是可选的"列表副本"（让宿主不加载代码也能列出参数），必须和 `define_params` 定义的一致，以代码为准。见 [清单参考](manifest.md#原生和脚本插件)。
