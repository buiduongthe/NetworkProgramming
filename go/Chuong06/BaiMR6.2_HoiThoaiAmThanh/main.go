// Bài mở rộng MR6.2 – Phòng hội thoại âm thanh nhiều client: RTP trên UDP, điều khiển JSON trên TCP,
// server trộn tiếng "mix-minus", bộ đệm jitter, đo mức dBFS, ghi WAV và phân tích bằng Goertzel.
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1, 6.4 (server nhiều client, chat),
// Bài 4.1 (UDP) và Bài 3.3 (định khung thông điệp). Giao thức giống hệt bản Python, nên client
// Python nối được vào server Go và ngược lại.
//
//	out/app server [host] [cổng_tcp] [cổng_udp]
//	out/app khach <tên> <host> <cổng_tcp> [-tan-so 440] [-wav tệp.wav] [-ghi out/ghi.wav]
//	              [-thoi-gian 10] [-mat-goi 0.05] [-dem 3]
//	out/app phantich <tệp.wav> <tần_số:nhãn> ...
//
// Lệnh server (stdin): tat_mic <tên|tat_ca>, bat_mic <tên|tat_ca>, do_loi <tên> <hệ số>,
// am_luong_chung <hệ số>, trang_thai, thoat. Lệnh client: tat_mic, bat_mic, am_luong <hệ số>, thoat.
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
		host, tcp, udp := "127.0.0.1", 41625, 41626
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
	case len(a) >= 4 && a[1] == "phantich":
		phanTich(a[2], a[3:])
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app server [host] [cổng_tcp] [cổng_udp]")
		fmt.Println("  out/app khach <tên> <host> <cổng_tcp> [-tan-so 440] [-ghi out/ghi.wav] ...")
		fmt.Println("  out/app phantich <tệp.wav> <tần_số:nhãn> ...")
		os.Exit(1)
	}
}
