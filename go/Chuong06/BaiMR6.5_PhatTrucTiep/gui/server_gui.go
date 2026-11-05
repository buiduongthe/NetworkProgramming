package main

// Cửa sổ "Máy chủ phát trực tiếp (SFU)": kết nối (IP, cổng TCP/UDP, START/STOP SERVER); công tắc Bộ
// đệm GOP; chỉ số mỗi giây (vào, ra, hệ số nhân bản, PLI); xem trước luồng (server tự giải mã MỘT bản
// chỉ để hiện ảnh – việc chuyển tiếp vẫn là nhân bản nguyên byte); bảng người xem, nút Ngắt người
// chọn; nhật ký; thanh trạng thái.
//
// Luồng: lõi tructiep.MayChuPhat (goroutine TCP từng client, MỘT goroutine UDP chuyển tiếp) ->
// goroutine hẹn giờ 100 ms đọc TrangThai() -> fyne.Do -> luồng giao diện vẽ.

import (
	"fmt"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"

	tt "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.5_PhatTrucTiep/tructiep"
)

var cotXem = cotCoDinh{70, 150, 50, 70, 60, 150}

type cuaSoServer struct {
	w                    fyne.Window
	lop                  *tt.MayChuPhat
	ip, congTCP, congUDP *widget.Entry
	batDau, dung         *widget.Button
	boDem                *widget.Check
	nhanBoDem            *widget.Label
	nhanVao, nhanRa      *widget.Label
	nhanHeSo, nhanPli    *widget.Label
	anh                  *canvas.Image
	cho                  *canvas.Text
	nhanDongHo           *widget.Label
	bang                 *widget.List
	ds                   []tt.TomTatXem
	chon                 uint16
	ngatChon             *widget.Button
	nhanPhat             *widget.Label
	trangThai            *widget.Label
	log                  *nhatKy
	dinhKy               *thongKeDinhKy
	daVe                 [2]int64
	dong                 chan struct{}
}

func moCuaSoServer(a fyne.App, host string, tcp, udp int) *cuaSoServer {
	s := &cuaSoServer{w: a.NewWindow("Máy chủ phát trực tiếp (SFU) – MR6.5"), log: moiNhatKy("server"),
		dinhKy: moiThongKeDinhKy("server"), daVe: [2]int64{-1, -1}, dong: make(chan struct{})}
	s.lop = tt.MoiMayChu(s.log.them)
	s.lop.XemTruoc = true
	s.ip, s.congTCP, s.congUDP = oNhap(host), oNhap(fmt.Sprint(tcp)), oNhap(fmt.Sprint(udp))
	s.batDau = nut("START SERVER", widget.SuccessImportance, s.khoiDong)
	s.dung = nut("STOP SERVER", widget.DangerImportance, s.dungServer)
	s.dung.Disable()
	s.boDem = widget.NewCheck("Bật", func(b bool) {
		if s.lop.Chay() {
			s.lop.DatBoDem(b)
		} else {
			s.lop.BoDemBat = b
		}
	})
	s.boDem.SetChecked(true)
	s.nhanBoDem = widget.NewLabel("Đang giữ: –")
	s.nhanVao, s.nhanRa = nhanDam("Vào: –"), nhanDam("Ra: –")
	s.nhanHeSo, s.nhanPli = nhanDam("Hệ số nhân bản (ra/vào): –"), widget.NewLabel("PLI chuyển: 0 (bỏ 0)")
	s.nhanHeSo.Importance = widget.HighImportance
	s.anh, s.cho = khungAnh(2*tt.Rong, 2*tt.Cao), chuCho("Chưa có luồng")
	s.nhanDongHo = widget.NewLabelWithStyle("Đồng hồ: –", fyne.TextAlignLeading, fyne.TextStyle{Monospace: true})
	s.bang = widget.NewList(func() int { return len(s.ds) },
		func() fyne.CanvasObject {
			return hangBang(cotXem, []bool{false, false, false, true, true, false}, false)
		},
		func(i widget.ListItemID, o fyne.CanvasObject) {
			x := s.ds[i]
			dau := "–"
			if x.KhungDauMs != nil {
				dau = fmt.Sprintf("%.0f ms (%s)", *x.KhungDauMs, x.NguonDau)
			}
			datHang(o.(*fyne.Container), []string{x.Ten, x.Udp, x.Nhan, fmt.Sprint(x.GoiRa), fmt.Sprintf("%.0f", x.Kbps), dau})
		})
	s.bang.OnSelected = func(i widget.ListItemID) {
		if i < len(s.ds) {
			s.chon = s.ds[i].Ma
		}
	}
	s.bang.OnUnselected = func(widget.ListItemID) { s.chon = 0 }
	s.ngatChon = nut("Ngắt người chọn", widget.WarningImportance, s.ngat)
	s.nhanPhat = widget.NewLabel("  Máy phát: –")
	s.trangThai = nhanCat("Trạng thái: server chưa chạy")

	ketNoi := nhom("Kết nối", container.New(layout.NewFormLayout(),
		widget.NewLabel("IP:"), s.ip, widget.NewLabel("TCP:"), s.congTCP, widget.NewLabel("UDP:"), s.congUDP),
		container.NewGridWithColumns(2, s.batDau, s.dung))
	boDem := nhom("Bộ đệm GOP (GOP cache)", container.NewHBox(s.boDem,
		widget.NewLabel("– người vào muộn nhận ngay\nkhung I mới nhất + các P sau nó")), s.nhanBoDem)
	chiSo := nhom("Chỉ số (mỗi giây)", s.nhanVao, s.nhanRa, s.nhanHeSo, s.nhanPli)
	tren := container.NewHBox(coDinh(ketNoi, 230, 190), coDinh(boDem, 290, 190), coDinh(chiSo, 330, 190))
	xemTruoc := nhom("Xem trước luồng (server tự giải mã)", oAnh(s.anh, s.cho), s.nhanDongHo)
	tieuDe := hangBang(cotXem, []bool{false, false, false, true, true, false}, true)
	datHang(tieuDe, []string{"Tên", "IP:cổng UDP", "Nhận", "Gói gửi", "kbit/s", "Khung I đầu"})
	nguoiXem := nhom("Người xem", tieuDe, coDinh(s.bang, 550, 230), container.NewHBox(s.ngatChon, s.nhanPhat))
	giua := container.NewHBox(xemTruoc, nguoiXem)
	nk := nhom("Nhật ký", coDinh(s.log.ds, 860, 150))
	s.w.SetContent(container.NewVBox(tren, giua, nk, widget.NewSeparator(), s.trangThai))
	s.w.SetFixedSize(true)
	s.w.SetOnClosed(func() { close(s.dong); go s.lop.Dung() })
	go s.henGio()
	giuVe(s.w, "server", s.dong)
	return s
}

func (s *cuaSoServer) oNhap(bat bool) {
	for _, o := range []*widget.Entry{s.ip, s.congTCP, s.congUDP} {
		if bat {
			o.Enable()
		} else {
			o.Disable()
		}
	}
}

func (s *cuaSoServer) khoiDong() {
	s.lop.BoDemBat = s.boDem.Checked
	if err := s.lop.BatDau(strings.TrimSpace(s.ip.Text), soNguyen(s.congTCP.Text, 41657), soNguyen(s.congUDP.Text, 41658)); err != nil {
		s.log.them("Không khởi động được: " + err.Error())
		return
	}
	s.batDau.Disable()
	s.dung.Enable()
	s.oNhap(false)
}

func (s *cuaSoServer) dungServer() {
	s.dung.Disable()
	go func() { // chờ goroutine nền dừng: không chặn giao diện
		s.lop.Dung()
		fyne.Do(func() {
			s.batDau.Enable()
			s.oNhap(true)
		})
	}()
}

func (s *cuaSoServer) ngat() {
	if s.chon > 0 {
		s.lop.Ngat("", s.chon)
		s.bang.UnselectAll()
	}
}

// chonNguoiXem – chọn dòng theo tên (dùng trong kịch bản demo).
func (s *cuaSoServer) chonNguoiXem(ten string) {
	for i, x := range s.ds {
		if x.Ten == ten {
			s.bang.Select(i)
		}
	}
}

func (s *cuaSoServer) henGio() {
	t := time.NewTicker(100 * time.Millisecond)
	defer t.Stop()
	for {
		select {
		case <-s.dong:
			return
		case <-t.C:
		}
		tr := s.lop.TrangThai()
		chay := s.lop.Chay()
		fyne.Do(func() { s.ve(tr, chay) })
	}
}

func (s *cuaSoServer) ve(t tt.TrangThaiMayChu, chay bool) {
	if chay {
		s.dinhKy.inNeuDenHan(func() string {
			phat := "chưa phát"
			if t.DangPhat {
				phat = "đang phát"
			}
			d := fmt.Sprintf("vào %.0f kbit/s (tiếng %.0f, hình %.0f), ra %.0f, hệ số %.2f, %s;",
				t.KbpsVao, t.KbpsVaoAm, t.KbpsVaoHinh, t.KbpsRa, t.HeSo, phat)
			for _, x := range t.NguoiXem {
				d += fmt.Sprintf(" %s %s %.0f", x.Ten, x.Nhan, x.Kbps)
			}
			return d
		})
		s.nhanVao.SetText(fmt.Sprintf("Vào: %.0f kbit/s (tiếng %.0f, hình %.0f)", t.KbpsVao, t.KbpsVaoAm, t.KbpsVaoHinh))
		s.nhanRa.SetText(fmt.Sprintf("Ra: %.0f kbit/s tới %d người xem", t.KbpsRa, len(t.NguoiXem)))
		s.nhanHeSo.SetText(fmt.Sprintf("Hệ số nhân bản (ra/vào): %.2f", t.HeSo))
		s.nhanPli.SetText(fmt.Sprintf("PLI chuyển: %d (bỏ %d do lọc 500 ms)", t.PliChuyen, t.PliBo))
		switch {
		case t.BoDemBat && t.BoDemSoI >= 0:
			s.nhanBoDem.SetText(fmt.Sprintf("Đang giữ: I #%d và các P sau: %d gói, %.0f KB", t.BoDemSoI, t.BoDemGoi,
				float64(t.BoDemByte)/1024))
		case t.BoDemBat:
			s.nhanBoDem.SetText("Đang giữ: – (chờ khung I)")
		default:
			s.nhanBoDem.SetText("Tắt: người vào muộn chờ khung I")
		}
		phat := "–"
		if t.TenPhat != "" {
			phat = t.TenPhat
		}
		if t.DangPhat {
			phat += " (đang phát)"
		}
		s.nhanPhat.SetText("  Máy phát: " + phat)
		trang := "chưa phát"
		if t.DangPhat {
			trang = "đang phát"
		}
		s.trangThai.SetText(fmt.Sprintf("Trạng thái: server chạy tại %s (UDP %d) – %s, %d người xem",
			s.lop.DiaChi, s.lop.CongUDP, trang, len(t.NguoiXem)))
	} else {
		s.trangThai.SetText("Trạng thái: server chưa chạy")
	}
	s.ds = t.NguoiXem
	if !chay {
		s.ds = nil
	}
	s.bang.Refresh()
	if a := t.Anh; a != nil && chay && (int64(a.Ts) != s.daVe[0] || a.So != s.daVe[1]) {
		s.anh.Image = tt.AnhNRGBA(a.Rgb, a.W, a.H)
		s.anh.Refresh()
		s.cho.Text = ""
		s.cho.Refresh()
		s.nhanDongHo.SetText(fmt.Sprintf("Đồng hồ phương tiện: t=%7.3f s, khung #%d", float64(a.Ts)/1000, a.So))
		s.daVe = [2]int64{int64(a.Ts), a.So}
	} else if a == nil && s.cho.Text == "" {
		s.anh.Image = tt.AnhNRGBA(nil, 4, 3)
		s.anh.Refresh()
		s.cho.Text = "Chưa có luồng"
		s.cho.Refresh()
		s.daVe = [2]int64{-1, -1}
	}
}
