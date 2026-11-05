package main

// demo_gui – kịch bản minh họa tự động trong MỘT tiến trình (chạy dưới xvfb-run trên Linux): bảng điều
// khiển máy chủ và ba phòng đấu giá An, Bình, Chi. Kịch bản "bấm" nút như người dùng (gọi đúng hàm xử lý
// trên luồng giao diện qua fyne.DoAndWait), in "[demo] …":
//   1–3. chế độ khoa: Bình và Chi cùng bấm "+1 bước" -> một người bị từ chối "giá đã thay đổi";
//   4.   chế độ khong_khoa: ba người cùng bấm "+1 bước" -> cả ba cùng "được chấp nhận" -> SAI PHẠM;
//        chụp ket_qua_gui_tranh_chap.png;
//   5.   So sánh 4 chế độ (mô phỏng 500 người × 5 lần, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
//   6–7. phiên 8 s, gia hạn 4 s: trả giá phút cuối -> gia hạn hai lần -> kết thúc, công bố người thắng;
//        chụp 4 cửa sổ ghép thành ket_qua_gui.png.

import (
	"fmt"
	"path/filepath"
	"strings"
	"time"

	"fyne.io/fyne/v2"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.6_DauGia/tranhchap"
)

func chayDemo(a fyne.App, host string, cong int, thuMuc string) {
	sv := moCuaSoServer(a, host, cong)
	an, binh, chi := moCuaSoDauGia(a, "An", host, cong), moCuaSoDauGia(a, "Bình", host, cong), moCuaSoDauGia(a, "Chi", host, cong)
	ds := []*cuaSoDauGia{an, binh, chi}
	k := &kichBan{}
	inCuaSo := func() {
		for _, cs := range ds {
			var t string
			k.lam(func() { t = cs.thongBao.text() })
			logDemo(fmt.Sprintf("   %s: %s", cs.ten, t))
		}
	}
	inServer := func() {
		tk := sv.lop.ThongKeDayDu()
		logDemo(fmt.Sprintf("   máy chủ: %d yc, tc %d, từ chối %d, sai phạm %d", tc.SoNguyen(tk["so_yc"]),
			tc.SoNguyen(tk["thanh_cong"]), tc.SoNguyen(tk["tu_choi"]), tc.SoNguyen(tk["sai_pham_tong"])))
		for i, v := range tc.DSChuoi(tk["vi_du_sai_pham"]) {
			if i < 2 {
				logDemo("     ví dụ: " + v)
			}
		}
	}
	dongHoCua := func(cs *cuaSoDauGia) string {
		var t string
		k.lam(func() { t = strings.TrimSpace(cs.nhanDongHo.Text + " " + cs.nhanGiaHan.Text) })
		return t
	}
	go func() {
		defer fyne.Do(a.Quit)
		k.cho(0.5)
		logDemo("1. Máy chủ: START SERVER (chế độ khoa, phiên 120 s)")
		k.lam(func() { bam(sv.nutStart) })
		k.cho(0.3)
		logDemo("   An, Bình, Chi: Kết nối")
		k.lam(func() {
			for _, cs := range ds {
				bam(cs.nutKetNoi)
			}
		})
		if !k.choDen(func() bool { return an.tt != nil && binh.tt != nil && chi.tt != nil }) {
			return
		}
		k.cho(1.0)
		logDemo("2. An bấm '+1 bước'")
		k.lam(func() { bam(an.nutBuoc[0]) })
		k.cho(0.8)
		logDemo("3. Bình và Chi CÙNG bấm '+1 bước' (cùng giá)")
		k.lam(func() { bam(binh.nutBuoc[0]); bam(chi.nutBuoc[0]) })
		k.cho(0.8)
		inCuaSo()
		inServer()
		logDemo("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)")
		k.lam(func() {
			sv.oTre.SetText("20")
			bam(sv.nutTre)
			sv.chonCheDo("khong_khoa")
		})
		if !k.choDen(func() bool {
			for _, cs := range ds {
				if cs.tt["che_do"] != "khong_khoa" {
					return false
				}
			}
			return true
		}) {
			return
		}
		k.cho(0.5)
		logDemo("   An, Bình, Chi CÙNG bấm '+1 bước'")
		k.lam(func() {
			for _, cs := range ds {
				bam(cs.nutBuoc[0])
			}
		})
		k.cho(0.8)
		inCuaSo()
		inServer()
		logDemo("   Lịch sử trả giá (mới nhất trước):")
		for i, x := range tc.DSM(sv.lop.TrangThai()["lich_su"]) {
			if i < 4 {
				logDemo(fmt.Sprintf("     %s %-5s %s", tc.Chuoi(x["gio"]), tc.Chuoi(x["nguoi"]), tc.Tien(tc.SoNguyen(x["gia"]))))
			}
		}
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui_tranh_chap.png"), [][]fyne.Window{{sv.w, binh.w}})
		logDemo("5. Máy chủ: khoa, trễ 0–2 ms; So sánh 4 chế độ")
		k.lam(func() {
			sv.oTre.SetText("2")
			bam(sv.nutTre)
			sv.chonCheDo("khoa")
			sv.oN.SetText("500")
		})
		k.choDen(func() bool { return sv.lop.Kho.CheDo() == "khoa" })
		k.cho(0.3)
		k.lam(func() { bam(sv.nutSoSanh) })
		k.cho(0.3)
		if !k.choDen(func() bool { return !sv.dangMoPhong }) {
			return
		}
		var hang []tc.HangSoSanh
		k.lam(func() { hang = sv.hangSoSanh })
		for _, d := range tc.BangSoSanh(hang) {
			logDemo(d)
		}
		var kl string
		k.lam(func() { kl = sv.nhanKetLuan.Text })
		for _, cau := range strings.Split(kl, ". ") {
			logDemo("   " + strings.TrimRight(cau, "."))
		}
		k.cho(0.5)
		logDemo("6. Đặt lại: phiên 8 s, gia hạn 4 s; An '+1 bước'")
		k.lam(func() {
			sv.datThamSo(tc.T("thoi_gian_s", 8, "gia_han_s", 4))
			bam(sv.nutDatLai)
		})
		k.cho(1.0)
		k.lam(func() { bam(an.nutBuoc[0]) })
		k.cho(4.2)
		logDemo(fmt.Sprintf("   Bình '+2 bước' khi còn %s", dongHoCua(an)))
		k.lam(func() { bam(binh.nutBuoc[1]) })
		k.cho(0.5)
		logDemo(fmt.Sprintf("   -> đồng hồ An: %s", dongHoCua(an)))
		k.choDen(func() bool { return an.hetHan != nil && time.Until(*an.hetHan) < 800*time.Millisecond })
		logDemo(fmt.Sprintf("7. An '+1 bước' khi còn %s", dongHoCua(an)))
		k.lam(func() { bam(an.nutBuoc[0]) })
		k.cho(0.5)
		logDemo(fmt.Sprintf("   -> đồng hồ Bình: %s", dongHoCua(binh)))
		if !k.choDen(func() bool { kt, _ := an.tt["ket_thuc"].(bool); return kt }) {
			return
		}
		k.cho(0.8)
		inCuaSo()
		logDemo("   máy chủ: " + tc.Chuoi(sv.lop.ThongKeDayDu()["tom_tat"]))
		logDemo("8. Chụp 4 cửa sổ")
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui.png"), [][]fyne.Window{{sv.w, an.w}, {binh.w, chi.w}})
		logDemo("9. Máy chủ: STOP SERVER")
		k.lam(func() { bam(sv.nutStop) })
		k.choDen(func() bool { return !sv.nutStart.Disabled() })
		k.cho(0.5)
		var t string
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
