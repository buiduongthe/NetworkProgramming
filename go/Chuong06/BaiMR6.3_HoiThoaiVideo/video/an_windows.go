package video

import (
	"os/exec"
	"syscall"
)

// anCuaSo – ứng dụng giao diện trên Windows gọi ffmpeg/ffplay mà không bật cửa sổ console đen
// (CREATE_NO_WINDOW = 0x08000000).
func anCuaSo(c *exec.Cmd) {
	c.SysProcAttr = &syscall.SysProcAttr{HideWindow: true, CreationFlags: 0x08000000}
}
