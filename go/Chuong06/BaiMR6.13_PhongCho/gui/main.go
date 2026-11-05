// Ứng dụng giao diện (Fyne v2) cho MR6.13 – bán vé có phòng chờ ảo: bảng điều khiển máy chủ (4 chế độ
// xử lý tranh chấp, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, vé còn, hàng đợi, thùng
// token, so sánh bật/tắt phòng chờ) và trang bán vé (áp phích, phòng chờ với thanh tiến trình vị trí,
// trang chọn vé có đồng hồ lượt). Giao thức giống bản dòng lệnh, nên cửa sổ nối được với máy chủ/bộ mô
// phỏng/người dùng dòng lệnh của mọi ngôn ngữ trong kho.
//
//	out/app_gui server_gui [host] [cổng]            (mặc định 127.0.0.1 41777; phòng chờ bật)
//	out/app_gui phong_cho_gui [tên] [host] [cổng]
//	out/app_gui demo_gui [host] [cổng]              (tự thao tác, chụp cửa sổ; chạy dưới xvfb-run)
//
// Chạy riêng từng cửa sổ: nhật ký cũng in ra dòng lệnh (tiền tố [server], [An]) và cứ 5 s in một dòng
// số liệu – chuyển hướng được vào tệp.
package main

import (
	"fmt"
	"os"
	"path/filepath"
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
	a := app.NewWithID("vn.edu.ltm.mr613")
	a.Settings().SetTheme(temSang{})
	switch thamSo(1, "") {
	case "server_gui":
		inRa = true
		cong, _ := strconv.Atoi(thamSo(3, "41777"))
		moCuaSoServer(a, thamSo(2, "127.0.0.1"), cong).w.ShowAndRun()
	case "phong_cho_gui":
		inRa = true
		cong, _ := strconv.Atoi(thamSo(4, "41777"))
		moCuaSoPhongCho(a, thamSo(2, "An"), thamSo(3, "127.0.0.1"), cong).w.ShowAndRun()
	case "demo_gui":
		cong, _ := strconv.Atoi(thamSo(3, "41777"))
		thuMuc, _ := filepath.Abs(".")
		chayDemo(a, thamSo(2, "127.0.0.1"), cong, thuMuc)
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app_gui server_gui [host] [cổng]")
		fmt.Println("  out/app_gui phong_cho_gui [tên] [host] [cổng]")
		fmt.Println("  out/app_gui demo_gui [host] [cổng]")
		os.Exit(1)
	}
}
