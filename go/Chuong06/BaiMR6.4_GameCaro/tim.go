// Bài mở rộng MR6.4 – Tìm server Caro trong LAN bằng UDP broadcast (DISCOVER của Bài 6.5).
//
//	out/app tim [cổng_khám_phá] [thời_gian_chờ_ms] [địa_chỉ_broadcast]   (mặc định 9999 1000 255.255.255.255)
package main

import (
	"fmt"
	"os"
	"strconv"
)

func runFind(args []string) {
	port, _ := strconv.Atoi(argOr(args, 0, "9999"))
	waitMs, _ := strconv.Atoi(argOr(args, 1, "1000"))
	bcast := argOr(args, 2, "255.255.255.255")
	fmt.Printf("[tim] Phát discover tới %s:%d, chờ %d ms\n", bcast, port, waitMs)
	list, err := discover(port, waitMs, bcast)
	if err != nil {
		fmt.Println("[tim] Lỗi:", err)
		os.Exit(1)
	}
	fmt.Printf("[tim] Tìm thấy %d server:\n", len(list))
	for _, f := range list {
		fmt.Printf("[tim]   %s tại %s:%d – %d phòng, %d người\n", f.name, f.ip, f.port, f.rooms, f.players)
	}
}
