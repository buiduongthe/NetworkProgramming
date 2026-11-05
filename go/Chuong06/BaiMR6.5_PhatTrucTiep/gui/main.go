// Ứng dụng giao diện (Fyne v2) cho MR6.5 – phát trực tiếp 1→n: cửa sổ máy chủ (SFU, bộ đệm GOP, xem
// trước luồng, bảng người xem), máy phát (camera thật/tệp video/testsrc/giả lập, micro thật/tiếng của
// tệp/giả lập) và người xem (đồng bộ tiếng-hình, đăng ký theo lớp, loa). Giao thức giống bản dòng
// lệnh, nên cửa sổ nối được với máy chủ/máy phát/người xem dòng lệnh của mọi ngôn ngữ trong kho.
//
//	out/app_gui server_gui [host] [cổng_tcp] [cổng_udp]
//	out/app_gui phat_gui   [tên] [host] [cổng_tcp]
//	out/app_gui xem_gui    [tên] [host] [cổng_tcp]
//	out/app_gui demo_gui   [host] [cổng_tcp] [cổng_udp] [ảnh.png]   (tự thao tác, chụp cửa sổ)
//
// Chạy riêng từng cửa sổ: nhật ký cũng in ra dòng lệnh (tiền tố [server], [phat P], [xem A]) và cứ
// 5 s in một dòng số liệu – chuyển hướng được vào tệp khi thử với thiết bị thật.
package main

import (
	"fmt"
	"os"
	"strconv"

	"fyne.io/fyne/v2/app"
)

func thamSo(i int, macDinh string) string {
	if len(os.Args) > i {
		return os.Args[i]
	}
	return macDinh
}

func main() {
	a := app.NewWithID("vn.edu.ltm.mr65")
	a.Settings().SetTheme(temSang{})
	switch thamSo(1, "") {
	case "server_gui":
		inRa = true
		tcp, _ := strconv.Atoi(thamSo(3, "41657"))
		udp, _ := strconv.Atoi(thamSo(4, strconv.Itoa(tcp+1)))
		moCuaSoServer(a, thamSo(2, "127.0.0.1"), tcp, udp).w.ShowAndRun()
	case "phat_gui":
		inRa = true
		cong, _ := strconv.Atoi(thamSo(4, "41657"))
		moCuaSoPhat(a, thamSo(2, "P"), thamSo(3, "127.0.0.1"), cong).w.ShowAndRun()
	case "xem_gui":
		inRa = true
		cong, _ := strconv.Atoi(thamSo(4, "41657"))
		moCuaSoXem(a, thamSo(2, "A"), thamSo(3, "127.0.0.1"), cong).w.ShowAndRun()
	case "demo_gui":
		tcp, _ := strconv.Atoi(thamSo(3, "41657"))
		udp, _ := strconv.Atoi(thamSo(4, strconv.Itoa(tcp+1)))
		chayDemo(a, thamSo(2, "127.0.0.1"), tcp, udp, thamSo(5, "ket_qua_gui.png"))
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app_gui server_gui [host] [cổng_tcp] [cổng_udp]")
		fmt.Println("  out/app_gui phat_gui [tên] [host] [cổng_tcp]")
		fmt.Println("  out/app_gui xem_gui [tên] [host] [cổng_tcp]")
		fmt.Println("  out/app_gui demo_gui [host] [cổng_tcp] [cổng_udp] [ảnh.png]")
		os.Exit(1)
	}
}
