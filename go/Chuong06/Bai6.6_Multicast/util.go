// Bài 6.6 – Các hàm tiện ích nhỏ: đọc tham số, in thông báo lỗi, bật tùy chọn socket.
package main

import (
	"fmt"
	"os"
	"strconv"
	"syscall"
)

const GROUP, MPORT = "239.255.42.99", 9998 // CONST GROUP, MPORT

// fail: in thông báo lỗi ra stderr rồi kết thúc.
func fail(msg string) {
	fmt.Fprintln(os.Stderr, msg)
	os.Exit(1)
}

// intArg: tham số nguyên thứ i (nếu có), ngược lại dùng giá trị mặc định.
func intArg(args []string, i int, def int) int {
	if len(args) <= i {
		return def
	}
	v, err := strconv.Atoi(args[i])
	if err != nil {
		fail("Tham số phải là số nguyên: " + args[i])
	}
	return v
}

// portArg: tham số cổng thứ i, kiểm tra phạm vi 1–65535.
func portArg(args []string, i int, def int) int {
	p := intArg(args, i, def)
	if p < 1 || p > 65535 {
		fail("Cổng không hợp lệ: " + strconv.Itoa(p))
	}
	return p
}

// setIPOpt: SETOPT(s, opt, value) ở mức IPPROTO_IP trên socket đã tạo (qua SyscallConn).
func setIPOpt(rc syscall.RawConn, opt, value int) error {
	var err error
	rc.Control(func(fd uintptr) {
		err = setsockoptInt(fd, syscall.IPPROTO_IP, opt, value)
	})
	return err
}
