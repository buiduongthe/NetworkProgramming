// Bài 7.8 – Thiết bị cảm biến và bảng theo dõi dùng MQTT (Mã giả 7.9).
//
// Một chương trình, hai vai trò:
//
//	out/app sensor <mã_thiết_bị> [host] [port] [số_lần] [chu_kỳ_ms] [--mat-ket-noi]
//	out/app dashboard [host] [port] [keep_alive_s] [client_id]
package main

import (
	"fmt"
	"os"
	"strconv"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	role, args := os.Args[1], os.Args[2:]
	switch role {
	case "sensor":
		sensorMain(args)
	case "dashboard":
		dashboardMain(args)
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Cách dùng: out/app sensor|dashboard [tham số...]")
	os.Exit(2)
}

func fail(msg string) {
	fmt.Fprintln(os.Stderr, msg)
	os.Exit(1)
}

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

func portArg(args []string, i int, def int) int {
	p := intArg(args, i, def)
	if p < 1 || p > 65535 {
		fail("Cổng không hợp lệ: " + strconv.Itoa(p))
	}
	return p
}
