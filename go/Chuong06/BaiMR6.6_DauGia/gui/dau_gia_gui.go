package main

// Phòng đấu giá (ứng dụng người dùng). Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở
// chung_gui.go (cuaSoNguoiDungCoSo); tệp này vẽ phòng đấu giá: ảnh món hàng "Bình gốm hoa lam" (vẽ bằng
// bút ảnh ở ve.go), giá hiện tại cỡ lớn, người dẫn đầu, đồng hồ đếm ngược (đỏ khi còn < 10 s, báo GIA
// HẠN), các nút "+1/+2/+5 bước", ô nhập giá, lịch sử trả giá, thông báo "Bạn bị trả giá cao hơn!" /
// "BẠN THẮNG!". Mọi số liệu đến từ cap_nhat của máy chủ (≤ 10 lần/giây); đồng hồ đếm ngược chạy cục bộ
// từ con_lai_ms của lần cap_nhat mới nhất.

import (
	"fmt"
	"image"
	"image/color"
	"math"
	"strconv"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.6_DauGia/tranhchap"
)

var lam_ = mau("#1f4fa0") // màu men lam (gốm hoa lam)

// dangBinh – nửa bề rộng của bình theo chiều cao (đơn vị trước khi nhân tỉ lệ): (y, nửa rộng).
var dangBinh = [][2]float64{{0, 24}, {6, 22}, {14, 15}, {38, 16}, {52, 34}, {70, 50}, {92, 55}, {118, 47},
	{142, 32}, {156, 22}, {162, 25}, {170, 25}}

func nuaRong(y float64) float64 {
	for i := 0; i+1 < len(dangBinh); i++ {
		a, b := dangBinh[i], dangBinh[i+1]
		if a[0] <= y && y <= b[0] {
			return a[1] + (b[1]-a[1])*(y-a[0])/(b[0]-a[0])
		}
	}
	return 0
}

// anhBinhGom – vẽ "Bình gốm hoa lam" trên ảnh w×h: tâm ngang cx, đỉnh miệng ở y = tren, tỉ lệ k.
func anhBinhGom(w, h int, cx, tren, k float64) image.Image {
	b := moiBut(w, h)
	b.chuNhat(0, 0, float64(w), float64(h), mau("#23232f")) // nền tối + quầng đèn chiếu
	for i, m := range []string{"#2c2c3c", "#363648", "#404056", "#4a4a62"} {
		r := (1.0 - float64(i)*0.2) * float64(w) * 0.5
		b.elip(cx-r, tren+80*k-r*0.9, cx+r, tren+80*k+r*0.9, pm(mau(m)), nil, 0, false)
	}
	day := tren + 170*k
	b.daGiac([]diem{{cx - 70*k, day}, {cx + 70*k, day}, {cx + 82*k, day + 22*k}, {cx - 82*k, day + 22*k}}, mau("#6b4226"))
	b.chuNhat(cx-82*k, day+22*k, cx+82*k, float64(h), mau("#4e2f1a"))
	b.elip(cx-48*k, day-5*k, cx+48*k, day+6*k, pm(mau("#1a1a1a")), nil, 0, false)
	var vo []diem // thân bình: viền trái từ trên xuống, viền phải từ dưới lên
	for _, p := range dangBinh {
		vo = append(vo, diem{cx - p[1]*k, tren + p[0]*k})
	}
	for i := len(dangBinh) - 1; i >= 0; i-- {
		vo = append(vo, diem{cx + dangBinh[i][1]*k, tren + dangBinh[i][0]*k})
	}
	vo = splineMin(vo, 6)
	b.daGiac(vo, mau("#f5f2e8"))
	b.gapKhuc(append(vo, vo[0]), 1, mau("#8c96a8"), false)
	b.elip(cx-24*k, tren-4*k, cx+24*k, tren+5*k, pm(mau("#dcd8cc")), pm(mau("#8c96a8")), 1, false)
	dai := func(y, d float64) { // đường viền men lam quanh thân ở độ cao y
		r := nuaRong(y)*k - 1
		b.duong(cx-r, tren+y*k, cx+r, tren+y*k, math.Max(1, math.Floor(d*k)), lam_)
	}
	for _, y := range []float64{10, 36, 54, 60, 138, 150} {
		if y == 54 || y == 138 {
			dai(y, 2)
		} else {
			dai(y, 1)
		}
	}
	for i := -3; i <= 3; i++ { // cánh sen quanh cổ
		x := cx + float64(i)*9*k
		b.elip(x-5*k, tren+18*k, x+5*k, tren+38*k, nil, pm(lam_), 1, true)
	}
	tamY := tren + 96*k // bông hoa giữa thân
	for i := 0; i < 8; i++ {
		g := float64(i) * math.Pi / 4
		x, y := cx+15*k*math.Cos(g), tamY+15*k*math.Sin(g)
		b.elip(x-7*k, y-7*k, x+7*k, y+7*k, pm(mau("#5b82c4")), pm(lam_), 1, false)
	}
	b.tron(cx, tamY, 8*k, lam_)
	for _, d := range []float64{-1, 1} { // dây lá cuốn hai bên
		b.gapKhuc([]diem{{cx + d*24*k, tamY}, {cx + d*34*k, tamY - 16*k}, {cx + d*44*k, tamY - 4*k},
			{cx + d*40*k, tamY + 14*k}, {cx + d*30*k, tamY + 22*k}}, 2, lam_, true)
		b.tron(cx+d*40*k, tamY-8*k, 4*k, lam_)
	}
	for i := -4; i <= 4; i++ { // cánh sen chân bình
		x := cx + float64(i)*8*k
		b.gapKhuc([]diem{{x - 4*k, tren + 150*k}, {x, tren + 140*k}, {x + 4*k, tren + 150*k}}, 1, lam_, false)
	}
	return b.anh()
}

// dongHo – đếm ngược dạng mm:ss (dưới 10 s thêm phần mười).
func dongHo(giay float64) string {
	if giay < 10 {
		return fmt.Sprintf("00:%04.1f", giay)
	}
	g := int(giay)
	return fmt.Sprintf("%02d:%02d", g/60, g%60)
}

type cuaSoDauGia struct {
	*cuaSoNguoiDungCoSo
	hetHan        *time.Time // thời điểm hết phiên theo cap_nhat
	soLanCu       int64
	danDau        bool
	soTra, soNhan int
	hanBaoGiaHan  time.Time
	nhanTenMon    *canvas.Text
	nhanGiaKD     *widget.Label
	nhanCuaToi    *widget.Label
	nhanGia       *canvas.Text
	nhanDanDau    *widget.Label
	nhanDongHo    *canvas.Text
	nhanGiaHan    *canvas.Text
	thongBao      *oMau
	nutBuoc       []*widget.Button
	oGia          *widget.Entry
	nutTra        *widget.Button
	ls            *dsChu
}

func moCuaSoDauGia(a fyne.App, ten, host string, cong int) *cuaSoDauGia {
	x := &cuaSoDauGia{}
	x.cuaSoNguoiDungCoSo = moCuaSoNguoiDungCoSo(a, "Phòng đấu giá", ten, host, cong, x)
	x.hoanTat(640)
	return x
}

func (x *cuaSoDauGia) ve() fyne.CanvasObject {
	anh := oAnhTinh(anhBinhGom(230, 240, 115, 26, 1), 230, 240)
	x.nhanTenMon = chu("–", Den, 14, true)
	x.nhanGiaKD = widget.NewLabel("")
	x.nhanCuaToi = widget.NewLabel("Bạn: chưa trả giá")
	trai := container.NewVBox(anh, x.nhanTenMon, x.nhanGiaKD, x.nhanCuaToi)

	x.nhanGia = chu("–", mau("#8b0000"), 30, true)
	x.nhanDanDau = widget.NewLabel("Dẫn đầu: –")
	x.nhanDongHo = chu("–", Den, 24, true)
	x.nhanDongHo.TextStyle.Monospace = true
	x.nhanGiaHan = chu("", DoDam, 13, true)
	x.thongBao = moiOMau("Chưa kết nối", XamNhat, Den, 14, 380, 30, true)
	hangNut := container.NewHBox()
	for i, n := range []int64{1, 2, 5} {
		muc := widget.HighImportance
		if i == 0 {
			muc = widget.WarningImportance
		}
		n := n
		b := nut(fmt.Sprintf("+%d bước", n), muc, func() { x.tang(n) })
		x.nutBuoc = append(x.nutBuoc, b)
		hangNut.Add(coDinh(b, 122, 44))
	}
	x.oGia = oNhap("")
	x.nutTra = nut("Trả giá", widget.WarningImportance, x.traONhap)
	x.ls = moiDsChu(9, 12, 380, 196)
	phai := container.NewVBox(chu("GIÁ HIỆN TẠI", XamChu, 12, true), x.nhanGia, x.nhanDanDau,
		container.NewHBox(container.NewCenter(widget.NewLabel("Còn lại")), x.nhanDongHo, container.NewCenter(x.nhanGiaHan)),
		x.thongBao.O, hangNut,
		container.NewHBox(widget.NewLabel("Giá của bạn:"), coDinh(x.oGia, 130, 30), x.nutTra),
		nhom("Lịch sử trả giá (mới nhất trên cùng)", x.ls.O))
	x.batNut(false)
	return container.NewHBox(trai, phai)
}

// ------------------------------------------------------------ trả giá

func (x *cuaSoDauGia) batNut(bat bool) {
	for _, b := range x.nutBuoc {
		batTat(bat, b)
	}
	batTat(bat, x.nutTra)
}

func (x *cuaSoDauGia) tang(n int64) {
	if x.tt != nil {
		x.tra(tc.SoNguyen(x.tt["gia"]) + n*tc.SoNguyen(x.tt["mon"].(tc.M)["buoc_gia"]))
	}
}

func (x *cuaSoDauGia) traONhap() {
	s := strings.NewReplacer(".", "", " ", "", "đ", "").Replace(x.oGia.Text)
	gia, err := strconv.ParseInt(s, 10, 64)
	if err != nil {
		x.log.them("Giá phải là số nguyên (đồng)")
		return
	}
	x.tra(gia)
}

func (x *cuaSoDauGia) tra(gia int64) {
	x.soTra++
	x.log.them(fmt.Sprintf("Trả %s…", tc.Tien(gia)))
	x.guiYC("tra_gia", tc.T("gia", gia))
}

func (x *cuaSoDauGia) ketQua(_ string, truong tc.TS, tin tc.M, rtt float64) {
	gv, _ := truong.Lay("gia")
	gia := tc.SoNguyen(gv)
	if ok, _ := tin["ok"].(bool); ok {
		x.soNhan++
		them := ""
		if gh, _ := tin["gia_han"].(bool); gh {
			them = " – phiên được GIA HẠN"
		}
		x.log.them(fmt.Sprintf("  chấp nhận %s (%.1f ms)%s", tc.Tien(gia), rtt, them))
		x.thongBao.dat("Bạn đang dẫn đầu", XanhSP, Den)
		x.danDau = true // cap_nhat sau đó báo nếu bị vượt (hoặc bị ghi đè)
	} else {
		x.log.them(fmt.Sprintf("  từ chối %s: %s", tc.Tien(gia), tc.StrPy(tin["ly_do"])))
		lyDo := tc.StrPy(tin["ly_do"])
		if tin["ma_loi"] == "gia_thap" {
			lyDo = "giá đã thay đổi"
		}
		x.thongBao.dat("Bị từ chối: "+lyDo, CamNhat, Den)
	}
	x.nhanCuaToi.SetText(fmt.Sprintf("Bạn: trả %d lần, được nhận %d", x.soTra, x.soNhan))
}

// ------------------------------------------------------------ nhận trạng thái

func (x *cuaSoDauGia) capNhat(tt tc.M) {
	mon := tt["mon"].(tc.M)
	gia, buoc := tc.SoNguyen(tt["gia"]), tc.SoNguyen(mon["buoc_gia"])
	datChu(x.nhanTenMon, tc.Chuoi(mon["ten"]), Den)
	x.nhanGiaKD.SetText(fmt.Sprintf("Khởi điểm %s · bước %s", tc.Tien(tc.SoNguyen(mon["gia_khoi_diem"])), tc.Tien(buoc)))
	datChu(x.nhanGia, tc.Tien(gia), x.nhanGia.Color)
	ai, _ := tt["nguoi_dan_dau"].(string)
	soLan := tc.SoNguyen(tt["so_lan"])
	kyHieu := ai
	if kyHieu == "" {
		kyHieu = "–"
	}
	x.nhanDanDau.SetText(fmt.Sprintf("Dẫn đầu: %s  (%d lượt trả)", kyHieu, soLan))
	for i, n := range []int64{1, 2, 5} {
		x.nutBuoc[i].SetText(fmt.Sprintf("+%d bước\n%s", n, tc.Tien(gia+n*buoc)))
	}
	ketThuc, _ := tt["ket_thuc"].(bool)
	var moi *time.Time
	if tt["con_lai_ms"] != nil {
		t := time.Now().Add(time.Duration(tc.SoThuc(tt["con_lai_ms"]) * float64(time.Millisecond)))
		moi = &t
	}
	if moi != nil && x.hetHan != nil && moi.After(x.hetHan.Add(500*time.Millisecond)) && !ketThuc && soLan > x.soLanCu {
		x.hanBaoGiaHan = time.Now().Add(3 * time.Second) // hạn lùi ra sau vì có giá mới (không phải đặt lại)
		x.log.them(fmt.Sprintf("GIA HẠN: còn %.1f s", tc.SoThuc(tt["con_lai_ms"])/1000))
	}
	x.hetHan, x.soLanCu = moi, soLan
	var dong []string
	var cuaToi []bool
	for _, h := range tc.DSM(tt["lich_su"]) {
		s := fmt.Sprintf("%s %-6s %14s", tc.Chuoi(h["gio"]), tc.Chuoi(h["nguoi"]), tc.Tien(tc.SoNguyen(h["gia"])))
		if gh, _ := h["gia_han"].(bool); gh {
			s += " gia hạn"
		}
		dong = append(dong, s)
		cuaToi = append(cuaToi, tc.Chuoi(h["nguoi"]) == x.ten)
	}
	x.ls.dat(dong, func(i int) color.Color {
		if cuaToi[i] {
			return mau("#0040c0")
		}
		return Den
	})
	laToi := ai != "" && ai == x.ten
	if ketThuc {
		x.batNut(false)
		if laToi {
			x.thongBao.dat("BẠN THẮNG! "+tc.Tien(gia), mau("#ffd700"), Den)
		} else {
			ten := ai
			if ten == "" {
				ten = "không ai"
			}
			x.thongBao.dat("Kết thúc – "+ten+" thắng", mau("#909090"), Trang)
		}
	} else {
		x.batNut(true)
		switch {
		case x.danDau && !laToi:
			x.thongBao.dat(fmt.Sprintf("Bạn bị trả giá cao hơn! (%s)", kyHieu), DoSP, Trang)
			x.log.them(fmt.Sprintf("Bạn bị trả giá cao hơn: %s %s", kyHieu, tc.Tien(gia)))
		case laToi:
			x.thongBao.dat("Bạn đang dẫn đầu", XanhSP, Den)
		case soLan == 0 || x.thongBao.text() == "Chưa kết nối":
			x.thongBao.dat("Phiên đang mở – mời trả giá", XamNhat, Den)
		}
	}
	x.danDau = laToi && !ketThuc
}

func (x *cuaSoDauGia) suKien(text string) { x.log.them("* " + text) }

func (x *cuaSoDauGia) moiNhip() {
	if x.tt == nil {
		return
	}
	if kt, _ := x.tt["ket_thuc"].(bool); kt {
		datChu(x.nhanDongHo, "00:00", XamChu)
	} else if x.hetHan == nil {
		datChu(x.nhanDongHo, "∞", Den)
	} else {
		con := math.Max(0, time.Until(*x.hetHan).Seconds())
		m := color.Color(XanhDam)
		if con < 10 {
			m = DoDam
		}
		datChu(x.nhanDongHo, dongHo(con), m)
	}
	gh := ""
	if time.Now().Before(x.hanBaoGiaHan) {
		gh = "GIA HẠN!"
	}
	datChu(x.nhanGiaHan, gh, DoDam)
}

func (x *cuaSoDauGia) dongDinhKy() string {
	tt := x.tt
	con := "∞"
	if x.hetHan != nil {
		con = fmt.Sprintf("%.1f s", math.Max(0, time.Until(*x.hetHan).Seconds()))
	}
	trang := "đang đấu"
	if kt, _ := tt["ket_thuc"].(bool); kt {
		trang = "KẾT THÚC"
	}
	ai, _ := tt["nguoi_dan_dau"].(string)
	if ai == "" {
		ai = "–"
	}
	return fmt.Sprintf("giá %s, dẫn đầu %s, còn %s, %s; bạn trả %d, nhận %d", tc.Tien(tc.SoNguyen(tt["gia"])), ai, con,
		trang, x.soTra, x.soNhan)
}

func (x *cuaSoDauGia) daKetNoi()  {}
func (x *cuaSoDauGia) matKetNoi() {}
