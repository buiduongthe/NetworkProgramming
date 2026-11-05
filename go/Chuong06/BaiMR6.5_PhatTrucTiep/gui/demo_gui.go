package main

// demo_gui – kịch bản tự thao tác trong MỘT tiến trình (chạy dưới xvfb-run trên Linux): cửa sổ máy
// chủ, máy phát P, người xem A và B. P dùng camera giả lập "bảng clapper" và micro giả lập; A vào
// ngay, đồng bộ tiếng-hình BẬT; B bỏ chọn đồng bộ rồi vào muộn ~3,5 s. Kịch bản "bấm" các nút như
// người dùng (gọi đúng hàm xử lý trên luồng giao diện qua fyne.DoAndWait), in số liệu "[demo] …",
// chụp 4 cửa sổ ghép thành ket_qua_gui.png. Có ffmpeg: thêm một lượt "Mẫu thử ffmpeg (testsrc)" và một
// lượt "Tệp video" (tạo tệp mẫu mp4 có chớp + bíp mỗi giây, phát cả hình lẫn tiếng của tệp; chụp máy
// phát và người xem A thành ket_qua_gui_tep.png).

import (
	"fmt"
	"image"
	"os"
	"path/filepath"
	"time"

	"fyne.io/fyne/v2"

	tt "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.5_PhatTrucTiep/tructiep"
)

func logDemo(text string) { fmt.Println("[demo] " + text) }

func inSoLieu(s *cuaSoServer, p *cuaSoPhat, ds map[string]*cuaSoXem) {
	t := s.lop.TrangThai()
	logDemo(fmt.Sprintf("   server: vào %.0f, ra %.0f kbit/s, hệ số %.2f", t.KbpsVao, t.KbpsRa, t.HeSo))
	for _, x := range t.NguoiXem {
		nguon := x.NguonDau
		if nguon == "" {
			nguon = "–"
		}
		logDemo(fmt.Sprintf("     -> %s: %s, %.0f kbit/s, I đầu %s ms (%s)", x.Ten, x.Nhan, x.Kbps, ms(x.KhungDauMs, false), nguon))
	}
	tp := p.lop.TrangThai()
	logDemo(fmt.Sprintf("   P: I %d P %d, TB I %.0f B, P %.0f B, PLI %d", tp.SoI, tp.SoP, tp.TbI, tp.TbP, tp.PliNhan))
	for _, ten := range []string{"A", "B"} {
		tx := ds[ten].lop.TrangThai()
		g := tx.GiaiDoan[len(tx.GiaiDoan)-1]
		var lech *float64
		if g.N > 0 {
			v := g.Tong / float64(g.N)
			lech = &v
		}
		che := "TẮT"
		if tx.DongBo {
			che = "BẬT"
		}
		logDemo(fmt.Sprintf("   %s: đồng bộ %s, khung đầu %s ms (%s)", ten, che, ms(tx.KhungDauMs, false), tx.NguonDau))
		logDemo(fmt.Sprintf("      lệch TB %s ms, trễ %s ms, mất tiếng %d, đứng %d", ms(lech, true), ms(tx.TreTB, false),
			tx.MatAm, tx.DungHinh))
	}
}

func chayDemo(a fyne.App, host string, tcp, udp int, anhRa string) {
	s := moCuaSoServer(a, host, tcp, udp)
	p := moCuaSoPhat(a, "P", host, tcp)
	ds := map[string]*cuaSoXem{"A": moCuaSoXem(a, "A", host, tcp), "B": moCuaSoXem(a, "B", host, tcp)}
	xa, xb := ds["A"], ds["B"]
	lam := func(f func()) { fyne.DoAndWait(f) }
	cho := func(giay float64) { time.Sleep(time.Duration(giay * float64(time.Second))) }
	choDen := func(dk func() bool) bool { // chờ tối đa 15 s tới khi điều kiện đúng
		for i := 0; i < 300; i++ {
			ok := false
			fyne.DoAndWait(func() { ok = dk() })
			if ok {
				return true
			}
			time.Sleep(50 * time.Millisecond)
		}
		logDemo("Kịch bản chờ quá lâu – dừng")
		return false
	}
	tieuDe := func(w *cuaSoXem) string { var t string; lam(func() { t = w.trangThai.Text }); return t }
	tieuDeP := func() string { var t string; lam(func() { t = p.trangThai.Text }); return t }

	go func() {
		defer fyne.Do(a.Quit)
		cho(1.0)
		logDemo("1. Server: bấm START SERVER")
		lam(func() { bam(s.batDau) })
		logDemo("2. P: Kết nối; camera giả lập (clapper), micro giả lập")
		lam(func() { // kịch bản luôn dùng micro giả lập (mặc định là micro thật); chọn cùng lượt với Kết nối
			p.chonMic.SetSelected(tenMicGia)
			bam(p.ketNoi)
		})
		if !choDen(func() bool { return !p.phat.Disabled() }) {
			return
		}
		logDemo("   P bấm PHÁT (10 fps, GOP 20)")
		lam(func() { bam(p.phat) })
		cho(1.0)
		logDemo("3. A: Kết nối ngay (đồng bộ tiếng-hình BẬT)")
		lam(func() { bam(xa.ketNoi) })
		cho(3.5)
		logDemo("4. B: bỏ chọn 'Đồng bộ', Kết nối (vào muộn 3,5 s)")
		lam(func() { xb.dongBo.SetChecked(false); bam(xb.ketNoi) })
		if !choDen(func() bool { return xb.lop != nil && xb.lop.TrangThai().KhungDauMs != nil }) {
			return
		}
		cho(3.0)
		logDemo("5. Số liệu (bộ đệm GOP bật):")
		inSoLieu(s, p, ds)
		logDemo("6. P bấm 'Khung khóa ngay'; A bấm 'Chụp'")
		lam(func() { bam(p.khungI) })
		cho(0.6)
		var duong string
		lam(func() { duong = xa.chup() })
		logDemo("   A đã chụp " + duong)
		cho(0.5)
		logDemo("7. Chụp 4 cửa sổ")
		anh := chupCuaSo([]fyne.Window{s.w, p.w, xa.w, xb.w})
		if W, H, err := ghepLuoi(anhRa, [][]image.Image{anh[:2], anh[2:]}); err != nil {
			logDemo("   Lỗi ghi ảnh: " + err.Error())
		} else {
			st, _ := os.Stat(anhRa)
			logDemo(fmt.Sprintf("   Đã ghi %s (%d KB)", anhRa, st.Size()/1024))
			_, _ = W, H
		}
		logDemo("8. Server: chọn B, bấm 'Ngắt người chọn'")
		lam(func() { s.chonNguoiXem("B"); bam(s.ngatChon) })
		cho(0.8)
		logDemo("   B: " + tieuDe(xb))
		if tt.CoFfmpeg() {
			logDemo("9. P: DỪNG PHÁT, chọn 'Mẫu thử ffmpeg (testsrc)', PHÁT")
			lam(func() { bam(p.dungPhat) })
			cho(0.5)
			lam(func() { p.chonCam.SetSelected(tenMauThu); bam(p.phat) })
			cho(4.0)
			tp := p.lop.TrangThai()
			logDemo(fmt.Sprintf("   P testsrc: I %d P %d, TB I %.0f B, P %.0f B", tp.SoI, tp.SoP, tp.TbI, tp.TbP))
			logDemo("   P: " + tp.Camera)
			ta := xa.lop.TrangThai()
			logDemo(fmt.Sprintf("   A: khung I %d, P %d, lệch %s ms", ta.KhungI, ta.KhungP, ms(ta.LechHien, true)))

			logDemo("10. P: DỪNG PHÁT, tạo tệp mẫu mp4, nguồn Tệp video, PHÁT")
			lam(func() { bam(p.dungPhat) })
			os.MkdirAll("out", 0o755)
			tep := filepath.Join("out", "tep_mau.mp4")
			if err := tt.TaoTepMau(tep, 10); err != nil {
				logDemo("   Không tạo được tệp mẫu: " + err.Error())
			} else {
				var chon string
				lam(func() { p.datTep(tep); chon = p.chonCam.Selected + " | " + p.chonMic.Selected })
				logDemo("   " + chon)
				cho(0.3)
				lam(func() { bam(p.phat) })
				cho(4.4)
				tp = p.lop.TrangThai()
				logDemo(fmt.Sprintf("   P tệp: I %d P %d, TB I %.0f B, P %.0f B", tp.SoI, tp.SoP, tp.TbI, tp.TbP))
				logDemo("   P: " + tt.CatRune(tp.Camera, 52))
				ta = xa.lop.TrangThai()
				logDemo(fmt.Sprintf("   A: khung I %d, P %d, lệch %s ms, mức tiếng %.1f dBFS", ta.KhungI, ta.KhungP,
					ms(ta.LechHien, true), ta.MucAm))
				anh := chupCuaSo([]fyne.Window{p.w, xa.w})
				ra := filepath.Join(filepath.Dir(anhRa), "ket_qua_gui_tep.png")
				if _, _, err := ghepLuoi(ra, [][]image.Image{anh}); err != nil {
					logDemo("   Lỗi ghi ảnh: " + err.Error())
				} else {
					st, _ := os.Stat(ra)
					logDemo(fmt.Sprintf("   Đã ghi %s (%d KB)", ra, st.Size()/1024))
				}
			}
		}
		logDemo("11. Server bấm STOP SERVER")
		lam(func() { bam(s.dung) })
		choDen(func() bool { return !s.batDau.Disabled() })
		cho(0.6)
		logDemo("   P: " + tt.CatRune(tieuDeP(), 46))
		logDemo("   A: " + tt.CatRune(tieuDe(xa), 46))
		lam(func() {
			for _, w := range []fyne.Window{p.w, xa.w, xb.w, s.w} {
				w.Close()
			}
		})
		cho(0.5)
	}()
	s.w.Show()
	p.w.Show()
	xa.w.Show()
	xb.w.Show()
	a.Run()
	logDemo("Kết thúc")
}
