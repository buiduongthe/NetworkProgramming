// Bài mở rộng MR6.9 – Quay số trúng thưởng: 100 000 lượt quay tranh 10 món quà. Máy chủ TCP đa luồng
// (một goroutine mỗi kết nối), 4 cách xử lý tranh chấp (không khóa, khóa loại trừ THEO TÀI NGUYÊN – ô
// kho quà và ô của từng người, lạc quan phiên bản + CAS, hàng đợi một goroutine), mỗi người quay đúng
// một lần (idempotency theo tên), sổ kiểm toán (thừa quà, quay 2 lần), bộ mô phỏng tải có rào xuất phát
// và pipelining (100 000 người trên 400 kết nối).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng). Giao thức giống hệt bản
// Python (và C#, Java, Rust), nên máy chủ, bộ mô phỏng và người dùng viết bằng các ngôn ngữ khác nhau
// chạy chung được. Lõi nằm ở gói con tranhchap/ (dùng chung với ứng dụng giao diện ở gui/).
//
//	out/app server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-qua 10] [--p 0.0005]
//	out/app mo_phong [host] [cổng] [--n 100000] [--k 200] [--lan 1] [--che-do khoa] [--dat-lai]
//	                 [--so-sanh] [--tre-ms 2]
//	out/app khach <host> <cổng> che_do <x> | tre <ms> | dat_lai k=v … | thong_ke     (quản trị)
//	out/app quay_so <tên> [host] [cổng] [--thoi-gian 0]                             (người quay)
//
// Lệnh (stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chương trình chạy):
//
//	server:  che_do khong_khoa|khoa|lac_quan|hang_doi, tre <ms>, dat_lai [k=v …], thong_ke, thoat
//	quay_so: quay, quay x2, gui_lai [k], thoat
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

const congMacDinh = 41735

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
	case "quay_so":
		chayNguoiDung(a[2:])
	default:
		cachDung()
	}
}

func cachDung() {
	fmt.Println("Cách dùng:")
	fmt.Println("  out/app server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-qua 10] [--p 0.0005]")
	fmt.Println("  out/app mo_phong [host] [cổng] [--n 200] [--k 200] [--lan 1] [--che-do x] [--dat-lai]")
	fmt.Println("                   [--so-sanh] [--tre-ms 2]")
	fmt.Println("  out/app khach <host> <cổng> che_do <x> | tre <ms> | dat_lai k=v … | thong_ke")
	fmt.Println("  out/app quay_so <tên> [host] [cổng] [--thoi-gian 0]")
	os.Exit(1)
}
