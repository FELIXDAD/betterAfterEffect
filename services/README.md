# services/

服务端程序（例如 `services/bae-hub` 协作服务，见工作流 10）。每个子目录是一个 Cargo crate，会被根 `Cargo.toml` 的 `services/*` 通配自动加入工作区。

> 这个说明文件必须保留：cargo 的通配成员在目录为空时会报错，有这个文件时通配能匹配到条目（文件本身会被 cargo 忽略）。
