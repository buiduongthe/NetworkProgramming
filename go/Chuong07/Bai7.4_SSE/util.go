// Bài 7.4 – Các hàm tiện ích nhỏ: đọc tham số cổng, báo lỗi, xử lý Ctrl+C.
package main

import (
	"fmt"
	"os"
	"os/signal"
	"strconv"
)

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

// portArg: tham số cổng thứ i (mặc định def), kiểm tra phạm vi 1–65535.
func portArg(args []string, i int, def int) int {
	if len(args) <= i {
		return def
	}
	p, err := strconv.Atoi(args[i])
	if err != nil {
		fail("Cổng phải là số nguyên: " + args[i])
	}
	if p < 1 || p > 65535 {
		fail("Cổng không hợp lệ: " + args[i])
	}
	return p
}
