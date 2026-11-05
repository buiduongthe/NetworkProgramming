// Ứng dụng giao diện (Fyne v2) cho MR6.2 – hội thoại âm thanh: server "Bảng điều khiển âm thanh"
// và "Audio Client". Dùng micro/loa thật qua ffmpeg/ffplay nếu có, không có thì tự chuyển sang
// nguồn giả lập (sóng sin) và chỉ đo mức. Giao thức giống bản dòng lệnh, nên client giao diện nối
// được với server dòng lệnh (của mọi ngôn ngữ trong kho) và ngược lại.
//
//	out/app_gui server_gui [host] [cổng_tcp] [cổng_udp]
//	out/app_gui client_gui [tên] [host] [cổng_tcp] [tần_số_giả_lập]
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
	vaiTro := thamSo(1, "")
	a := app.NewWithID("vn.edu.ltm.mr62")
	a.Settings().SetTheme(temSang{})
	switch vaiTro {
	case "server_gui":
		tcp, _ := strconv.Atoi(thamSo(3, "41625"))
		udp, _ := strconv.Atoi(thamSo(4, "41626"))
		s := moCuaSoServer(a, thamSo(2, "127.0.0.1"), tcp, udp)
		s.w.ShowAndRun()
	case "client_gui":
		cong, _ := strconv.Atoi(thamSo(4, "41625"))
		f, _ := strconv.ParseFloat(thamSo(5, "440"), 64)
		c := moCuaSoClient(a, thamSo(2, "A"), thamSo(3, "127.0.0.1"), cong, f)
		c.w.ShowAndRun()
	case "demo_gui":
		tcp, _ := strconv.Atoi(thamSo(3, "41632"))
		udp, _ := strconv.Atoi(thamSo(4, "41633"))
		chayDemo(a, thamSo(2, "127.0.0.1"), tcp, udp, thamSo(5, "ket_qua_gui.png"))
	default:
		fmt.Println("Cách dùng:")
		fmt.Println("  out/app_gui server_gui [host] [cổng_tcp] [cổng_udp]")
		fmt.Println("  out/app_gui client_gui [tên] [host] [cổng_tcp] [tần_số_giả_lập]")
		fmt.Println("  out/app_gui demo_gui [host] [cổng_tcp] [cổng_udp] [ảnh.png]")
		os.Exit(1)
	}
}
