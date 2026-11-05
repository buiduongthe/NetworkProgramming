// Bài mở rộng MR6.13 – Bán vé concert có PHÒNG CHỜ ẢO: 500 vé cho rất nhiều người cùng lúc. Máy chủ TCP
// đa luồng (một goroutine mỗi kết nối), 4 cách xử lý tranh chấp (không khóa, khóa loại trừ, lạc quan
// phiên bản + CAS, hàng đợi một goroutine), phòng chờ cấp lượt theo THÙNG TOKEN (token bucket) với yêu
// cầu "chờ dài" (long polling), lượt mua có hạn, giới hạn số yêu cầu mua đang xử lý (quá tải -> từ chối),
// sổ kiểm toán (vượt vé, sai lượt, đảo số thứ tự), bộ mô phỏng tải có rào xuất phát và pipelining, so
// sánh bật/tắt phòng chờ.
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng). Giao thức giống hệt bản
// Python (và C#, Java, Rust), nên máy chủ, bộ mô phỏng và người dùng viết bằng các ngôn ngữ khác nhau
// chạy chung được. Lõi nằm ở gói con tranhchap/ (dùng chung với ứng dụng giao diện ở gui/).
//
//	out/app server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--tong-ve 500] [--toc-do 50]
//	               [--dung-luong 50] [--luot-s 20] [--gioi-han 64] [--tat-phong-cho]
//	out/app mo_phong [host] [cổng] [--n 200] [--k 200] [--lan 1] [--che-do khoa] [--dat-lai]
//	                 [--so-sanh] [--tre-ms 2]
//	out/app so_sanh_phong_cho [host] [cổng] [--n 1000] [--k 200] [--che-do khoa]
//	out/app khach <host> <cổng> che_do <x> | tre <ms> | dat_lai k=v … | thong_ke     (quản trị)
//	out/app mua_ve <tên> [host] [cổng] [--tu-dong] [--so-ve 2] [--thoi-gian 0]      (người mua)
//
// Lệnh (stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chương trình chạy):
//
//	server:  che_do khong_khoa|khoa|lac_quan|hang_doi, tre <ms>, toc_do <người/s>, dat_lai [k=v …],
//	         thong_ke, thoat
//	mua_ve:  xep_hang, mua <n>, mua2 <n> (bấm đúp), thoat
package main

import (
	"flag"
	"fmt"
	"io"
	"os"
	"strconv"
	"strings"
	"sync"
)

const congMacDinh = 41775

var khoaIn sync.Mutex

// inDong – nhiều goroutine cùng in: mỗi dòng in trọn vẹn.
func inDong(tienTo, text string) {
	khoaIn.Lock()
	defer khoaIn.Unlock()
	fmt.Printf("[%s] %s\n", tienTo, text)
}

// viTri – tách các tham số vị trí đứng đầu (host, cổng…) khỏi phần tùy chọn "--x".
func viTri(a []string, toiDa int) ([]string, []string) {
	i := 0
	for i < len(a) && i < toiDa && !strings.HasPrefix(a[i], "-") {
		i++
	}
	return a[:i], a[i:]
}

func lay(ds []string, i int, macDinh string) string {
	if i < len(ds) {
		return ds[i]
	}
	return macDinh
}

func soCong(s string) int {
	n, err := strconv.Atoi(s)
	if err != nil {
		fmt.Println("cổng không hợp lệ:", s)
		os.Exit(2)
	}
	return n
}

func boTuyChon(ten string) *flag.FlagSet {
	fs := flag.NewFlagSet(ten, flag.ExitOnError)
	fs.SetOutput(io.Discard)
	fs.Usage = func() {}
	return fs
}

func main() {
	a := os.Args
	if len(a) < 2 {
		cachDung()
	}
	switch a[1] {
	case "server":
		chayServer(a[2:])
	case "mo_phong":
		chayMoPhong(a[2:])
	case "khach":
		chayQuanTri(a[2:])
	case "mua_ve":
		chayNguoiDung(a[2:])
	case "so_sanh_phong_cho":
		chaySoSanhPhongCho(a[2:])
	default:
		cachDung()
	}
}

func cachDung() {
	fmt.Println("Cách dùng:")
	fmt.Println("  out/app server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--tong-ve 500] [--toc-do 50]")
	fmt.Println("                 [--dung-luong 50] [--luot-s 20] [--gioi-han 64] [--tat-phong-cho]")
	fmt.Println("  out/app mo_phong [host] [cổng] [--n 200] [--k 200] [--lan 1] [--che-do x] [--dat-lai]")
	fmt.Println("                   [--so-sanh] [--tre-ms 2]")
	fmt.Println("  out/app khach <host> <cổng> che_do <x> | tre <ms> | dat_lai k=v … | thong_ke")
	fmt.Println("  out/app so_sanh_phong_cho [host] [cổng] [--n 1000] [--k 200] [--che-do khoa]")
	fmt.Println("  out/app mua_ve <tên> [host] [cổng] [--tu-dong] [--so-ve 2] [--thoi-gian 0]")
	os.Exit(1)
}
