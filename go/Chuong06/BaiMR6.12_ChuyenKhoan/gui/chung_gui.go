package main

// Thành phần giao diện Fyne dùng chung cho nhóm bài tranh chấp MR6.6–MR6.13 (bài đầu: MR6.6 Đấu giá) –
// tệp này GIỐNG NHAU ở các bài (chỉ khác đường dẫn gói tranhchap), như giao_dien_chung.py. Gồm:
//   - Tiện ích (như MR6.5): chủ đề sáng, nhatKy, thongKeDinhKy, nut, nhom, oNhap, oMau, bảng cột cố định,
//     chụp cửa sổ, ghép ảnh, giuVe (chống "cửa sổ trắng" trên Windows khi dùng SetFixedSize).
//   - cuaSoServerCoSo: bảng điều khiển máy chủ đủ mọi phần chung – START/STOP, 4 nút chọn chế độ, độ trễ
//     xử lý, chỉ số trực tiếp (yc/s, thành công, từ chối, xung đột, hàng đợi, độ trễ, SAI PHẠM tô đỏ),
//     khung mô phỏng tải (Bắn đồng loạt, So sánh 4 chế độ + bảng), nhật ký. Bài chỉ vẽ khung "Tài nguyên"
//     (giao diện taiNguyenServer: veTaiNguyen, capNhatTaiNguyen, thamSoDatLai).
//   - cuaSoNguoiDungCoSo: khung cửa sổ người dùng – kết nối/rời, nhật ký, thanh trạng thái, gửi yêu cầu;
//     bài vẽ phần nghiệp vụ (giao diện nghiepVuNguoiDung: ve, capNhat, ketQua, suKien, moiNhip, dongDinhKy).
//
// QUY TẮC LUỒNG (Fyne ≥ 2.6): chỉ luồng giao diện chạm vào widget. Goroutine mạng/mô phỏng/hẹn giờ gói
// việc vẽ trong fyne.Do(func(){…}); hàm xử lý sự kiện (OnTapped…) không làm việc chặn (kết nối, đổi chế
// độ – có thể chờ hàng đợi cạn, mô phỏng) mà đẩy sang goroutine.

import (
	"fmt"
	"image"
	"image/color"
	"image/draw"
	"image/png"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.12_ChuyenKhoan/tranhchap"
)

// Màu như bản Python.
var (
	XanhLa   = color.NRGBA{0x90, 0xEE, 0x90, 0xFF}
	DoNhat   = color.NRGBA{0xF0, 0x80, 0x80, 0xFF}
	DoDam    = color.NRGBA{0xC0, 0x00, 0x00, 0xFF}
	XanhDam  = color.NRGBA{0x00, 0x64, 0x00, 0xFF}
	Den      = color.NRGBA{0x20, 0x20, 0x20, 0xFF}
	Trang    = color.NRGBA{0xFF, 0xFF, 0xFF, 0xFF}
	XamNhat  = color.NRGBA{0xE0, 0xE0, 0xE0, 0xFF}
	XamChu   = color.NRGBA{0x60, 0x60, 0x60, 0xFF}
	XanhSP   = color.NRGBA{0x70, 0xD0, 0x70, 0xFF}
	DoSP     = color.NRGBA{0xFF, 0x60, 0x60, 0xFF}
	CamNhat  = color.NRGBA{0xFF, 0xB3, 0x47, 0xFF}
	VangNhat = color.NRGBA{0xFF, 0xF3, 0xC4, 0xFF}
)

func mau(s string) color.NRGBA { // "#rrggbb"
	v, _ := strconv.ParseUint(strings.TrimPrefix(s, "#"), 16, 32)
	return color.NRGBA{uint8(v >> 16), uint8(v >> 8), uint8(v), 0xFF}
}

// temSang – chủ đề sáng cố định (dễ đọc khi in ảnh chụp), nền xanh thép nhạt như bản Python.
type temSang struct{}

func (temSang) Color(n fyne.ThemeColorName, _ fyne.ThemeVariant) color.Color {
	switch n {
	case theme.ColorNameBackground:
		return color.NRGBA{0xC4, 0xD3, 0xE8, 0xFF}
	case theme.ColorNameInputBackground, theme.ColorNameMenuBackground, theme.ColorNameOverlayBackground:
		return color.White
	}
	return theme.DefaultTheme().Color(n, theme.VariantLight)
}
func (temSang) Font(s fyne.TextStyle) fyne.Resource     { return theme.DefaultTheme().Font(s) }
func (temSang) Icon(n fyne.ThemeIconName) fyne.Resource { return theme.DefaultTheme().Icon(n) }
func (temSang) Size(n fyne.ThemeSizeName) float32 {
	switch n {
	case theme.SizeNameText:
		return 12
	case theme.SizeNamePadding:
		return 3
	case theme.SizeNameInnerPadding:
		return 5
	case theme.SizeNameSubHeadingText:
		return 13
	}
	return theme.DefaultTheme().Size(n)
}

// ---------------------------------------------------------------- Nhật ký

// inRa – true khi chạy riêng từng cửa sổ (không qua demo_gui): mỗi dòng nhật ký cũng in ra dòng lệnh.
var inRa bool

// nhatKy – danh sách cuộn các dòng sự kiện (giữ tối đa 500 dòng).
type nhatKy struct {
	ten  string
	dong []string
	ds   *widget.List
}

func moiNhatKy(ten string) *nhatKy {
	n := &nhatKy{ten: ten}
	n.ds = widget.NewList(func() int { return len(n.dong) },
		func() fyne.CanvasObject { return nhanCat("") },
		func(i widget.ListItemID, o fyne.CanvasObject) { o.(*widget.Label).SetText(n.dong[i]) })
	return n
}

// them – an toàn khi gọi từ goroutine bất kỳ: việc sửa danh sách được chuyển về luồng giao diện.
func (n *nhatKy) them(s string) {
	gio := time.Now().Format("15:04:05")
	if inRa {
		fmt.Printf("%s [%s] %s\n", gio, n.ten, s)
	}
	dong := gio + " " + s
	fyne.Do(func() {
		n.dong = append(n.dong, dong)
		if len(n.dong) > 500 {
			n.dong = n.dong[len(n.dong)-500:]
		}
		n.ds.Refresh()
		n.ds.ScrollToBottom()
	})
}

// thongKeDinhKy – khi chạy riêng (inRa): cứ 5 giây in một dòng số liệu ra dòng lệnh.
type thongKeDinhKy struct {
	ten string
	han time.Time
}

func moiThongKeDinhKy(ten string) *thongKeDinhKy {
	return &thongKeDinhKy{ten, time.Now().Add(5 * time.Second)}
}

func (t *thongKeDinhKy) inNeuDenHan(taoDong func() string) {
	if !inRa || time.Now().Before(t.han) {
		return
	}
	t.han = time.Now().Add(5 * time.Second)
	fmt.Printf("%s [%s] %s\n", time.Now().Format("15:04:05"), t.ten, taoDong())
}

// ---------------------------------------------------------------- Mảnh nhỏ

// cotCoDinh – bố cục một hàng: các ô đặt cạnh nhau theo độ rộng cho trước (bảng).
type cotCoDinh []float32

func (c cotCoDinh) Layout(o []fyne.CanvasObject, kt fyne.Size) {
	x := float32(0)
	for i, ob := range o {
		w := c[min(i, len(c)-1)]
		ob.Move(fyne.NewPos(x, 0))
		ob.Resize(fyne.NewSize(w, kt.Height))
		x += w
	}
}

func (c cotCoDinh) MinSize(o []fyne.CanvasObject) fyne.Size {
	w, h := float32(0), float32(0)
	for i, ob := range o {
		w += c[min(i, len(c)-1)]
		h = max(h, ob.MinSize().Height)
	}
	return fyne.NewSize(w, h)
}

// coDinh – giữ đối tượng ở kích thước cho trước.
func coDinh(o fyne.CanvasObject, w, h float32) fyne.CanvasObject {
	return container.New(layout.NewGridWrapLayout(fyne.NewSize(w, h)), o)
}

func nhanCat(chu string) *widget.Label {
	l := widget.NewLabel(chu)
	l.Truncation = fyne.TextTruncateEllipsis
	return l
}

func nhanDam(chu string) *widget.Label {
	l := widget.NewLabel(chu)
	l.TextStyle.Bold = true
	return l
}

func nhanBoc(chu string) *widget.Label {
	l := widget.NewLabel(chu)
	l.Wrapping = fyne.TextWrapWord
	return l
}

// chu – canvas.Text có màu, cỡ, đậm.
func chu(s string, m color.Color, co float32, dam bool) *canvas.Text {
	t := canvas.NewText(s, m)
	t.TextSize = co
	t.TextStyle.Bold = dam
	return t
}

func datChu(t *canvas.Text, s string, m color.Color) {
	if t.Text != s || t.Color != m {
		t.Text, t.Color = s, m
		t.Refresh()
	}
}

func nhom(tieuDe string, noiDung ...fyne.CanvasObject) *widget.Card {
	return widget.NewCard("", tieuDe, container.NewVBox(noiDung...))
}

func nut(chu string, muc widget.Importance, f func()) *widget.Button {
	b := widget.NewButton(chu, f)
	b.Importance = muc
	return b
}

func oNhap(giaTri string) *widget.Entry {
	e := widget.NewEntry()
	e.SetText(giaTri)
	return e
}

func batTat(bat bool, ds ...fyne.Disableable) {
	for _, o := range ds {
		if bat {
			o.Enable()
		} else {
			o.Disable()
		}
	}
}

// oMau – ô chữ có nền màu (SAI PHẠM, thông báo của ứng dụng người dùng).
type oMau struct {
	nen *canvas.Rectangle
	chu *canvas.Text
	O   fyne.CanvasObject
}

func moiOMau(s string, nen, mauChu color.Color, co float32, w, h float32, giua bool) *oMau {
	x := &oMau{nen: canvas.NewRectangle(nen), chu: chu(s, mauChu, co, true)}
	x.nen.SetMinSize(fyne.NewSize(w, h))
	var c fyne.CanvasObject = container.NewHBox(layout.NewSpacer(), x.chu, layout.NewSpacer())
	if !giua {
		c = container.NewHBox(widget.NewLabel(""), x.chu)
	}
	x.O = container.NewStack(x.nen, container.NewCenter(c))
	if !giua {
		x.O = container.NewStack(x.nen, container.NewVBox(layout.NewSpacer(), c, layout.NewSpacer()))
	}
	return x
}

func (x *oMau) dat(s string, nen, mauChu color.Color) {
	x.chu.Text, x.chu.Color = s, mauChu
	x.nen.FillColor = nen
	x.chu.Refresh()
	x.nen.Refresh()
}

func (x *oMau) text() string { return x.chu.Text }

// bam – mô phỏng người dùng bấm nút (demo tự thao tác); gọi trên luồng giao diện.
func bam(b *widget.Button) {
	if !b.Disabled() && b.OnTapped != nil {
		b.OnTapped()
	}
}

func soNguyen(s string, macDinh int) int {
	if n, err := strconv.Atoi(strings.TrimSpace(s)); err == nil && n > 0 && n < 65536 {
		return n
	}
	return macDinh
}

// msUs – µs -> "1.5" (ms) hoặc "–".
func msUs(v any) string {
	if v == nil {
		return "–"
	}
	return fmt.Sprintf("%.1f", tc.SoThuc(v)/1000)
}

// dsChu – một cột các dòng chữ (lịch sử trả giá) trên nền trắng, mỗi dòng một màu.
type dsChu struct {
	dong []*canvas.Text
	O    fyne.CanvasObject
}

func moiDsChu(soDong int, co float32, w, h float32) *dsChu {
	d := &dsChu{}
	hop := container.NewVBox()
	for i := 0; i < soDong; i++ {
		t := chu("", Den, co, false)
		t.TextStyle.Monospace = true
		d.dong = append(d.dong, t)
		hop.Add(t)
	}
	nen := canvas.NewRectangle(Trang)
	nen.StrokeColor, nen.StrokeWidth = mau("#a0a0a0"), 1
	d.O = coDinh(container.NewStack(nen, container.NewPadded(hop)), w, h)
	return d
}

func (d *dsChu) dat(ds []string, mauDong func(i int) color.Color) {
	for i, t := range d.dong {
		s, m := "", color.Color(Den)
		if i < len(ds) {
			s = ds[i]
			if mauDong != nil {
				m = mauDong(i)
			}
		}
		datChu(t, s, m)
	}
}

// vungBam – lớp trong suốt phủ lên bản đồ/sơ đồ: bắt chuột trái (Tapped) và chuột phải (TappedSecondary).
type vungBam struct {
	widget.BaseWidget
	trai, phai func(fyne.Position)
}

func moiVungBam(trai, phai func(fyne.Position)) *vungBam {
	v := &vungBam{trai: trai, phai: phai}
	v.ExtendBaseWidget(v)
	return v
}

func (v *vungBam) CreateRenderer() fyne.WidgetRenderer {
	return widget.NewSimpleRenderer(canvas.NewRectangle(color.Transparent))
}

func (v *vungBam) Tapped(e *fyne.PointEvent) {
	if v.trai != nil {
		v.trai(e.Position)
	}
}

func (v *vungBam) TappedSecondary(e *fyne.PointEvent) {
	if v.phai != nil {
		v.phai(e.Position)
	}
}

// ---------------------------------------------------------------- Bảng so sánh

var cotSoSanh = cotCoDinh{92, 76, 66, 66, 56, 60}

type hangBang struct {
	nen *canvas.Rectangle
	o   []*canvas.Text
	c   fyne.CanvasObject
}

func moiHangBang(gt []string, dam bool) *hangBang {
	h := &hangBang{nen: canvas.NewRectangle(color.Transparent)}
	var ds []fyne.CanvasObject
	for i, s := range gt {
		t := chu(s, Den, 12, dam)
		if i > 0 {
			t.Alignment = fyne.TextAlignTrailing
		}
		h.o = append(h.o, t)
		ds = append(ds, container.NewPadded(t))
	}
	h.c = container.NewStack(h.nen, container.New(cotSoSanh, ds...))
	return h
}

func (h *hangBang) dat(gt []string, nen, mauChu color.Color) {
	for i, t := range h.o {
		s := ""
		if i < len(gt) {
			s = gt[i]
		}
		t.Text, t.Color = s, mauChu
		t.Refresh()
	}
	h.nen.FillColor = nen
	h.nen.Refresh()
}

// ================================================================ bảng điều khiển máy chủ

// taiNguyenServer – phần riêng của bài trong cửa sổ máy chủ.
type taiNguyenServer interface {
	veTaiNguyen() fyne.CanvasObject
	capNhatTaiNguyen(tt, tk tc.M)
	thamSoDatLai() (tc.TS, error)
}

type cuaSoServerCoSo struct {
	w                 fyne.Window
	lop               *tc.MayChuCoSo
	tn                taiNguyenServer
	oIP, oCong        *widget.Entry
	nutStart, nutStop *widget.Button
	nutCheDo          *widget.RadioGroup
	nhanCheDo         map[string]string // nhãn nút -> chế độ
	dongBoCheDo       bool
	oTre              *widget.Entry
	nutTre, nutDatLai *widget.Button
	chiSo             map[string]*widget.Label
	oSaiPham          *oMau
	nhanSaiPhamCt     *widget.Label
	oN, oK, oLan      *widget.Entry
	nutBan, nutSoSanh *widget.Button
	nhanMoPhong       *canvas.Text
	bang              []*hangBang
	nhanKetLuan       *widget.Label
	log               *nhatKy
	dinhKy            *thongKeDinhKy
	thanhTT           *widget.Label
	dangMoPhong       bool
	hangSoSanh        []tc.HangSoSanh
	dong              chan struct{}
}

func nhanNutCheDo(cd string) string { return fmt.Sprintf("%-11s %s", cd, tc.TenCheDo[cd]) }

func moCuaSoServerCoSo(a fyne.App, tieuDe, tenTaiNguyen string, taoLop func(bao func(string)) *tc.MayChuCoSo,
	tn taiNguyenServer, host string, cong, n, k, lan int) *cuaSoServerCoSo {
	s := &cuaSoServerCoSo{w: a.NewWindow(tieuDe + " – bảng điều khiển"), tn: tn, log: moiNhatKy("server"),
		dinhKy: moiThongKeDinhKy("server"), dong: make(chan struct{}), nhanCheDo: map[string]string{}}
	s.lop = taoLop(s.log.them)
	s.oIP, s.oCong = oNhap(host), oNhap(strconv.Itoa(cong))
	s.nutStart = nut("START SERVER", widget.SuccessImportance, s.batDau)
	s.nutStop = nut("STOP SERVER", widget.DangerImportance, s.dungServer)
	s.nutStop.Disable()
	ketNoi := nhom("Kết nối", container.New(layout.NewFormLayout(), widget.NewLabel("IP:"), s.oIP,
		widget.NewLabel("Cổng:"), s.oCong), s.nutStart, s.nutStop)

	var nhanCD []string
	for _, cd := range tc.CheDoDS {
		nh := nhanNutCheDo(cd)
		nhanCD = append(nhanCD, nh)
		s.nhanCheDo[nh] = cd
	}
	s.nutCheDo = widget.NewRadioGroup(nhanCD, func(nh string) {
		if s.dongBoCheDo || nh == "" {
			return
		}
		cd := s.nhanCheDo[nh]
		go func() { // có thể chờ hàng đợi cạn: không chặn giao diện
			if err := s.lop.DatCheDo(cd); err != nil {
				s.log.them("Không đổi được chế độ: " + err.Error())
			}
		}()
	})
	s.dongBoCheDo = true
	s.nutCheDo.SetSelected(nhanNutCheDo(s.lop.Kho.CheDo()))
	s.dongBoCheDo = false
	s.oTre = oNhap(tc.FmtG(s.lop.Kho.TreMs()))
	s.nutTre = nut("Áp dụng", widget.HighImportance, s.doiTre)
	cheDo := nhom("Cách xử lý tranh chấp (đổi được khi đang chạy)", s.nutCheDo,
		container.NewHBox(widget.NewLabel("Độ trễ xử lý: ngẫu nhiên 0 –"), coDinh(s.oTre, 46, 30),
			widget.NewLabel("ms"), s.nutTre))

	s.chiSo = map[string]*widget.Label{}
	dong := [][2]string{{"ket_noi", "Kết nối / người"}, {"yc_s", "Yêu cầu/giây"}, {"so_yc", "Tổng yêu cầu"},
		{"thanh_cong", "Thành công"}, {"tu_choi", "Từ chối"}, {"xung_dot", "Xung đột / thử lại"},
		{"hang_doi", "Hàng đợi (lớn nhất)"}, {"tre", "Xử lý p50/p95/p99"}}
	luoi := container.New(layout.NewFormLayout())
	for _, d := range dong {
		s.chiSo[d[0]] = nhanCat("–")
		s.chiSo[d[0]].TextStyle.Bold = true
		luoi.Add(widget.NewLabel(d[1] + ":"))
		luoi.Add(coDinh(s.chiSo[d[0]], 230, 20))
	}
	s.oSaiPham = moiOMau("SAI PHẠM: –", XamNhat, Den, 16, 250, 30, false)
	s.nhanSaiPhamCt = nhanBoc("")
	chiSo := nhom("Chỉ số trực tiếp (từ lần đặt lại)", container.NewHBox(luoi,
		container.NewVBox(s.oSaiPham.O, coDinh(s.nhanSaiPhamCt, 260, 120))))
	tren := container.NewBorder(nil, nil, container.NewHBox(coDinh(ketNoi, 200, 216), coDinh(cheDo, 360, 216)), nil,
		chiSo)

	s.nutDatLai = nut("Đặt lại", widget.WarningImportance, s.datLai)
	taiNguyen := nhom(tenTaiNguyen, tn.veTaiNguyen(), container.NewHBox(layout.NewSpacer(), s.nutDatLai))

	s.oN, s.oK, s.oLan = oNhap(strconv.Itoa(n)), oNhap(strconv.Itoa(k)), oNhap(strconv.Itoa(lan))
	s.nutBan = nut("Bắn đồng loạt", widget.WarningImportance, s.ban)
	s.nutSoSanh = nut("So sánh 4 chế độ", widget.HighImportance, s.soSanh)
	s.nhanMoPhong = chu("", DoDam, 12, false)
	tieuDeBang := moiHangBang([]string{"Chế độ", "Thành công", "Sai phạm", "Xung đột", "yc/s", "p99 ms"}, true)
	tieuDeBang.nen.FillColor = XamNhat
	bang := container.NewVBox(tieuDeBang.c)
	for range tc.CheDoDS {
		h := moiHangBang(make([]string, len(cotSoSanh)), false)
		s.bang = append(s.bang, h)
		bang.Add(h.c)
	}
	s.nhanKetLuan = nhanBoc("")
	moPhong := nhom("Mô phỏng tải (n người ảo trên k kết nối, rào xuất phát)",
		container.NewHBox(widget.NewLabel("n:"), coDinh(s.oN, 64, 30), widget.NewLabel("k:"), coDinh(s.oK, 54, 30),
			widget.NewLabel("lần/người:"), coDinh(s.oLan, 40, 30)),
		container.NewHBox(s.nutBan, s.nutSoSanh, container.NewCenter(s.nhanMoPhong)),
		bang, coDinh(s.nhanKetLuan, 420, 40))
	giua := container.NewBorder(nil, nil, taiNguyen, nil, moPhong)

	nk := nhom("Nhật ký", coDinh(s.log.ds, 1000, 150))
	s.thanhTT = nhanCat("Trạng thái: máy chủ chưa chạy")
	s.w.SetContent(container.NewVBox(tren, giua, nk, widget.NewSeparator(), coDinh(s.thanhTT, 1000, 22)))
	s.w.SetFixedSize(true)
	s.w.SetOnClosed(func() { close(s.dong); go s.lop.Dung() })
	go s.henGio()
	giuVe(s.w, "server", s.dong)
	return s
}

// ------------------------------------------------------------ nút

func (s *cuaSoServerCoSo) batDau() {
	cong, err := strconv.Atoi(strings.TrimSpace(s.oCong.Text))
	if err == nil {
		err = s.lop.BatDau(strings.TrimSpace(s.oIP.Text), cong)
	}
	if err != nil {
		s.log.them("Không khởi động được: " + err.Error())
		return
	}
	s.nutStart.Disable()
	s.nutStop.Enable()
	batTat(false, s.oIP, s.oCong)
}

func (s *cuaSoServerCoSo) dungServer() {
	s.nutStop.Disable()
	go func() { // chờ goroutine nền dừng: không chặn giao diện
		s.lop.Dung()
		fyne.Do(func() {
			s.nutStart.Enable()
			batTat(true, s.oIP, s.oCong)
		})
	}()
}

// chonCheDo – chọn chế độ như người dùng bấm nút (kịch bản demo).
func (s *cuaSoServerCoSo) chonCheDo(cd string) { s.nutCheDo.SetSelected(nhanNutCheDo(cd)) }

func (s *cuaSoServerCoSo) doiTre() {
	ms, err := strconv.ParseFloat(strings.TrimSpace(s.oTre.Text), 64)
	if err == nil {
		err = s.lop.DatTre(ms)
	}
	if err != nil {
		s.log.them("Độ trễ không hợp lệ: " + err.Error())
	}
}

func (s *cuaSoServerCoSo) datLai() {
	ts, err := s.tn.thamSoDatLai()
	if err == nil {
		go func() { // chờ thao tác đang chạy xong: không chặn giao diện
			if err := s.lop.DatLai(ts); err != nil {
				s.log.them("Không đặt lại được: " + err.Error())
			}
		}()
		return
	}
	s.log.them("Không đặt lại được: " + err.Error())
}

func (s *cuaSoServerCoSo) docMoPhong() (int, int, int, error) {
	n, e1 := strconv.Atoi(strings.TrimSpace(s.oN.Text))
	k, e2 := strconv.Atoi(strings.TrimSpace(s.oK.Text))
	lan, e3 := strconv.Atoi(strings.TrimSpace(s.oLan.Text))
	if e1 != nil || e2 != nil || e3 != nil || n < 1 || k < 1 || lan < 1 {
		return 0, 0, 0, fmt.Errorf("n, k, lần ≥ 1")
	}
	return n, min(k, n), lan, nil
}

// chayNen – chạy mô phỏng trên goroutine nền (kết nối TCP tới chính máy chủ này như client bên ngoài).
func (s *cuaSoServerCoSo) chayNen(ham func(host string, cong int, bao func(string)) ([]tc.HangSoSanh, error)) {
	if s.dangMoPhong || !s.lop.Chay() {
		s.log.them("Cần START SERVER và chờ lượt mô phỏng trước xong")
		return
	}
	s.dangMoPhong = true
	batTat(false, s.nutBan, s.nutSoSanh)
	datChu(s.nhanMoPhong, "đang chạy…", DoDam)
	i := strings.LastIndex(s.lop.DiaChi, ":")
	host, cong := s.lop.DiaChi[:i], soNguyen(s.lop.DiaChi[i+1:], 0)
	bao := func(text string) { s.log.them("[mô phỏng] " + text) }
	go func() {
		hang, err := ham(host, cong, bao)
		if err != nil {
			bao("lỗi: " + err.Error())
		}
		fyne.Do(func() { s.xongMoPhong(hang) })
	}()
}

func (s *cuaSoServerCoSo) ban() {
	n, k, lan, err := s.docMoPhong()
	if err != nil {
		s.log.them(err.Error())
		return
	}
	s.chayNen(func(h string, c int, b func(string)) ([]tc.HangSoSanh, error) {
		x, err := tc.ChayLuot(h, c, n, k, lan, b, "", false, nil)
		if x == nil {
			return nil, err
		}
		return []tc.HangSoSanh{*x}, err
	})
}

func (s *cuaSoServerCoSo) soSanh() {
	n, k, lan, err := s.docMoPhong()
	if err != nil {
		s.log.them(err.Error())
		return
	}
	s.chayNen(func(h string, c int, b func(string)) ([]tc.HangSoSanh, error) {
		return tc.SoSanh(h, c, n, k, lan, b, nil)
	})
}

func (s *cuaSoServerCoSo) xongMoPhong(hang []tc.HangSoSanh) {
	s.dangMoPhong = false
	batTat(true, s.nutBan, s.nutSoSanh)
	datChu(s.nhanMoPhong, "xong", XanhDam)
	if len(hang) == 0 {
		return
	}
	if len(hang) == len(tc.CheDoDS) {
		s.hangSoSanh = hang
		for _, h := range s.bang {
			h.dat(nil, color.Transparent, Den)
		}
	}
	var sai, dung []string
	for _, h := range hang {
		p99 := "–"
		if h.P99Ms != nil {
			p99 = fmt.Sprintf("%.1f", *h.P99Ms)
		}
		gt := []string{h.CheDo, fmt.Sprint(h.ThanhCong), fmt.Sprint(h.SaiPham), fmt.Sprint(h.XungDot),
			fmt.Sprint(h.YcS), p99}
		i := 0
		for j, cd := range tc.CheDoDS {
			if cd == h.CheDo {
				i = j
			}
		}
		if h.SaiPham > 0 {
			s.bang[i].dat(gt, mau("#ffc8c8"), DoDam)
			sai = append(sai, h.CheDo)
		} else {
			s.bang[i].dat(gt, mau("#d8f5d8"), Den)
			dung = append(dung, h.CheDo)
		}
	}
	kl := ""
	if len(sai) > 0 {
		kl = "Sai phạm: " + strings.Join(sai, ", ") + ". "
	}
	if len(dung) > 0 {
		kl += "Không sai phạm: " + strings.Join(dung, ", ") + "."
	}
	s.nhanKetLuan.SetText(kl)
}

// ------------------------------------------------------------ vẽ lại định kỳ (250 ms)

func (s *cuaSoServerCoSo) henGio() {
	t := time.NewTicker(250 * time.Millisecond)
	defer t.Stop()
	for {
		select {
		case <-s.dong:
			return
		case <-t.C:
		}
		chay := s.lop.Chay()
		var tk, tt tc.M
		if chay {
			tk, tt = s.lop.ThongKeDayDu(), s.lop.TrangThai()
		}
		fyne.Do(func() { s.capNhat(chay, tk, tt) })
	}
}

func (s *cuaSoServerCoSo) capNhat(chay bool, tk, tt tc.M) {
	if !chay {
		s.thanhTT.SetText("Trạng thái: máy chủ chưa chạy")
		return
	}
	n := tc.SoNguyen
	c := s.chiSo
	c["ket_noi"].SetText(fmt.Sprintf("%d / %d", n(tk["so_ket_noi"]), n(tk["so_nguoi"])))
	c["yc_s"].SetText(fmt.Sprintf("%d  (TB %d)", n(tk["yc_s"]), n(tk["thong_luong"])))
	c["so_yc"].SetText(fmt.Sprint(n(tk["so_yc"])))
	c["thanh_cong"].SetText(fmt.Sprint(n(tk["thanh_cong"])))
	tuChoi := fmt.Sprint(n(tk["tu_choi"]))
	if ml, _ := tk["ma_loi"].(tc.M); len(ml) > 0 {
		var ds []string
		for m := range ml {
			ds = append(ds, m)
		}
		sort.Strings(ds)
		for i, m := range ds {
			ds[i] = fmt.Sprintf("%s %d", m, n(ml[m]))
		}
		tuChoi += "  (" + strings.Join(ds, ", ") + ")"
	}
	c["tu_choi"].SetText(tuChoi)
	c["xung_dot"].SetText(fmt.Sprintf("%d / %d (hết lượt %d)", n(tk["xung_dot"]), n(tk["thu_lai"]), n(tk["het_lan_thu"])))
	c["hang_doi"].SetText(fmt.Sprintf("%d  (%d)", n(tk["hang_doi"]), n(tk["hang_doi_max"])))
	c["tre"].SetText(fmt.Sprintf("%s / %s / %s ms", msUs(tk["p50_us"]), msUs(tk["p95_us"]), msUs(tk["p99_us"])))
	sp := n(tk["sai_pham_tong"])
	if sp > 0 {
		s.oSaiPham.dat(fmt.Sprintf("SAI PHẠM: %d", sp), DoSP, Trang)
	} else {
		s.oSaiPham.dat("SAI PHẠM: 0", XanhSP, Den)
	}
	var ct []string
	var p []string
	for _, x := range tc.DSM(tk["sai_pham"]) {
		p = append(p, fmt.Sprintf("%s %d", tc.Chuoi(x["ten"]), n(x["so"])))
	}
	ct = append(ct, strings.Join(p, ", "))
	for i, v := range tc.DSChuoi(tk["vi_du_sai_pham"]) {
		if i < 2 {
			ct = append(ct, "• "+v)
		}
	}
	s.nhanSaiPhamCt.SetText(strings.Join(ct, "\n"))
	if cd := tc.Chuoi(tk["che_do"]); s.nutCheDo.Selected != nhanNutCheDo(cd) && !s.dangMoPhong {
		s.dongBoCheDo = true // mô phỏng/quản trị đổi chế độ từ xa
		s.nutCheDo.SetSelected(nhanNutCheDo(cd))
		s.dongBoCheDo = false
	}
	s.tn.capNhatTaiNguyen(tt, tk)
	s.thanhTT.SetText(fmt.Sprintf("Trạng thái: máy chủ chạy tại %s – chế độ %s, %d kết nối – %s", s.lop.DiaChi,
		tc.Chuoi(tk["che_do"]), n(tk["so_ket_noi"]), tc.Chuoi(tk["tom_tat"])))
	s.dinhKy.inNeuDenHan(func() string {
		return fmt.Sprintf("%s: %d yc, tc %d, %d yc/s, sai phạm %d; %s", tc.Chuoi(tk["che_do"]), n(tk["so_yc"]),
			n(tk["thanh_cong"]), n(tk["yc_s"]), sp, tc.Chuoi(tk["tom_tat"]))
	})
}

// ================================================================ cửa sổ người dùng

// nghiepVuNguoiDung – phần riêng của bài trong cửa sổ người dùng (gọi trên luồng giao diện).
type nghiepVuNguoiDung interface {
	ve() fyne.CanvasObject
	capNhat(tt tc.M)
	ketQua(loai string, truong tc.TS, tin tc.M, rtt float64)
	suKien(text string)
	moiNhip()
	dongDinhKy() string
	daKetNoi()
	matKetNoi()
}

type cuaSoNguoiDungCoSo struct {
	w                 fyne.Window
	ten               string
	nv                nghiepVuNguoiDung
	oTen, oIP, oCong  *widget.Entry
	nutKetNoi, nutRoi *widget.Button
	log               *nhatKy
	dinhKy            *thongKeDinhKy
	thanhTT           *widget.Label
	kn                *tc.KetNoiKhach
	tt                tc.M // cap_nhat mới nhất
	rttCuoi           *float64
	dong              chan struct{}
}

func moCuaSoNguoiDungCoSo(a fyne.App, tieuDe, ten, host string, cong int, nv nghiepVuNguoiDung) *cuaSoNguoiDungCoSo {
	c := &cuaSoNguoiDungCoSo{w: a.NewWindow(tieuDe + " – " + ten), ten: ten, nv: nv, log: moiNhatKy(ten),
		dinhKy: moiThongKeDinhKy(ten), dong: make(chan struct{})}
	c.oTen, c.oIP, c.oCong = oNhap(ten), oNhap(host), oNhap(strconv.Itoa(cong))
	c.nutKetNoi = nut("Kết nối", widget.SuccessImportance, c.ketNoi)
	c.nutRoi = nut("Rời", widget.DangerImportance, c.roi)
	c.nutRoi.Disable()
	c.thanhTT = nhanCat("Chưa kết nối")
	return c
}

// hoanTat – dựng nội dung cửa sổ (gọi sau khi bài đã gắn con trỏ khung: ve() của bài dùng được c).
func (c *cuaSoNguoiDungCoSo) hoanTat(rong float32) {
	tren := container.NewHBox(widget.NewLabel("Tên:"), coDinh(c.oTen, 80, 30), widget.NewLabel("IP:"),
		coDinh(c.oIP, 100, 30), widget.NewLabel("Cổng:"), coDinh(c.oCong, 64, 30), c.nutKetNoi, c.nutRoi)
	khung := c.nv.ve()
	nk := nhom("Nhật ký", coDinh(c.log.ds, rong, 88))
	c.w.SetContent(container.NewVBox(tren, khung, nk, widget.NewSeparator(), coDinh(c.thanhTT, rong, 22)))
	c.w.SetFixedSize(true)
	c.w.SetOnClosed(func() {
		close(c.dong)
		if c.kn != nil {
			c.kn.TatBaoDong()
			c.kn.Dong()
		}
	})
	go func() { // _nhip: 200 ms
		t := time.NewTicker(200 * time.Millisecond)
		defer t.Stop()
		for {
			select {
			case <-c.dong:
				return
			case <-t.C:
			}
			fyne.Do(c.nhip)
		}
	}()
	giuVe(c.w, c.ten, c.dong)
}

// ------------------------------------------------------------ kết nối

func (c *cuaSoNguoiDungCoSo) ketNoi() {
	ten, host := strings.TrimSpace(c.oTen.Text), strings.TrimSpace(c.oIP.Text)
	cong, err := strconv.Atoi(strings.TrimSpace(c.oCong.Text))
	if err != nil {
		c.log.them("Cổng không hợp lệ")
		return
	}
	c.ten = ten
	c.nutKetNoi.Disable()
	kn := tc.MoiKetNoiKhach()
	kn.KhiCapNhat = func(tin tc.M) { fyne.Do(func() { c.nhanCapNhat(tin) }) }
	kn.KhiTin = func(tin tc.M) { fyne.Do(func() { c.nhanTin(tin) }) }
	kn.KhiDong = func() { fyne.Do(func() { c.matKetNoi(kn, "Mất kết nối máy chủ") }) }
	go func() { // kết nối trên goroutine nền: không đơ giao diện
		if _, err := kn.KetNoi(host, cong, ten, "nguoi_dung", 0, 5); err != nil {
			fyne.Do(func() {
				c.log.them("Không kết nối được: " + err.Error())
				c.nutKetNoi.Enable()
			})
			return
		}
		fyne.Do(func() { c.daKetNoi(kn) })
	}()
}

func (c *cuaSoNguoiDungCoSo) daKetNoi(kn *tc.KetNoiKhach) {
	c.kn = kn
	c.nutRoi.Enable()
	batTat(false, c.oTen, c.oIP, c.oCong)
	c.log.them(fmt.Sprintf("Đã kết nối %s (chế độ %s)", tc.Chuoi(kn.Chao["may_chu"]), tc.Chuoi(kn.Chao["che_do"])))
	c.nhanCapNhat(kn.TrangThai())
	c.nv.daKetNoi()
	kn.BatDauDoc()
}

func (c *cuaSoNguoiDungCoSo) roi() {
	if c.kn != nil {
		kn := c.kn
		c.kn = nil
		kn.TatBaoDong()
		kn.Dong()
		c.matKetNoi(nil, "Đã rời")
	}
}

func (c *cuaSoNguoiDungCoSo) matKetNoi(kn *tc.KetNoiKhach, lyDo string) {
	if kn != nil && kn != c.kn {
		return
	}
	c.kn = nil
	c.log.them(lyDo)
	c.thanhTT.SetText(lyDo)
	c.nutKetNoi.Enable()
	c.nutRoi.Disable()
	batTat(true, c.oTen, c.oIP, c.oCong)
	c.nv.matKetNoi()
}

func (c *cuaSoNguoiDungCoSo) nhanCapNhat(tt tc.M) {
	c.tt = tt
	c.nv.capNhat(tt)
	c.capNhatThanhTT()
}

func (c *cuaSoNguoiDungCoSo) nhanTin(tin tc.M) {
	switch tin["loai"] {
	case "su_kien":
		c.nv.suKien(tc.Chuoi(tin["noi_dung"]))
	case "loi":
		c.log.them("Lỗi: " + tc.StrPy(tin["ly_do"]))
	}
}

func (c *cuaSoNguoiDungCoSo) capNhatThanhTT() {
	if c.kn != nil && c.tt != nil {
		rtt := ""
		if c.rttCuoi != nil {
			rtt = fmt.Sprintf(", khứ hồi %.1f ms", *c.rttCuoi)
		}
		c.thanhTT.SetText(fmt.Sprintf("Kết nối %s:%s – máy chủ chế độ %s%s", c.oIP.Text, c.oCong.Text,
			tc.Chuoi(c.tt["che_do"]), rtt))
	}
}

// guiYC – gửi một yêu cầu nghiệp vụ; trả lời được chuyển về luồng giao diện (ketQua của bài).
func (c *cuaSoNguoiDungCoSo) guiYC(loai string, truong tc.TS) {
	if c.kn == nil {
		c.log.them("Chưa kết nối")
		return
	}
	if _, err := c.kn.GuiYC(loai, func(tin tc.M, rtt float64) {
		fyne.Do(func() {
			c.rttCuoi = &rtt
			c.nv.ketQua(loai, truong, tin, rtt)
			c.capNhatThanhTT()
		})
	}, truong); err != nil {
		c.log.them("Gửi lỗi: " + err.Error())
	}
}

func (c *cuaSoNguoiDungCoSo) nhip() {
	c.nv.moiNhip()
	if c.kn != nil && c.tt != nil {
		c.dinhKy.inNeuDenHan(c.nv.dongDinhKy)
	}
}

// ================================================================ chụp cửa sổ (demo dưới Xvfb)

// chupCuaSo – ảnh của từng cửa sổ. Dưới Xvfb (Linux, không có trình quản lý cửa sổ) mọi cửa sổ nằm
// chồng ở góc (0,0) và Canvas.Capture() của OpenGL phần mềm trả về ảnh trắng, nên: chỉ hiện một
// cửa sổ, chụp màn hình bằng ImageMagick "import -window root" rồi cắt đúng kích thước cửa sổ.
// Trên Windows/macOS (có GPU thật) dùng Canvas().Capture() của Fyne.
func chupCuaSo(ds []fyne.Window) []image.Image {
	dungImport := runtime.GOOS == "linux" && os.Getenv("DISPLAY") != "" && coLenh("import")
	var kq []image.Image
	for _, w := range ds {
		if !dungImport {
			var a image.Image
			fyne.DoAndWait(func() { a = w.Canvas().Capture() })
			kq = append(kq, a)
			continue
		}
		fyne.DoAndWait(func() {
			for _, w2 := range fyne.CurrentApp().Driver().AllWindows() {
				if w2 != w {
					w2.Hide() // ẩn mọi cửa sổ khác để không bị che
				}
			}
			w.Show()
		})
		time.Sleep(700 * time.Millisecond) // chờ cửa sổ được vẽ lại
		kq = append(kq, chupManHinh(w))
	}
	if dungImport {
		fyne.DoAndWait(func() {
			for _, w := range ds {
				w.Show()
			}
		})
	}
	return kq
}

func chupManHinh(w fyne.Window) image.Image {
	tam := filepath.Join(os.TempDir(), fmt.Sprintf("chup_%d.png", time.Now().UnixNano()))
	defer os.Remove(tam)
	var kt fyne.Size
	var tl float32
	fyne.DoAndWait(func() { kt, tl = w.Canvas().Size(), w.Canvas().Scale() })
	if err := exec.Command("import", "-window", "root", tam).Run(); err == nil {
		if f, err := os.Open(tam); err == nil {
			defer f.Close()
			if anh, err := png.Decode(f); err == nil {
				r := image.Rect(0, 0, int(kt.Width*tl), int(kt.Height*tl)).Intersect(anh.Bounds())
				ra := image.NewNRGBA(image.Rect(0, 0, r.Dx(), r.Dy()))
				draw.Draw(ra, ra.Bounds(), anh, r.Min, draw.Src)
				return ra
			}
		}
	}
	var a image.Image
	fyne.DoAndWait(func() { a = w.Canvas().Capture() })
	return a
}

func coLenh(ten string) bool { _, err := exec.LookPath(ten); return err == nil }

// ghepLuoi – ghép ảnh theo hàng (mỗi hàng các ảnh cạnh nhau, căn trên), nền xám #d0d0d0, viền 4
// điểm (như "convert +append/-append" ở bản Python), ghi PNG.
func ghepLuoi(duongDan string, hang [][]image.Image) error {
	W, H := 0, 0
	for _, h := range hang {
		w, c := 0, 0
		for _, a := range h {
			w += a.Bounds().Dx()
			c = max(c, a.Bounds().Dy())
		}
		W, H = max(W, w), H+c
	}
	W, H = W+8, H+8
	ra := image.NewNRGBA(image.Rect(0, 0, W, H))
	draw.Draw(ra, ra.Bounds(), image.NewUniform(color.NRGBA{0xD0, 0xD0, 0xD0, 0xFF}), image.Point{}, draw.Src)
	y := 4
	for _, h := range hang {
		x, c := 4, 0
		for _, a := range h {
			b := a.Bounds()
			draw.Draw(ra, image.Rect(x, y, x+b.Dx(), y+b.Dy()), a, b.Min, draw.Src)
			x += b.Dx()
			c = max(c, b.Dy())
		}
		y += c
	}
	f, err := os.Create(duongDan)
	if err != nil {
		return err
	}
	defer f.Close()
	return (&png.Encoder{CompressionLevel: png.BestCompression}).Encode(f, ra)
}

// ghiAnh – chụp các cửa sổ (theo hàng) rồi ghép thành một tệp; in "[demo] Đã ghi … (… KB)".
func ghiAnh(duong string, hang [][]fyne.Window) {
	var anh [][]image.Image
	for _, h := range hang {
		anh = append(anh, chupCuaSo(h))
	}
	if err := ghepLuoi(duong, anh); err != nil {
		logDemo("   Lỗi ghi ảnh: " + err.Error())
		return
	}
	st, _ := os.Stat(duong)
	logDemo(fmt.Sprintf("   Đã ghi %s (%d KB)", filepath.Base(duong), st.Size()/1024))
}

func logDemo(text string) { fmt.Println("[demo] " + text) }

// kichBan – công cụ cho demo_gui: lam(f) chạy f trên luồng giao diện và chờ; cho(giây); choDen(điều
// kiện) chờ tối đa 90 s tới khi điều kiện (kiểm trên luồng giao diện) đúng.
type kichBan struct{ hetGio bool }

func (k *kichBan) lam(f func()) { fyne.DoAndWait(f) }

func (k *kichBan) cho(giay float64) { time.Sleep(time.Duration(giay * float64(time.Second))) }

func (k *kichBan) choDen(dk func() bool) bool {
	for i := 0; i < 1800; i++ {
		ok := false
		fyne.DoAndWait(func() { ok = dk() })
		if ok {
			return true
		}
		time.Sleep(50 * time.Millisecond)
	}
	logDemo("Kịch bản chờ quá lâu – dừng")
	k.hetGio = true
	return false
}

// giuVe – chống lỗi "cửa sổ trắng" đã gặp trên Windows (MR6.5): sau một lúc (thường là sau khi cửa sổ bị
// che, thu nhỏ rồi mở lại) Fyne chỉ còn vẽ màu nền, các điều khiển không hiện dù chương trình vẫn chạy.
// Mỗi giây (trên luồng giao diện): canvas nhỏ hơn nội dung tối thiểu (vd 0×0 sau khi thu nhỏ) thì đặt lại
// kích thước cửa sổ; nội dung lệch vị trí/kích thước so với canvas thì đặt lại; rồi buộc vẽ lại toàn bộ
// cây đối tượng. Khi chạy riêng (inRa) in kích thước vào nhật ký lúc phát hiện lệch – để chẩn đoán.
func giuVe(w fyne.Window, ten string, dong <-chan struct{}) {
	go func() {
		t := time.NewTicker(time.Second)
		defer t.Stop()
		for {
			select {
			case <-dong:
				return
			case <-t.C:
			}
			fyne.Do(func() { veLai(w, ten) })
		}
	}()
}

func veLai(w fyne.Window, ten string) {
	c := w.Content()
	if c == nil {
		return
	}
	var dem float32
	if w.Padded() {
		dem = theme.Padding()
	}
	kt := w.Canvas().Size()
	toiThieu := c.MinSize().Add(fyne.NewSquareSize(2 * dem))
	canPos := fyne.NewSquareOffsetPos(dem)
	canKt := kt.Subtract(fyne.NewSquareSize(2 * dem))
	lech := c.Size() != canKt || c.Position() != canPos
	if lech || kt.Width < toiThieu.Width || kt.Height < toiThieu.Height {
		if inRa {
			fmt.Printf("%s [%s] (vẽ lại) canvas %.0fx%.0f, nội dung %.0fx%.0f tại %.0f,%.0f, tối thiểu %.0fx%.0f, tỉ lệ %.2f\n",
				time.Now().Format("15:04:05"), ten, kt.Width, kt.Height, c.Size().Width, c.Size().Height,
				c.Position().X, c.Position().Y, toiThieu.Width, toiThieu.Height, w.Canvas().Scale())
		}
		if kt.Width < toiThieu.Width || kt.Height < toiThieu.Height {
			w.Resize(toiThieu)
		} else {
			c.Move(canPos)
			c.Resize(canKt)
		}
	}
	c.Refresh()
}
