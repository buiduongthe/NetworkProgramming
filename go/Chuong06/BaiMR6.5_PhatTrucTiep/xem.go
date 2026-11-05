package main

// Người xem phát trực tiếp (bản dòng lệnh). Lõi ở tructiep/nguoixem.go (NguoiXem). Mỗi giây in: số gói
// tiếng nhận, tổng gói mất, trễ đầu-cuối TB, số khung hiện và lệch tiếng-hình TB trong giây đó; cuối
// cùng in TỔNG KẾT. Không phát ra loa (bản giao diện mới phát loa nếu có ffplay) – đồng hồ phát vẫn
// chạy đúng nhịp 20 ms.
//
// Lệnh (stdin): chup <nhãn>   dong_bo bat|tat   dang_ky am_thanh|video|ca_hai   thoat
//
//	-thoi-gian  tổng thời gian chạy (giây, tính cả thời gian chờ -tre)
//	-tre        chờ bao nhiêu giây rồi mới vào xem (người vào muộn)
//	-mat-goi    giả lập tỉ lệ mất gói ở chiều nhận (cả tiếng và hình)

import (
	"flag"
	"fmt"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	tt "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.5_PhatTrucTiep/tructiep"
)

func chayXem(ten, host string, cong int, args []string) {
	fs := flag.NewFlagSet("xem", flag.ExitOnError)
	thoiGian := fs.Float64("thoi-gian", 10, "tổng thời gian chạy (giây)")
	tre := fs.Float64("tre", 0, "chờ bao nhiêu giây rồi mới vào xem")
	dem := fs.Int("dem", 5, "độ sâu bộ đệm jitter (khung 20 ms)")
	khongDongBo := fs.Bool("khong-dong-bo", false, "tắt đồng bộ tiếng-hình")
	chiAm := fs.Bool("chi-am-thanh", false, "chỉ nhận âm thanh")
	chiVideo := fs.Bool("chi-video", false, "chỉ nhận video")
	matGoi := fs.Float64("mat-goi", 0, "giả lập tỉ lệ mất gói nhận")
	ghep := fs.String("ghep", "", "ghép các ảnh đã chụp thành tệp PNG này khi kết thúc")
	fs.Parse(args)

	log := func(s string) { inDong("[xem "+ten+"] ", s) }
	t0 := tt.BayGio()
	nx := tt.MoiNguoiXem(ten, log, max(1, min(25, *dem)), !*khongDongBo, !*chiVideo, !*chiAm, *matGoi, nil)
	var khoaChup sync.Mutex
	var daChup []tt.AnhNhan
	xong := make(chan struct{}, 1)
	lenh := func(dong string) {
		p := strings.Fields(dong)
		log("> " + dong)
		switch {
		case p[0] == "dong_bo" && len(p) == 2 && (p[1] == "bat" || p[1] == "tat"):
			nx.DatDongBo(p[1] == "bat")
		case p[0] == "dang_ky" && len(p) == 2 && (p[1] == "am_thanh" || p[1] == "video" || p[1] == "ca_hai"):
			nx.DangKy(p[1] != "video", p[1] != "am_thanh")
		case p[0] == "chup":
			c := nx.Chup()
			if c == nil {
				log("chưa có hình để chụp")
				return
			}
			w, h, rgb := tt.AnhChuThich(c)
			os.MkdirAll("out", 0o755)
			khoaChup.Lock()
			duong := fmt.Sprintf("out/%s_chup_%d.png", ten, len(daChup)+1)
			tt.GhiPNG(duong, rgb, w, h)
			nhan := strings.Join(p[1:], " ")
			if nhan == "" {
				nhan = duong
			}
			daChup = append(daChup, tt.AnhNhan{W: w, H: h, Rgb: rgb, Nhan: nhan})
			khoaChup.Unlock()
			tieng := "–"
			if c.DongHo != nil {
				tieng = fmt.Sprintf("%.3f s", *c.DongHo/1000)
			}
			log(fmt.Sprintf("Đã chụp %s: hình %.3f s, tiếng %s", duong, float64(c.Ts)/1000, tieng))
		case p[0] == "thoat":
			xong <- struct{}{}
		default:
			log("lệnh không hợp lệ")
		}
	}
	go tt.DocLenhStdin(t0, lenh)
	tinHieu := make(chan os.Signal, 1)
	signal.Notify(tinHieu, os.Interrupt, syscall.SIGTERM)
	if *tre > 0 {
		log(fmt.Sprintf("Chờ %.1f s rồi mới vào xem (vào muộn)", *tre))
		time.Sleep(time.Duration(*tre * float64(time.Second)))
	}
	if _, err := nx.KetNoi(host, cong); err != nil {
		log("Không kết nối được: " + err.Error())
		return
	}
	tr := nx.TrangThai()
	nhan := "tiếng+hình"
	if !tr.Video {
		nhan = "tiếng"
	} else if !tr.AmThanh {
		nhan = "hình"
	}
	dongBo := "TẮT"
	if tr.DongBo {
		dongBo = "BẬT"
	}
	mat := ""
	if *matGoi > 0 {
		mat = ", mất " + strconv.FormatFloat(*matGoi*100, 'g', 6, 64) + "%"
	}
	log(fmt.Sprintf("Nhận %s, đồng bộ %s, jitter %d ms%s", nhan, dongBo, tr.DoSau*20, mat))
	log("Mỗi giây: gói tiếng, mất, trễ ms | khung hiện, lệch ms")
	giay, moc := 0, tt.BayGio()
vong:
	for tt.BayGio()-t0 < *thoiGian && nx.DaKetNoi() {
		if tt.BayGio()-moc >= 1.0 {
			moc += 1.0
			giay++
			r := nx.ThongKeMoiGiay()
			log(fmt.Sprintf("%2ds tiếng %2d mất %d trễ %3s | hình %2d lệch %4s", giay, r.Am, r.Mat,
				tt.ChuMs(r.Tre, false), r.Hien, tt.ChuMs(r.Lech, true)))
		}
		select {
		case <-xong:
			break vong
		case <-tinHieu:
			break vong
		case <-time.After(20 * time.Millisecond):
		}

	}
	trang := nx.TrangThai()
	nx.Dong()
	for _, d := range tt.DongTongKetXem(trang, *tre) {
		log(d)
	}
	if *matGoi > 0 {
		log(fmt.Sprintf("giả lập mất %d gói nhận", trang.BoGiaLap))
	}
	khoaChup.Lock()
	defer khoaChup.Unlock()
	if *ghep != "" && len(daChup) > 0 {
		W, H, err := tt.GhepAnh(*ghep, daChup, 2)
		if err != nil {
			log("Lỗi ghi ảnh: " + err.Error())
		} else {
			log(fmt.Sprintf("Đã ghép %d ảnh vào %s (%dx%d)", len(daChup), *ghep, W, H))
		}
	}
}
