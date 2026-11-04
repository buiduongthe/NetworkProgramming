// Bài 6.5 – Các hàm tiện ích nhỏ: đọc tham số, in thông báo, xử lý Ctrl+C, bật tùy chọn socket.
package main

import (
	"fmt"
	"os"
	"os/signal"
	"strconv"
	"syscall"
)

const DISC_PORT = 9999 // CONST DISC_PORT ← 9999

// fail: in thông báo lỗi ra stderr rồi kết thúc.
func fail(msg string) {
	fmt.Fprintln(os.Stderr, msg)
	os.Exit(1)
}

// onInterrupt: khi nhấn Ctrl+C (SIGINT), in thông báo rồi kết thúc chương trình.
func onInterrupt(msg string) {
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		fmt.Println(msg)
		os.Exit(0)
	}()
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

// setOpt trả về hàm Control cho net.ListenConfig: bật một tùy chọn SOL_SOCKET (SETOPT(s, opt, TRUE))
// ngay sau SOCKET và trước BIND.
func setOpt(opt int) func(network, address string, c syscall.RawConn) error {
	return func(network, address string, c syscall.RawConn) error {
		var err error
		c.Control(func(fd uintptr) {
			err = setsockoptInt(fd, syscall.SOL_SOCKET, opt, 1)
		})
		return err
	}
}
