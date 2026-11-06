package main

// demo_gui – kịch bản minh họa tự động trong MỘT tiến trình (chạy dưới xvfb-run trên Linux): bảng điều
// khiển máy chủ và ba ứng dụng đặt vé An, Bình, Chi. Kịch bản chọn đoạn, bấm chỗ, bấm nút như người dùng
// (gọi đúng hàm xử lý trên luồng giao diện qua fyne.DoAndWait), in "[demo] …":
//   1–3. xe giường nằm, chế độ khoa: An và Bình chọn D01 Sài Gòn → Đà Lạt, Chi chọn D01 Đà Lạt → Nha Trang,
//        cả ba CÙNG bấm "GIỮ CHỖ" -> một trong An/Bình giữ được, Chi CŨNG giữ được (đoạn không chồng nhau);
//        hai người giữ được thanh toán -> cùng một giường bán hai vé;
//   4.   chế độ khong_khoa, trễ 0–20 ms: cả ba chọn D02 Sài Gòn → Nha Trang, CÙNG bấm -> cả ba "Đã giữ",
//        rồi hai người thấy "Chỗ D02 đã bị … lấy mất!" -> SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
//   5.   So sánh 4 chế độ (mô phỏng 500 người cùng giữ ghế nóng D01, đoạn ngẫu nhiên, 200 kết nối) -> bảng;
//   6.   đặt lại: xe ghế ngồi 50 chỗ, giữ chỗ 6 s: An giữ 05, 06 Sài Gòn → Bảo Lộc, không thanh toán; Bình
//        bấm 06 cho Sài Gòn → Đà Lạt: đang giữ; hết hạn -> Bình giữ và mua 05, 06 Sài Gòn → Đà Lạt; Chi mua
//        05 Đà Lạt → Nha Trang; An giữ 10, 11 (đồng hồ chạy), Chi chọn 21; chụp 4 cửa sổ ghép thành
//        ket_qua_gui.png.

import (
	"fmt"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.14_DatVeXeKhach/tranhchap"
)

func chayDemo(a fyne.App, host string, cong int, thuMuc string) {
	sv := moCuaSoServer(a, host, cong)
	an, binh, chi := moCuaSoDatVeXe(a, "An", host, cong), moCuaSoDatVeXe(a, "Bình", host, cong), moCuaSoDatVeXe(a, "Chi", host, cong)
	ds := []*cuaSoDatVeXe{an, binh, chi}
	k := &kichBan{}
	chuTB := func(cs *cuaSoDatVeXe) string { var t string; k.lam(func() { t = cs.thongBao.text() }); return t }
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
		logDemo("1. Máy chủ: START SERVER (chế độ khoa, xe giường nằm, giữ chỗ 30 s)")
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
		logDemo("2. An, Bình: D01 Sài Gòn → Đà Lạt; Chi: D01 Đà Lạt → Nha Trang; CÙNG bấm 'GIỮ CHỖ'")
		k.lam(func() {
			an.datDoan(0, 2)
			binh.datDoan(0, 2)
			chi.datDoan(2, 3)
			for _, cs := range ds {
				cs.bam("D01")
			}
			for _, cs := range ds {
				bam(cs.nutGiu)
			}
		})
		k.choDen(hetDangGiu)
		k.cho(0.4)
		inCuaSo()
		inServer()
		var duoc []*cuaSoDatVeXe
		k.lam(func() {
			for _, cs := range ds {
				if len(cs.giu) > 0 {
					duoc = append(duoc, cs)
				}
			}
		})
		if len(duoc) == 0 {
			logDemo("   không ai giữ được – dừng")
			return
		}
		var ten []string
		for _, cs := range duoc {
			ten = append(ten, cs.ten)
		}
		logDemo(fmt.Sprintf("3. %s bấm 'THANH TOÁN'", strings.Join(ten, " và ")))
		k.lam(func() {
			for _, cs := range duoc {
				bam(cs.nutTT)
			}
		})
		k.choDen(func() bool {
			for _, cs := range duoc {
				if len(cs.veCua) == 0 {
					return false
				}
			}
			return true
		})
		k.cho(0.4)
		for _, cs := range duoc {
			logDemo(fmt.Sprintf("   %s: %s", cs.ten, chuTB(cs)))
		}
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
		logDemo("   Cả ba chọn D02 Sài Gòn → Nha Trang rồi CÙNG bấm 'GIỮ CHỖ'")
		truoc := map[string]int{}
		k.lam(func() {
			for _, cs := range ds {
				truoc[cs.ten] = cs.soGiu
				cs.datDoan(0, 3)
				cs.bam("D02")
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
					dong = "ĐÃ GIỮ D02"
				}
			})
			logDemo(fmt.Sprintf("   %s: máy chủ trả lời %s", cs.ten, dong))
		}
		k.cho(0.6)
		logDemo("   …sau cap_nhat của máy chủ:")
		inCuaSo()
		inServer()
		tt := sv.lop.TrangThai()
		var thuoc []string
		for k := 0; k < tc.SoChang; k++ {
			thuoc = append(thuoc, chuCua(tt, fmt.Sprintf("D02#%d", k)))
		}
		logDemo("   D02 thuộc: " + strings.Join(thuoc, ", "))
		nanNhan := binh
		for _, cs := range ds {
			if strings.Contains(chuTB(cs), "lấy mất") {
				nanNhan = cs
				break
			}
		}
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui_tranh_chap.png"), [][]fyne.Window{{sv.w, nanNhan.w}})
		logDemo("5. khoa, trễ 0–2 ms; So sánh 4 chế độ (ghế nóng D01, đoạn ngẫu nhiên)")
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
		logDemo("6. Đặt lại: xe ghế ngồi 50 chỗ, giữ chỗ 6 s; An giữ 05, 06 Sài Gòn → Bảo Lộc (không thanh toán)")
		k.lam(func() {
			sv.datThamSo(tc.T("ttl_s", 6, "loai_xe", "ghe"))
			bam(sv.nutDatLai)
		})
		k.choDen(func() bool {
			for _, cs := range ds {
				d, _ := cs.tt["dem"].(tc.M)
				if tc.SoThuc(cs.tt["ttl_s"]) != 6 || tc.Chuoi(cs.tt["loai_xe"]) != "ghe" || tc.SoNguyen(d["ban"]) != 0 {
					return false
				}
			}
			return true
		})
		k.cho(0.8)
		k.lam(func() { an.datDoan(0, 1); an.bam("05"); an.bam("06"); bam(an.nutGiu) })
		k.choDen(func() bool { return len(an.giu) > 0 })
		k.cho(0.5)
		k.lam(func() { binh.datDoan(0, 2); binh.bam("06") })
		logDemo("   Bình bấm 06 (Sài Gòn → Đà Lạt): " + chuTB(binh))
		k.choDen(func() bool { return len(an.giu) == 0 })
		logDemo("   hết hạn – An: " + chuTB(an))
		k.cho(0.3)
		logDemo("   Bình giữ 05, 06 Sài Gòn → Đà Lạt rồi thanh toán; Chi mua 05 Đà Lạt → Nha Trang")
		k.lam(func() {
			binh.bam("05")
			binh.bam("06")
			chi.datDoan(2, 3)
			chi.bam("05")
			bam(binh.nutGiu)
			bam(chi.nutGiu)
		})
		k.choDen(func() bool { return len(binh.giu) > 0 && len(chi.giu) > 0 })
		k.cho(0.4)
		k.lam(func() { bam(binh.nutTT); bam(chi.nutTT) })
		k.choDen(func() bool { return len(binh.veCua) > 0 && len(chi.veCua) > 0 })
		k.cho(0.4)
		logDemo("   An giữ 10, 11 Sài Gòn → Nha Trang; Chi chọn 21")
		k.lam(func() {
			an.datDoan(0, 3)
			an.bam("10")
			an.bam("11")
			bam(an.nutGiu)
			chi.datDoan(0, 3)
			chi.bam("21")
		})
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
