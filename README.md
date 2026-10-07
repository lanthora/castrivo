# Castrivo

[简体中文](README.zh-CN.md)

Cast videos to your desktop.

Open Castrivo, choose it from a compatible video app's casting menu, and watch on your computer. No separate player is required.

## Get started

1. Open Castrivo on your computer.
2. Connect your casting device and computer to the same network.
3. Open a video in a compatible app and tap its casting button.
4. Select **Castrivo**, or the device name shown in the Castrivo window.

Keep Castrivo open while watching. You can control playback from the casting app or directly in the video window.

## Installation

Public downloads are not available yet. To build Castrivo yourself, see [Building from source](docs/building.md).

On macOS, copy `Castrivo.app` to your Applications folder and open it. The current macOS build requires Apple Silicon and macOS 26 or later.

## Playback

Move the pointer over the video to reveal the controls. Click or drag the progress bar to seek, and use the volume slider to adjust the sound. Controls stay visible while paused and hide automatically during playback.

| Shortcut | Action |
| --- | --- |
| Space | Pause or resume |
| Left / Right | Seek backward or forward five seconds |
| F | Enter or leave fullscreen |
| Tab | Keep controls visible or return to automatic hiding |
| Escape | Quit Castrivo |

Casting a new video replaces the current one. Playback continues if the casting device disconnects. Closing the window quits Castrivo.

## Compatibility

Castrivo is in early development. Casting from Bilibili has been tested successfully on macOS. Other apps and videos may behave differently; some apps may not list Castrivo or may restrict which devices can play their videos.

Currently tested on macOS with Apple Silicon. Windows and Linux are not yet verified.

Castrivo plays cast videos; it does not mirror your device's screen. Live videos may not support seeking.

## If Castrivo does not appear

- Make sure Castrivo is running and both devices are on the same network.
- Check whether a VPN, guest network, or firewall is preventing devices from finding each other.
- Reopen the casting menu in your video app.

If a video fails to play, try another video or app. To report a problem, [open an issue](https://github.com/lanthora/castrivo/issues) with your operating system, casting app, and what happened.
