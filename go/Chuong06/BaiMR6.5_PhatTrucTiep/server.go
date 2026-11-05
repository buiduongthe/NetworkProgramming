package main

// Máy chủ trung chuyển phát trực tiếp 1→n (bản dòng lệnh). Lõi nằm ở tructiep/maychu.go (MayChuPhat,
// dùng chung với gui/server_gui.go); tệp này chỉ in sự kiện và nhận lệnh bàn phím.
//
// Lệnh (stdin): trang_thai   bo_dem bat|tat   ngat <tên>   thoat

import (
	"fmt"
	"os"
	"os/signal"
	"strings"
	"syscall"

	tt "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.5_PhatTrucTiep/tructiep"
)

func chayServer(host string, congTCP, congUDP int) {
	log := func(s string) { inDong("[server] ", s) }
	sv := tt.MoiMayChu(log)
	if err := sv.BatDau(host, congTCP, congUDP); err != nil {
		log("Không khởi động được: " + err.Error())
		os.Exit(1)
	}
	xong := make(chan struct{}, 1)
	lenh := func(dong string) {
		p := strings.Fields(dong)
		log("> " + dong)
		switch {
		case p[0] == "trang_thai":
			for _, d := range tt.BangTrangThai(sv.TrangThai()) {
				log(d)
			}
		case p[0] == "bo_dem" && len(p) == 2 && (p[1] == "bat" || p[1] == "tat"):
			sv.DatBoDem(p[1] == "bat")
		case p[0] == "ngat" && len(p) == 2:
			log(fmt.Sprintf("Đã ngắt %d người xem", sv.Ngat(p[1], 0)))
		case p[0] == "thoat":
			xong <- struct{}{}
		default:
			log("lệnh không hợp lệ")
		}
	}
	t0 := tt.BayGio()
	go tt.DocLenhStdin(t0, lenh)
	tinHieu := make(chan os.Signal, 1) // Ctrl+C (công cụ demo dừng server bằng SIGINT)
	signal.Notify(tinHieu, os.Interrupt, syscall.SIGTERM)
	select {
	case <-xong:
	case <-tinHieu:
	}
	for _, d := range tt.BangTongKet(sv.TrangThai()) {
		log(d)
	}
	sv.Dung()
}
