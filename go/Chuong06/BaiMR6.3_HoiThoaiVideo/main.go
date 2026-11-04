// Bài mở rộng MR6.3 – Phòng hội thoại video nhiều client: khung hình nén zlib, chia mảnh ≤ 1200 byte
// trên UDP, điều khiển JSON trên TCP, server ghép lưới (MCU), giới hạn fps/độ phân giải phía gửi
// và phía nhận, chụp lưới thành PNG.
//
// Bài mở rộng, không có trong giáo trình; nối tiếp MR6.2 (hội thoại âm thanh), Bài 6.1, 6.4 (server
// nhiều client) và Bài 4.1 (UDP). Giao thức giống hệt bản Python, nên hai bản nối được với nhau.
//
//	out/app server [host] [cổng_tcp] [cổng_udp]
//	out/app khach <tên> <host> <cổng_tcp> [-fps 10] [-thoi-gian 10] [-mat-goi 0.02] [-ghep tệp.png]
//
// Lệnh server (stdin): tat_camera <tên|tat_ca>, bat_camera <tên|tat_ca>, fps_toi_da <tên|tat_ca> <n>,
// do_phan_giai <tên|tat_ca> <1|2|4>, fps_chung <n>, trang_thai, thoat.
// Lệnh client: tat_camera, bat_camera, nhan_fps <n>, nhan_do_phan_giai <1|2>, chup <nhãn>, thoat.
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chương trình chạy (dùng cho kịch bản).
package main

import (
	"fmt"
	"os"
	"strconv"
)

func main() {
	a := os.Args
	switch {
	case len(a) >= 2 && a[1] == "server":
		host, tcp, udp := "127.0.0.1", 41635, 41636
		if len(a) > 2 {
			host = a[2]
		}
		if len(a) > 3 {
			tcp, _ = strconv.Atoi(a[3])
		}
		if len(a) > 4 {
			udp, _ = strconv.Atoi(a[4])
		}
		runServer(host, tcp, udp)
	case len(a) >= 5 && a[1] == "khach":
		cong, _ := strconv.Atoi(a[4])
		runClient(a[2], a[3], cong, a[5:])
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app server [host] [cổng_tcp] [cổng_udp]")
		fmt.Println("  out/app khach <tên> <host> <cổng_tcp> [-fps 10] [-thoi-gian 10] [-ghep tệp.png]")
		os.Exit(1)
	}
}
