package main

// demo_gui – kịch bản minh họa tự động trong MỘT tiến trình (chạy dưới xvfb-run trên Linux): bảng điều
// khiển máy chủ và ba ứng dụng ngân hàng An (TK01), Bình (TK02), Chi (TK03). Kịch bản bấm nút như người
// dùng (gọi đúng hàm xử lý trên luồng giao diện qua fyne.DoAndWait), in "[demo] …":
//   1–2. chế độ khoa (khóa theo thứ tự mã): An→Bình và Bình→An CÙNG LÚC -> cả hai thành công;
//   3.   bật "khóa ngây thơ" + Đặt lại, trễ 0–50 ms: An→Bình và Bình→An cùng lúc -> KHÓA CHẾT, sau 2 s
//        một giao dịch bị hủy ("khóa chết: …"), giao dịch kia đi tiếp; chụp ket_qua_gui_khoa_chet.png;
//   4.   tắt "khóa ngây thơ" + Đặt lại: lặp lại -> cả hai thành công ngay;
//   5.   chế độ khong_khoa, trễ 0–20 ms: An→Bình, Bình→Chi, Chi→An cùng lúc -> TỔNG TIỀN LỆCH (đỏ);
//        chụp ket_qua_gui_tranh_chap.png;
//   6.   So sánh 4 chế độ (mô phỏng 500 người × 5 lần, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
//   7.   Đặt lại, An chuyển cho Chi; chụp 4 cửa sổ ghép thành ket_qua_gui.png.

import (
	"fmt"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.12_ChuyenKhoan/tranhchap"
)

func chayDemo(a fyne.App, host string, cong int, thuMuc string) {
	sv := moCuaSoServer(a, host, cong)
	an, binh, chi := moCuaSoNganHang(a, "An", host, cong), moCuaSoNganHang(a, "Bình", host, cong),
		moCuaSoNganHang(a, "Chi", host, cong)
	ds := []*cuaSoNganHang{an, binh, chi}
	k := &kichBan{}
	inCuaSo := func(ds []*cuaSoNganHang) {
		for _, cs := range ds {
			var t string
			k.lam(func() { t = strings.Split(cs.thongBao.text(), "\n")[0] })
			logDemo(fmt.Sprintf("   %s: %s", cs.ten, t))
		}
	}
	inServer := func() {
		tk := sv.lop.ThongKeDayDu()
		logDemo(fmt.Sprintf("   máy chủ: %d yc, tc %d, từ chối %d, sai phạm %d", tc.SoNguyen(tk["so_yc"]),
			tc.SoNguyen(tk["thanh_cong"]), tc.SoNguyen(tk["tu_choi"]), tc.SoNguyen(tk["sai_pham_tong"])))
		logDemo("   " + tc.Chuoi(tk["tom_tat"]))
		for i, v := range tc.DSChuoi(tk["vi_du_sai_pham"]) {
			if i < 2 {
				logDemo("     ví dụ: " + v)
			}
		}
	}
	chuyen := func(cs *cuaSoNganHang, den string, soTien int64) { // trên luồng giao diện
		cs.chonNguoiNhan(den)
		cs.datTien(soTien)
		bam(cs.nutChuyen)
	}
	tre := func(ms string) {
		k.lam(func() {
			sv.oTre.SetText(ms)
			bam(sv.nutTre)
		})
	}
	soKq := func(ds ...*cuaSoNganHang) int {
		n := 0
		for _, cs := range ds {
			n += cs.soKq
		}
		return n
	}
	cheo := func() { // An→Bình 500.000 đ và Bình→An 300.000 đ trong cùng một nhịp giao diện (cùng lúc)
		var dem int
		k.lam(func() {
			dem = soKq(an, binh)
			chuyen(an, "TK02", 500_000)
			chuyen(binh, "TK01", 300_000)
		})
		k.choDen(func() bool { return soKq(an, binh) == dem+2 })
	}
	ngayTho := func(cs *cuaSoNganHang) bool { b, _ := cs.tt["khoa_ngay_tho"].(bool); return b }
	go func() {
		defer fyne.Do(a.Quit)
		k.cho(0.5)
		logDemo("1. Máy chủ: START SERVER (khoa, khóa theo thứ tự mã)")
		k.lam(func() { bam(sv.nutStart) })
		k.cho(0.3)
		logDemo("   An, Bình, Chi: Kết nối (TK01, TK02, TK03)")
		k.lam(func() {
			for _, cs := range ds {
				bam(cs.nutKetNoi)
			}
		})
		if !k.choDen(func() bool { return an.tt != nil && binh.tt != nil && chi.tt != nil }) {
			return
		}
		k.cho(1.0)
		logDemo("2. An→Bình 500.000 đ và Bình→An 300.000 đ CÙNG LÚC")
		cheo()
		k.cho(0.4)
		inCuaSo(ds[:2])
		logDemo("3. Bật 'khóa ngây thơ', Đặt lại; trễ 0–50 ms")
		k.lam(func() {
			sv.datKhoaNgayTho(true)
			bam(sv.nutDatLai)
		})
		tre("50")
		k.choDen(func() bool { return ngayTho(an) && tc.SoNguyen(an.tt["so_gd"]) == 0 })
		k.cho(0.5)
		logDemo("   An→Bình và Bình→An cùng lúc…")
		cheo()
		k.cho(0.4)
		inCuaSo(ds[:2])
		logDemo(fmt.Sprintf("   máy chủ: khóa chết phát hiện %d", tc.SoNguyen(sv.lop.TrangThai()["khoa_chet"])))
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui_khoa_chet.png"), [][]fyne.Window{{sv.w}, {an.w, binh.w}})
		logDemo("4. Tắt 'khóa ngây thơ', Đặt lại; lặp lại chuyển chéo")
		k.lam(func() {
			sv.datKhoaNgayTho(false)
			bam(sv.nutDatLai)
		})
		k.choDen(func() bool { return !ngayTho(an) && tc.SoNguyen(an.tt["so_gd"]) == 0 })
		k.cho(0.5)
		cheo()
		k.cho(0.4)
		inCuaSo(ds[:2])
		logDemo("5. Máy chủ: khong_khoa, trễ 0–20 ms; Đặt lại")
		tre("20")
		k.lam(func() { sv.chonCheDo("khong_khoa") })
		k.choDen(func() bool {
			for _, cs := range ds {
				if cs.tt["che_do"] != "khong_khoa" {
					return false
				}
			}
			return true
		})
		k.lam(func() { bam(sv.nutDatLai) })
		k.choDen(func() bool { return tc.SoNguyen(an.tt["so_gd"]) == 0 })
		k.cho(0.5)
		logDemo("   An→Bình, Bình→Chi, Chi→An 1.000.000 đ cùng lúc")
		var dem int
		k.lam(func() {
			dem = soKq(ds...)
			for i, cs := range ds {
				chuyen(cs, []string{"TK02", "TK03", "TK01"}[i], 1_000_000)
			}
		})
		k.choDen(func() bool { return soKq(ds...) == dem+3 })
		k.cho(0.6)
		inCuaSo(ds)
		inServer()
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui_tranh_chap.png"), [][]fyne.Window{{sv.w, chi.w}})
		logDemo("6. Máy chủ: khoa, trễ 0–2 ms; So sánh 4 chế độ")
		tre("2")
		k.lam(func() { sv.chonCheDo("khoa") })
		k.choDen(func() bool { return sv.lop.Kho.CheDo() == "khoa" })
		k.cho(0.3)
		k.lam(func() { bam(sv.nutSoSanh) })
		k.cho(0.3)
		if !k.choDen(func() bool { return !sv.dangMoPhong }) {
			return
		}
		var hang []tc.HangSoSanh
		var kl string
		k.lam(func() { hang, kl = sv.hangSoSanh, sv.nhanKetLuan.Text })
		for _, d := range tc.BangSoSanh(hang) {
			logDemo(d)
		}
		for _, cau := range strings.Split(kl, ". ") {
			logDemo("   " + strings.TrimRight(cau, "."))
		}
		k.cho(0.5)
		logDemo("7. Đặt lại; An chuyển 200.000 đ cho Chi")
		k.lam(func() { bam(sv.nutDatLai) })
		k.choDen(func() bool { return tc.SoNguyen(an.tt["so_gd"]) == 0 })
		k.cho(0.5)
		k.lam(func() {
			dem = an.soKq
			chuyen(an, "TK03", 200_000)
		})
		k.choDen(func() bool { return an.soKq == dem+1 })
		k.cho(0.8)
		inCuaSo([]*cuaSoNganHang{an})
		var t string
		k.lam(func() { t = chi.nhanDen.text() })
		logDemo("   Chi: " + t)
		logDemo("   máy chủ: " + tc.Chuoi(sv.lop.ThongKeDayDu()["tom_tat"]))
		logDemo("8. Chụp 4 cửa sổ")
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui.png"), [][]fyne.Window{{sv.w, an.w}, {binh.w, chi.w}})
		logDemo("9. Máy chủ: STOP SERVER")
		k.lam(func() { bam(sv.nutStop) })
		k.choDen(func() bool { return !sv.nutStart.Disabled() })
		k.cho(0.5)
		k.lam(func() { t = binh.thanhTT.Text })
		logDemo("   Bình: " + t)
		k.lam(func() {
			for _, w := range []fyne.Window{an.w, binh.w, chi.w, sv.w} {
				w.Close()
			}
		})
		k.cho(0.3)
	}()
	for _, w := range []fyne.Window{sv.w, an.w, binh.w, chi.w} {
		w.Show()
	}
	a.Run()
	logDemo("Kết thúc")
}
