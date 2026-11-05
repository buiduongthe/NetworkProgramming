package main

// Sinh viên đăng ký học phần (bản dòng lệnh). Kết nối máy chủ, in bảng lớp (mã lớp, tên, đã đăng ký/sĩ
// số) và nhận lệnh:
//
//	dk <mã lớp>       đăng ký lớp            dk <mã lớp> x2    "bấm đúp": gửi HAI yêu cầu liên tiếp không chờ
//	huy <mã lớp>      hủy lớp đã đăng ký     xem               in lại bảng lớp            thoat
//
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
//	out/app dang_ky <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.10_DangKyHocPhan/tranhchap"
)

type sinhVien struct {
	ten  string
	khoa sync.Mutex
	tt   tc.M
	kn   *tc.KetNoiKhach
}

func (sv *sinhVien) log(text string) { inDong(sv.ten, text) }

func (sv *sinhVien) trangThai() tc.M {
	sv.khoa.Lock()
	defer sv.khoa.Unlock()
	return sv.tt
}

func (sv *sinhVien) ketNoi(host string, cong int) error {
	sv.kn = tc.MoiKetNoiKhach()
	sv.kn.KhiCapNhat, sv.kn.KhiTin, sv.kn.KhiDong = sv.capNhat, sv.tinKhac, func() { sv.log("Mất kết nối máy chủ") }
	if _, err := sv.kn.KetNoi(host, cong, sv.ten, "nguoi_dung", 0, 5); err != nil {
		return err
	}
	sv.tt = sv.kn.TrangThai()
	sv.log(fmt.Sprintf("Vào cổng đăng ký (%s), sĩ số %d", tc.Chuoi(sv.tt["che_do"]), tc.SoNguyen(sv.tt["si_so"])))
	sv.xem()
	sv.kn.BatDauDoc()
	return nil
}

func (sv *sinhVien) xem() {
	for _, x := range tc.DSM(sv.trangThai()["lop"]) {
		cuaToi := ""
		for _, t := range tc.DSChuoi(x["sv"]) {
			if t == sv.ten {
				cuaToi = " *"
			}
		}
		sv.log(fmt.Sprintf("  %s %-15s %3d/%d%s", tc.Chuoi(x["ma"]), tc.Chuoi(x["ten"]), tc.SoNguyen(x["da_dk"]),
			tc.SoNguyen(x["si_so"]), cuaToi))
	}
}

// ------------------------------------------------------------ nhận

func (sv *sinhVien) capNhat(tt tc.M) {
	sv.khoa.Lock()
	sv.tt = tt
	sv.khoa.Unlock()
}

func (sv *sinhVien) tinKhac(tin tc.M) {
	if tin["loai"] == "su_kien" {
		sv.log("* " + tc.Chuoi(tin["noi_dung"]))
	}
}

// ------------------------------------------------------------ gửi

func (sv *sinhVien) gui(loai, ma string) {
	_, err := sv.kn.GuiYC(loai, func(tin tc.M, rtt float64) {
		if ok, _ := tin["ok"].(bool); ok {
			viec := "HỦY"
			if loai == "dang_ky" {
				viec = "ĐĂNG KÝ"
			}
			sv.log(fmt.Sprintf("  %s %s thành công (%d/%d, %.1f ms)", viec, ma, tc.SoNguyen(tin["da_dk"]),
				tc.SoNguyen(sv.trangThai()["si_so"]), rtt))
		} else {
			sv.log(fmt.Sprintf("  %s: TỪ CHỐI – %s", ma, tc.StrPy(tin["ly_do"])))
		}
	}, tc.T("ma_lop", ma))
	if err != nil {
		sv.log("lỗi: " + err.Error())
	}
}

func (sv *sinhVien) lenh(dong string) {
	p := strings.Fields(dong)
	switch {
	case p[0] == "dk" && len(p) == 2:
		sv.log("> đăng ký " + p[1])
		sv.gui("dang_ky", p[1])
	case p[0] == "dk" && len(p) == 3 && p[2] == "x2":
		sv.log("> đăng ký " + p[1] + " (bấm đúp: 2 yêu cầu)")
		sv.gui("dang_ky", p[1])
		sv.gui("dang_ky", p[1])
	case p[0] == "huy" && len(p) == 2:
		sv.log("> hủy " + p[1])
		sv.gui("huy", p[1])
	case p[0] == "xem":
		sv.xem()
	case p[0] == "thoat":
		sv.kn.TatBaoDong()
		sv.kn.Dong()
	default:
		sv.log("lệnh: dk <mã> [x2] | huy <mã> | xem | thoat")
	}
}

func chayNguoiDung(a []string) {
	pos, con := viTri(a, 3)
	fs := boTuyChon("dang_ky")
	thoiGian := fs.Float64("thoi-gian", 0, "số giây chạy (0 = tới khi hết stdin)")
	if err := fs.Parse(con); err != nil || len(pos) < 1 {
		fmt.Println("Dùng: out/app dang_ky <tên> [host] [cổng] [--thoi-gian 0]")
		os.Exit(2)
	}
	t0 := time.Now()
	sv := &sinhVien{ten: pos[0]}
	if err := sv.ketNoi(lay(pos, 1, "127.0.0.1"), soCong(lay(pos, 2, strconv.Itoa(congMacDinh)))); err != nil {
		sv.log("Không kết nối được: " + err.Error())
		os.Exit(1)
	}
	hetStdin := make(chan struct{})
	go tc.DocLenhStdin(t0, sv.lenh, func() { close(hetStdin) })
	if *thoiGian > 0 { // chạy đúng --thoi-gian giây (hoặc tới khi thoat)
		for sv.kn.Mo() && time.Since(t0).Seconds() < *thoiGian {
			time.Sleep(100 * time.Millisecond)
		}
	} else { // tới khi hết stdin / thoat
	cho:
		for sv.kn.Mo() {
			select {
			case <-hetStdin:
				break cho
			case <-time.After(100 * time.Millisecond):
			}
		}
		time.Sleep(500 * time.Millisecond) // chờ trả lời của lệnh cuối
	}
	sv.kn.TatBaoDong()
	sv.kn.Dong()
}
