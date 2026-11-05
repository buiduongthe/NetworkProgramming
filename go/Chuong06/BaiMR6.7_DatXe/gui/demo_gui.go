package main

// demo_gui – kịch bản minh họa tự động trong MỘT tiến trình (chạy dưới xvfb-run trên Linux): bảng điều
// khiển máy chủ và ba ứng dụng gọi xe An, Bình, Chi. Kịch bản "bấm" nút như người dùng (gọi đúng hàm xử
// lý trên luồng giao diện qua fyne.DoAndWait), in "[demo] …":
//   1–3. chế độ khoa, 1 xe: An, Bình, Chi CÙNG bấm "ĐẶT XE" -> một người được xe, hai người "Hết chỗ";
//        người được xe hủy chuyến;
//   4.   chế độ khong_khoa, trễ 0–20 ms: cả ba CÙNG bấm -> cả ba "Đặt thành công – tài xế Hùng", rồi hai
//        người thấy "Tài xế Hùng đã nhận khách khác!" -> SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
//   5.   đặt lại 3 xe, So sánh 4 chế độ (mô phỏng 1000 người, 200 kết nối) -> bảng, khong_khoa tô đỏ;
//   6.   đặt lại (3 xe): An, Bình, Chi cùng đặt -> mỗi người một xe, xe chạy tới điểm đón;
//        chụp 4 cửa sổ ghép thành ket_qua_gui.png.

import (
	"fmt"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.7_DatXe/tranhchap"
)

func chayDemo(a fyne.App, host string, cong int, thuMuc string) {
	sv := moCuaSoServer(a, host, cong)
	an, binh, chi := moCuaSoGoiXe(a, "An", host, cong), moCuaSoGoiXe(a, "Bình", host, cong), moCuaSoGoiXe(a, "Chi", host, cong)
	ds := []*cuaSoGoiXe{an, binh, chi}
	k := &kichBan{}
	chuTT := func(cs *cuaSoGoiXe) string { var t string; k.lam(func() { t = cs.nhanTT.text() }); return t }
	inCuaSo := func() {
		for _, cs := range ds {
			logDemo(fmt.Sprintf("   %s: %s", cs.ten, chuTT(cs)))
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
	cungBam := func() { // ba lần bấm liền nhau trên luồng giao diện
		k.lam(func() {
			for _, cs := range ds {
				bam(cs.nutDat)
			}
		})
	}
	hetTim := func() bool {
		for _, cs := range ds {
			if cs.trang == "dang_tim" {
				return false
			}
		}
		return true
	}
	go func() {
		defer fyne.Do(a.Quit)
		k.cho(0.5)
		logDemo("1. Máy chủ: START SERVER (chế độ khoa, 1 xe)")
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
		logDemo("2. An, Bình, Chi CÙNG bấm 'ĐẶT XE'")
		cungBam()
		k.choDen(hetTim)
		k.cho(0.5)
		inCuaSo()
		inServer()
		var duoc *cuaSoGoiXe
		k.lam(func() {
			for _, cs := range ds {
				if cs.trang == "da_dat" && duoc == nil {
					duoc = cs
				}
			}
		})
		if duoc == nil {
			logDemo("   không ai đặt được xe – dừng")
			return
		}
		k.cho(1.0)
		logDemo(fmt.Sprintf("3. %s bấm 'Hủy chuyến'", duoc.ten))
		k.lam(func() { bam(duoc.nutHuy) })
		k.choDen(func() bool { return duoc.trang == "xong" })
		k.cho(0.5)
		logDemo(fmt.Sprintf("   %s: %s", duoc.ten, chuTT(duoc)))
		logDemo("   máy chủ: " + tc.Chuoi(sv.lop.ThongKeDayDu()["tom_tat"]))
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
		logDemo("   An, Bình, Chi CÙNG bấm 'ĐẶT XE'")
		cungBam()
		k.choDen(hetTim)
		for _, cs := range ds {
			logDemo(fmt.Sprintf("   %s nhận: %s", cs.ten, chuTT(cs)))
		}
		k.cho(0.6)
		logDemo("   …sau cap_nhat của máy chủ:")
		inCuaSo()
		inServer()
		xe := tc.DSM(sv.lop.TrangThai()["xe"])
		logDemo("   xe 1 chở: " + tc.StrPy(xe[0]["khach"]))
		nanNhan := binh
		k.lam(func() {
			for _, cs := range ds {
				if cs.trang == "mat_xe" {
					nanNhan = cs
					break
				}
			}
		})
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui_tranh_chap.png"), [][]fyne.Window{{sv.w, nanNhan.w}})
		logDemo("5. Máy chủ: khoa, trễ 0–2 ms, đặt lại 3 xe")
		logDemo("   So sánh 4 chế độ (1000 người)")
		k.lam(func() {
			sv.oTre.SetText("2")
			bam(sv.nutTre)
			sv.chonCheDo("khoa")
			sv.datThamSo(3)
			bam(sv.nutDatLai)
			sv.oN.SetText("1000")
		})
		k.choDen(func() bool { return sv.lop.Kho.CheDo() == "khoa" && tc.SoNguyen(sv.lop.TrangThai()["so_xe"]) == 3 })
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
		logDemo("6. Đặt lại (3 xe); An, Bình, Chi CÙNG bấm 'ĐẶT XE'")
		k.lam(func() { bam(sv.nutDatLai) })
		k.choDen(func() bool {
			for _, cs := range ds {
				if tc.SoNguyen(cs.tt["trong"]) != 3 {
					return false
				}
			}
			return true
		})
		k.cho(0.8)
		cungBam()
		k.choDen(hetTim)
		k.cho(1.6)
		inCuaSo()
		for _, cs := range ds {
			var dong string
			k.lam(func() {
				if cs.chuyen != nil {
					dong = fmt.Sprintf("   %s: %s %s, đến sau %d phút", cs.ten, strings.Fields(cs.nhanTX.Text)[0],
						tc.Chuoi(cs.chuyen["bien_so"]), tc.SoNguyen(cs.chuyen["phut"]))
				}
			})
			if dong != "" {
				logDemo(dong)
			}
		}
		logDemo("   máy chủ: " + tc.Chuoi(sv.lop.ThongKeDayDu()["tom_tat"]))
		logDemo("7. Chụp 4 cửa sổ")
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui.png"), [][]fyne.Window{{sv.w, an.w}, {binh.w, chi.w}})
		k.choDen(func() bool {
			for _, cs := range ds {
				if cs.hh != nil && !cs.daToi() {
					return false
				}
			}
			return true
		})
		k.cho(0.3)
		inCuaSo()
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
