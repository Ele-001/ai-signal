# macOS 单文件安装包

版本：0.1.5。打包日期：2026-10-28。

下载：[AI-Signal-0.1.5-macOS-arm64.dmg](../downloads/AI-Signal-0.1.5-macOS-arm64.dmg?raw=true)。安装包位于本仓库的 downloads 目录。

安装盘只包含 `AI Signal.app`、指向 `/Applications` 的安装入口和中文 `安装说明.html`。打开后拖入“应用程序”；更新前退出旧版。源码由 GitHub 单独提供，日常使用无需安装 Node.js、Rust 或其他开发工具。

适用 Apple Silicon（M 系列）、macOS 12.3+；实测环境为 macOS 26.3，未另测最低系统版本、Intel 或 Windows。

## 验证

- 压缩磁盘映像完整性校验通过。
- 只读挂载后确认版本 0.1.5、arm64 可执行文件、Applications 链接及中文安装说明。
- 挂载应用通过 `codesign --verify --deep --strict`；二进制与封装前的签名副本一致。
- 使用本机临时签名封装完整应用资源，未使用 Developer ID 证书、未公证；首次打开可能需要按系统提示确认来源，无需关闭安全保护。
- 本次只整合已验证的 0.1.5 应用，未改变业务代码。

文件大小：3,540,157 字节。SHA256：

```text
32dddfdd42d1c253cb49a93961f68c0b3828b3c394cf6257588d051e41b9fdbe
```
