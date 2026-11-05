package main

// demo_gui – kịch bản minh họa tự động trong MỘT tiến trình (chạy dưới xvfb-run trên Linux): bảng điều
// khiển máy chủ và ba trang flash sale của An, Bình, Chi. Kịch bản bấm nút như người dùng (gọi đúng hàm xử
// lý trên luồng giao diện qua fyne.DoAndWait), in "[demo] …":
//   1–2. chế độ khoa, kho 3, mở bán sau 3 s: nút MUA NGAY bị khóa trong lúc đếm ngược; mở bán, An (2),
//        Bình (1), Chi (2) CÙNG bấm -> An, Bình được; Chi "HẾT HÀNG";
//   3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác -> cả ba "thành công", bán 5/3 -> SAI PHẠM (tồn
//        âm, lệch sổ); chụp ket_qua_gui_tranh_chap.png;
//   4.   So sánh 4 chế độ (10 000 người, kho 100, trễ 0–1 ms) -> bảng, dòng khong_khoa tô đỏ;
//   5–6. kho 100, mở bán sau 4 s: chụp Chi lúc đếm ngược; mở bán: An mua 2, 50 người ảo, Bình mua 2;
//        ghép 4 cửa sổ thành ket_qua_gui.png.

import (
	"fmt"
	"image"
	"os"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.11_FlashSale/tranhchap"
)

func chayDemo(a fyne.App, host string, cong int, thuMuc string) {
	sv := moCuaSoServer(a, host, cong)
	an, binh, chi := moCuaSoFlashSale(a, "An", host, cong), moCuaSoFlashSale(a, "Bình", host, cong),
		moCuaSoFlashSale(a, "Chi", host, cong)
	ds := []*cuaSoFlashSale{an, binh, chi}
	k := &kichBan{}
	inCuaSo := func(ds []*cuaSoFlashSale) {
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
		logDemo("   " + tc.Chuoi(tk["tom_tat"]))
	}
	datLai := func(ts tc.TS) {
		k.lam(func() {
			sv.datThamSo(ts)
			bam(sv.nutDatLai)
		})
	}
	datTre := func(ms string) {
		k.lam(func() {
			sv.oTre.SetText(ms)
			bam(sv.nutTre)
		})
	}
	cungMua := func() {
		k.lam(func() {
			for i, cs := range ds {
				cs.mua([]int64{2, 1, 2}[i])
			}
		})
	}
	xong := func() bool {
		for _, cs := range ds {
			if cs.dangCho {
				return false
			}
		}
		return true
	}
	moHet := func() bool {
		for _, cs := range ds {
			if cs.nutMua.Disabled() {
				return false
			}
		}
		return true
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
		logDemo("2. Đặt lại: kho 3 sản phẩm, mở bán sau 3 s")
		datLai(tc.T("ton_dau", 3, "mo_sau_s", 3))
		k.choDen(func() bool {
			for _, cs := range ds {
				if tc.SoNguyen(cs.tt["ton_dau"]) != 3 {
					return false
				}
			}
			return true
		})
		k.cho(0.5)
		var dh, nhanNut, ttNut string
		k.lam(func() { dh, nhanNut, ttNut = an.dongHo.text(), an.nutMua.Text, an.trangThaiNut() })
		logDemo(fmt.Sprintf("   An thấy: %s, nút '%s' (%s)", dh, nhanNut, ttNut))
		k.choDen(moHet)
		logDemo("   Mở bán: An (2), Bình (1), Chi (2) CÙNG bấm MUA NGAY")
		cungMua()
		k.choDen(xong)
		k.cho(0.5)
		inCuaSo(ds)
		inServer()
		logDemo("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; đặt lại")
		datTre("20")
		datLai(tc.T("ton_dau", 3, "mo_sau_s", 2))
		k.lam(func() { sv.chonCheDo("khong_khoa") })
		k.choDen(func() bool { return sv.lop.Kho.CheDo() == "khong_khoa" })
		k.choDen(func() bool {
			for _, cs := range ds {
				if cs.tt["che_do"] != "khong_khoa" {
					return false
				}
			}
			return moHet()
		})
		logDemo("   Mở bán: An (2), Bình (1), Chi (2) CÙNG bấm MUA NGAY")
		cungMua()
		k.choDen(xong)
		k.cho(0.6)
		inCuaSo(ds)
		inServer()
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui_tranh_chap.png"), [][]fyne.Window{{sv.w, chi.w}})
		logDemo("4. Máy chủ: khoa, trễ 0–1 ms, kho 100; So sánh 4 chế độ")
		datTre("1")
		datLai(tc.T("ton_dau", 100, "mo_sau_s", 30))
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
		logDemo("5. Đặt lại: kho 100, mở bán sau 4 s")
		datLai(tc.T("ton_dau", 100, "mo_sau_s", 4))
		k.choDen(func() bool {
			for _, cs := range ds {
				if mo, _ := cs.tt["mo_ban"].(bool); tc.SoNguyen(cs.tt["ton"]) != 100 || mo {
					return false
				}
			}
			return true
		})
		k.cho(1.5)
		var tt string
		k.lam(func() { tt, dh, nhanNut = chi.nhanTrThai.Text, chi.dongHo.text(), chi.nutMua.Text })
		logDemo(fmt.Sprintf("   Chi thấy: %s %s, nút '%s'", tt, dh, nhanNut))
		anhChi := chupCuaSo([]fyne.Window{chi.w})
		k.choDen(func() bool { return !an.nutMua.Disabled() })
		logDemo("6. Mở bán: An mua 2; 50 người ảo cùng mua; Bình mua 2")
		k.lam(func() { an.mua(2) })
		k.choDen(func() bool { return !an.dangCho })
		k.lam(func() { sv.oN.SetText("50") })
		k.lam(func() { bam(sv.nutBan) })
		k.cho(0.3)
		k.choDen(func() bool { return !sv.dangMoPhong })
		k.lam(func() { sv.xongMoPhong(sv.hangSoSanh) }) // bảng: hiện lại kết quả so sánh 4 chế độ
		k.lam(func() { binh.mua(2) })
		k.choDen(func() bool { return !binh.dangCho })
		k.cho(0.6)
		inCuaSo([]*cuaSoFlashSale{an, binh})
		inServer()
		logDemo("7. Chụp cửa sổ (Chi: lúc đếm ngược)")
		ba := chupCuaSo([]fyne.Window{sv.w, an.w, binh.w})
		ra := filepath.Join(thuMuc, "ket_qua_gui.png")
		if err := ghepLuoi(ra, [][]image.Image{{ba[0], ba[1]}, {ba[2], anhChi[0]}}); err == nil {
			logDemo("   Đã ghi " + filepath.Base(ra) + kichThuoc(ra))
		}
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

// kichThuoc – " (… KB)" của một tệp ảnh vừa ghi.
func kichThuoc(duong string) string {
	st, err := os.Stat(duong)
	if err != nil {
		return ""
	}
	return fmt.Sprintf(" (%d KB)", st.Size()/1024)
}
