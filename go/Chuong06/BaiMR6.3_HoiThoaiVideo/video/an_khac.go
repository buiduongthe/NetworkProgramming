//go:build !windows

package video

import "os/exec"

// anCuaSo – Linux/macOS không mở cửa sổ console cho tiến trình con, không cần làm gì.
func anCuaSo(c *exec.Cmd) {}
