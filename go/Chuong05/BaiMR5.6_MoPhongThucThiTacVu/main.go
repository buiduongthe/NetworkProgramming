// Bài mở rộng MR5.6 – Mô phỏng thực thi tác vụ (bản dòng lệnh; bản giao diện Fyne: thư mục gui/).
//
// Bài mở rộng, không có trong giáo trình; minh họa Chương 5 (luồng, bất đồng bộ) và Chương 6 (máy chủ đa
// luồng, nhóm luồng): cùng ba tác vụ T1, T2, T3 (CPU → chờ I/O → CPU) và một việc khẩn U, chạy theo bốn chế
// độ đơn luồng/đa luồng × đồng bộ/bất đồng bộ trên bộ lập lịch mô phỏng (gói laplich). Chỉ thư viện chuẩn.
//
// Cách dùng:
//
//	go run . <chế_độ> [--t1 4] [--t2 3] [--t3 2] [--loi 2] [--lat 0.5] [--khan 0.3]
//	         [--khong-khan] [--khong-phu-thuoc] [--khong-chia]
//	chế_độ: don_dong_bo | da_dong_bo | don_bat_dong_bo | da_bat_dong_bo | so_sanh | tat_ca
package main

import (
	"flag"
	"fmt"
	"os"
	"slices"

	L "github.com/buiduongthe/NetworkProgramming/go/Chuong05/BaiMR5.6_MoPhongThucThiTacVu/laplich"
)

var nhanXet = map[string][]string{
	"don_dong_bo": {"Nhận xét: một luồng, lời gọi chặn: khi chờ I/O luồng đứng",
		"  yên (~), lõi CPU bỏ không; tác vụ làm lần lượt theo thứ tự",
		"  đến nên việc khẩn U phải chờ tất cả."},
	"da_dong_bo": {"Nhận xét: mỗi tác vụ một luồng: I/O của luồng này chồng lên",
		"  CPU của luồng khác; luồng nhiều hơn lõi phải chờ lõi (-);",
		"  hệ điều hành chia lát thời gian và cho U (ưu tiên cao) chen",
		"  ngang ngay; T3 chờ join (.); nửa sau của T1 chạy song song",
		"  trên luồng T1b."},
	"don_bat_dong_bo": {"Nhận xét: một luồng vẫn chồng được các lần chờ I/O (hàng",
		"  I/O chờ có nhiều việc cùng lúc) mà không cần thêm luồng;",
		"  nhưng bước CPU chạy hết mới nhường (hợp tác): U phải xếp",
		"  hàng, hai nửa của T1 chạy lần lượt – bất đồng bộ không",
		"  phải là song song."},
	"da_bat_dong_bo": {"Nhận xét: nhóm thợ + bất đồng bộ: I/O không giữ luồng, bước",
		"  CPU giao cho thợ rảnh nên một tác vụ có thể chạy tiếp trên",
		"  luồng khác sau khi chờ I/O; hai nửa của T1 chạy song song",
		"  trên 2 thợ; số luồng chỉ bằng số lõi."},
}

func inThamSo(ts *L.ThamSo) {
	fmt.Printf("Tác vụ: T1 %s s, T2 %s s, T3 %s s; mỗi tác vụ: CPU 1/4 →\n", L.Giay(ts.D[0]), L.Giay(ts.D[1]), L.Giay(ts.D[2]))
	fmt.Printf("  chờ I/O 1/2 → CPU phần còn lại; %d lõi CPU; lát thời gian %s s\n", ts.SoLoi, L.Giay(ts.Lat))
	var them []string
	if ts.PhuThuoc {
		them = append(them, "T3 chờ T1, T2")
	}
	if ts.Chia {
		them = append(them, "CPU cuối T1 chia 2 nửa")
	}
	if ts.Khan {
		them = append(them, fmt.Sprintf("việc khẩn U lúc %s s", L.Giay(ts.TKhan)))
	}
	if len(them) > 0 {
		s := them[0]
		for _, x := range them[1:] {
			s += "; " + x
		}
		fmt.Println("  " + s)
	} else {
		fmt.Println("  không phụ thuộc, không chia, không việc khẩn")
	}
}

func chayMot(cheDo string, ts *L.ThamSo) {
	fmt.Printf("=== %s ===\n", L.CheDo[cheDo])
	inThamSo(ts)
	kq := L.Chay(cheDo, ts)
	fmt.Println()
	fmt.Println("Ký hiệu: 1 2 3 U tác vụ đang dùng CPU; ~ luồng bị chặn chờ I/O;")
	fmt.Println("  - sẵn sàng nhưng chờ lõi; . chờ join; I/O chờ: số I/O bất đồng")
	fmt.Println("  bộ đang chờ (không giữ luồng nào)")
	for _, d := range L.Gantt(kq, 48) {
		fmt.Println(d)
	}
	fmt.Println()
	fmt.Println("Nhật ký:")
	for _, d := range kq.NhatKy {
		fmt.Printf("  %s s  %s\n", L.Phai(L.Giay(d.T), 4), d.Text)
	}
	fmt.Println()
	fmt.Println("  Tác vụ   Đến  Bắt đầu   Xong  Thời gian")
	for _, v := range kq.Viec {
		fmt.Printf("  %s %s %s %s %s s\n", L.Trai(v.Ten, 6), L.Phai(L.Giay(v.Den), 5), L.Phai(L.Giay(v.BatDau), 8),
			L.Phai(L.Giay(v.Xong), 6), L.Phai(L.Giay(v.Xong-v.Den), 9))
	}
	s := L.TinhChiSo(kq)
	fmt.Printf("  Hoàn tất lúc %s s; số luồng %d; lõi bận %d %%\n", L.Giay(s.Tong), s.Luong, s.Ban)
	dong := fmt.Sprintf("  Chen ngang %d lần; hết lát thời gian %d lần", s.Chen, s.Lat)
	if s.UCho >= 0 {
		dong += fmt.Sprintf("; U chờ %s s", L.Giay(s.UCho))
	}
	fmt.Println(dong)
	fmt.Println()
	for _, d := range nhanXet[cheDo] {
		fmt.Println(d)
	}
	fmt.Println()
}

func soSanh(ts *L.ThamSo) {
	fmt.Println("=== So sánh bốn chế độ ===")
	inThamSo(ts)
	fmt.Println()
	fmt.Printf("  %s %s %s %s %s %s %s\n", L.Trai("Chế độ", 14), L.Phai("Hoàn tất", 8), L.Phai("Luồng", 5),
		L.Phai("Lõi bận", 7), L.Phai("U chờ", 6), L.Phai("Chen", 4), L.Phai("Hết lát", 7))
	for _, m := range L.DanhSachCheDo {
		s := L.TinhChiSo(L.Chay(m, ts))
		u := "–"
		if s.UCho >= 0 {
			u = L.Giay(s.UCho) + " s"
		}
		fmt.Printf("  %s %s %s %s %s %s %s\n", L.Trai(L.TenNgan[m], 14), L.Phai(L.Giay(s.Tong)+" s", 8),
			L.Phai(fmt.Sprint(s.Luong), 5), L.Phai(fmt.Sprintf("%d %%", s.Ban), 7), L.Phai(u, 6),
			L.Phai(fmt.Sprint(s.Chen), 4), L.Phai(fmt.Sprint(s.Lat), 7))
	}
	fmt.Println()
	fmt.Println("Ghi nhớ: đa luồng hệ điều hành chen ngang được (ưu tiên, lát")
	fmt.Println("  thời gian) nhưng tốn luồng; bất đồng bộ dùng ít luồng, chồng")
	fmt.Println("  được I/O nhưng chỉ nhường ở điểm await; nhóm thợ + bất đồng")
	fmt.Println("  bộ dùng hết các lõi mà số luồng vẫn bằng số lõi.")
}

func main() {
	fs := flag.NewFlagSet("mophong", flag.ExitOnError)
	fs.Usage = func() {
		fmt.Fprintln(os.Stderr, "Mô phỏng thực thi tác vụ: đơn/đa luồng × đồng bộ/bất đồng bộ")
		fmt.Fprintln(os.Stderr, "Cách dùng: go run . <don_dong_bo|da_dong_bo|don_bat_dong_bo|da_bat_dong_bo|so_sanh|tat_ca> [tùy chọn]")
		fs.PrintDefaults()
	}
	t1 := fs.Float64("t1", 4.0, "thời lượng T1 (giây)")
	t2 := fs.Float64("t2", 3.0, "thời lượng T2 (giây)")
	t3 := fs.Float64("t3", 2.0, "thời lượng T3 (giây)")
	loi := fs.Int("loi", 2, "số lõi CPU")
	lat := fs.Float64("lat", 0.5, "lát thời gian (giây)")
	khan := fs.Float64("khan", 0.3, "lúc việc khẩn U đến (giây)")
	khongKhan := fs.Bool("khong-khan", false, "không có việc khẩn U")
	khongPhuThuoc := fs.Bool("khong-phu-thuoc", false, "T3 không chờ T1, T2")
	khongChia := fs.Bool("khong-chia", false, "không chia CPU cuối của T1")
	// chế độ (tham số vị trí) có thể đứng trước hoặc sau các cờ, như argparse của Python
	var viTri []string
	args := os.Args[1:]
	for {
		fs.Parse(args)
		if fs.NArg() == 0 {
			break
		}
		viTri = append(viTri, fs.Arg(0))
		args = fs.Args()[1:]
	}
	chon := append(slices.Clone(L.DanhSachCheDo), "so_sanh", "tat_ca")
	if len(viTri) != 1 || !slices.Contains(chon, viTri[0]) {
		fs.Usage()
		os.Exit(2)
	}
	ts := L.TaoThamSo(*t1, *t2, *t3, *loi, *lat, !*khongPhuThuoc, !*khongChia, !*khongKhan, *khan)
	switch viTri[0] {
	case "so_sanh":
		soSanh(ts)
	case "tat_ca":
		for _, m := range L.DanhSachCheDo {
			chayMot(m, ts)
		}
		soSanh(ts)
	default:
		chayMot(viTri[0], ts)
	}
}
