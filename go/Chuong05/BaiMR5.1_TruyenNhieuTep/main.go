// Bài mở rộng MR5.1 – Truyền nhiều tệp đồng thời có điều khiển start/pause/resume/stop,
// giới hạn tốc độ theo phiên và theo tệp (thùng token), tải tiếp bằng offset + kiểm SHA-256.
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.5 tải tệp có tiến trình/hủy,
// Bài 5.1 khung bất đồng bộ) và Chương 3 (Bài 3.9/3.10 gửi tệp có SHA-256). Dùng goroutine + kênh.
//
//	app server <host> <port> [thư_mục_lưu]
//	app client <host> <port> [thư_mục_tải]   (giao diện dòng lệnh qua bàn phím)
//	app demo   <host> <port>                 (tự dựng server cục bộ, chạy kịch bản tự động)
package main

import (
	"bufio"
	"crypto/rand"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"
)

func main() {
	a := os.Args
	if len(a) < 4 {
		fmt.Println("Cách dùng:")
		fmt.Println("  app server <host> <port> [thư_mục_lưu]")
		fmt.Println("  app client <host> <port> [thư_mục_tải]")
		fmt.Println("  app demo   <host> <port>")
		os.Exit(1)
	}
	port, _ := strconv.Atoi(a[3])
	switch a[1] {
	case "server":
		kho := "kho_server"
		if len(a) > 4 {
			kho = a[4]
		}
		chayServer(a[2], port, kho)
	case "client":
		thuMuc := "tai_ve"
		if len(a) > 4 {
			thuMuc = a[4]
		}
		chayGiaoDien(a[2], port, thuMuc)
	case "demo":
		chayDemo(a[2], port)
	default:
		fmt.Println("Vai phải là server, client hoặc demo")
		os.Exit(1)
	}
}

// ----------------------------- Vai: giao diện dòng lệnh -----------------------------
func chayGiaoDien(host string, port int, thuMuc string) {
	q := newQuanLy(host, port, thuMuc, "kho_server")
	fmt.Println("[client] Nhập lệnh (start/pause/resume/stop/speed/status/quit).")
	sc := bufio.NewScanner(os.Stdin)
	for sc.Scan() {
		p := strings.Fields(sc.Text())
		if len(p) == 0 {
			continue
		}
		switch p[0] {
		case "start":
			if len(p) < 2 {
				fmt.Println("[client] Thiếu tham số")
				continue
			}
			huong := "download"
			if len(p) > 2 {
				huong = p[2]
			}
			rate := khongGioiHan
			if len(p) > 3 {
				rate = docSoByte(p[3])
			}
			q.batDau(p[1], huong, rate)
		case "pause":
			if len(p) > 1 {
				q.tamDung(p[1])
			}
		case "resume":
			if len(p) > 1 {
				q.taiTiep(p[1])
			}
		case "stop":
			if len(p) > 1 {
				q.dungHan(p[1])
			}
		case "speed":
			if len(p) > 1 {
				q.datTocDoPhien(docSoByte(p[1]))
			}
		case "status":
			q.trangThai()
		case "quit", "exit":
			return
		default:
			fmt.Println("[client] Lệnh lạ")
		}
	}
}

// ----------------------------- Vai: demo tự động -----------------------------
func taoTep(path string, n int) {
	f, _ := os.Create(path)
	defer f.Close()
	buf := make([]byte, 1024*1024)
	for i := 0; i < n; i++ {
		rand.Read(buf)
		f.Write(buf)
	}
}

func choDen(dieuKien func() bool, han time.Duration) {
	het := time.Now().Add(han)
	for time.Now().Before(het) {
		if dieuKien() {
			return
		}
		time.Sleep(100 * time.Millisecond)
	}
}

func chayDemo(host string, port int) {
	goc, _ := os.MkdirTemp("", "mr51_")
	kho := filepath.Join(goc, "kho_server")
	tai := filepath.Join(goc, "tai_ve")
	nguon := filepath.Join(goc, "nguon")
	for _, d := range []string{kho, tai, nguon} {
		os.MkdirAll(d, 0o755)
	}

	ln, err := net.Listen("tcp", fmt.Sprintf("%s:%d", host, port))
	if err != nil {
		fmt.Println("Không mở được listener:", err)
		os.Exit(1)
	}
	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			go phucVu(conn, kho)
		}
	}()
	fmt.Printf("[demo] Server cục bộ tại %s:%d, thư mục tạm %s\n", host, port, goc)

	cacTep := []string{"tep1.bin", "tep2.bin", "tep3.bin"}
	for _, ten := range cacTep {
		taoTep(filepath.Join(kho, ten), 3)
	}
	fmt.Printf("[demo] Đã tạo %d tệp thử, mỗi tệp 3 MiB\n", len(cacTep))

	q := newQuanLy(host, port, tai, kho)

	dangChay := true
	go func() {
		truocByte := map[string]int64{}
		truocTG := map[string]time.Time{}
		for dangChay {
			time.Sleep(800 * time.Millisecond)
			q.mu.Lock()
			snap := make([]*LuotTruyen, 0, len(q.luot))
			for _, t := range q.luot {
				snap = append(snap, t)
			}
			q.mu.Unlock()
			for _, t := range snap {
				if t.tt() != ttChay || t.tong == 0 {
					continue
				}
				now := time.Now()
				pb, ok := truocByte[t.ten]
				pt := truocTG[t.ten]
				if !ok || t.daLam() < pb {
					pb, pt = 0, t.t0
				}
				toc := float64(t.daLam()-pb) / maxF(now.Sub(pt).Seconds(), 0.001)
				if toc < 0 {
					toc = 0
				}
				truocByte[t.ten] = t.daLam()
				truocTG[t.ten] = now
				pct := 100 * float64(t.daLam()) / float64(t.tong)
				fmt.Printf("[tiến trình] %s %s %5.1f%%  %s/s\n", t.ten, thanhTienDo(pct), pct, dinhDangByte(int64(toc)))
			}
		}
	}()

	fmt.Println("\n===== KỊCH BẢN: tải xuống 3 tệp đồng thời, giới hạn phiên 2 MiB/s =====")
	q.datTocDoPhien(docSoByte("2M"))
	for _, ten := range cacTep {
		q.batDau(ten, "download", khongGioiHan)
	}

	time.Sleep(1200 * time.Millisecond)
	q.tamDung("tep2.bin")
	time.Sleep(800 * time.Millisecond)
	q.dungHan("tep3.bin")
	time.Sleep(300 * time.Millisecond)
	fmt.Println("\n===== Nâng tốc độ phiên lên 8 MiB/s, tải tiếp tep2, chạy lại tep3 =====")
	q.datTocDoPhien(docSoByte("8M"))
	q.taiTiep("tep2.bin")
	q.batDau("tep3.bin", "download", khongGioiHan)

	choDen(func() bool {
		for _, ten := range cacTep {
			s := q.layLuot(ten).tt()
			if s != ttXong && s != ttLoi {
				return false
			}
		}
		return true
	}, 30*time.Second)
	q.trangThai()

	fmt.Println("\n===== KỊCH BẢN upload: đẩy tep1 (đã tải) lên server, giới hạn tệp 1 MiB/s =====")
	dich := filepath.Join(nguon, "tep1_tai_len.bin")
	saoChep(filepath.Join(tai, "tep1.bin"), dich)
	t := &LuotTruyen{ten: "tep1_tai_len.bin", huong: "upload", cucBo: dich,
		trangThai: ttChay, bucket: newThung(docSoByte("1M")), t0: time.Now()}
	q.mu.Lock()
	q.luot["tep1_tai_len.bin"] = t
	q.mu.Unlock()
	go q.taiLen(t)
	fmt.Println("[client] BẮT ĐẦU upload tep1_tai_len.bin, giới hạn tệp 1.00 MiB/s")
	choDen(func() bool { s := t.tt(); return s == ttXong || s == ttLoi }, 20*time.Second)

	dangChay = false
	time.Sleep(200 * time.Millisecond)
	fmt.Println("\n===== TỔNG KẾT =====")
	for _, ten := range cacTep {
		tt := q.layLuot(ten)
		fmt.Printf("[demo] %s: %s, %s, SHA-256 %s\n", ten, tt.trangThai, dinhDangByte(tt.xongByte), khopHoacGach(tt.shaOK))
	}
	fmt.Printf("[demo] tep1_tai_len.bin: %s, server kiểm SHA-256 %s\n", t.trangThai, khopHoacGach(t.shaOK))
	fmt.Printf("[demo] Server còn giữ: %s\n", lietKe(kho))

	ln.Close()
	os.RemoveAll(goc)
	fmt.Printf("[demo] Đã dọn thư mục tạm %s\n", goc)
}

func saoChep(src, dst string) {
	b, _ := os.ReadFile(src)
	os.WriteFile(dst, b, 0o644)
}

func khopHoacGach(ok bool) string {
	if ok {
		return "KHỚP"
	}
	return "—"
}

func lietKe(dir string) string {
	ents, _ := os.ReadDir(dir)
	var ten []string
	for _, e := range ents {
		ten = append(ten, e.Name())
	}
	sort.Strings(ten)
	for i := range ten {
		ten[i] = "'" + ten[i] + "'"
	}
	return "[" + strings.Join(ten, ", ") + "]"
}
