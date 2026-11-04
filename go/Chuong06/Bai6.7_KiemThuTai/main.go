// Bài 6.7 – Kiểm thử tải: thông lượng và phân vị độ trễ (Mã giả 6.9), kèm hai server của Bài 6.1.
//
// Một chương trình, ba vai trò:
//
//	out/app threaded [host] [port] [max_clients] [delay_ms]   server mỗi kết nối một luồng
//	out/app pool [host] [port] [k] [queue_size] [delay_ms]    server nhóm luồng
//	out/app load [host] [port] [n_requests] [levels]          bộ sinh tải
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
	case "threaded":
		loadGlossary()
		threadedServer(args)
	case "pool":
		loadGlossary()
		poolServer(args)
	case "load":
		loadMain(args)
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Cách dùng: out/app threaded|pool|load [tham số...]")
	os.Exit(2)
}
