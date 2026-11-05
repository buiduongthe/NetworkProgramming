// Bài mở rộng MR6.5 – Phát trực tiếp âm thanh + video 1→n: máy chủ trung chuyển SFU nhân bản gói,
// khung I/P (zlib + XOR), bộ đệm GOP, xin khung khóa (PLI), đồng bộ tiếng-hình với âm thanh làm
// đồng hồ chủ, đăng ký theo lớp.
//
// Bài mở rộng, không có trong giáo trình; nối tiếp MR6.2 (âm thanh RTP), MR6.3 (video chia mảnh),
// Bài 6.1 (server đa luồng), Bài 6.6 (một nguồn – nhiều người nhận). Giao thức giống hệt bản Python
// (và C#, Java, Rust), nên máy phát/máy chủ/người xem viết bằng các ngôn ngữ khác nhau chạy chung được.
// Lõi nằm ở gói con tructiep/ (dùng chung với ứng dụng giao diện ở gui/).
//
//	out/app server [host] [cổng_tcp] [cổng_udp]                       (mặc định cổng_udp = cổng_tcp + 1)
//	out/app phat <tên> <host> <cổng_tcp> [-camera gia_lap|testsrc|<thiết bị>] [-mic gia_lap|<chỉ số>]
//	             [-fps 10] [-gop 20] [-thoi-gian 30] [-mat-goi 0.0]
//	out/app xem <tên> <host> <cổng_tcp> [-thoi-gian 10] [-tre 0] [-dem 5] [-khong-dong-bo]
//	            [-chi-am-thanh] [-chi-video] [-mat-goi 0.0] [-ghep ket_qua_video.png]
//
// Lệnh (stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chương trình chạy):
//
//	server: trang_thai, bo_dem bat|tat, ngat <tên>, thoat
//	phat:   khung_khoa, dung
//	xem:    chup <nhãn>, dong_bo bat|tat, dang_ky am_thanh|video|ca_hai, thoat
package main

import (
	"fmt"
	"os"
	"strconv"
	"sync"
)

var khoaIn sync.Mutex

// inDong – nhiều goroutine cùng in: mỗi dòng in trọn vẹn.
func inDong(tienTo, text string) {
	khoaIn.Lock()
	defer khoaIn.Unlock()
	fmt.Println(tienTo + text)
}

func main() {
	a := os.Args
	switch {
	case len(a) >= 2 && a[1] == "server":
		host, tcp := "127.0.0.1", 41655
		if len(a) > 2 {
			host = a[2]
		}
		if len(a) > 3 {
			tcp, _ = strconv.Atoi(a[3])
		}
		udp := tcp + 1
		if len(a) > 4 {
			udp, _ = strconv.Atoi(a[4])
		}
		chayServer(host, tcp, udp)
	case len(a) >= 5 && a[1] == "phat":
		cong, _ := strconv.Atoi(a[4])
		chayPhat(a[2], a[3], cong, a[5:])
	case len(a) >= 5 && a[1] == "xem":
		cong, _ := strconv.Atoi(a[4])
		chayXem(a[2], a[3], cong, a[5:])
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app server [host] [cổng_tcp] [cổng_udp]")
		fmt.Println("  out/app phat <tên> <host> <cổng_tcp> [-camera gia_lap|testsrc|<thiết bị>] [-mic gia_lap|<chỉ số>]")
		fmt.Println("               [-fps 10] [-gop 20] [-thoi-gian 30] [-mat-goi 0.0]")
		fmt.Println("  out/app xem <tên> <host> <cổng_tcp> [-thoi-gian 10] [-tre 0] [-dem 5] [-khong-dong-bo]")
		fmt.Println("              [-chi-am-thanh] [-chi-video] [-mat-goi 0.0] [-ghep tệp.png]")
		os.Exit(1)
	}
}
