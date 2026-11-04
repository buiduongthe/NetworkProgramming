//go:build !linux

// Bài 6.3 – Trên hệ điều hành khác Linux: cấu trúc FdSet của gói syscall khác nhau giữa các hệ
// điều hành nên server reactor chỉ được viết cho Linux (client vẫn chạy được ở mọi nơi).
package main

func reactorServer(args []string) {
	fail("[server] Bản Go của server reactor dùng syscall.Select của Linux; hãy chạy trên Linux hoặc WSL")
}
