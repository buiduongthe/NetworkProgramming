// Bài 4.5 – Xác thực địa chỉ nguồn bằng mã thách thức không trạng thái (Mã giả 4.8; Mục 4.6.1 Giáo trình Lập trình mạng).
//
// Chạy trong thư mục bài (server đọc glossary.json ở thư mục hiện hành):
//
//	go run . server [host] [port]                    (mặc định 0.0.0.0 9100)
//	go run . client [host] [port] [cookie_ban_dau]   (mặc định 127.0.0.1 9100)
//	go run . raw <host> <port> <nội dung>            (công cụ thử: gửi nguyên văn một datagram)
//
// cookie_ban_dau (thêm cho bài thực hành): mô phỏng client dùng cookie giả mạo hoặc của địa chỉ khác.
package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	rest := os.Args[2:]
	switch os.Args[1] {
	case "server":
		os.Exit(RunCookieServer(rest))
	case "client":
		os.Exit(RunCookieClient(rest))
	case "raw":
		os.Exit(RunRawSend(rest))
	}
	usage()
}

func usage() {
	fmt.Println("Cách dùng: go run . server|client [host] [port] [cookie_ban_dau] | raw <host> <port> <nội dung>")
	os.Exit(1)
}
