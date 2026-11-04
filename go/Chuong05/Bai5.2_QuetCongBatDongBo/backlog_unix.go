//go:build unix

package main

import (
	"net"
	"syscall"
)

// setBacklog gọi lại listen() với backlog mới trên socket đang lắng nghe.
// net.Listen của Go luôn dùng backlog lớn nhất hệ điều hành cho phép; Linux cho phép gọi listen()
// lần nữa để đổi kích thước hàng đợi.
func setBacklog(ln net.Listener, n int) error {
	rc, err := ln.(*net.TCPListener).SyscallConn()
	if err != nil {
		return err
	}
	var lerr error
	err = rc.Control(func(fd uintptr) { lerr = syscall.Listen(int(fd), n) })
	if err != nil {
		return err
	}
	return lerr
}
