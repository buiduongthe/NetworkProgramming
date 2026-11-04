//go:build !windows

// Bài 6.6 – SETOPT trên Linux, macOS: mô tả tệp của socket là số nguyên int.
package main

import "syscall"

func setsockoptInt(fd uintptr, level, opt, value int) error {
	return syscall.SetsockoptInt(int(fd), level, opt, value)
}
