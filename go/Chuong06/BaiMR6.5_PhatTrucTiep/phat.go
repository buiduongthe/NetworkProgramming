package main

// Máy phát trực tiếp (bản dòng lệnh): camera + micro -> máy chủ. Lõi ở tructiep/mayphat.go (MayPhat).
// Mặc định camera giả lập "bảng clapper" (đồng hồ, số khung, chớp trắng mỗi giây tròn) và micro giả
// lập (440 Hz + bíp 1000 Hz mỗi giây tròn).
//
//	-camera testsrc     ảnh mẫu thử của ffmpeg (cần ffmpeg)
//	-camera <thiết bị>  camera thật qua ffmpeg (/dev/video0, "Tên camera" trên Windows, 0 trên macOS)
//	-mic <chỉ số>       micro thật thứ <chỉ số> (0, 1, …) trong danh sách ffmpeg liệt kê được
//	-tep <tệp video>    phát lại hình + tiếng của tệp (mp4, mkv…) qua ffmpeg, lặp lại khi hết tệp;
//	                    thêm -mic gia_lap|<chỉ số> để thay tiếng của tệp bằng micro
//	-tao-tep-mau        tạo trước tệp mẫu 10 s (chớp trắng + bíp mỗi giây) tại đường dẫn của -tep
//
// Thiết bị thật không mở được -> tự lùi về nguồn giả lập (có báo).
// Lệnh (stdin): khung_khoa   dung

import (
	"flag"
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"strconv"
	"syscall"
	"time"

	tt "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.5_PhatTrucTiep/tructiep"
)

func chayPhat(ten, host string, cong int, args []string) {
	fs := flag.NewFlagSet("phat", flag.ExitOnError)
	camera := fs.String("camera", tt.GiaLap, "gia_lap | testsrc | <thiết bị>")
	mic := fs.String("mic", "", "gia_lap | <chỉ số micro> (mặc định: gia_lap, hoặc tiếng của tệp khi có -tep)")
	tep := fs.String("tep", "", "tệp video phát lại (cần ffmpeg)")
	taoMau := fs.Bool("tao-tep-mau", false, "tạo tệp mẫu tại đường dẫn -tep trước khi phát")
	fps := fs.Int("fps", tt.FpsMacDinh, "khung/giây")
	gop := fs.Int("gop", tt.GopMacDinh, "khung I mỗi bao nhiêu khung")
	thoiGian := fs.Float64("thoi-gian", 30, "thời gian phát (giây)")
	matGoi := fs.Float64("mat-goi", 0, "giả lập tỉ lệ mất gói khi gửi")
	fs.Parse(args)

	log := func(s string) { inDong("[phat "+ten+"] ", s) }
	mp := tt.MoiMayPhat(ten, log, *matGoi)
	ma, err := mp.KetNoi(host, cong)
	if err != nil {
		log("Không kết nối được: " + err.Error())
		return
	}
	var cam tt.Camera
	var mc tt.Micro
	if *tep != "" {
		err := error(nil)
		if *taoMau {
			os.MkdirAll(filepath.Dir(*tep), 0o755)
			if err = tt.TaoTepMau(*tep, 10); err == nil {
				log("Đã tạo tệp mẫu " + filepath.Base(*tep) + " (10 s)")
			}
		}
		if err == nil {
			c, m, e := tt.MoTep(*tep, *fps, *mic == "")
			if err = e; err == nil {
				cam = c
				if m != nil {
					mc = m
				}
			}
		}
		if err != nil {
			log("Không mở được tệp video: " + err.Error())
		}
	}
	if cam == nil {
		if *camera != tt.GiaLap && tt.CoFfmpeg() {
			c, err := tt.MoCameraFfmpeg(*camera, *camera, *fps)
			if err != nil {
				cam = tt.MoiCameraGiaLap(ten, int(ma), *fps)
			} else {
				cam = c
			}
		} else {
			if *camera != tt.GiaLap {
				log("Không có ffmpeg: dùng camera giả lập")
			}
			cam = tt.MoiCameraGiaLap(ten, int(ma), *fps)
		}
	}
	if mc == nil {
		tenMic := *mic
		if tenMic == "" {
			tenMic = "gia_lap"
		}
		var tb *tt.ThietBi
		if tenMic != "gia_lap" {
			ds := tt.LietKeMicro()
			if i, err := strconv.Atoi(tenMic); err == nil && i >= 0 && i < len(ds) {
				tb = &ds[i]
			}
		}
		m, loi := tt.MoMic(tb)
		if loi != "" || (tenMic != "gia_lap" && tb == nil) {
			log("Không mở được micro thật: dùng giả lập")
		}
		mc = m
	}
	log(fmt.Sprintf("Mã %d; camera %s", ma, cam.MoTa()))
	log("Micro " + mc.MoTa())
	xong := make(chan struct{}, 1)
	lenh := func(dong string) {
		log("> " + dong)
		switch dong {
		case "khung_khoa":
			mp.KhungKhoaNgay("lệnh khung_khoa")
		case "dung":
			xong <- struct{}{}
		default:
			log("lệnh không hợp lệ")
		}
	}
	t0 := tt.BayGio()
	go tt.DocLenhStdin(t0, lenh)
	tinHieu := make(chan os.Signal, 1)
	signal.Notify(tinHieu, os.Interrupt, syscall.SIGTERM)
	mp.BatDauPhat(cam, mc, *fps, *gop)
	log("Mỗi giây: khung I/P, kbit/s hình, tiếng (cả header)")
	giay, moc := 0, tt.BayGio()
vong:
	for tt.BayGio()-t0 < *thoiGian && mp.DaKetNoi() {
		if tt.BayGio()-moc >= 1.0 {
			moc += 1.0
			giay++
			r := mp.ThongKeMoiGiay()
			log(fmt.Sprintf("%2ds khung I %d P %2d | hình %4.0f tiếng %3.0f kbit/s", giay, r.I, r.P, r.KbpsHinh, r.KbpsAm))
		}
		select {
		case <-xong:
			break vong
		case <-tinHieu:
			break vong
		case <-time.After(20 * time.Millisecond):
		}

	}
	tong := tt.BayGio() - t0
	trang := mp.TrangThai()
	mp.Dong()
	for _, d := range tt.DongTongKetPhat(trang, tong) {
		log(d)
	}
	if *matGoi > 0 {
		log(fmt.Sprintf("giả lập mất %d gói gửi", mp.BoGiaLap))
	}
}
