// Bài 7.3 – Server WebSocket tối giản (Mã giả 7.3, 7.4), client thử và chương trình kiểm thử khung.
//
// Một chương trình, ba vai trò:
//
//	out/app server [host] [port]    server WebSocket
//	out/app client [host] [port]    client: gửi từng dòng nhập từ bàn phím
//	out/app kiemthu [host] [port]   năm ca kiểm thử với khung tự tạo
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	role, args := os.Args[1], os.Args[2:]
	switch role {
	case "server":
		serverMain(args)
	case "client":
		clientMain(args)
	case "kiemthu":
		kiemThuMain(args)
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Cách dùng: out/app server|client|kiemthu [tham số...]")
	os.Exit(2)
}
