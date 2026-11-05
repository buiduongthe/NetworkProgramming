package main

// demo_gui – kịch bản minh họa tự động trong MỘT tiến trình (chạy dưới xvfb-run trên Linux): bảng điều
// khiển máy chủ và ba vòng quay: An, Bình và "Bình (máy 2)" – cùng một người mở trên hai thiết bị. Kịch
// bản bấm nút như người dùng (gọi đúng hàm xử lý trên luồng giao diện qua fyne.DoAndWait), in "[demo] …":
//   1–2. chế độ khoa, 1 món quà, p = 1: cả ba cùng bấm QUAY -> đúng một người trúng, thiết bị thứ hai
//        của Bình bị từ chối "đã quay";
//   3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác đó -> nhiều người "trúng" một món quà -> SAI PHẠM
//        (thừa quà, quay 2 lần); chụp ket_qua_gui_tranh_chap.png;
//   4.   So sánh 4 chế độ (10 000 người, p = 1/20, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
//   5–6. 10 quà, p = 0.8: An, Bình quay; Bình (máy 2) quay -> "đã quay"; bắn đồng loạt 500 người ->
//        danh sách người trúng trực tiếp; chụp 4 cửa sổ ghép thành ket_qua_gui.png.

import (
	"fmt"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.9_QuaySo/tranhchap"
)

func chayDemo(a fyne.App, host string, cong int, thuMuc string) {
	sv := moCuaSoServer(a, host, cong)
	an, binh, binh2 := moCuaSoVongQuay(a, "An", host, cong), moCuaSoVongQuay(a, "Bình", host, cong),
		moCuaSoVongQuay(a, "Bình", host, cong)
	binh2.nhanHien = "Bình (máy 2)" // cùng người, thiết bị thứ hai
	binh2.w.SetTitle("Vòng quay may mắn – Bình (máy 2)")
	ds := []*cuaSoVongQuay{an, binh, binh2}
	k := &kichBan{}
	inCuaSo := func() {
		for _, cs := range ds {
			var t string
			k.lam(func() { t = cs.thongBao.text() })
			logDemo(fmt.Sprintf("   %s: %s", cs.nhanHien, t))
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
		logDemo("   " + tc.Chuoi(tk["tom_tat"]))
	}
	datLai := func(ts tc.TS) {
		k.lam(func() {
			sv.datThamSo(ts)
			bam(sv.nutDatLai)
		})
	}
	cungBam := func() {
		k.lam(func() {
			for _, cs := range ds {
				bam(cs.nutQuay)
			}
		})
	}
	dungHet := func() bool {
		for _, cs := range ds {
			if cs.pha != "" {
				return false
			}
		}
		return true
	}
	doiTre := func(ms string) {
		k.lam(func() {
			sv.oTre.SetText(ms)
			bam(sv.nutTre)
		})
	}
	go func() {
		defer fyne.Do(a.Quit)
		k.cho(0.5)
		logDemo("1. Máy chủ: START SERVER (chế độ khoa)")
		k.lam(func() { bam(sv.nutStart) })
		k.cho(0.3)
		logDemo("   An, Bình, Bình (máy 2): Kết nối")
		k.lam(func() {
			for _, cs := range ds {
				bam(cs.nutKetNoi)
			}
		})
		if !k.choDen(func() bool { return an.tt != nil && binh.tt != nil && binh2.tt != nil }) {
			return
		}
		logDemo("2. Đặt lại: 1 món quà, p = 1 (lượt đầu chắc trúng)")
		datLai(tc.T("so_qua", 1, "p", 1))
		k.choDen(func() bool {
			for _, cs := range ds {
				if tc.SoNguyen(cs.tt["so_qua"]) != 1 {
					return false
				}
			}
			return true
		})
		k.cho(0.5)
		logDemo("   An, Bình, Bình (máy 2) CÙNG bấm QUAY")
		cungBam()
		k.cho(0.3)
		k.choDen(dungHet)
		inCuaSo()
		inServer()
		logDemo("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; Đặt lại")
		doiTre("20")
		datLai(tc.T("so_qua", 1, "p", 1))
		k.lam(func() { sv.chonCheDo("khong_khoa") })
		k.choDen(func() bool { return sv.lop.Kho.CheDo() == "khong_khoa" })
		k.choDen(func() bool {
			for _, cs := range ds {
				if cs.tt["che_do"] != "khong_khoa" || cs.nutQuay.Disabled() {
					return false
				}
			}
			return true
		})
		k.cho(0.5)
		logDemo("   An, Bình, Bình (máy 2) CÙNG bấm QUAY")
		cungBam()
		k.cho(0.3)
		k.choDen(dungHet)
		k.cho(0.5)
		inCuaSo()
		inServer()
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui_tranh_chap.png"), [][]fyne.Window{{sv.w, binh2.w}})
		logDemo("4. Máy chủ: khoa, trễ 0–2 ms; 10 quà, p = 0.05 (1/20);")
		logDemo("   So sánh 4 chế độ, 10000 người")
		doiTre("2")
		datLai(tc.T("so_qua", 10, "p", 0.05))
		k.lam(func() { sv.chonCheDo("khoa") })
		k.choDen(func() bool { return sv.lop.Kho.CheDo() == "khoa" })
		k.lam(func() { sv.oN.SetText("10000") })
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
		logDemo("5. Đặt lại: 10 quà, p = 0.8; An quay, rồi Bình quay")
		datLai(tc.T("so_qua", 10, "p", 0.8))
		k.choDen(func() bool {
			for _, cs := range ds {
				if tc.SoThuc(cs.tt["p"]) != 0.8 || cs.nutQuay.Disabled() {
					return false
				}
			}
			return true
		})
		k.cho(0.5)
		k.lam(func() { bam(an.nutQuay) })
		k.cho(0.4)
		k.lam(func() { bam(binh.nutQuay) })
		k.cho(0.3)
		k.choDen(func() bool { return an.pha == "" && binh.pha == "" })
		logDemo("   Bình (máy 2) bấm QUAY")
		k.lam(func() { bam(binh2.nutQuay) })
		k.cho(0.3)
		k.choDen(func() bool { return binh2.pha == "" })
		inCuaSo()
		logDemo("6. Bắn đồng loạt 500 người (chế độ khoa)")
		k.lam(func() { sv.oN.SetText("500") })
		k.lam(func() { bam(sv.nutBan) })
		k.cho(0.3)
		k.choDen(func() bool { return !sv.dangMoPhong })
		k.cho(0.8)
		k.lam(func() { sv.xongMoPhong(sv.hangSoSanh) }) // bảng: hiện lại kết quả so sánh 4 chế độ
		inServer()
		logDemo("   Người trúng gần nhất (cửa sổ An):")
		var ls []string
		k.lam(func() { ls = append(ls, an.dongLs...) })
		for i, d := range ls {
			if i < 4 {
				logDemo("     " + d)
			}
		}
		logDemo("7. Chụp 4 cửa sổ")
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui.png"), [][]fyne.Window{{sv.w, an.w}, {binh.w, binh2.w}})
		logDemo("8. Máy chủ: STOP SERVER")
		k.lam(func() { bam(sv.nutStop) })
		k.choDen(func() bool { return !sv.nutStart.Disabled() })
		k.cho(0.5)
		var t string
		k.lam(func() { t = an.thanhTT.Text })
		logDemo("   An: " + t)
		k.lam(func() {
			for _, w := range []fyne.Window{an.w, binh.w, binh2.w, sv.w} {
				w.Close()
			}
		})
		k.cho(0.3)
	}()
	for _, w := range []fyne.Window{sv.w, an.w, binh.w, binh2.w} {
		w.Show()
	}
	a.Run()
	logDemo("Kết thúc")
}
