package main

// Các thủ tục dùng chung (chia mảnh/ghép khung, header 20 byte, vẽ ảnh RGB, camera tổng hợp, nén
// zlib, PNG, JSON theo dòng) nằm trong gói con video/ để ứng dụng giao diện (thư mục gui/, mô-đun
// riêng vì cần Fyne) dùng lại được – Go không cho nhập gói main. Tệp này chỉ đặt tên ngắn cho
// chương trình dòng lệnh. Giải thích chi tiết kích thước khung, nén, chia mảnh: xem video/giaothuc.go.

import (
	"time"

	vd "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.3_HoiThoaiVideo/video"
)

const (
	rong, cao  = vd.Rong, vd.Cao // 160×120
	loaiManh   = vd.LoaiManh     // mảnh khung hình
	maServer   = vd.MaServer     // mã nguồn của luồng lưới do server gửi
	fpsMacDinh = vd.FpsMacDinh   // 10 fps
)

type (
	header    = vd.Header
	ghepKhung = vd.GhepKhung
	mau       = vd.Mau
)

var (
	t0         = time.Now() // mốc thời gian cho kịch bản "@t lệnh"
	chiaManh   = vd.ChiaManh
	goiGiu     = vd.GoiGiu
	tachHeader = vd.TachHeader
	moiGhep    = vd.MoiGhep
	toMau      = vd.ToMau
	veChu      = vd.VeChu
	thuNho     = vd.ThuNho
	phongTo    = vd.PhongTo
	moiCamera  = vd.MoiCamera
	nenZlib    = vd.NenZlib
	giaiNen    = vd.GiaiNen
	ghiPNG     = vd.GhiPNG
	guiJSON    = vd.GuiJSON
	soNguyen   = vd.SoNguyen
)
