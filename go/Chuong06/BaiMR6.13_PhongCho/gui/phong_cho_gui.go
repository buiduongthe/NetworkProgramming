package main

// Trang bán vé có phòng chờ ảo (ứng dụng người dùng). Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi
// yêu cầu) ở chung_gui.go (cuaSoNguoiDungCoSo); tệp này vẽ ba "trang" như một trang bán vé thật:
//  1. Sự kiện: áp phích concert (vẽ bằng bút ảnh), giá vé, thanh "vé còn lại", nút "VÀO HÀNG CHỜ MUA VÉ"
//     (phòng chờ tắt: "MUA VÉ NGAY");
//  2. Phòng chờ: số thứ tự, THANH TIẾN TRÌNH vị trí, "Còn khoảng x người phía trước – chờ ~y s", số đang
//     phục vụ, tốc độ cấp lượt; cập nhật trực tiếp theo cap_nhat (≤ 10 lần/giây);
//  3. Chọn vé: "ĐẾN LƯỢT BẠN!", ĐỒNG HỒ đếm ngược thời hạn lượt (20 s, đỏ khi < 5 s), chọn số vé, thành
//     tiền, nút "THANH TOÁN", kết quả ("Mua thành công…", "Hết vé", "Lượt đã hết hạn", "Hệ thống quá tải").
//
// Tới lượt được báo bằng trả lời của yêu cầu "chờ dài" cho_luot (máy chủ giữ yêu cầu tới khi cấp lượt).

import (
	"fmt"
	"image"
	"image/color"
	"math"
	"math/rand/v2"
	"strconv"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.13_PhongCho/tranhchap"
)

var (
	tim  = mau("#3b0a5e")
	hong = mau("#e0407a")
)

func tronMau(c1, c2 color.NRGBA, t float64) color.NRGBA {
	f := func(a, b uint8) uint8 { return uint8(math.Round(float64(a) + (float64(b)-float64(a))*t)) }
	return color.NRGBA{f(c1.R, c2.R), f(c1.G, c2.G), f(c1.B, c2.B), 0xFF}
}

// anhApPhich – nền áp phích concert: chuyển màu, sao, đèn chiếu sân khấu, sân khấu, đám đông.
func anhApPhich(w, h int) image.Image {
	b := moiBut(w, h)
	W, H := float64(w), float64(h)
	for y := 0; y < h; y++ {
		b.chuNhat(0, float64(y), W, float64(y+1), tronMau(mau("#12002a"), mau("#6a1b6e"), float64(y)/H))
	}
	rnd := rand.New(rand.NewPCG(7, 7))
	for i := 0; i < 40; i++ { // sao
		x, y := float64(rnd.IntN(w+1)), float64(rnd.IntN(h/2+1))
		b.elip(x, y, x+2, y+2, pm(mau("#fff4c0")), nil, 0, false)
	}
	for _, d := range []struct {
		x float64
		m string
	}{{W * 0.18, "#ffd36e"}, {W * 0.5, "#7fd6ff"}, {W * 0.82, "#ff8fc0"}} { // đèn chiếu (trong mờ)
		m := mau(d.m)
		trong := m
		trong.A = 0x50
		b.daGiac([]diem{{d.x, H * 0.62}, {d.x - 70, -10}, {d.x + 70, -10}}, trong)
		b.tron(d.x, H*0.62, 7, m)
	}
	b.chuNhat(0, H*0.66, W, H*0.72, mau("#2a0f3a")) // sân khấu
	den := mau("#0a0012")
	for i := 0; i <= int(W/14); i++ { // đám đông (bóng đầu, tay giơ)
		x := float64(i*14 + rnd.IntN(7) - 3)
		y := H - 18 + float64(rnd.IntN(9)-4)
		b.tron(x, y, 7, den)
		b.chuNhat(x-9, y+4, x+9, H, den)
		if rnd.Float64() < 0.3 {
			b.duong(x+5, y, x+10, y-20, 4, den)
		}
	}
	b.chuNhat(W-120, 14, W-14, 40, hong)
	return b.anh()
}

// dongHoLuot – 00:19 (≥ 10 s) hoặc 00:04.2.
func dongHoLuot(giay float64) string {
	giay = math.Max(0, giay)
	if giay >= 10 {
		return fmt.Sprintf("00:%02d", int(giay))
	}
	return fmt.Sprintf("00:%04.1f", giay)
}

// thanhTien – thanh tiến trình không chữ (như ttk.Progressbar).
func thanhTien(rong float32) (*widget.ProgressBar, fyne.CanvasObject) {
	p := widget.NewProgressBar()
	p.TextFormatter = func() string { return "" }
	return p, coDinh(p, rong, 14)
}

// leTrang – bố cục: nội dung cách lề trái l điểm ảnh, kéo hết chiều ngang còn lại.
type leTrang struct{ l float32 }

func (b *leTrang) Layout(o []fyne.CanvasObject, kt fyne.Size) {
	for _, x := range o {
		x.Move(fyne.NewPos(b.l, 0))
		x.Resize(fyne.NewSize(kt.Width-2*b.l, kt.Height))
	}
}

func (b *leTrang) MinSize(o []fyne.CanvasObject) fyne.Size {
	var w, h float32
	for _, x := range o {
		w, h = max(w, x.MinSize().Width+2*b.l), max(h, x.MinSize().Height)
	}
	return fyne.NewSize(w, h)
}

type cuaSoPhongCho struct {
	*cuaSoNguoiDungCoSo
	trang       string // "su_kien" | "cho" | "mua"
	so          *int64 // số thứ tự trong hàng
	truocDau    int64  // số người phía trước lúc vào hàng
	han         *time.Time
	coLuot      bool
	soKq, soDon int
	tenCu       string
	nhanTen     *canvas.Text
	trangSK     fyne.CanvasObject
	trangCho    fyne.CanvasObject
	trangMua    fyne.CanvasObject
	// trang sự kiện
	nhanGia   *canvas.Text
	nhanConSK *canvas.Text
	thanhSK   *widget.ProgressBar
	nhanPC    *widget.Label
	nutVao    *widget.Button
	nhanKqSK  *canvas.Text
	// trang phòng chờ
	nhanSo     *canvas.Text
	thanhCho   *widget.ProgressBar
	nhanTruoc  *canvas.Text
	nhanPhucVu *canvas.Text
	nhanConCho *canvas.Text
	// trang mua
	bangLuot   *oMau
	nhanDongHo *canvas.Text
	chonSoVe   *widget.RadioGroup
	nhanTien   *canvas.Text
	nutMua     *widget.Button
	nhanKq     *canvas.Text
	nutVe      *widget.Button
}

func moCuaSoPhongCho(a fyne.App, ten, host string, cong int) *cuaSoPhongCho {
	x := &cuaSoPhongCho{}
	x.cuaSoNguoiDungCoSo = moCuaSoNguoiDungCoSo(a, "Bán vé concert", ten, host, cong, x)
	x.hoanTat(560)
	return x
}

func dongTrang(o ...fyne.CanvasObject) fyne.CanvasObject {
	return container.New(&leTrang{12}, container.NewVBox(o...))
}

func (x *cuaSoPhongCho) ve() fyne.CanvasObject {
	phich := oAnhTinh(anhApPhich(540, 150), 540, 150)
	x.nhanTen = chu("ĐÊM NHẠC", Trang, 24, true)
	phu := chu("20:00 · 20.12.2026 · Sân khấu ngoài trời", mau("#ffd9f0"), 13, false)
	moBan := chu("MỞ BÁN", Trang, 13, true)
	apPhich := container.NewWithoutLayout(phich, x.nhanTen, phu, moBan)
	phich.Resize(fyne.NewSize(540, 150))
	x.nhanTen.Move(fyne.NewPos(20, 30-x.nhanTen.MinSize().Height/2))
	phu.Move(fyne.NewPos(22, 58-phu.MinSize().Height/2))
	moBan.Move(fyne.NewPos(540-67-moBan.MinSize().Width/2, 27-moBan.MinSize().Height/2))

	// 1. sự kiện
	x.nhanGia = chu("–", tim, 15, true)
	x.nhanConSK = chu("Vé còn lại: –", Den, 13, false)
	var o fyne.CanvasObject
	x.thanhSK, o = thanhTien(510)
	x.nhanPC = widget.NewLabel("")
	x.nhanPC.Wrapping = fyne.TextWrapWord
	x.nutVao = nut("VÀO HÀNG CHỜ MUA VÉ", widget.DangerImportance, x.vaoHang)
	x.nhanKqSK = chu("", DoDam, 13, true)
	x.nhanKqSK.Alignment = fyne.TextAlignCenter
	x.trangSK = dongTrang(widget.NewLabel(""), x.nhanGia, x.nhanConSK, o, coDinh(x.nhanPC, 510, 44),
		container.NewCenter(coDinh(x.nutVao, 300, 44)), x.nhanKqSK)
	x.nutVao.Disable()

	// 2. phòng chờ
	x.nhanSo = chu("Số thứ tự của bạn: –", Den, 14, false)
	x.thanhCho, o = thanhTien(510)
	x.thanhCho.Max = 1000
	x.nhanTruoc = chu("–", Den, 16, true)
	x.nhanPhucVu = chu("", mau("#404040"), 12, false)
	x.nhanConCho = chu("", mau("#404040"), 12, false)
	ghiChu := container.NewStack(canvas.NewRectangle(mau("#fff3c4")),
		container.NewPadded(chu("Vui lòng không tải lại trang – bạn vẫn giữ nguyên chỗ trong hàng.", Den, 12, false)))
	x.trangCho = dongTrang(widget.NewLabel(""), chu("Bạn đang ở trong phòng chờ", tim, 17, true), x.nhanSo, o,
		x.nhanTruoc, x.nhanPhucVu, x.nhanConCho, widget.NewLabel(""), ghiChu)

	// 3. chọn vé
	x.bangLuot = moiOMau("ĐẾN LƯỢT BẠN!", mau("#2e9e4f"), Trang, 15, 538, 32, true)
	x.nhanDongHo = chu("–", XanhDam, 30, true)
	x.nhanDongHo.TextStyle.Monospace = true
	x.chonSoVe = widget.NewRadioGroup([]string{"1", "2", "3", "4"}, func(string) { x.tinhTien() })
	x.chonSoVe.Horizontal = true
	x.chonSoVe.Required = true
	x.chonSoVe.SetSelected("2")
	x.nhanTien = chu("", Den, 13, false)
	x.nutMua = nut("THANH TOÁN", widget.WarningImportance, x.thanhToan)
	x.nhanKq = chu("", Den, 14, true)
	x.nhanKq.Alignment = fyne.TextAlignCenter
	x.nutVe = nut("Về trang sự kiện", widget.MediumImportance, x.veSuKien)
	x.nutVe.Hide()
	x.trangMua = container.NewVBox(x.bangLuot.O, dongTrang(
		container.NewHBox(container.NewCenter(widget.NewLabel("Thời gian còn lại để hoàn tất:")), x.nhanDongHo),
		container.NewHBox(widget.NewLabel("Số vé:"), x.chonSoVe, container.NewCenter(x.nhanTien)),
		container.NewCenter(coDinh(x.nutMua, 240, 40)), x.nhanKq, container.NewCenter(x.nutVe)))
	nen := canvas.NewRectangle(Trang)
	nen.StrokeColor, nen.StrokeWidth = mau("#9090b0"), 1
	hop := coDinh(container.NewStack(nen, x.trangSK, x.trangCho, x.trangMua), 540, 250)
	x.sang("su_kien")
	return container.NewVBox(coDinh(apPhich, 540, 150), hop)
}

func (x *cuaSoPhongCho) sang(trang string) {
	for t, o := range map[string]fyne.CanvasObject{"su_kien": x.trangSK, "cho": x.trangCho, "mua": x.trangMua} {
		if t == trang {
			o.Show()
		} else {
			o.Hide()
		}
	}
	x.trang = trang
}

// ------------------------------------------------------------ thao tác

func (x *cuaSoPhongCho) vaoHang() {
	datChu(x.nhanKqSK, "", DoDam)
	if pc, _ := x.tt["phong_cho"].(bool); !pc { // phòng chờ tắt: vào thẳng trang mua
		x.moTrangMua(nil)
		return
	}
	x.nutVao.Disable()
	x.log.them("Vào phòng chờ…")
	x.guiYC("xep_hang", nil)
}

func (x *cuaSoPhongCho) moTrangMua(hanMs *float64) {
	x.coLuot = true
	x.han = nil
	if hanMs != nil {
		t := time.Now().Add(time.Duration(*hanMs * float64(time.Millisecond)))
		x.han = &t
		x.bangLuot.dat("ĐẾN LƯỢT BẠN! Hãy hoàn tất trước khi hết giờ", mau("#2e9e4f"), Trang)
	} else {
		x.bangLuot.dat("MUA TRỰC TIẾP (phòng chờ tắt)", tim, Trang)
	}
	datChu(x.nhanKq, "", Den)
	x.nutMua.Enable()
	x.nutVe.Hide()
	x.tinhTien()
	x.sang("mua")
}

func (x *cuaSoPhongCho) soVe() int64 { n, _ := strconv.ParseInt(x.chonSoVe.Selected, 10, 64); return n }

func (x *cuaSoPhongCho) thanhToan() {
	n := x.soVe()
	x.log.them(fmt.Sprintf("Thanh toán %d vé…", n))
	datChu(x.nhanKq, "Đang xử lý thanh toán…", mau("#404040"))
	x.guiYC("mua", tc.T("so_ve", n))
}

func (x *cuaSoPhongCho) veSuKien() {
	x.so, x.han, x.truocDau = nil, nil, 0
	x.coLuot = false
	if het, _ := x.tt["het_ve"].(bool); x.tt != nil && !het {
		x.nutVao.Enable()
	} else {
		x.nutVao.Disable()
	}
	x.sang("su_kien")
}

func (x *cuaSoPhongCho) tinhTien() {
	if x.tt == nil || x.nhanTien == nil {
		return
	}
	sk, _ := x.tt["su_kien"].(tc.M)
	n, gia := x.soVe(), tc.SoNguyen(sk["gia_ve"])
	datChu(x.nhanTien, fmt.Sprintf("× %s = %s", tc.Tien(gia), tc.Tien(n*gia)), Den)
}

func (x *cuaSoPhongCho) ketQua(loai string, _ tc.TS, tin tc.M, rtt float64) {
	x.soKq++
	ok, _ := tin["ok"].(bool)
	lyDo := tc.StrPy(tin["ly_do"])
	switch {
	case loai == "xep_hang":
		if !ok {
			x.log.them("  không vào được hàng: " + lyDo)
			datChu(x.nhanKqSK, "Rất tiếc: "+lyDo, DoDam)
			x.nutVao.Enable()
			return
		}
		so := tc.SoNguyen(tin["so"])
		x.so, x.truocDau = &so, max(1, tc.SoNguyen(tin["truoc"]))
		x.log.them(fmt.Sprintf("  số thứ tự #%d, %d người phía trước, ~%.1f s", so, tc.SoNguyen(tin["truoc"]),
			tc.SoThuc(tin["cho_s"])))
		x.sang("cho")
		x.veViTri()
		x.guiYC("cho_luot", nil) // chờ dài: máy chủ trả lời khi tới lượt
	case loai == "cho_luot":
		if ok {
			x.log.them(fmt.Sprintf("  ĐẾN LƯỢT sau %.1f s chờ, còn %.0f s để mua", rtt/1000, tc.SoThuc(tin["han_ms"])/1000))
			h := tc.SoThuc(tin["han_ms"])
			x.moTrangMua(&h)
		} else {
			x.log.them("  " + lyDo)
			x.veSuKien()
			datChu(x.nhanKqSK, "Rất tiếc: "+lyDo, DoDam)
		}
	case ok:
		x.soDon++
		x.log.them(fmt.Sprintf("  MUA THÀNH CÔNG (%.1f ms): đơn #%d, %d vé", rtt, tc.SoNguyen(tin["ma_don"]),
			tc.SoNguyen(tin["so_ve"])))
		datChu(x.nhanKq, fmt.Sprintf("Mua thành công %d vé – đơn #%d, %s", tc.SoNguyen(tin["so_ve"]),
			tc.SoNguyen(tin["ma_don"]), tc.Tien(tc.SoNguyen(tin["thanh_tien"]))), XanhDam)
		x.nutMua.Disable()
		x.han, x.coLuot = nil, false
		datChu(x.nhanDongHo, "✓", XanhDam)
		x.nutVe.Show()
	default:
		ma, _ := tin["ma_loi"].(string)
		x.log.them(fmt.Sprintf("  từ chối (%.1f ms): %s", rtt, lyDo))
		text := map[string]string{"qua_tai": "Hệ thống đang quá tải, vui lòng thử lại", "het_ve": "Rất tiếc, đã bán hết vé",
			"da_mua": "Lượt này đã được dùng để mua"}[ma]
		if text == "" {
			text = tc.Chuoi(tin["ly_do"])
		}
		datChu(x.nhanKq, text, DoDam)
		if ma == "het_ve" || ma == "het_luot" || ma == "da_mua" {
			x.nutMua.Disable()
			x.nutVe.Show()
		}
	}
}

func (x *cuaSoPhongCho) veViTri() {
	tt := x.tt
	if x.so == nil || tt == nil {
		return
	}
	truoc := max(0, *x.so-tc.SoNguyen(tt["dang_phuc_vu"])-1)
	datChu(x.nhanSo, fmt.Sprintf("Số thứ tự của bạn: #%d", *x.so), Den)
	x.thanhCho.SetValue(1000 * (1 - float64(truoc)/float64(max(x.truocDau, 1))))
	tocDo := tc.SoThuc(tt["toc_do"])
	if truoc > 0 {
		datChu(x.nhanTruoc, fmt.Sprintf("Còn khoảng %d người phía trước – chờ ~%.0f s", truoc, float64(truoc+1)/tocDo), Den)
	} else {
		datChu(x.nhanTruoc, "Sắp tới lượt bạn…", Den)
	}
	sk, _ := tt["su_kien"].(tc.M)
	datChu(x.nhanPhucVu, fmt.Sprintf("Đang phục vụ tới số #%d · cấp lượt %s người/s · %d người đang chờ",
		tc.SoNguyen(tt["dang_phuc_vu"]), tc.FmtG(tocDo), tc.SoNguyen(tt["dang_cho"])), mau("#404040"))
	datChu(x.nhanConCho, fmt.Sprintf("Vé còn lại: %d / %d", tc.SoNguyen(tt["con_lai"]), tc.SoNguyen(sk["tong_ve"])),
		mau("#404040"))
}

// ------------------------------------------------------------ nhận trạng thái

func (x *cuaSoPhongCho) capNhat(tt tc.M) {
	sk, _ := tt["su_kien"].(tc.M)
	if ten := tc.Chuoi(sk["ten"]); ten != x.tenCu {
		x.tenCu = ten
		datChu(x.nhanTen, strings.ToUpper(ten), Trang)
	}
	datChu(x.nhanGia, fmt.Sprintf("Giá vé %s · tối đa %d vé/người", tc.Tien(tc.SoNguyen(sk["gia_ve"])),
		tc.SoNguyen(sk["ve_toi_da"])), tim)
	het, _ := tt["het_ve"].(bool)
	con := fmt.Sprintf("Vé còn lại: %d / %d", tc.SoNguyen(tt["con_lai"]), tc.SoNguyen(sk["tong_ve"]))
	if het {
		con += "  – ĐÃ BÁN HẾT"
	}
	datChu(x.nhanConSK, con, Den)
	x.thanhSK.Max = math.Max(1, tc.SoThuc(sk["tong_ve"]))
	x.thanhSK.SetValue(math.Max(0, tc.SoThuc(tt["con_lai"])))
	pc, _ := tt["phong_cho"].(bool)
	if pc {
		x.nhanPC.SetText(fmt.Sprintf("Phòng chờ ảo BẬT: mọi người được xếp hàng theo thứ tự đến, mỗi giây cấp lượt cho"+
			" %s người; mỗi lượt có %s s để mua.", tc.FmtG(tc.SoThuc(tt["toc_do"])), tc.FmtG(tc.SoThuc(sk["luot_s"]))))
		if x.nutVao.Text != "VÀO HÀNG CHỜ MUA VÉ" {
			x.nutVao.SetText("VÀO HÀNG CHỜ MUA VÉ")
		}
	} else {
		x.nhanPC.SetText("Phòng chờ TẮT: ai cũng vào mua trực tiếp cùng lúc.")
		if x.nutVao.Text != "MUA VÉ NGAY" {
			x.nutVao.SetText("MUA VÉ NGAY")
		}
	}
	var lc []string
	for i := int64(1); i <= max(1, tc.SoNguyen(sk["ve_toi_da"])); i++ {
		lc = append(lc, strconv.FormatInt(i, 10))
	}
	if strings.Join(lc, ",") != strings.Join(x.chonSoVe.Options, ",") {
		x.chonSoVe.Options = lc
		if x.soVe() > int64(len(lc)) {
			x.chonSoVe.SetSelected(lc[len(lc)-1])
		}
		x.chonSoVe.Refresh()
	}
	if x.trang == "su_kien" {
		if het {
			x.nutVao.Disable()
		} else {
			x.nutVao.Enable()
		}
	}
	x.tinhTien()
	x.veViTri()
}

func (x *cuaSoPhongCho) suKien(text string) { x.log.them("* " + text) }

func (x *cuaSoPhongCho) moiNhip() {
	if x.trang != "mua" || !x.coLuot {
		return
	}
	if x.han == nil {
		datChu(x.nhanDongHo, "∞", XanhDam)
		return
	}
	con := time.Until(*x.han).Seconds()
	m := XanhDam
	if con < 5 {
		m = DoDam
	}
	datChu(x.nhanDongHo, dongHoLuot(con), m)
	if con <= 0 {
		x.coLuot = false
		x.nutMua.Disable()
		datChu(x.nhanKq, "Hết thời gian giữ lượt – bạn cần xếp hàng lại", DoDam)
		x.nutVe.Show()
		x.log.them("Lượt mua đã hết hạn")
	}
}

func (x *cuaSoPhongCho) dongDinhKy() string {
	trang := map[string]string{"su_kien": "sự kiện", "cho": "phòng chờ", "mua": "mua"}[x.trang]
	viTri := ""
	if x.so != nil {
		viTri = fmt.Sprintf(", số #%d, phục vụ #%d", *x.so, tc.SoNguyen(x.tt["dang_phuc_vu"]))
	}
	return fmt.Sprintf("trang %s%s; vé còn %d; đơn của bạn %d", trang, viTri, tc.SoNguyen(x.tt["con_lai"]), x.soDon)
}

func (x *cuaSoPhongCho) daKetNoi()  {}
func (x *cuaSoPhongCho) matKetNoi() {}
