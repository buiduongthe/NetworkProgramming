//go:build windows

// Bài 6.5 – SETOPT trên Windows: socket là một Handle.
package main

import "syscall"

func setsockoptInt(fd uintptr, level, opt, value int) error {
	return syscall.SetsockoptInt(syscall.Handle(fd), level, opt, value)
}
