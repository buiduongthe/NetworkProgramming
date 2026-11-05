package main

// demo_gui – kịch bản minh họa tự động trong MỘT tiến trình (chạy dưới xvfb-run trên Linux): bảng điều
// khiển máy chủ và ba trang bán vé của An, Bình, Chi. Kịch bản bấm nút như người dùng (gọi đúng hàm xử lý
// trên luồng giao diện qua fyne.DoAndWait), in "[demo] …":
//   1–3. phòng chờ bật, 2000 vé, 50 người/s: Chi vào hàng đầu tiên -> tới lượt ngay; 300 người ảo ùa vào;
//        An, Bình vào hàng sau họ (thanh tiến trình, "còn khoảng … người phía trước"); chụp
//        ket_qua_gui_phong_cho.png; An tới lượt, mua 3 vé;
//   4.   chế độ khong_khoa, trễ 0–20 ms: hai tab của An (cửa sổ Chi đăng nhập lại tên An) cùng có một
//        lượt, cùng bấm THANH TOÁN -> cả hai đơn được nhận -> SAI PHẠM "sai lượt"; chụp
//        ket_qua_gui_tranh_chap.png;
//   5.   khoa: So sánh 4 chế độ (1000 người) và So sánh bật/tắt phòng chờ -> hai bảng;
//   6.   20 người/s, thùng 2: Bình, Chi mua; 400 người ảo; An đứng trong hàng; chụp 4 cửa sổ ghép thành
//        ket_qua_gui.png.

import (
	"fmt"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.13_PhongCho/tranhchap"
)

func chayDemo(a fyne.App, host string, cong int, thuMuc string) {
	sv := moCuaSoServer(a, host, cong)
	an, binh, chi := moCuaSoPhongCho(a, "An", host, cong), moCuaSoPhongCho(a, "Bình", host, cong),
		moCuaSoPhongCho(a, "Chi", host, cong)
	ds := []*cuaSoPhongCho{an, binh, chi}
	k := &kichBan{}
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
	tre := func(ms string) {
		k.lam(func() {
			sv.oTre.SetText(ms)
			bam(sv.nutTre)
		})
	}
	mua := func(cs *cuaSoPhongCho, n string) { // trên luồng giao diện
		cs.chonSoVe.SetSelected(n)
		cs.tinhTien()
		bam(cs.nutMua)
	}
	ban := func(n, kk string) { // bấm "Bắn đồng loạt" với n người trên kk kết nối
		k.lam(func() {
			sv.oN.SetText(n)
			sv.oK.SetText(kk)
			bam(sv.nutBan)
		})
	}
	datLai := func(ts tc.TS) {
		k.lam(func() {
			sv.datThamSo(ts)
			bam(sv.nutDatLai)
			for _, cs := range ds {
				cs.veSuKien()
			}
		})
	}
	text := func(f func() string) string { var t string; k.lam(func() { t = f() }); return t }
	soCap := func() int64 { return tc.SoNguyen(sv.lop.TrangThai()["so_cap"]) }
	sk := func(cs *cuaSoPhongCho) tc.M { m, _ := cs.tt["su_kien"].(tc.M); return m }
	go func() {
		defer fyne.Do(a.Quit)
		k.cho(0.5)
		logDemo("1. Máy chủ: START SERVER (khoa, phòng chờ bật)")
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
		k.cho(0.8)
		logDemo("2. Đặt lại: 2000 vé, cấp lượt 50 người/s, thùng 50")
		datLai(tc.T("tong_ve", 2000, "toc_do", 50, "dung_luong", 50, "phong_cho", 1))
		k.choDen(func() bool { return tc.SoNguyen(sk(chi)["tong_ve"]) == 2000 })
		k.cho(0.4)
		logDemo("   Chi: VÀO HÀNG CHỜ (người đầu tiên)")
		k.lam(func() { bam(chi.nutVao) })
		k.choDen(func() bool { return chi.trang == "mua" })
		logDemo("   Chi: " + text(chi.bangLuot.text))
		logDemo("   Mô phỏng: 300 người ùa vào hàng (Bắn đồng loạt)")
		ban("300", "100")
		k.choDen(func() bool { return soCap() >= 250 })
		logDemo("   An, Bình: VÀO HÀNG CHỜ")
		k.lam(func() {
			bam(an.nutVao)
			bam(binh.nutVao)
		})
		k.choDen(func() bool { return an.trang == "cho" && binh.trang == "cho" })
		k.cho(0.5)
		logDemo("   Chi: chọn 2 vé, THANH TOÁN")
		k.lam(func() { mua(chi, "2") })
		k.cho(1.0)
		logDemo("   Chi: " + text(func() string { return chi.nhanKq.Text }))
		logDemo("   An: " + text(func() string { return an.nhanSo.Text }))
		logDemo("   An: " + text(func() string { return an.nhanTruoc.Text }))
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui_phong_cho.png"), [][]fyne.Window{{sv.w}, {an.w, chi.w}})
		k.choDen(func() bool { return an.trang == "mua" })
		k.cho(0.5)
		logDemo(fmt.Sprintf("3. An tới lượt: đồng hồ %s; chọn 3 vé, THANH TOÁN",
			text(func() string { return an.nhanDongHo.Text })))
		k.lam(func() { mua(an, "3") })
		k.choDen(func() bool { return an.soDon == 1 })
		logDemo("   An: " + text(func() string { return an.nhanKq.Text }))
		k.choDen(func() bool { return !sv.dangMoPhong })
		inServer()
		logDemo("4. Máy chủ: khong_khoa, trễ 0–20 ms; Đặt lại (500 vé)")
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
		datLai(tc.T("tong_ve", 500))
		k.choDen(func() bool { return tc.SoNguyen(sk(an)["tong_ve"]) == 500 && tc.SoNguyen(an.tt["da_cap_luot"]) == 0 })
		logDemo("   Chi: Rời, đăng nhập lại tên 'An' (tab thứ hai của An)")
		k.lam(func() {
			bam(chi.nutRoi)
			chi.oTen.SetText("An")
			bam(chi.nutKetNoi)
		})
		k.choDen(func() bool { return chi.kn != nil && chi.tt != nil })
		k.cho(0.5)
		logDemo("   Hai tab của An cùng VÀO HÀNG CHỜ")
		k.lam(func() {
			bam(an.nutVao)
			bam(chi.nutVao)
		})
		k.choDen(func() bool { return an.trang == "mua" && chi.trang == "mua" })
		logDemo("   Hai tab cùng bấm THANH TOÁN 2 vé (một lượt)")
		var dem int
		k.lam(func() {
			dem = an.soKq + chi.soKq
			mua(an, "2")
			mua(chi, "2")
		})
		k.choDen(func() bool { return an.soKq+chi.soKq == dem+2 })
		k.cho(0.5)
		logDemo("   tab 1: " + text(func() string { return an.nhanKq.Text }))
		logDemo("   tab 2: " + text(func() string { return chi.nhanKq.Text }))
		inServer()
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui_tranh_chap.png"), [][]fyne.Window{{sv.w, chi.w}})
		logDemo("5. khoa, trễ 0–2 ms; Chi vào lại; So sánh 4 chế độ")
		k.lam(func() {
			bam(chi.nutRoi)
			chi.oTen.SetText("Chi")
			bam(chi.nutKetNoi)
		})
		tre("2")
		k.lam(func() { sv.chonCheDo("khoa") })
		k.choDen(func() bool { return sv.lop.Kho.CheDo() == "khoa" && chi.kn != nil })
		k.lam(func() {
			sv.oN.SetText("1000")
			sv.oK.SetText("200")
		})
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
		logDemo("   So sánh bật/tắt phòng chờ (khoa, 1000 người)")
		k.lam(func() { bam(sv.nutSoSanhPC) })
		k.cho(0.3)
		k.choDen(func() bool { return !sv.dangMoPhong })
		var hpc []tc.HangPhongCho
		k.lam(func() { hpc = sv.hangPC })
		for _, d := range tc.BangPhongCho(hpc) {
			logDemo(d)
		}
		logDemo("6. Đặt lại: 2000 vé, 20 người/s, thùng 2")
		datLai(tc.T("tong_ve", 2000, "toc_do", 20, "dung_luong", 2, "phong_cho", 1))
		k.choDen(func() bool {
			return tc.SoNguyen(sk(an)["tong_ve"]) == 2000 && tc.SoThuc(an.tt["toc_do"]) == 20
		})
		k.cho(0.4)
		logDemo("   Bình, Chi vào hàng; Chi mua 1 vé")
		k.lam(func() {
			bam(binh.nutVao)
			bam(chi.nutVao)
		})
		k.choDen(func() bool { return binh.trang == "mua" && chi.trang == "mua" })
		k.lam(func() { mua(chi, "1") })
		logDemo("   400 người ùa vào; An vào hàng sau họ")
		ban("400", "100")
		k.choDen(func() bool { return soCap() >= 350 })
		k.lam(func() { bam(an.nutVao) })
		k.choDen(func() bool { return an.trang == "cho" })
		k.cho(2.0)
		logDemo("   An: " + text(func() string { return an.nhanTruoc.Text }))
		logDemo("   Bình: đồng hồ " + text(func() string { return binh.nhanDongHo.Text }))
		logDemo("   Chi: " + text(func() string { return chi.nhanKq.Text }))
		logDemo("7. Chụp 4 cửa sổ")
		ghiAnh(filepath.Join(thuMuc, "ket_qua_gui.png"), [][]fyne.Window{{sv.w, an.w}, {binh.w, chi.w}})
		logDemo("8. Máy chủ: STOP SERVER")
		k.lam(func() { bam(sv.nutStop) })
		k.choDen(func() bool { return !sv.nutStart.Disabled() })
		k.cho(0.5)
		logDemo("   Bình: " + text(func() string { return binh.thanhTT.Text }))
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
