//go:build !windows

package tructiep

import "os/exec"

// anCuaSo – Linux/macOS không mở cửa sổ console cho tiến trình con, không cần làm gì.
func anCuaSo(c *exec.Cmd) {}
