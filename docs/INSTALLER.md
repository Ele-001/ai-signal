# macOS 单文件安装包

版本：0.1.7。打包日期：2026-10-30。

下载：[AI红路灯-0.1.7-macOS-arm64.dmg](../downloads/AI%E7%BA%A2%E8%B7%AF%E7%81%AF-0.1.7-macOS-arm64.dmg?raw=true)。安装包位于本仓库的 downloads 目录。

打开后将 AI红路灯.app 拖入 Applications（应用程序）。更新前从托盘退出旧版；设置继续保留。安装盘内含中文安装说明，日常使用无需开发工具。

面向 Apple Silicon（M 系列）、macOS 12.3+。实测系统为 macOS 26.3，最低系统、Intel 和 Windows 未另行实测。使用 macOS 内置的 LZMA 磁盘映像压缩，应用文件内容不变。

## 验证

- 生产前端和 macOS arm64 应用构建通过。
- 磁盘映像完整性校验通过；只读挂载核对版本、应用标识、Applications 链接及安装说明。
- 挂载应用通过 `codesign --verify --deep --strict`，可执行文件与封装前签名副本一致。
- 使用本机临时签名，未使用 Developer ID 证书、未公证。首次打开可能需要按系统提示确认来源，无需关闭系统安全保护。
- 前端 10 项、后台 33 项、Python 事件适配 10 项测试通过。
- 本次新增的按需创建及关闭设置窗口，尚未完成桌面重复打开实测。完整范围和性能数据见 [PERFORMANCE.md](PERFORMANCE.md)。

文件大小：2,260,181 字节。SHA256：

```text
94b34426eea254b2a2a613b04c873155b6b86dd33ba1c1d1eb702b6c7bccbf8f
```

## 变更

新增银白、机械两款竖版灯体，移除趣味款，保留经典。设置窗口按需创建与释放，减少重复任务快照、标题索引读取和无变化界面刷新，保留原有扫描频率与红灯恢复逻辑。
