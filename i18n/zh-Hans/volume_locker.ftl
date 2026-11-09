## Device list / 设备列表

output-devices = 输出设备
input-devices = 输入设备
keep-volume-locked = 锁定音量
notify-on-volume-restore = 音量恢复时通知
keep-unmuted = 保持不静音
notify-on-unmute = 取消静音时通知
device-properties = 属性...
device-settings = 设置...
playback-devices = 播放设备...
recording-devices = 录制设备...

## Windows shortcuts / Windows 快捷入口

sound-settings = 声音设置...
volume-mixer = 音量合成器...

## Default device order / 默认设备顺序

default-output-device-order = 默认输出设备顺序
default-input-device-order = 默认输入设备顺序
device-order = 设备顺序
communication-device-order = 通信设备顺序
current-order = 当前顺序
no-devices-added = 尚未添加设备
no-devices-to-add = 没有可添加的设备
follow-default-output-device = 跟随默认输出设备
follow-default-input-device = 跟随默认输入设备
add-device = 添加设备
add-to-top = 添加到顶部
add-to-bottom = 添加到底部
move-up = 上移
move-down = 下移
move-to-top = 移到顶部
move-to-bottom = 移到底部
remove-device = 移除设备
notify-on-switch = 切换后通知
current-device = 当前设备：{ $device }
no-current-device = 无

## Temporary default device / 临时指定设备

temporary-default-device = 临时指定设备
no-devices-available = 没有可用的设备
temporary-output = 输出
temporary-input = 输入
temporary-communication = 通信
temporary-with-device = { $target }：{ $device }

## Preferences / 首选项

preferences = 首选项
settings-transfer = 设置迁移
transfer-device-settings = 自动迁移设备设置
transfer-device-name-and-icon = 同时迁移设备名称和图标
notify-on-settings-transfer = 迁移时通知
skip-disconnected-wireless-headsets = 跳过未连接的无线耳机
auto-launch = 登录时自动启动
check-updates-on-launch = 启动时检查更新

## Troubleshooting and updates / 疑难解答与更新

troubleshooting = 疑难解答
open-app-folder = 打开应用文件夹...
check-for-updates = 检查更新
update-to-version = 更新到 { $version }...
downloading-update = 正在下载更新...
quit = 退出

## Notifications / 通知。标题说结果，正文说原因与对象。

volume-restored-title = 音量已恢复
volume-restored = 「{ $device }」的音量已从 { $from }% 恢复到 { $to }%。
output-unmuted-title = 输出设备已取消静音
input-unmuted-title = 输入设备已取消静音
device-unmuted = 已按「保持不静音」设置，为「{ $device }」取消静音。
output-switched-title = 默认输出设备已切换
input-switched-title = 默认输入设备已切换
output-communication-switched-title = 默认通信输出设备已切换
input-communication-switched-title = 默认通信输入设备已切换
switched-by-order = 已按设备顺序切换到「{ $device }」。
unknown-device = 未知设备
settings-transferred-title = 设备设置已迁移
settings-transferred = 「{ $device }」的设置已迁移到它的新连接。
update-available-title = 有可用更新
update-available = 新版本 { $version } 已发布，可在托盘菜单中安装。
no-updates-title = 已是最新版本
no-updates = 当前已是最新版本。

## Errors / 错误。标题说结果，正文说动作；{ $error } 是技术细节，保持原文。

directory-not-writable-title = Volume Locker 目录不可写
directory-not-writable = 请把 Volume Locker 放到可写目录，或修正「{ $path }」的权限。
save-state-failed-title = 保存状态失败
save-state-failed = 保存状态时出错：{ $error }
save-state-after-update-failed = 设备更新后保存状态时出错：{ $error }
update-failed-title = 更新失败
update-failed = 更新时出错：{ $error }
update-check-failed-title = 检查更新失败
update-check-failed = 检查更新时出错：{ $error }
auto-launch-failed-title = 登录时自动启动切换失败
auto-launch-failed = 修改登录时自动启动设置时出错：{ $error }
rebuild-menu-failed-title = 重建托盘菜单失败
rebuild-menu-failed = 重建托盘菜单时出错：{ $error }
lock-volume-failed-title = 锁定音量失败
lock-volume-failed = 读取「{ $device }」的音量时出错，音量锁定未生效。
