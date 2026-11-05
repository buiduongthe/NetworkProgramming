package main

// demo_gui – kịch bản minh họa tự động trong MỘT tiến trình (chạy dưới xvfb-run trên Linux): bảng điều
// khiển máy chủ và ba cổng đăng ký của An, Bình, Chi. Kịch bản bấm nút như người dùng (gọi đúng hàm xử lý
// trên luồng giao diện qua fyne.DoAndWait), in "[demo] …":
//   1–2. chế độ khoa, sĩ số 1: cả ba cùng bấm "Đăng ký" lớp 123013-01 -> một người được, hai người
//        "Lớp đã đủ";
//   3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác đó -> cả ba "thành công" -> SAI PHẠM vượt sĩ số;
//        danh sách lớp chỉ còn một tên, hai cửa sổ kia báo "đăng ký đã bị MẤT";
//        chụp ket_qua_gui_tranh_chap.png;
//   4.   So sánh 4 chế độ (2000 sinh viên, 3 lớp × 50, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
//   5–6. đợt mới (3 lớp × 50): An đăng ký 2 lớp, Bình 1 lớp; 500 sinh viên ảo đăng ký -> lớp đủ;
//        Chi đăng ký lớp đủ -> "Lớp đã đủ"; An hủy một lớp -> Chi đăng ký được chỗ vừa nhả;
//        chụp 4 cửa sổ ghép thành ket_qua_gui.png.

import (
	"fmt"
	"image"
	"os"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.10_DangKyHocPhan/tranhchap"
)

const lop1, lop2 = "123013-01", "123005-01"

func chayDemo(a fyne.App, host string, cong int, thuMuc string) {
	sv := moCuaSoServer(a, host, cong)
	an, binh, chi := moCuaSoDangKy(a, "An", host, cong), moCuaSoDangKy(a, "Bình", host, cong), moCuaSoDangKy(a, "Chi", host, cong)
	ds := []*cuaSoDangKy{an, binh, chi}
	k := &kichBan{}
	chuTB := func(cs *cuaSoDangKy) string { var t string; k.lam(func() { t = cs.thongBao.text() }); return t }
	inCuaSo := func(ds []*cuaSoDangKy) {
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
		logDemo("   " + tc.Chuoi(tk["tom_tat"]))
	}
	inLop := func() {
		for _, x := range tc.DSM(sv.lop.TrangThai()["lop"]) {
			ten := tc.DSChuoi(x["sv"])
			if len(ten) > 3 {
				ten = ten[:3]
			}
			logDemo(strings.TrimRight(fmt.Sprintf("   %s: %d/%d %s", tc.Chuoi(x["ma"]), tc.SoNguyen(x["da_dk"]),
				tc.SoNguyen(x["si_so"]), strings.Join(ten, " ")), " "))
		}
	}
	datLai := func(ts tc.TS) {
		k.lam(func() {
			sv.datThamSo(ts)
			bam(sv.nutDatLai)
		})
	}
	xong := func() bool {
		for _, cs := range ds {
			if len(cs.dangCho) > 0 {
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
	nhanNut := func(cs *cuaSoDangKy, ma string) string {
		var t string
		k.lam(func() {
			if h := cs.hang[ma]; h != nil {
				t = h.nut.Text
			}
		})
		return t
	}
	go func() {
		defer fyne.Do(a.Quit)
		k.cho(0.5)
		logDemo("1. Máy chủ: START SERVER (chế độ khoa)")
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
		logDemo("2. Đặt lại: 3 lớp, sĩ số 1 (còn đúng 1 chỗ mỗi lớp)")
		datLai(tc.T("so_lop", 3, "si_so", 1))
		k.choDen(func() bool {
			for _, cs := range ds {
				if tc.SoNguyen(cs.tt["si_so"]) != 1 {
					return false
				}
			}
			return true
		})
		k.cho(0.5)
		logDemo("   An, Bình, Chi CÙNG bấm Đăng ký " + lop1)
		k.lam(func() {
			for _, cs := range ds {
				cs.dangKy(lop1)
			}
		})
		k.choDen(xong)
		k.cho(0.8)
		inCuaSo(ds)
		inServer()
		logDemo("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; Đặt lại")
		doiTre("20")
		datLai(tc.T("so_lop", 3, "si_so", 1))
		k.lam(func() { sv.chonCheDo("khong_khoa") })
		k.choDen(func() bool { return sv.lop.Kho.CheDo() == "khong_khoa" })
		k.choDen(func() bool {
			for _, cs := range ds {
				if cs.tt["che_do"] != "khong_khoa" || len(cs.cuaToi) > 0 {
					return false
				}
			}
			return true
		})
		k.cho(0.5)
		logDemo("   An, Bình, Chi CÙNG bấm Đăng ký " + lop1)
		k.lam(func() {
			for _, cs := range ds {
				cs.dangKy(lop1)
			}
		})
		k.choDen(xong)
		logDemo("   ngay khi có trả lời:")
		inCuaSo(ds)
		k.cho(1.2)
		logDemo("   1 giây sau (đối chiếu danh sách lớp):")
		inCuaSo(ds)
		inServer()
		inLop()
		mat := binh
		for _, cs := range ds {
			if strings.Contains(chuTB(cs), "MẤT") {
				mat = cs
				break
			}
		}
		ra := filepath.Join(thuMuc, "ket_qua_gui_tranh_chap.png")
		if err := ghepLuoi(ra, [][]image.Image{chupCuaSo([]fyne.Window{sv.w, mat.w})}); err == nil {
			st, _ := os.Stat(ra)
			logDemo(fmt.Sprintf("   Đã ghi %s (%d KB, cửa sổ %s)", filepath.Base(ra), st.Size()/1024, mat.ten))
		}
		logDemo("4. Máy chủ: khoa, trễ 0–2 ms, 3 lớp × 50; So sánh")
		doiTre("2")
		datLai(tc.T("so_lop", 3, "si_so", 50))
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
		logDemo("5. Đặt lại; An đăng ký 2 lớp, Bình 1 lớp")
		datLai(tc.T("so_lop", 3, "si_so", 50))
		k.choDen(func() bool {
			for _, cs := range ds {
				if h := cs.hang[lop1]; len(cs.cuaToi) > 0 || h == nil || h.nut.Text != "Đăng ký" {
					return false
				}
			}
			return true
		})
		k.cho(0.5)
		k.lam(func() {
			an.dangKy(lop1)
			an.dangKy(lop2)
			binh.dangKy(lop1)
		})
		k.choDen(xong)
		k.cho(0.5)
		logDemo("   Bắn đồng loạt 500 sinh viên ảo (mỗi người 1 lớp)")
		k.lam(func() { sv.oN.SetText("500") })
		k.lam(func() { bam(sv.nutBan) })
		k.cho(0.3)
		k.choDen(func() bool { return !sv.dangMoPhong })
		k.cho(0.8)
		k.lam(func() { sv.xongMoPhong(sv.hangSoSanh) }) // bảng: hiện lại kết quả so sánh 4 chế độ
		inLop()
		logDemo(fmt.Sprintf("   Chi bấm '%s' ở %s", nhanNut(chi, lop2), lop2))
		k.lam(func() { chi.dangKy(lop2) })
		k.choDen(xong)
		k.cho(0.5)
		inCuaSo([]*cuaSoDangKy{chi})
		logDemo(fmt.Sprintf("6. An bấm Hủy %s, rồi Chi bấm Đăng ký %s", lop2, lop2))
		k.lam(func() { an.dangKy(lop2) })
		k.choDen(xong)
		k.choDen(func() bool { h := chi.hang[lop2]; return h != nil && h.nut.Text == "Đăng ký" && !h.nut.Disabled() })
		k.lam(func() { chi.dangKy(lop2) })
		k.choDen(xong)
		k.cho(0.6)
		inCuaSo(ds)
		inServer()
		logDemo("7. Chụp 4 cửa sổ")
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui.png"), [][]fyne.Window{{sv.w, an.w}, {binh.w, chi.w}})
		logDemo("8. Máy chủ: STOP SERVER")
		k.lam(func() { bam(sv.nutStop) })
		k.choDen(func() bool { return !sv.nutStart.Disabled() })
		k.cho(0.5)
		var t string
		k.lam(func() { t = an.thanhTT.Text })
		logDemo("   An: " + t)
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
