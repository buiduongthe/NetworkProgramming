package main

// Người gọi xe (bản dòng lệnh). Kết nối máy chủ, in diễn biến (số xe trống, xe của mình bị "nhận khách
// khác") và nhận lệnh:
//
//	dat            đặt xe từ điểm đón mặc định của mình (theo tên) tới điểm đến mặc định
//	huy            hủy chuyến đã đặt                       thoat
//
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
//	out/app dat_xe <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.7_DatXe/tranhchap"
)

// diemMacDinh – điểm đón và điểm đến cố định theo tên (để mỗi người một chỗ đứng khác nhau).
func diemMacDinh(ten string) ([2]float64, [2]float64) {
	s := 0
	for _, c := range ten {
		s += int(c)
	}
	don := tc.NgaTu[(s*7)%len(tc.NgaTu)]
	den := tc.NgaTu[(s*11+9)%len(tc.NgaTu)]
	if tc.Km(don, den) < 5 { // quá gần: chọn góc xa nhất
		den = [2]float64{0.1, 0.1}
		if don[0] < 0.5 {
			den[0] = 0.9
		}
		if don[1] < 0.5 {
			den[1] = 0.9
		}
	}
	return don, den
}

type nguoiGoiXe struct {
	ten      string
	don, den [2]float64
	khoa     sync.Mutex
	tt       tc.M  // cap_nhat mới nhất
	xeToi    int64 // mã xe mình được báo đặt thành công (0 = chưa có)
	phienToi int64 // số thứ tự lần ghi (kq) của chuyến mình đặt
	dangHuy  bool
	kn       *tc.KetNoiKhach
}

func (ng *nguoiGoiXe) log(text string) { inDong(ng.ten, text) }

func (ng *nguoiGoiXe) ketNoi(host string, cong int) error {
	ng.kn = tc.MoiKetNoiKhach()
	ng.kn.KhiCapNhat, ng.kn.KhiTin, ng.kn.KhiDong = ng.capNhat, ng.tinKhac, func() { ng.log("Mất kết nối máy chủ") }
	if _, err := ng.kn.KetNoi(host, cong, ng.ten, "nguoi_dung", 0, 5); err != nil {
		return err
	}
	tt := ng.kn.TrangThai()
	ng.tt = tt
	ng.log(fmt.Sprintf("Mở ứng dụng (%s): %d/%d xe trống", tc.Chuoi(tt["che_do"]), tc.SoNguyen(tt["trong"]),
		tc.SoNguyen(tt["so_xe"])))
	ng.log("  đón: " + tc.TenDiem(ng.don))
	ng.log("  đến: " + tc.TenDiem(ng.den))
	ng.kn.BatDauDoc()
	return nil
}

// ------------------------------------------------------------ nhận

func (ng *nguoiGoiXe) capNhat(tt tc.M) {
	ng.khoa.Lock()
	defer ng.khoa.Unlock()
	cu := ng.tt
	ng.tt = tt
	if tc.SoNguyen(tt["trong"]) != tc.SoNguyen(cu["trong"]) {
		ng.log(fmt.Sprintf("Xe trống: %d/%d", tc.SoNguyen(tt["trong"]), tc.SoNguyen(tt["so_xe"])))
	}
	// bỏ qua ảnh chụp trước lần ghi của mình
	if ng.xeToi != 0 && !ng.dangHuy && tc.SoNguyen(tt["phien"]) >= ng.phienToi {
		var xe tc.M
		for _, x := range tc.DSM(tt["xe"]) {
			if tc.SoNguyen(x["ma"]) == ng.xeToi {
				xe = x
			}
		}
		khach, _ := xe["khach"].(string)
		switch {
		case xe != nil && khach != "" && khach != ng.ten:
			ng.log(fmt.Sprintf("  Tài xế %s đã nhận khách khác (%s)!", tc.Chuoi(xe["tai_xe"]), khach))
			ng.xeToi = 0
		case xe == nil || khach == "":
			ng.log("  Chuyến đã kết thúc (máy chủ đặt lại)")
			ng.xeToi = 0
		}
	}
}

func (ng *nguoiGoiXe) tinKhac(tin tc.M) {
	if tin["loai"] == "su_kien" {
		ng.log("* " + tc.Chuoi(tin["noi_dung"]))
	}
}

// ------------------------------------------------------------ gửi

func (ng *nguoiGoiXe) dat() {
	ng.log(fmt.Sprintf("> Đặt xe – đang tìm tài xế… (dự kiến %s)", tc.Tien(tc.Cuoc(ng.don, ng.den))))
	_, err := ng.kn.GuiYC("dat_xe", func(tin tc.M, rtt float64) {
		switch ok, _ := tin["ok"].(bool); {
		case ok:
			ng.khoa.Lock()
			ng.xeToi, ng.phienToi = tc.SoNguyen(tin["xe"]), tc.SoNguyen(tin["phien"])
			ng.khoa.Unlock()
			ng.log(fmt.Sprintf("  ĐẶT THÀNH CÔNG – tài xế %s (%.1f ms)", tc.Chuoi(tin["tai_xe"]), rtt))
			ng.log(fmt.Sprintf("  %s, xe %s, đến sau %d phút, cước %s", tc.Chuoi(tin["bien_so"]), tc.Chuoi(tin["mau"]),
				tc.SoNguyen(tin["phut"]), tc.Tien(tc.SoNguyen(tin["cuoc"]))))
		case tin["ma_loi"] == "het_cho":
			ng.log("  HẾT CHỖ: " + tc.StrPy(tin["ly_do"]))
		default:
			ng.log("  TỪ CHỐI: " + tc.StrPy(tin["ly_do"]))
		}
	}, tc.T("don", []any{ng.don[0], ng.don[1]}, "den", []any{ng.den[0], ng.den[1]}))
	if err != nil {
		ng.log("lỗi: " + err.Error())
	}
}

func (ng *nguoiGoiXe) huy() {
	ng.log("> Hủy chuyến")
	ng.khoa.Lock()
	ng.dangHuy = true
	ng.khoa.Unlock()
	_, err := ng.kn.GuiYC("huy", func(tin tc.M, rtt float64) {
		ng.khoa.Lock()
		ng.dangHuy = false
		ok, _ := tin["ok"].(bool)
		if ok {
			ng.xeToi = 0
		}
		ng.khoa.Unlock()
		if ok {
			ng.log(fmt.Sprintf("  Đã hủy, xe %d (%s) trống", tc.SoNguyen(tin["xe"]), tc.Chuoi(tin["tai_xe"])))
		} else {
			ng.log("  TỪ CHỐI: " + tc.StrPy(tin["ly_do"]))
		}
	}, nil)
	if err != nil {
		ng.log("lỗi: " + err.Error())
	}
}

func (ng *nguoiGoiXe) lenh(dong string) {
	switch strings.Join(strings.Fields(dong), " ") {
	case "dat":
		ng.dat()
	case "huy":
		ng.huy()
	case "thoat":
		ng.kn.TatBaoDong()
		ng.kn.Dong()
	default:
		ng.log("lệnh: dat | huy | thoat")
	}
}

func chayNguoiDung(a []string) {
	pos, con := viTri(a, 3)
	fs := boTuyChon("dat_xe")
	thoiGian := fs.Float64("thoi-gian", 0, "số giây chạy (0 = tới khi hết stdin)")
	if err := fs.Parse(con); err != nil || len(pos) < 1 {
		fmt.Println("Dùng: out/app dat_xe <tên> [host] [cổng] [--thoi-gian 0]")
		os.Exit(2)
	}
	t0 := time.Now()
	ng := &nguoiGoiXe{ten: pos[0]}
	ng.don, ng.den = diemMacDinh(ng.ten)
	if err := ng.ketNoi(lay(pos, 1, "127.0.0.1"), soCong(lay(pos, 2, strconv.Itoa(congMacDinh)))); err != nil {
		ng.log("Không kết nối được: " + err.Error())
		os.Exit(1)
	}
	hetStdin := make(chan struct{})
	go tc.DocLenhStdin(t0, ng.lenh, func() { close(hetStdin) })
	if *thoiGian > 0 { // chạy đúng --thoi-gian giây (hoặc tới khi thoat)
		for ng.kn.Mo() && time.Since(t0).Seconds() < *thoiGian {
			time.Sleep(100 * time.Millisecond)
		}
	} else { // tới khi hết stdin / thoat
	cho:
		for ng.kn.Mo() {
			select {
			case <-hetStdin:
				break cho
			case <-time.After(100 * time.Millisecond):
			}
		}
		time.Sleep(300 * time.Millisecond) // chờ trả lời của lệnh cuối
	}
	ng.kn.TatBaoDong()
	ng.kn.Dong()
}
