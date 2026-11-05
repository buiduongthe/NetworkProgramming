// Ứng dụng giao diện (Fyne v2) cho MR6.3 – hội thoại video: "Camera Server" (lưới 4×4 camera các
// client) và "Camera Client". Camera thật qua tiến trình ffmpeg nếu có, không có thì "Camera giả lập"
// (khung tổng hợp như bản dòng lệnh). Giao thức giống bản dòng lệnh, nên client giao diện nối được
// với server dòng lệnh (của mọi ngôn ngữ trong kho) và ngược lại.
//
//	out/app_gui server_gui [host] [cổng_tcp] [cổng_udp]
//	out/app_gui client_gui [tên] [host] [cổng_tcp]
//	out/app_gui demo_gui   [host] [cổng_tcp] [cổng_udp] [ảnh.png]   (tự thao tác, chụp cửa sổ)
//
// Mặc định nghe 127.0.0.1; muốn nhận client từ máy khác thì nhập IP của máy (hoặc 0.0.0.0).
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
	a := app.NewWithID("vn.edu.ltm.mr63")
	a.Settings().SetTheme(temSang{})
	switch thamSo(1, "") {
	case "server_gui":
		tcp, _ := strconv.Atoi(thamSo(3, "41635"))
		udp, _ := strconv.Atoi(thamSo(4, "41636"))
		s := moCuaSoServer(a, thamSo(2, "127.0.0.1"), tcp, udp)
		s.w.ShowAndRun()
	case "client_gui":
		cong, _ := strconv.Atoi(thamSo(4, "41635"))
		c := moCuaSoClient(a, thamSo(2, "A"), thamSo(3, "127.0.0.1"), cong)
		c.w.ShowAndRun()
	case "demo_gui":
		tcp, _ := strconv.Atoi(thamSo(3, "41642"))
		udp, _ := strconv.Atoi(thamSo(4, "41643"))
		chayDemo(a, thamSo(2, "127.0.0.1"), tcp, udp, thamSo(5, "ket_qua_gui.png"))
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app_gui server_gui [host] [cổng_tcp] [cổng_udp]")
		fmt.Println("  out/app_gui client_gui [tên] [host] [cổng_tcp]")
		fmt.Println("  out/app_gui demo_gui [host] [cổng_tcp] [cổng_udp] [ảnh.png]")
		os.Exit(1)
	}
}
