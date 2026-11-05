package main

// Người mua flash sale (bản dòng lệnh). Kết nối máy chủ, in diễn biến (đếm ngược, mở bán, tồn kho, hết
// hàng) và nhận lệnh:
//
//	mua [n]     đặt một đơn n sản phẩm (mặc định 1)          thoat
//
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
//	out/app flash_sale <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.11_FlashSale/tranhchap"
)

type nguoiMua struct {
	ten  string
	khoa sync.Mutex
	tt   tc.M
	kn   *tc.KetNoiKhach
}

func (ng *nguoiMua) log(text string) { inDong(ng.ten, text) }

func (ng *nguoiMua) ketNoi(host string, cong int) error {
	ng.kn = tc.MoiKetNoiKhach()
	ng.kn.KhiCapNhat, ng.kn.KhiTin, ng.kn.KhiDong = ng.capNhat, ng.tinKhac, func() { ng.log("Mất kết nối máy chủ") }
	if _, err := ng.kn.KetNoi(host, cong, ng.ten, "nguoi_dung", 0, 5); err != nil {
		return err
	}
	tt := ng.kn.TrangThai()
	ng.tt = tt
	sp, _ := tt["sp"].(tc.M)
	ng.log(fmt.Sprintf("Vào trang sale (%s): %s", tc.Chuoi(tt["che_do"]), tc.Chuoi(sp["ten"])))
	ng.log(fmt.Sprintf("  %s -> %s, kho %d/%d", tc.Tien(tc.SoNguyen(sp["gia_goc"])), tc.Tien(tc.SoNguyen(sp["gia_sale"])),
		tc.SoNguyen(tt["ton"]), tc.SoNguyen(tt["ton_dau"])))
	if mo, _ := tt["mo_ban"].(bool); !mo {
		ng.log(fmt.Sprintf("  mở bán sau %.1f s", tc.SoThuc(tt["mo_con_ms"])/1000))
	}
	ng.kn.BatDauDoc()
	return nil
}

// ------------------------------------------------------------ nhận

func (ng *nguoiMua) capNhat(tt tc.M) {
	ng.khoa.Lock()
	cu := ng.tt
	ng.tt = tt
	ng.khoa.Unlock()
	if tc.SoNguyen(tt["ton"]) == 0 && tc.SoNguyen(cu["ton"]) != 0 {
		ng.log("  kho: HẾT HÀNG")
	}
}

func (ng *nguoiMua) tinKhac(tin tc.M) {
	if tin["loai"] == "su_kien" {
		ng.log("* " + tc.Chuoi(tin["noi_dung"]))
	}
}

// ------------------------------------------------------------ gửi

func (ng *nguoiMua) mua(sl int64) {
	ng.log(fmt.Sprintf("> MUA NGAY %d sản phẩm", sl))
	_, err := ng.kn.GuiYC("mua", func(tin tc.M, rtt float64) {
		if ok, _ := tin["ok"].(bool); ok {
			ng.log(fmt.Sprintf("  ĐẶT HÀNG OK: đơn %s, %s (%.1f ms)", tc.Chuoi(tin["ma_don"]),
				tc.Tien(tc.SoNguyen(tin["thanh_tien"])), rtt))
		} else {
			ng.log("  TỪ CHỐI: " + tc.StrPy(tin["ly_do"]))
		}
	}, tc.T("so_luong", sl))
	if err != nil {
		ng.log("lỗi: " + err.Error())
	}
}

func (ng *nguoiMua) lenh(dong string) {
	p := strings.Fields(dong)
	switch {
	case p[0] == "mua" && len(p) <= 2:
		sl := int64(1)
		if len(p) == 2 {
			n, err := strconv.ParseInt(p[1], 10, 64)
			if err != nil {
				ng.log(fmt.Sprintf("lỗi: invalid literal for int() with base 10: '%s'", p[1]))
				return
			}
			sl = n
		}
		ng.mua(sl)
	case p[0] == "thoat":
		ng.kn.TatBaoDong()
		ng.kn.Dong()
	default:
		ng.log("lệnh: mua [n] | thoat")
	}
}

func chayNguoiDung(a []string) {
	pos, con := viTri(a, 3)
	fs := boTuyChon("flash_sale")
	thoiGian := fs.Float64("thoi-gian", 0, "số giây chạy (0 = tới khi hết stdin)")
	if err := fs.Parse(con); err != nil || len(pos) < 1 {
		fmt.Println("Dùng: out/app flash_sale <tên> [host] [cổng] [--thoi-gian 0]")
		os.Exit(2)
	}
	t0 := time.Now()
	ng := &nguoiMua{ten: pos[0]}
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
		time.Sleep(500 * time.Millisecond) // chờ trả lời của lệnh cuối
	}
	ng.kn.TatBaoDong()
	ng.kn.Dong()
}
