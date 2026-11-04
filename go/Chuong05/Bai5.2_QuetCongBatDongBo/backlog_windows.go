//go:build windows

package main

import (
	"net"
	"syscall"
)

// setBacklog gọi lại listen() với backlog mới (Winsock dùng kiểu Handle cho socket).
// Trên Windows, hàng đợi đầy thường được trả lời bằng RST nên cổng hiện là CLOSED.
func setBacklog(ln net.Listener, n int) error {
	rc, err := ln.(*net.TCPListener).SyscallConn()
	if err != nil {
		return err
	}
	var lerr error
	err = rc.Control(func(fd uintptr) { lerr = syscall.Listen(syscall.Handle(fd), n) })
	if err != nil {
		return err
	}
	return lerr
}
