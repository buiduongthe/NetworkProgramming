// Bài 6.6 – Phát và nhận thông báo dịch vụ qua multicast (Mã giả 6.8).
//
// Một chương trình, hai vai trò:
//
//	out/app announcer [service_port] [tên] [period_ms] [count] [mport]   ANNOUNCER
//	out/app listener [run_ms] [mport]                                     LISTENER
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
	case "announcer":
		announcerMain(args)
	case "listener":
		listenerMain(args)
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Cách dùng: out/app announcer|listener [tham số...]")
	os.Exit(2)
}
