package main

// Cửa sổ client – "Audio Client": kết nối; nhận âm thanh (âm lượng nghe, Start/Stop Audio,
// "Server Vol", thanh đo tiếng nhận); micro (chọn thiết bị thật/giả lập, Start/Stop Mic, Mic Volume,
// thanh đo); dòng trạng thái.
//
// Luồng: luồng giao diện <- fyne.Do <- goroutine hẹn giờ 100 ms đọc amthanh.Khach.TrangThai();
// lõi Khach có goroutine đọc TCP, goroutine nhận UDP và vòng 20 ms thu/gửi/nhận/phát.

import (
	"fmt"
	"strconv"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	ht "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.2_HoiThoaiAmThanh/amthanh"
)

type cuaSoClient struct {
	w                      fyne.Window
	khach                  *ht.Khach
	ip, cong, ten          *widget.Entry
	ketNoi, ngat           *widget.Button
	amLuong                *widget.Slider
	nhanAmLuong, serverVol *widget.Label
	batAudio, tatAudio     *widget.Button
	mucNhan                *thanhDo
	nhanLoa                *widget.Label
	chonMic                *widget.Select
	batMic, tatMic         *widget.Button
	heSoMic                *widget.Slider
	nhanHeSo, nguon, micTT *widget.Label
	mucMic                 *thanhDo
	trangThai              *widget.Label
	thietBi                []ht.ThietBi
	tenGiaLap              string
	tanSo                  float64
	mic                    *ht.NguonMic
	log                    *nhatKy
	dungHenGio             chan struct{}
	TrangThaiCuoi          ht.TrangThaiKhach // dùng cho demo in số liệu
}

func moCuaSoClient(a fyne.App, ten, host string, cong int, tanSo float64) *cuaSoClient {
	c := &cuaSoClient{w: a.NewWindow("Audio Client – " + ten), tanSo: tanSo, log: moiNhatKy(),
		tenGiaLap: "Giả lập (sin " + strconv.FormatFloat(tanSo, 'f', -1, 64) + " Hz)"}
	c.ip, c.cong, c.ten = oNhap(host), oNhap(fmt.Sprint(cong)), oNhap(ten)
	c.ketNoi = nut("Connect", widget.SuccessImportance, c.lamKetNoi)
	c.ngat = nut("Disconnect", widget.DangerImportance, c.lamNgat)

	c.nhanAmLuong = widget.NewLabel("Âm lượng nghe: 100%")
	c.amLuong = widget.NewSlider(0, 200)
	c.amLuong.Step = 5
	c.amLuong.SetValue(100)
	c.amLuong.OnChanged = func(v float64) {
		c.nhanAmLuong.SetText(fmt.Sprintf("Âm lượng nghe: %.0f%%", v))
		if c.khach != nil {
			c.khach.DatAmLuongNghe(v / 100)
		}
	}
	c.batAudio = nut("Start Audio", widget.SuccessImportance, c.lamBatAudio)
	c.tatAudio = nut("Stop Audio", widget.DangerImportance, c.lamTatAudio)
	c.serverVol = widget.NewLabelWithStyle("Server Vol: –", fyne.TextAlignTrailing, fyne.TextStyle{Bold: true})
	c.mucNhan = moiThanhDo()
	c.nhanLoa = nhanCat("Loa: tắt (vẫn nhận và đo mức)")

	c.chonMic = widget.NewSelect([]string{c.tenGiaLap}, nil)
	c.chonMic.SetSelected(c.tenGiaLap)
	c.batMic = nut("Start Mic", widget.SuccessImportance, c.lamBatMic)
	c.tatMic = nut("Stop Mic", widget.DangerImportance, c.lamTatMic)
	c.nhanHeSo = widget.NewLabel("Mic Volume: 100%")
	c.heSoMic = widget.NewSlider(0, 200)
	c.heSoMic.Step = 5
	c.heSoMic.SetValue(100)
	c.heSoMic.OnChanged = func(v float64) {
		c.nhanHeSo.SetText(fmt.Sprintf("Mic Volume: %.0f%%", v))
		if c.khach != nil {
			c.khach.DatHeSoMic(v / 100)
		}
	}
	c.mucMic = moiThanhDo()
	c.nguon = nhanCat("Nguồn: chưa bật micro")
	c.micTT = nhanCat("Mic: tắt")
	c.trangThai = nhanCat("Chưa kết nối")
	c.trangThai.Importance = widget.DangerImportance
	c.datDieuKhien(false)

	ketNoi := nhom("Kết nối",
		container.NewGridWithColumns(2, widget.NewLabel("IP:"), c.ip, widget.NewLabel("Cổng:"), c.cong,
			widget.NewLabel("Tên:"), c.ten),
		container.NewGridWithColumns(2, c.ketNoi, c.ngat))
	nhan := nhom("Nhận âm thanh", container.NewBorder(nil, nil, c.nhanAmLuong, nil, c.amLuong),
		container.NewGridWithColumns(3, c.batAudio, c.tatAudio, c.serverVol), c.mucNhan.bar, c.nhanLoa)
	micro := nhom("Micro", c.chonMic, container.NewGridWithColumns(2, c.batMic, c.tatMic),
		container.NewBorder(nil, nil, c.nhanHeSo, nil, c.heSoMic), c.mucMic.bar, c.nguon, c.micTT)
	c.w.SetContent(container.NewBorder(container.NewVBox(ketNoi, nhan, micro, c.trangThai), nil, nil, nil, c.log.ds))
	c.w.Resize(fyne.NewSize(440, 760))
	c.w.SetOnClosed(c.lamNgat)

	go func() { // liệt kê micro thật ở goroutine nền
		ds := ht.LietKeMicro()
		fyne.Do(func() {
			c.thietBi = ds
			tuyChon := []string{c.tenGiaLap}
			for _, tb := range ds {
				tuyChon = append(tuyChon, tb.Ten)
			}
			c.chonMic.SetOptions(tuyChon)
		})
	}()
	return c
}

func (c *cuaSoClient) datDieuKhien(daKetNoi bool) {
	ds := []*widget.Button{c.ngat, c.batAudio, c.tatAudio, c.batMic, c.tatMic}
	for _, b := range ds {
		if daKetNoi {
			b.Enable()
		} else {
			b.Disable()
		}
	}
	if daKetNoi {
		c.ketNoi.Disable()
		c.tatAudio.Disable()
		c.tatMic.Disable()
	} else {
		c.ketNoi.Enable()
	}
}

func (c *cuaSoClient) lamKetNoi() {
	ten, host, cong := c.ten.Text, c.ip.Text, soNguyen(c.cong.Text, 41625)
	c.ketNoi.Disable()
	c.trangThai.SetText("Đang kết nối…")
	go func() { // net.Dial có thể chặn tới 5 s: không làm trên luồng giao diện
		k, err := ht.KetNoi(ten, host, cong, c.log.them)
		fyne.Do(func() {
			if err != nil {
				c.trangThai.SetText("Không kết nối được: " + err.Error())
				c.ketNoi.Enable()
				return
			}
			c.khach = k
			k.DatAmLuongNghe(c.amLuong.Value / 100)
			k.DatHeSoMic(c.heSoMic.Value / 100)
			c.w.SetTitle(fmt.Sprintf("Audio Client – %s (#%d)", ten, k.Ma))
			c.trangThai.SetText(fmt.Sprintf("Đã kết nối %s:%d, mã %d", host, cong, k.Ma))
			c.trangThai.Importance = widget.SuccessImportance
			c.trangThai.Refresh()
			c.datDieuKhien(true)
			c.dungHenGio = make(chan struct{})
			go c.henGio(k, c.dungHenGio)
		})
	}()
}

func (c *cuaSoClient) lamNgat() {
	if c.khach == nil {
		return
	}
	c.lamTatMic()
	close(c.dungHenGio)
	k := c.khach
	c.khach = nil
	k.Ngat()
	c.datTrangThaiNgat("Đã ngắt kết nối")
}

func (c *cuaSoClient) datTrangThaiNgat(chu string) {
	c.trangThai.SetText(chu)
	c.trangThai.Importance = widget.DangerImportance
	c.trangThai.Refresh()
	c.serverVol.SetText("Server Vol: –")
	c.nhanLoa.SetText("Loa: tắt")
	c.datDieuKhien(false)
}

func (c *cuaSoClient) lamBatAudio() {
	k := c.khach
	c.batAudio.Disable()
	c.nhanLoa.SetText("Loa: đang mở ffplay…")
	go func() {
		l, err := ht.MoLoa()
		fyne.Do(func() {
			if c.khach != k {
				if l != nil {
					l.Dong()
				}
				return
			}
			c.tatAudio.Enable()
			if err != nil {
				c.nhanLoa.SetText("Không phát được ra loa – chỉ đo mức (" + err.Error() + ")")
				return
			}
			k.DatLoa(l)
			c.nhanLoa.SetText("Loa: đang phát (ffplay)")
		})
	}()
}

func (c *cuaSoClient) lamTatAudio() {
	if c.khach != nil {
		c.khach.DatLoa(nil)
	}
	c.batAudio.Enable()
	c.tatAudio.Disable()
	c.nhanLoa.SetText("Loa: tắt (vẫn nhận và đo mức)")
}

// lamBatMic – Start Mic: chỉ lúc này mới mở micro thật.
func (c *cuaSoClient) lamBatMic() {
	var tb *ht.ThietBi
	for i := range c.thietBi {
		if c.thietBi[i].Ten == c.chonMic.Selected {
			tb = &c.thietBi[i]
		}
	}
	k := c.khach
	c.batMic.Disable()
	go func() {
		n := ht.MoNguonMic(tb, c.tanSo)
		fyne.Do(func() {
			if c.khach != k {
				n.Dong()
				return
			}
			c.mic = n
			k.BatMic(n)
			c.tatMic.Enable()
		})
	}()
}

func (c *cuaSoClient) lamTatMic() {
	if c.mic == nil {
		return
	}
	if c.khach != nil {
		c.khach.TatMic()
	}
	c.mic.Dong()
	c.mic = nil
	c.nguon.SetText("Nguồn: chưa bật micro")
	c.batMic.Enable()
	c.tatMic.Disable()
}

func (c *cuaSoClient) henGio(k *ht.Khach, dung chan struct{}) {
	t := time.NewTicker(100 * time.Millisecond)
	defer t.Stop()
	for {
		select {
		case <-dung:
			return
		case <-t.C:
		}
		tt := k.TrangThai()
		fyne.Do(func() { c.ve(k, tt) })
		if tt.DaNgat {
			return
		}
	}
}

func (c *cuaSoClient) ve(k *ht.Khach, t ht.TrangThaiKhach) {
	if c.khach != k {
		return
	}
	c.TrangThaiCuoi = t
	if t.DaNgat {
		c.lamTatMic()
		c.khach = nil
		c.datTrangThaiNgat("Mất kết nối: " + t.LyDoNgat)
		return
	}
	c.serverVol.SetText(fmt.Sprintf("Server Vol: %.0f%%", t.DoLoiServer*100))
	c.mucNhan.datMuc(t.MucNhan)
	c.mucMic.datMuc(t.MucMic)
	if t.LoaLoi {
		c.nhanLoa.SetText("ffplay đã dừng – chỉ đo mức")
	}
	if c.mic != nil {
		chu := "Nguồn: " + c.mic.MoTa()
		if c.mic.GiaLap() {
			chu += "  [GIẢ LẬP]"
		} else {
			chu += "  [THIẾT BỊ THẬT]"
		}
		c.nguon.SetText(chu)
	}
	switch {
	case !t.CoNguon:
		c.micTT.SetText("Mic: tắt (chưa Start Mic)")
	case t.TuChoi != "":
		c.micTT.SetText("Mic: bị từ chối – " + t.TuChoi)
	case t.MicMo:
		c.micTT.SetText(fmt.Sprintf("Mic: mở – đang gửi %.0f kbit/s", t.KbpsGui))
	default:
		c.micTT.SetText("Mic: " + t.LyDoMic)
	}
	chu := fmt.Sprintf("Đã kết nối, mã %d · nhận %d gói, mất %d, jitter %.1f ms", t.Ma, t.GoiNhan, t.Mat, t.JitterMs)
	if t.TuChoi != "" {
		chu = "Bị từ chối: " + t.TuChoi
	}
	c.trangThai.SetText(chu)
}
