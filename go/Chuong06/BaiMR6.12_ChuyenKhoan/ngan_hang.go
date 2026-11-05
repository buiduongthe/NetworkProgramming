package main

// Khách hàng ngân hàng (bản dòng lệnh). Kết nối máy chủ, đăng nhập một tài khoản (mặc định tài khoản mang
// tên mình, ví dụ An -> TK01), in số dư, tiền đến ("+ Nhận …") và kết quả chuyển khoản; nhận lệnh:
//
//	chuyen <TK đích> <số tiền> [nội dung]     so_du     thoat
//
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
//	out/app ngan_hang <tên> [host] [cổng] [--tk TK01] [--thoi-gian 0]   (0 = tới khi hết stdin/thoat)

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.12_ChuyenKhoan/tranhchap"
)

type khachHang struct {
	ten, tk string
	khoa    sync.Mutex
	tt      tc.M
	daThay  map[int64]bool // ma_gd đã biết (không báo tiền đến hai lần)
	kn      *tc.KetNoiKhach
}

func (kh *khachHang) log(text string) { inDong(kh.ten, text) }

func (kh *khachHang) soDu(tt tc.M) int64 {
	for _, t := range tc.DSM(tt["tai_khoan"]) {
		if tc.Chuoi(t["ma"]) == kh.tk {
			return tc.SoNguyen(t["so_du"])
		}
	}
	return 0
}

func (kh *khachHang) chu(tt tc.M, ma string) string {
	for _, t := range tc.DSM(tt["tai_khoan"]) {
		if tc.Chuoi(t["ma"]) == ma {
			return tc.Chuoi(t["chu"])
		}
	}
	return "?"
}

func (kh *khachHang) ketNoi(host string, cong int) error {
	kh.kn = tc.MoiKetNoiKhach()
	kh.kn.KhiCapNhat, kh.kn.KhiTin, kh.kn.KhiDong = kh.capNhat, kh.tinKhac, func() { kh.log("Mất kết nối máy chủ") }
	if _, err := kh.kn.KetNoi(host, cong, kh.ten, "nguoi_dung", 0, 5); err != nil {
		return err
	}
	tt := kh.kn.TrangThai()
	kh.tt = tt
	if kh.tk == "" { // tài khoản mang tên mình, không có thì TK01
		ds := tc.DSM(tt["tai_khoan"])
		kh.tk = tc.Chuoi(ds[0]["ma"])
		for _, t := range ds {
			if tc.Chuoi(t["chu"]) == kh.ten {
				kh.tk = tc.Chuoi(t["ma"])
				break
			}
		}
	}
	for _, g := range tc.DSM(tt["lich_su"]) {
		kh.daThay[tc.SoNguyen(g["ma"])] = true
	}
	kh.log(fmt.Sprintf("Đăng nhập %s – %s (%s)", kh.tk, kh.chu(tt, kh.tk), tc.Chuoi(tt["che_do"])))
	kh.log("  số dư " + tc.Tien(kh.soDu(tt)))
	kh.kn.BatDauDoc()
	return nil
}

// ------------------------------------------------------------ nhận

func (kh *khachHang) capNhat(tt tc.M) {
	kh.khoa.Lock()
	kh.tt = tt
	ls := tc.DSM(tt["lich_su"])
	var moi []tc.M
	for i := len(ls) - 1; i >= 0; i-- {
		if ma := tc.SoNguyen(ls[i]["ma"]); !kh.daThay[ma] {
			kh.daThay[ma] = true
			moi = append(moi, ls[i])
		}
	}
	kh.khoa.Unlock()
	for _, g := range moi {
		if tc.Chuoi(g["den"]) == kh.tk {
			tu := tc.Chuoi(g["tu"])
			kh.log(fmt.Sprintf("+ Nhận %s từ %s (%s), số dư %s", tc.Tien(tc.SoNguyen(g["so_tien"])), tu, kh.chu(tt, tu),
				tc.Tien(kh.soDu(tt))))
		}
	}
}

func (kh *khachHang) tinKhac(tin tc.M) {
	if tin["loai"] == "su_kien" {
		kh.log("* " + tc.Chuoi(tin["noi_dung"]))
	}
}

// ------------------------------------------------------------ gửi

func (kh *khachHang) chuyen(den string, soTien int64, noiDung string) {
	kh.log(fmt.Sprintf("> chuyển %s %s→%s", tc.Tien(soTien), kh.tk, den))
	_, err := kh.kn.GuiYC("chuyen", func(tin tc.M, rtt float64) {
		if ok, _ := tin["ok"].(bool); ok {
			kh.khoa.Lock()
			kh.daThay[tc.SoNguyen(tin["ma_gd"])] = true
			kh.khoa.Unlock()
			kh.log(fmt.Sprintf("  THÀNH CÔNG (%.1f ms), số dư %s", rtt, tc.Tien(tc.SoNguyen(tin["so_du"]))))
		} else {
			kh.log(fmt.Sprintf("  TỪ CHỐI (%.0f ms): %s", rtt, tc.StrPy(tin["ly_do"])))
		}
	}, tc.T("tu", kh.tk, "den", den, "so_tien", soTien, "noi_dung", noiDung))
	if err != nil {
		kh.log("lỗi: " + err.Error())
	}
}

func (kh *khachHang) lenh(dong string) {
	p := strings.Fields(dong)
	switch {
	case p[0] == "chuyen" && len(p) >= 3:
		x, err := strconv.ParseInt(p[2], 10, 64)
		if err != nil {
			kh.log(fmt.Sprintf("lỗi: invalid literal for int() with base 10: '%s'", p[2]))
			return
		}
		kh.chuyen(p[1], x, strings.Join(p[3:], " "))
	case p[0] == "so_du":
		kh.khoa.Lock()
		tt := kh.tt
		kh.khoa.Unlock()
		kh.log(fmt.Sprintf("Số dư %s: %s", kh.tk, tc.Tien(kh.soDu(tt))))
	case p[0] == "thoat":
		kh.kn.TatBaoDong()
		kh.kn.Dong()
	default:
		kh.log("lệnh: chuyen <TK> <số tiền> [nội dung] | so_du | thoat")
	}
}

func chayNguoiDung(a []string) {
	pos, con := viTri(a, 3)
	fs := boTuyChon("ngan_hang")
	tk := fs.String("tk", "", "mã tài khoản (mặc định: tài khoản mang tên mình)")
	thoiGian := fs.Float64("thoi-gian", 0, "số giây chạy (0 = tới khi hết stdin)")
	if err := fs.Parse(con); err != nil || len(pos) < 1 {
		fmt.Println("Dùng: out/app ngan_hang <tên> [host] [cổng] [--tk TK01] [--thoi-gian 0]")
		os.Exit(2)
	}
	t0 := time.Now()
	kh := &khachHang{ten: pos[0], tk: *tk, daThay: map[int64]bool{}}
	if err := kh.ketNoi(lay(pos, 1, "127.0.0.1"), soCong(lay(pos, 2, strconv.Itoa(congMacDinh)))); err != nil {
		kh.log("Không kết nối được: " + err.Error())
		os.Exit(1)
	}
	hetStdin := make(chan struct{})
	go tc.DocLenhStdin(t0, kh.lenh, func() { close(hetStdin) })
	if *thoiGian > 0 { // chạy đúng --thoi-gian giây (hoặc tới khi thoat)
		for kh.kn.Mo() && time.Since(t0).Seconds() < *thoiGian {
			time.Sleep(100 * time.Millisecond)
		}
	} else { // tới khi hết stdin / thoat
	cho:
		for kh.kn.Mo() {
			select {
			case <-hetStdin:
				break cho
			case <-time.After(100 * time.Millisecond):
			}
		}
		time.Sleep(300 * time.Millisecond) // chờ trả lời của lệnh cuối
	}
	kh.kn.TatBaoDong()
	kh.kn.Dong()
}
