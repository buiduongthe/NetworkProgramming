package main

// Các thủ tục dùng chung (gói RTP, PCM, đo mức dBFS, bộ đệm jitter, Goertzel, WAV, JSON theo dòng)
// nằm trong gói con amthanh/ để ứng dụng giao diện (thư mục gui/, mô-đun riêng vì cần Fyne) dùng
// lại được – Go không cho nhập gói main. Tệp này chỉ đặt tên ngắn cho chương trình dòng lệnh.
// Giải thích chi tiết PCM 16 bit, khung 20 ms, dBFS: xem amthanh/giaothuc.go.

import ht "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.2_HoiThoaiAmThanh/amthanh"

const (
	tanSoMau     = ht.TanSoMau     // 16000 mẫu/giây
	khungMs      = ht.KhungMs      // 20 ms mỗi khung
	mauMoiKhung  = ht.MauMoiKhung  // 320 mẫu
	byteMoiKhung = ht.ByteMoiKhung // 640 byte
	ptAmThanh    = ht.PtAmThanh    // payload type 96: L16/16000/mono
	ptGiuKetNoi  = ht.PtGiuKetNoi  // 127: gói rỗng giữ kết nối
	ssrcServer   = ht.SsrcServer   // 0: luồng trộn do server gửi
)

type boDemJitter = ht.BoDemJitter

var (
	dongGoiRTP  = ht.DongGoiRTP
	tachRTP     = ht.TachRTP
	pcmSangByte = ht.PcmSangByte
	byteSangPCM = ht.ByteSangPCM
	catNgon     = ht.CatNgon
	mucDBFS     = ht.MucDBFS
	thanhMuc    = ht.ThanhMuc
	chuDB       = ht.ChuDB
	moiBoDem    = ht.MoiBoDem
	mucAmDBFS   = ht.MucAmDBFS
	ghiWAV      = ht.GhiWAV
	docWAV      = ht.DocWAV
	guiJSON     = ht.GuiJSON
)
