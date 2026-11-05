package main

// Cửa sổ "Camera Server": lưới 4×4 ô (mỗi client một ô: hình mới nhất, tên, fps nhận được, chấm đỏ
// khi camera tắt); khung phải: kết nối, START/STOP SERVER, danh sách client, điều khiển client đang
// chọn (Tắt/Bật camera, fps gửi tối đa, độ phân giải gửi), fps chung, nhật ký; thanh trạng thái.
//
// Luồng: lõi video.Phong (goroutine TCP từng client, UDP nhận + ghép mảnh, vòng gửi lưới 5 ms)
// -> goroutine hẹn giờ 100 ms đọc TrangThai() -> fyne.Do -> luồng giao diện vẽ.

import (
	"fmt"
	"image/color"
	"strconv"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"

	vd "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.3_HoiThoaiVideo/video"
)

var dsFps = []string{"1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15"}

// oCamera – một ô trong lưới 4×4.
type oCamera struct {
	anh   *canvas.Image
	ten   *canvas.Text
	fps   *canvas.Text
	cham  *canvas.Circle
	tat   *canvas.Text // "CAMERA TẮT" giữa ô
	khung fyne.CanvasObject
	ma    uint16
	soAnh int
}

func moiO() *oCamera {
	o := &oCamera{anh: khungAnh(200, 150), ten: canvas.NewText("", color.White), fps: canvas.NewText("", color.NRGBA{255, 230, 0, 255}),
		cham: canvas.NewCircle(color.NRGBA{230, 30, 30, 255}), tat: canvas.NewText("CAMERA TẮT", color.NRGBA{255, 120, 120, 255})}
	o.tat.TextStyle.Bold = true
	o.tat.Hide()
	o.ten.TextStyle.Bold = true
	o.fps.TextSize = 11
	o.anh.Hide()
	o.cham.Hide()
	nen := canvas.NewRectangle(color.NRGBA{0x2A, 0x2E, 0x35, 0xFF})
	vien := canvas.NewRectangle(color.Transparent)
	vien.StrokeColor, vien.StrokeWidth = color.NRGBA{0x80, 0x88, 0x90, 0xFF}, 2
	cham := container.NewGridWrap(fyne.NewSize(14, 14), o.cham)
	tren := container.NewHBox(o.ten, layout.NewSpacer(), cham)
	o.khung = container.NewStack(nen, o.anh, container.NewCenter(o.tat),
		container.NewPadded(container.NewBorder(tren, o.fps, nil, nil)), vien)
	return o
}

type cuaSoServer struct {
	w                    fyne.Window
	phong                *vd.Phong
	ip, congTCP, congUDP *widget.Entry
	batDau, dung         *widget.Button
	o                    [16]*oCamera
	ds                   []vd.TrangThaiCamera
	danhSach             *widget.List
	chon                 uint16
	nhanChon             *widget.Label
	nutCamera            *widget.Button
	fpsGui, phanGiai     *widget.Select
	fpsChung             *widget.Select
	dongBo               bool // đang đặt lại Select theo client được chọn: không gửi lệnh
	trangThai            *widget.Label
	log                  *nhatKy
	dungHenGio           chan struct{}
}

func moCuaSoServer(a fyne.App, host string, tcp, udp int) *cuaSoServer {
	s := &cuaSoServer{w: a.NewWindow("Camera Server"), log: moiNhatKy()}
	s.ip, s.congTCP, s.congUDP = oNhap(host), oNhap(fmt.Sprint(tcp)), oNhap(fmt.Sprint(udp))
	s.batDau = nut("START SERVER", widget.SuccessImportance, s.khoiDong)
	s.dung = nut("STOP SERVER", widget.DangerImportance, s.dungServer)
	s.dung.Disable()
	luoi := container.NewGridWithColumns(4)
	for i := range s.o {
		s.o[i] = moiO()
		luoi.Add(s.o[i].khung)
	}
	s.danhSach = widget.NewList(func() int { return len(s.ds) },
		func() fyne.CanvasObject { return nhanCat("") },
		func(i widget.ListItemID, o fyne.CanvasObject) {
			t := s.ds[i]
			o.(*widget.Label).SetText(fmt.Sprintf("#%d %s – camera %s – vào %.0f fps %.0f kbit/s", t.Ma, t.Ten,
				t.LyDo, t.FpsVao, t.KbpsVao))
		})
	s.danhSach.OnSelected = func(i widget.ListItemID) {
		if i < len(s.ds) {
			s.chonClient(s.ds[i])
		}
	}
	s.nhanChon = nhanCat("Chưa chọn client (bấm vào danh sách)")
	s.nutCamera = nut("Tắt camera", widget.DangerImportance, func() {
		for _, t := range s.ds {
			if t.Ma == s.chon && s.phong != nil {
				s.phong.TatCamera(t.Ma, !t.ServerTat)
			}
		}
	})
	s.fpsGui = widget.NewSelect(dsFps, func(v string) {
		if n, _ := strconv.Atoi(v); !s.dongBo && s.phong != nil && s.chon != 0 && n > 0 {
			s.phong.DatFpsToiDa(s.chon, n)
		}
	})
	s.phanGiai = widget.NewSelect([]string{"1/1", "1/2", "1/4"}, func(v string) {
		n := map[string]int{"1/1": 1, "1/2": 2, "1/4": 4}[v]
		if !s.dongBo && s.phong != nil && s.chon != 0 && n > 0 {
			s.phong.DatDoPhanGiai(s.chon, n)
		}
	})
	s.fpsChung = widget.NewSelect(dsFps, func(v string) {
		if n, _ := strconv.Atoi(v); !s.dongBo && s.phong != nil && n > 0 {
			s.phong.DatFpsChung(n)
		}
	})
	s.dongBo = true
	s.fpsChung.SetSelected("10")
	s.dongBo = false
	s.trangThai = nhanCat("Trạng thái: server chưa chạy")
	s.datDieuKhien(false)

	tieuDe := func(t string) *widget.Label {
		return widget.NewLabelWithStyle(t, fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	}
	ketNoi := container.NewVBox(tieuDe("Kết nối"),
		container.NewGridWithColumns(2, widget.NewLabel("IP:"), s.ip, widget.NewLabel("Cổng TCP:"), s.congTCP,
			widget.NewLabel("Cổng UDP:"), s.congUDP),
		container.NewGridWithColumns(2, s.batDau, s.dung))
	dieuKhien := container.NewVBox(tieuDe("Client đang chọn"), s.nhanChon, s.nutCamera,
		container.NewGridWithColumns(2, widget.NewLabel("fps gửi tối đa:"), s.fpsGui,
			widget.NewLabel("Độ phân giải gửi:"), s.phanGiai, widget.NewLabel("fps chung (mọi người):"), s.fpsChung))
	dsBox := container.NewBorder(tieuDe("Danh sách client"), nil, nil, nil, s.danhSach)
	logBox := container.NewBorder(tieuDe("Nhật ký"), nil, nil, nil, s.log.ds)
	phai := container.NewBorder(container.NewVBox(ketNoi, widget.NewSeparator()), dieuKhien, nil, nil,
		container.NewVSplit(dsBox, logBox))
	noiDung := container.NewHSplit(container.NewPadded(luoi), phai)
	noiDung.Offset = 0.70
	s.w.SetContent(container.NewBorder(nil, container.NewVBox(widget.NewSeparator(), s.trangThai), nil, nil, noiDung))
	s.w.Resize(fyne.NewSize(1250, 760))
	s.w.SetOnClosed(s.dungServer)
	return s
}

func (s *cuaSoServer) datDieuKhien(chay bool) {
	if chay {
		s.batDau.Disable()
		s.dung.Enable()
		s.fpsChung.Enable()
	} else {
		s.batDau.Enable()
		s.dung.Disable()
		s.fpsChung.Disable()
		s.chon = 0
	}
	s.datDieuKhienChon()
}

func (s *cuaSoServer) datDieuKhienChon() {
	if s.chon == 0 || s.phong == nil {
		s.nutCamera.Disable()
		s.fpsGui.Disable()
		s.phanGiai.Disable()
		return
	}
	s.nutCamera.Enable()
	s.fpsGui.Enable()
	s.phanGiai.Enable()
}

func (s *cuaSoServer) khoiDong() {
	host, tcp, udp := s.ip.Text, soNguyen(s.congTCP.Text, 41635), soNguyen(s.congUDP.Text, 41636)
	s.batDau.Disable()
	go func() {
		p, err := vd.MoPhong(host, tcp, udp, s.log.them)
		fyne.Do(func() {
			if err != nil {
				s.trangThai.SetText("Trạng thái: không mở được server – " + err.Error())
				s.batDau.Enable()
				return
			}
			s.phong = p
			if n, _ := strconv.Atoi(s.fpsChung.Selected); n > 0 && n != vd.FpsMacDinh {
				p.DatFpsChung(n)
			}
			s.trangThai.SetText(fmt.Sprintf("Trạng thái: server đang chạy tại %s:%d (UDP %d)", host, tcp, udp))
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
	p := s.phong
	s.phong = nil
	go p.Dong()
	s.ds = nil
	s.danhSach.UnselectAll()
	s.danhSach.Refresh()
	for _, o := range s.o {
		o.ma, o.soAnh = 0, 0
		o.anh.Hide()
		o.cham.Hide()
		o.tat.Hide()
		o.ten.Text, o.fps.Text = "", ""
		o.ten.Refresh()
		o.fps.Refresh()
	}
	s.trangThai.SetText("Trạng thái: server đã dừng")
	s.datDieuKhien(false)
}

// chonClient – người dùng chọn một dòng: hiện tham số của client đó lên các ô chọn (không gửi lệnh).
func (s *cuaSoServer) chonClient(t vd.TrangThaiCamera) {
	s.chon = t.Ma
	s.dongBo = true
	s.fpsGui.SetSelected(strconv.Itoa(t.FpsToiDa))
	s.phanGiai.SetSelected(fmt.Sprintf("1/%d", t.Chia))
	s.dongBo = false
	s.capNhatChon()
	s.datDieuKhienChon()
}

func (s *cuaSoServer) capNhatChon() {
	for _, t := range s.ds {
		if t.Ma != s.chon {
			continue
		}
		s.nhanChon.SetText(fmt.Sprintf("#%d %s: camera %s, gửi ≤ %d fps, 1/%d", t.Ma, t.Ten, t.LyDo, t.FpsToiDa, t.Chia))
		if t.ServerTat {
			s.nutCamera.SetText("Bật camera (gỡ lệnh tắt)")
			s.nutCamera.Importance = widget.SuccessImportance
		} else {
			s.nutCamera.SetText("Tắt camera")
			s.nutCamera.Importance = widget.DangerImportance
		}
		s.nutCamera.Refresh()
	}
}

func (s *cuaSoServer) henGio(p *vd.Phong, dung chan struct{}) {
	t := time.NewTicker(100 * time.Millisecond)
	defer t.Stop()
	daCo := map[uint16]int{}
	for {
		select {
		case <-dung:
			return
		case <-t.C:
		}
		ds, fpsChung := p.TrangThai(daCo)
		for _, c := range ds {
			daCo[c.Ma] = c.SoAnh
		}
		fyne.Do(func() { s.ve(ds, fpsChung) })
	}
}

func (s *cuaSoServer) ve(ds []vd.TrangThaiCamera, fpsChung int) {
	if s.phong == nil {
		return
	}
	s.ds = ds
	s.danhSach.Refresh()
	tongVao := 0.0
	for i, o := range s.o {
		if i >= len(ds) {
			if o.ma != 0 {
				o.ma = 0
				o.anh.Hide()
				o.cham.Hide()
				o.tat.Hide()
				o.ten.Text, o.fps.Text = "", ""
				o.ten.Refresh()
				o.fps.Refresh()
			}
			continue
		}
		t := ds[i]
		tongVao += t.KbpsVao
		if o.ma != t.Ma {
			o.ma, o.soAnh = t.Ma, -1
		}
		if t.Anh != nil && o.soAnh != t.SoAnh {
			o.anh.Image = anhRGB(t.Anh, vd.Rong, vd.Cao)
			o.anh.Show()
			o.anh.Refresh()
		}
		o.soAnh = t.SoAnh
		if !t.CamMo {
			o.anh.Hide()
			o.cham.Show()
			o.tat.Show()
		} else {
			o.cham.Hide()
			o.tat.Hide()
		}
		o.ten.Text = fmt.Sprintf("#%d %s", t.Ma, t.Ten)
		o.fps.Text = fmt.Sprintf("%.0f fps · %.0f kbit/s · camera %s", t.FpsVao, t.KbpsVao, t.LyDo)
		if t.Chia > 1 {
			o.fps.Text += fmt.Sprintf(" · 1/%d", t.Chia)
		}
		o.ten.Refresh()
		o.fps.Refresh()
	}
	s.capNhatChon()
	if s.fpsChung.Selected != strconv.Itoa(fpsChung) {
		s.dongBo = true
		s.fpsChung.SetSelected(strconv.Itoa(fpsChung))
		s.dongBo = false
	}
	s.trangThai.SetText(fmt.Sprintf("Trạng thái: server đang chạy – %d client, fps chung %d, tổng vào %.0f kbit/s",
		len(ds), fpsChung, tongVao))
}
