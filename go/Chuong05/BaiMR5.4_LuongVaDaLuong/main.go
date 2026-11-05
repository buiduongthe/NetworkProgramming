// Bài mở rộng MR5.4 – Luồng (thread) và đa luồng trong chương trình mạng (Go: goroutine).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1–5.6) và Chương 6 (Bài 6.1, 6.2).
// Khác MR5.2 (so sánh mô hình trên một khối việc), bài này đi từng bước:
//
//	(a) tạo – chạy – chờ goroutine, truyền tham số, lấy kết quả           → luong
//	(b) server: mỗi client một goroutine / nhóm worker + hàng đợi giới hạn (503) → server, tai
//	(c) trạng thái dùng chung: tranh chấp, Mutex, atomic, sync.Map            → khoa
//	(d) nhà sản xuất – người tiêu dùng bằng kênh có bộ đệm, poison pill      → hang-doi
//	(e) semaphore (kênh có bộ đệm) giới hạn kết nối ra ngoài                  → luong
//	(f) bế tắc: chờ có hạn, thứ tự lấy khóa                                   → khoa
//	(g) nhật ký theo goroutine: Go không có thread-local → truyền *NhatKy tường minh
//
// Goroutine KHÔNG phải luồng hệ điều hành: runtime Go lập lịch M:N – nhiều goroutine (G) chạy trên vài
// luồng HĐH (M), mỗi luồng cần một "bộ xử lý logic" P; số P = GOMAXPROCS (mặc định = số CPU).
//
//	out/app dich-vu|server|luong|hang-doi|tai|khoa <host> <port>
package main

import (
	"bufio"
	"fmt"
	"net"
	"os"
	"runtime"
	"strings"
	"sync"
	"time"
)

const soSuat, nGoiRa, msGoiRa = 3, 8, 150

// goiDichVu – một lời gọi mạng kiểu chặn: kết nối, gửi một dòng, đọc một dòng trả lời.
func goiDichVu(host, port, line string) string {
	conn, err := net.DialTimeout("tcp", net.JoinHostPort(host, port), 10*time.Second)
	if err != nil {
		return "LOI " + err.Error()
	}
	defer conn.Close()
	fmt.Fprintf(conn, "%s\n", line)
	s, _ := bufio.NewReader(conn).ReadString('\n')
	return strings.TrimSpace(s)
}

// (a) tạo, chạy, chờ goroutine; tham số và kết quả
func coBan(host, port string) {
	say("(a) Tạo – chạy – chờ goroutine: 3 goroutine, mỗi goroutine nhận tham số (mã, thời gian xử lý) và trả kết quả")
	thamSo := [][2]int{{1, 300}, {2, 100}, {3, 200}}
	type ketQua struct {
		traLoi string
		ms     float64
	}
	kq := make([]ketQua, len(thamSo)) // mỗi goroutine ghi vào ô RIÊNG → không cần khóa
	var wg sync.WaitGroup             // WaitGroup = "join" cho nhiều goroutine
	chinh := nk("chính")
	chinh.Log("tạo 3 goroutine tải-1..tải-3 rồi cho chạy (go)")
	t := time.Now()
	for i, ts := range thamSo {
		wg.Add(1)
		go func(i, ma, ms int) { // tham số truyền qua đối số của hàm
			defer wg.Done()
			n := &NhatKy{Ten: fmt.Sprintf("tải-%d", ma), NguCanh: fmt.Sprintf("yc=%d", ma)}
			n.Log("kết nối dịch vụ, gửi VIEC %d %d", ma, ms)
			t := time.Now()
			tl := goiDichVu(host, port, fmt.Sprintf("VIEC %d %d", ma, ms))
			dt := float64(time.Since(t).Microseconds()) / 1000
			n.Log("nhận %s sau %.0f ms", tl, dt)
			kq[i] = ketQua{tl, dt}
		}(i, ts[0], ts[1])
	}
	wg.Wait()
	tong := time.Since(t).Seconds()
	tuanTu := 0
	for _, ts := range thamSo {
		tuanTu += ts[1]
	}
	chinh.Log("chờ (wg.Wait) xong cả 3 goroutine sau %.2f s (tuần tự sẽ ≈ %.2f s)", tong, float64(tuanTu)/1000)
	say("")
	say("  %-10s%-16s%-10s%10s", "Goroutine", "Tham số", "Kết quả", "Thời gian")
	for i, ts := range thamSo {
		say("  %-10s%-16s%-10s%7.0f ms", fmt.Sprintf("tải-%d", ts[0]), fmt.Sprintf("mã %d, %d ms", ts[0], ts[1]), kq[i].traLoi, kq[i].ms)
	}
	goiDichVu(host, port, "THONGKE") // đặt lại thống kê của dịch vụ trước phần (e)
}

// (e) semaphore giới hạn kết nối ra ngoài: kênh có bộ đệm soSuat – gửi vào = lấy suất, nhận ra = trả suất
func gioiHan(host, port string) {
	say("")
	say("(e) Giới hạn kết nối ra ngoài: %d goroutine gọi dịch vụ, tối đa %d kết nối cùng lúc (semaphore)", nGoiRa, soSuat)
	suat := make(chan struct{}, soSuat)
	var mu sync.Mutex
	dangDung, dinh := 0, 0
	var wg sync.WaitGroup
	t := time.Now()
	for k := 1; k <= nGoiRa; k++ {
		wg.Add(1)
		go func(k int) {
			defer wg.Done()
			n := &NhatKy{Ten: fmt.Sprintf("gọi-%d", k), NguCanh: fmt.Sprintf("yc=%d", k)}
			select {
			case suat <- struct{}{}: // còn suất
			default:
				n.Log("hết suất → chờ")
				suat <- struct{}{} // chặn tới khi có goroutine trả suất
			}
			mu.Lock()
			dangDung++
			d := dangDung
			dinh = max(dinh, runtime.NumGoroutine())
			mu.Unlock()
			n.Log("được suất (đang dùng %d/%d), gửi VIEC %d %d", d, soSuat, 10+k, msGoiRa)
			goiDichVu(host, port, fmt.Sprintf("VIEC %d %d", 10+k, msGoiRa))
			mu.Lock()
			dangDung--
			mu.Unlock()
			<-suat // trả suất
			n.Log("xong, trả suất")
		}(k)
	}
	wg.Wait()
	dt := time.Since(t).Seconds()
	p := strings.Fields(goiDichVu(host, port, "THONGKE"))
	dot := (nGoiRa + soSuat - 1) / soSuat
	nk("chính").Log("dịch vụ xác nhận: %s yêu cầu, đồng thời tối đa %s; tổng %.2f s (≈ ⌈%d/%d⌉ × %d ms = %.2f s)",
		p[1], p[2], dt, nGoiRa, soSuat, msGoiRa, float64(dot*msGoiRa)/1000)
	say("")
	say("(g) Nhật ký theo goroutine: mỗi dòng ghi tên goroutine; ngữ cảnh \"yc=\" nằm trong giá trị *NhatKy")
	say("    truyền cho goroutine (Go không có biến cục bộ luồng).")
	say("")
	say("Đặc thù Go: goroutine không phải luồng HĐH – runtime lập lịch M:N, GOMAXPROCS = %d, lúc cao điểm", runtime.GOMAXPROCS(0))
	say("  có %d goroutine đang sống; goroutine chờ socket không giữ luồng HĐH (netpoller). runtime.LockOSThread()", dinh)
	say("  ghim một goroutine vào luồng HĐH hiện tại – chỉ cần khi thư viện C/GUI đòi đúng một luồng.")
}

func main() {
	a := os.Args
	vai := ""
	if len(a) > 1 {
		vai = a[1]
	}
	if len(a) < 4 {
		vai = ""
	}
	switch vai {
	case "dich-vu":
		runDichVu(a[2], a[3])
	case "server":
		runServer(a[2], a[3])
	case "luong":
		coBan(a[2], a[3])
		gioiHan(a[2], a[3])
	case "hang-doi":
		runHangDoi(a[2], a[3])
	case "tai":
		runTai(a[2], a[3])
	case "khoa":
		runKhoa(a[2], a[3])
	default:
		fmt.Println("Cách dùng: out/app dich-vu|server|luong|hang-doi|tai|khoa <host> <port>")
		os.Exit(1)
	}
}
