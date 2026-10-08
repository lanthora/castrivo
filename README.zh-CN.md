# Castrivo

[English](README.md)

把视频投屏到电脑。

打开 Castrivo，在支持投屏的视频应用中选择它，即可在电脑上观看。无需另外安装播放器。

## 开始使用

1. 在电脑上打开 Castrivo。
2. 将投屏设备和电脑连接到同一个网络。
3. 在支持投屏的应用中打开视频，点击投屏按钮。
4. 选择 **Castrivo**，或 Castrivo 窗口中显示的设备名称。

观看时请保持 Castrivo 运行。你可以通过投屏应用控制播放，也可以直接使用视频窗口中的控件。

## 安装

从 [GitHub Releases](https://github.com/lanthora/castrivo/releases) 下载 Windows、macOS（Apple Silicon）或 Linux 测试版。如需自行构建，请参阅[源码构建说明（英文）](docs/building.md)。

在 macOS 上，将 `Castrivo.app` 放入「应用程序」文件夹后打开。面向 macOS 11 构建的版本仅适用于 Apple Silicon；旧版 macOS 的实际运行兼容性尚未验证。

## 播放操作

将鼠标移到视频上即可显示控件。点击或拖动进度条可以跳转播放位置，使用音量滑块可以调整声音。暂停时控件保持显示，播放时会自动隐藏。

| 快捷键 | 操作 |
| --- | --- |
| 空格 | 暂停或继续播放 |
| 左 / 右方向键 | 后退或前进五秒 |
| F | 进入或退出全屏 |
| Tab | 固定显示控件，或恢复自动隐藏 |
| Escape | 退出 Castrivo |

投屏新视频会替换当前视频。投屏设备断开连接后，视频仍会继续播放。关闭窗口会退出 Castrivo。

## 兼容性

Castrivo 仍处于早期开发阶段，已在 macOS 上成功测试哔哩哔哩投屏。其他应用和视频的表现可能不同；部分应用可能无法发现 Castrivo，或限制可播放视频的设备。

目前已在搭载 Apple Silicon 芯片的 Mac 上测试，Windows 和 Linux 尚未验证。

Castrivo 用于播放投屏视频，不支持设备屏幕镜像。直播视频可能不支持跳转播放位置。

## 找不到 Castrivo 时

- 确认 Castrivo 正在运行，且两台设备连接到同一个网络。
- 检查 VPN、访客网络或防火墙是否阻止设备相互发现。
- 重新打开视频应用中的投屏菜单。

如果视频无法播放，可以尝试其他视频或应用。如需反馈问题，请[提交 Issue](https://github.com/lanthora/castrivo/issues)，说明操作系统、投屏应用和遇到的情况。

## 许可证

GPL-3.0-or-later. [许可证全文](LICENSE) · [第三方依赖说明](THIRD-PARTY-NOTICES.md)
