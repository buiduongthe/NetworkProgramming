package main

// Cửa sổ server – "Bảng điều khiển âm thanh": kết nối, điều khiển chung, micro của server, nghe,
// một hàng cho mỗi client (mic, thanh đo mức, khóa/gỡ khóa, độ lợi), nhật ký.
//
// Luồng: luồng giao diện (Fyne) <- fyne.Do <- goroutine hẹn giờ 100 ms đọc amthanh.Phong.TrangThai();
// lõi Phong chạy các goroutine mạng (TCP từng client, UDP nhận) và vòng trộn 20 ms.

import (
	"fmt"
	"math"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	ht "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.2_HoiThoaiAmThanh/amthanh"
)

const tenGiaLapServer = "Giả lập (sin 330 Hz)"

// hangKhach – các widget của một client trong bảng.
type hangKhach struct {
	ma                 uint32
	khung              fyne.CanvasObject
	ten, mic, nghe     *widget.Label
	muc                *thanhDo
	khoa               *widget.Button
	doLoi              *widget.Slider
	nhanDoLoi          *widget.Label
	dangKhoa, dangKeoD bool
}

type cuaSoServer struct {
	w                    fyne.Window
	phong                *ht.Phong
	ip, congTCP, congUDP *widget.Entry
	batDau, dung         *widget.Button
	moTatCa, tatTatCa    *widget.Button
	amLuongChung         *widget.Slider
	nhanAmLuong          *widget.Label
	chonMic              *widget.Select
	batMic, tatMic       *widget.Button
	heSoMic              *widget.Slider
	nhanMic, nguonMic    *widget.Label
	mucMic               *thanhDo
	nghe                 *widget.Check
	mucNghe              *thanhDo
	nhanNghe             *widget.Label
	trangThai, soKhach   *widget.Label
	bang                 *fyne.Container
	hang                 map[uint32]*hangKhach
	log                  *nhatKy
	thietBi              []ht.ThietBi
	micServer            *ht.NguonMic
	dungHenGio           chan struct{}
}

func moCuaSoServer(a fyne.App, host string, tcp, udp int) *cuaSoServer {
	s := &cuaSoServer{w: a.NewWindow("Bảng điều khiển âm thanh – Server"), hang: map[uint32]*hangKhach{},
		log: moiNhatKy()}
	s.ip, s.congTCP, s.congUDP = oNhap(host), oNhap(fmt.Sprint(tcp)), oNhap(fmt.Sprint(udp))
	s.batDau = nut("Start Server", widget.SuccessImportance, s.khoiDong)
	s.dung = nut("Stop Server", widget.DangerImportance, s.dungServer)
	s.dung.Disable()

	s.moTatCa = nut("Mở mic tất cả", widget.HighImportance, func() { s.phong.KhoaMic(0, false) })
	s.tatTatCa = nut("Tắt mic tất cả", widget.WarningImportance, func() { s.phong.KhoaMic(0, true) })
	s.nhanAmLuong = widget.NewLabel("Âm lượng chung: 100%")
	s.amLuongChung = widget.NewSlider(0, 200)
	s.amLuongChung.Step = 5
	s.amLuongChung.SetValue(100)
	s.amLuongChung.OnChanged = func(v float64) {
		s.nhanAmLuong.SetText(fmt.Sprintf("Âm lượng chung: %.0f%%", v))
		if s.phong != nil {
			s.phong.DatAmLuongChung(v / 100)
		}
	}

	s.chonMic = widget.NewSelect([]string{tenGiaLapServer}, nil)
	s.chonMic.SetSelected(tenGiaLapServer)
	s.batMic = nut("Start Mic", widget.SuccessImportance, s.batMicServer)
	s.tatMic = nut("Stop Mic", widget.DangerImportance, s.tatMicServer)
	s.nhanMic = widget.NewLabel("Mic Volume: 100%")
	s.heSoMic = widget.NewSlider(0, 200)
	s.heSoMic.Step = 5
	s.heSoMic.SetValue(100)
	s.heSoMic.OnChanged = func(v float64) {
		s.nhanMic.SetText(fmt.Sprintf("Mic Volume: %.0f%%", v))
		if s.phong != nil {
			s.phong.DatHeSoMicServer(v / 100)
		}
	}
	s.nguonMic = nhanCat("Micro server: tắt")
	s.mucMic = moiThanhDo()

	s.nghe = widget.NewCheck("Phát tiếng client ra loa server", s.datNghe)
	s.mucNghe = moiThanhDo()
	s.nhanNghe = nhanCat("Loa: tắt")

	s.trangThai = nhanCat("Server chưa chạy")
	s.trangThai.Importance = widget.DangerImportance
	s.soKhach = widget.NewLabel("Số client: 0")
	s.bang = container.NewVBox()
	s.datDieuKhien(false)

	ketNoi := nhom("Kết nối",
		container.NewGridWithColumns(2, widget.NewLabel("IP:"), s.ip, widget.NewLabel("Cổng TCP:"), s.congTCP,
			widget.NewLabel("Cổng UDP:"), s.congUDP),
		container.NewGridWithColumns(2, s.batDau, s.dung))
	chung := nhom("Điều khiển chung", container.NewGridWithColumns(2, s.moTatCa, s.tatTatCa),
		s.nhanAmLuong, s.amLuongChung)
	micro := nhom("Micro của server", s.chonMic, container.NewGridWithColumns(2, s.batMic, s.tatMic),
		container.NewBorder(nil, nil, s.nhanMic, nil, s.heSoMic), s.mucMic.bar, s.nguonMic)
	nghe := nhom("Nghe", s.nghe, s.mucNghe.bar, s.nhanNghe)
	tren := container.NewGridWithColumns(4, ketNoi, chung, micro, nghe)
	dongTT := container.NewVBox(s.trangThai, s.soKhach)
	tieuDeBang := widget.NewLabelWithStyle("Client trong phòng", fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	tieuDeLog := widget.NewLabelWithStyle("Nhật ký sự kiện", fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	giua := container.NewHSplit(
		container.NewBorder(tieuDeBang, nil, nil, nil, container.NewVScroll(s.bang)),
		container.NewBorder(tieuDeLog, nil, nil, nil, s.log.ds))
	giua.Offset = 0.62
	s.w.SetContent(container.NewBorder(container.NewVBox(tren, dongTT, widget.NewSeparator()), nil, nil, nil, giua))
	s.w.Resize(fyne.NewSize(1320, 700))
	s.w.SetOnClosed(func() { s.dungServer(); s.tatMicServer() })

	go func() { // liệt kê micro thật (ffmpeg có thể mất ~1 s) – không làm trên luồng giao diện
		ds := ht.LietKeMicro()
		fyne.Do(func() {
			s.thietBi = ds
			tuyChon := []string{tenGiaLapServer}
			for _, tb := range ds {
				tuyChon = append(tuyChon, tb.Ten)
			}
			s.chonMic.SetOptions(tuyChon)
			if !ht.CoLenh("ffmpeg") {
				s.nguonMic.SetText("Không có ffmpeg: chỉ dùng giả lập")
			}
		})
	}()
	return s
}

// datDieuKhien – bật/tắt các nút phụ thuộc server đang chạy (gọi trên luồng giao diện).
func (s *cuaSoServer) datDieuKhien(chay bool) {
	for _, b := range []*widget.Button{s.moTatCa, s.tatTatCa, s.batMic, s.tatMic} {
		if chay {
			b.Enable()
		} else {
			b.Disable()
		}
	}
	if chay {
		s.nghe.Enable()
		s.batDau.Disable()
		s.dung.Enable()
		s.tatMic.Disable()
	} else {
		s.nghe.Disable()
		s.batDau.Enable()
		s.dung.Disable()
	}
}

func (s *cuaSoServer) khoiDong() {
	host, tcp, udp := s.ip.Text, soNguyen(s.congTCP.Text, 41625), soNguyen(s.congUDP.Text, 41626)
	s.batDau.Disable()
	go func() { // mở cổng ngoài luồng giao diện
		p, err := ht.MoPhong(host, tcp, udp, s.log.them)
		fyne.Do(func() {
			if err != nil {
				s.trangThai.SetText("Không mở được server: " + err.Error())
				s.batDau.Enable()
				return
			}
			s.phong = p
			p.DatAmLuongChung(s.amLuongChung.Value / 100)
			p.DatHeSoMicServer(s.heSoMic.Value / 100)
			s.trangThai.SetText(fmt.Sprintf("Server đang chạy tại %s:%d (UDP %d)", host, tcp, udp))
			s.trangThai.Importance = widget.SuccessImportance
			s.trangThai.Refresh()
			s.datDieuKhien(true)
			s.dungHenGio = make(chan struct{})
			go s.henGio(p, s.dungHenGio)
		})
	}()
}

func (s *cuaSoServer) dungServer() {
	if s.phong == nil {
		return
	}
	close(s.dungHenGio)
	s.tatMicServer()
	s.nghe.SetChecked(false)
	p := s.phong
	s.phong = nil
	go p.Dong()
	s.trangThai.SetText("Server đã dừng")
	s.trangThai.Importance = widget.DangerImportance
	s.trangThai.Refresh()
	s.soKhach.SetText("Số client: 0")
	s.bang.RemoveAll()
	s.hang = map[uint32]*hangKhach{}
	s.datDieuKhien(false)
}

// batMicServer – Start Mic: chỉ lúc này mới mở micro thật (nếu người dùng chọn thiết bị thật).
func (s *cuaSoServer) batMicServer() {
	var tb *ht.ThietBi
	for i := range s.thietBi {
		if s.thietBi[i].Ten == s.chonMic.Selected {
			tb = &s.thietBi[i]
		}
	}
	s.batMic.Disable()
	go func() {
		n := ht.MoNguonMic(tb, 330)
		fyne.Do(func() {
			if s.phong == nil {
				n.Dong()
				return
			}
			s.micServer = n
			s.phong.DatNguonServer(n)
			s.tatMic.Enable()
			s.log.them("Server bật micro: " + n.MoTa())
		})
	}()
}

func (s *cuaSoServer) tatMicServer() {
	if s.micServer == nil {
		return
	}
	if s.phong != nil {
		s.phong.DatNguonServer(nil)
	}
	s.micServer.Dong()
	s.micServer = nil
	s.nguonMic.SetText("Micro server: tắt")
	s.batMic.Enable()
	s.tatMic.Disable()
	s.log.them("Server tắt micro")
}

// datNghe – bật/tắt phát ra loa của máy server (mở ffplay ở goroutine nền).
func (s *cuaSoServer) datNghe(bat bool) {
	if s.phong == nil {
		return
	}
	p := s.phong
	if !bat {
		p.DatNghe(false, nil)
		s.nhanNghe.SetText("Loa: tắt")
		return
	}
	s.nhanNghe.SetText("Loa: đang mở ffplay…")
	go func() {
		l, err := ht.MoLoa()
		fyne.Do(func() {
			if !s.nghe.Checked {
				if l != nil {
					l.Dong()
				}
				return
			}
			if err != nil {
				p.DatNghe(true, nil)
				s.nhanNghe.SetText("Không phát được ra loa (chỉ đo mức): " + err.Error())
				return
			}
			p.DatNghe(true, l)
			s.nhanNghe.SetText("Loa: đang phát (ffplay)")
		})
	}()
}

// henGio – goroutine nền: 10 lần/giây chụp trạng thái lõi rồi chuyển việc vẽ về luồng giao diện.
func (s *cuaSoServer) henGio(p *ht.Phong, dung chan struct{}) {
	t := time.NewTicker(100 * time.Millisecond)
	defer t.Stop()
	for {
		select {
		case <-dung:
			return
		case <-t.C:
		}
		ds, mucServer, mucNghe, _ := p.TrangThai()
		fyne.Do(func() { s.ve(ds, mucServer, mucNghe) })
	}
}

// ve – cập nhật widget theo ảnh chụp trạng thái (luồng giao diện).
func (s *cuaSoServer) ve(ds []ht.TrangThaiThanhVien, mucServer, mucNghe float64) {
	if s.phong == nil {
		return
	}
	s.soKhach.SetText(fmt.Sprintf("Số client: %d", len(ds)))
	s.mucMic.datMuc(mucServer)
	if s.micServer != nil {
		chu := "Nguồn: " + s.micServer.MoTa()
		if s.micServer.GiaLap() {
			chu += "  [GIẢ LẬP]"
		} else {
			chu += "  [THIẾT BỊ THẬT]"
		}
		s.nguonMic.SetText(chu)
	}
	s.mucNghe.datMuc(mucNghe)
	con := map[uint32]bool{}
	for _, t := range ds {
		con[t.Ma] = true
		h := s.hang[t.Ma]
		if h == nil {
			h = s.moiHang(t)
			s.hang[t.Ma] = h
			s.bang.Add(h.khung)
		}
		h.ten.SetText(fmt.Sprintf("#%d %s  (%s)", t.Ma, t.Ten, t.DiaChi))
		mic := "Mic: " + t.Mic
		switch {
		case t.Khoa:
			mic = "Mic: BỊ KHÓA (server)"
		case t.TuTat:
			mic = "Mic: tự tắt"
		}
		h.mic.SetText(mic)
		h.muc.datMuc(t.MucDB)
		h.nghe.SetText(fmt.Sprintf("Nghe: %.0f%% · gói %d, mất %d, jitter %.1f ms", t.AmLuongNghe*100,
			t.GoiNhan, t.Mat, t.JitterMs))
		h.dangKhoa = t.Khoa
		if t.Khoa {
			h.khoa.SetText("Gỡ khóa")
			h.khoa.Importance = widget.SuccessImportance
		} else {
			h.khoa.SetText("Khóa mic")
			h.khoa.Importance = widget.DangerImportance
		}
		h.khoa.Refresh()
	}
	for ma, h := range s.hang {
		if !con[ma] {
			s.bang.Remove(h.khung)
			delete(s.hang, ma)
		}
	}
}

func (s *cuaSoServer) moiHang(t ht.TrangThaiThanhVien) *hangKhach {
	h := &hangKhach{ma: t.Ma, ten: widget.NewLabelWithStyle("", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
		mic: nhanCat(""), nghe: nhanCat(""), muc: moiThanhDo(),
		nhanDoLoi: widget.NewLabel(fmt.Sprintf("Độ lợi: %.0f%%", t.DoLoi*100))}
	h.khoa = nut("Khóa mic", widget.DangerImportance, func() {
		if s.phong != nil {
			s.phong.KhoaMic(h.ma, !h.dangKhoa)
		}
	})
	h.doLoi = widget.NewSlider(0, 200)
	h.doLoi.Step = 5
	h.doLoi.SetValue(math.Round(t.DoLoi * 100))
	h.doLoi.OnChanged = func(v float64) {
		h.nhanDoLoi.SetText(fmt.Sprintf("Độ lợi: %.0f%%", v))
		if s.phong != nil {
			s.phong.DatDoLoi(h.ma, v/100)
		}
	}
	trai := container.NewVBox(h.ten, h.mic)
	giua := container.NewVBox(h.muc.bar, h.nghe)
	phai := container.NewBorder(nil, nil, h.nhanDoLoi, nil, h.doLoi)
	h.khung = widget.NewCard("", "", container.NewGridWithColumns(4, trai, giua, container.NewCenter(h.khoa), phai))
	return h
}
