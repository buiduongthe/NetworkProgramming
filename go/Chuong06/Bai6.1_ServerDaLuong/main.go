// Bài 6.1 – Server tra cứu đa luồng và server nhóm luồng (Mã giả 6.1, 6.2).
//
// Một chương trình, ba vai trò:
//
//	out/app threaded [host] [port] [max_clients]   server mỗi kết nối một luồng (Mã giả 6.1)
//	out/app pool [host] [port] [k] [queue_size]    server nhóm luồng (Mã giả 6.2)
//	out/app client [host] [port] [n] [hold_ms]     mở n client đồng thời
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
	case "client":
		multiClient(args)
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Cách dùng: out/app threaded|pool|client [tham số...]")
	os.Exit(2)
}
