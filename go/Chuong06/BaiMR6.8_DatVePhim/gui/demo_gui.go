package main

// demo_gui – kịch bản minh họa tự động trong MỘT tiến trình (chạy dưới xvfb-run trên Linux): bảng điều
// khiển máy chủ và ba ứng dụng đặt vé An, Bình, Chi. Kịch bản bấm ghế, bấm nút như người dùng (gọi đúng
// hàm xử lý trên luồng giao diện qua fyne.DoAndWait), in "[demo] …":
//   1–3. chế độ khoa: cả ba chọn E7 rồi CÙNG bấm "GIỮ CHỖ" -> một người giữ được, hai người bị từ chối;
//        người giữ được thanh toán -> vé;
//   4.   chế độ khong_khoa, trễ 0–20 ms: cả ba chọn E8, CÙNG bấm -> cả ba "Đã giữ", rồi hai người thấy
//        "Ghế E8 đã bị … lấy mất!" -> SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
//   5.   So sánh 4 chế độ (mô phỏng 500 người cùng giữ ghế nóng E7, 200 kết nối) -> bảng;
//   6.   đặt lại, giữ chỗ 6 s: An giữ F5, F6 không thanh toán; Bình thấy ghế đang giữ; hết hạn -> ghế
//        trống lại, Bình giữ và mua; Chi mua G7, G8; An giữ E5, E6 (đồng hồ chạy), Chi chọn H6;
//        chụp 4 cửa sổ ghép thành ket_qua_gui.png.

import (
	"fmt"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.8_DatVePhim/tranhchap"
)

func chayDemo(a fyne.App, host string, cong int, thuMuc string) {
	sv := moCuaSoServer(a, host, cong)
	an, binh, chi := moCuaSoDatVe(a, "An", host, cong), moCuaSoDatVe(a, "Bình", host, cong), moCuaSoDatVe(a, "Chi", host, cong)
	ds := []*cuaSoDatVe{an, binh, chi}
	k := &kichBan{}
	chuTB := func(cs *cuaSoDatVe) string { var t string; k.lam(func() { t = cs.thongBao.text() }); return t }
	inCuaSo := func() {
		for _, cs := range ds {
			logDemo(fmt.Sprintf("   %s: %s", cs.ten, chuTB(cs)))
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
	hetDangGiu := func() bool {
		for _, cs := range ds {
			if strings.Contains(cs.thongBao.text(), "Đang giữ chỗ") {
				return false
			}
		}
		return true
	}
	tomTat := func() string { return tc.Chuoi(sv.lop.ThongKeDayDu()["tom_tat"]) }
	go func() {
		defer fyne.Do(a.Quit)
		k.cho(0.5)
		logDemo("1. Máy chủ: START SERVER (chế độ khoa, giữ chỗ 30 s)")
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
		logDemo("2. Cả ba chọn ghế E7 rồi CÙNG bấm 'GIỮ CHỖ'")
		k.lam(func() {
			for _, cs := range ds {
				cs.bam("E7")
			}
			for _, cs := range ds {
				bam(cs.nutGiu)
			}
		})
		k.choDen(hetDangGiu)
		k.cho(0.4)
		inCuaSo()
		inServer()
		var duoc *cuaSoDatVe
		k.lam(func() {
			for _, cs := range ds {
				if len(cs.giu) > 0 && duoc == nil {
					duoc = cs
				}
			}
		})
		if duoc == nil {
			logDemo("   không ai giữ được – dừng")
			return
		}
		logDemo(fmt.Sprintf("3. %s bấm 'THANH TOÁN'", duoc.ten))
		k.lam(func() { bam(duoc.nutTT) })
		k.choDen(func() bool { return len(duoc.veCua) > 0 })
		k.cho(0.4)
		logDemo(fmt.Sprintf("   %s: %s", duoc.ten, chuTB(duoc)))
		logDemo("   máy chủ: " + tomTat())
		logDemo("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)")
		k.lam(func() {
			sv.oTre.SetText("20")
			bam(sv.nutTre)
			sv.chonCheDo("khong_khoa")
		})
		k.choDen(func() bool {
			for _, cs := range ds {
				if cs.tt["che_do"] != "khong_khoa" {
					return false
				}
			}
			return true
		})
		k.cho(0.5)
		logDemo("   Cả ba chọn E8 rồi CÙNG bấm 'GIỮ CHỖ'")
		truoc := map[string]int{}
		k.lam(func() {
			for _, cs := range ds {
				truoc[cs.ten] = cs.soGiu
				cs.bam("E8")
			}
			for _, cs := range ds {
				bam(cs.nutGiu)
			}
		})
		k.choDen(hetDangGiu)
		for _, cs := range ds {
			var dong string
			k.lam(func() {
				dong = "từ chối"
				if cs.soGiu > truoc[cs.ten] {
					dong = "ĐÃ GIỮ E8"
				}
			})
			logDemo(fmt.Sprintf("   %s: máy chủ trả lời %s", cs.ten, dong))
		}
		k.cho(0.6)
		logDemo("   …sau cap_nhat của máy chủ:")
		inCuaSo()
		inServer()
		chu, _ := sv.lop.TrangThai()["chu"].(tc.M)
		logDemo("   E8 thuộc: " + tc.StrPy(chu["E8"]))
		nanNhan := binh
		for _, cs := range ds {
			if strings.Contains(chuTB(cs), "lấy mất") {
				nanNhan = cs
				break
			}
		}
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui_tranh_chap.png"), [][]fyne.Window{{sv.w, nanNhan.w}})
		logDemo("5. khoa, trễ 0–2 ms; So sánh 4 chế độ (ghế nóng E7)")
		k.lam(func() {
			sv.oTre.SetText("2")
			bam(sv.nutTre)
			sv.chonCheDo("khoa")
		})
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
		logDemo("6. Đặt lại: giữ chỗ 6 s; An giữ F5, F6 (không thanh toán)")
		k.lam(func() {
			sv.datThamSo(tc.T("ttl_s", 6))
			bam(sv.nutDatLai)
		})
		k.choDen(func() bool {
			for _, cs := range ds {
				d, _ := cs.tt["dem"].(tc.M)
				if tc.SoThuc(cs.tt["ttl_s"]) != 6 || tc.SoNguyen(d["ban"]) != 0 {
					return false
				}
			}
			return true
		})
		k.cho(0.8)
		k.lam(func() { an.bam("F5"); an.bam("F6"); bam(an.nutGiu) })
		k.choDen(func() bool { return len(an.giu) > 0 })
		k.cho(0.5)
		k.lam(func() { binh.bam("F6") })
		logDemo("   Bình bấm F6: " + chuTB(binh))
		k.choDen(func() bool { return len(an.giu) == 0 })
		logDemo("   hết hạn – An: " + chuTB(an))
		k.cho(0.3)
		logDemo("   Bình giữ F5, F6 rồi thanh toán; Chi mua G7, G8")
		k.lam(func() {
			binh.bam("F5")
			binh.bam("F6")
			chi.bam("G7")
			chi.bam("G8")
			bam(binh.nutGiu)
			bam(chi.nutGiu)
		})
		k.choDen(func() bool { return len(binh.giu) > 0 && len(chi.giu) > 0 })
		k.cho(0.4)
		k.lam(func() { bam(binh.nutTT); bam(chi.nutTT) })
		k.choDen(func() bool { return len(binh.veCua) > 0 && len(chi.veCua) > 0 })
		k.cho(0.4)
		logDemo("   An giữ E5, E6; Chi chọn H6")
		k.lam(func() { an.bam("E5"); an.bam("E6"); bam(an.nutGiu); chi.bam("H6") })
		k.choDen(func() bool { return len(an.giu) > 0 })
		k.cho(1.5)
		inCuaSo()
		var dh string
		k.lam(func() { dh = an.nhanDongHo.Text })
		logDemo("   An: đồng hồ giữ chỗ " + dh)
		logDemo("   máy chủ: " + tomTat())
		inServer()
		logDemo("7. Chụp 4 cửa sổ")
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui.png"), [][]fyne.Window{{sv.w, an.w}, {binh.w, chi.w}})
		logDemo("8. Máy chủ: STOP SERVER")
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
