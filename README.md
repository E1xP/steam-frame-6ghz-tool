# Steam Frame 6 GHz 设置工具

用于查看 Steam Frame USB 适配器的国家码与 6 GHz 状态，并将运行地区设置为 US。

## 使用

1. 插入适配器，以管理员身份运行 `steam-frame-6ghz-tool.exe`。
2. 只有一个匹配设备时自动选中并查询状态；多个设备时，选择后自动查询。
3. 点击“设置 US”，确认后执行一次并自动复查。
4. 需要重新查询时点击“刷新”（多个设备需重新选择）；“保存日志”可导出操作记录。

## 注意

- 当前程序仅在有限环境下测试，尚未经过广泛验证，不保证在所有设备、系统及驱动版本上正常工作。
- 支持 Windows 10/11 x64。已验证原厂驱动 `5.32.908.2026`；其他版本仅提示未验证，不限制操作。
- 设置仅影响运行时状态，重启或重新插拔后可能需要再次设置。目前没有后台自动应用功能。
- 使用时关闭其他适配器诊断工具。操作失败或结果不确定时，先查看日志，稍后刷新查询。
- 仅用于授权的屏蔽实验环境。设置 US 会改变无线运行策略，请遵守所在地无线电规定。

## 构建

安装 Rust MSVC 工具链、Visual Studio C++ Build Tools 和 Windows SDK 后执行：

```powershell
cargo test --locked
cargo build --release --locked
```

生成文件：`target/release/steam-frame-6ghz-tool.exe`。

## 发布

推送与 `Cargo.toml` 版本一致的标签后，GitHub Actions 会自动测试、构建并发布 Release，附带 Windows x64 程序和 SHA256 校验文件。

```powershell
git tag v0.1.0
git push origin v0.1.0
```

也可在 Actions → Build and release → Run workflow 中选择 `main`，填写已有标签（如 `v0.1.0-preview.1`）手动发布。构建使用该标签的代码。
