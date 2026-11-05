package main

// (c) Trạng thái dùng chung giữa các goroutine xử lý kết nối; (f) bế tắc.
//
// (c) Server đếm chạy ngay trong chương trình: 4 goroutine "gửi-k" mở 4 kết nối TCP, mỗi kết nối gửi 2000
//
//	dòng "DEM GET"/"DEM PUT"; mỗi kết nối một goroutine "xử-lý-k" cập nhật bốn bộ đếm dùng chung:
//	1. không khóa: đọc → runtime.Gosched() (nhường CPU, giống ghi nhật ký giữa chừng) → ghi → mất cập nhật
//	   (đây là data race thật: `go run -race .` báo lỗi, cần cgo);
//	2. sync.Mutex quanh đúng đoạn đọc – nhường – ghi; 3. atomic.Int64.Add;
//	4. sync.Map (bảng đồng thời) với giá trị *atomic.Int64 theo loại thông điệp GET/PUT.
//
// (f) "chuyển-AB" khóa phiên A rồi B, "chuyển-BA" khóa B rồi A. sync.Mutex không có chờ có hạn, nên khóa
//
//	ở đây là kênh có bộ đệm 1 (gửi = lấy khóa, nhận = nhả khóa) để chờ có hạn bằng select + time.After.
import (
	"bufio"
	"fmt"
	"net"
	"runtime"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

const (
	nKetNoi, nTin   = 4, 2000
	giuMs, hanChoMs = 50, 500
)

type boDem struct {
	khongKhoa int64
	coKhoa    int64
	mu        sync.Mutex
	nguyenTu  atomic.Int64
	bang      sync.Map // string → *atomic.Int64
}

func (b *boDem) capNhat(loai string) {
	v := b.khongKhoa  // 1. đọc
	runtime.Gosched() //    nhường CPU: goroutine khác có thể chen vào đây
	b.khongKhoa = v + 1
	b.mu.Lock() // 2. đoạn găng có khóa
	v = b.coKhoa
	runtime.Gosched()
	b.coKhoa = v + 1
	b.mu.Unlock()
	b.nguyenTu.Add(1) // 3. đọc – cộng – ghi trong một lệnh CPU
	c, _ := b.bang.LoadOrStore(loai, new(atomic.Int64))
	c.(*atomic.Int64).Add(1) // 4. bảng đồng thời
}

func (b *boDem) layBang(loai string) int64 {
	if c, ok := b.bang.Load(loai); ok {
		return c.(*atomic.Int64).Load()
	}
	return 0
}

func tranhChap(host, port string) {
	say("(c) Trạng thái dùng chung: %d kết nối × %d thông điệp DEM, mỗi kết nối một goroutine xử lý", nKetNoi, nTin)
	main := nk("chính")
	bd := &boDem{}
	ln, err := net.Listen("tcp", net.JoinHostPort(host, port))
	if err != nil {
		say("Lỗi: %v", err)
		return
	}
	main.Log("server đếm lắng nghe %s:%s; %d goroutine gửi bắt đầu", host, port, nKetNoi)
	var wg sync.WaitGroup
	var sb strings.Builder
	for i := range nTin {
		if i%2 == 0 {
			sb.WriteString("DEM GET\n")
		} else {
			sb.WriteString("DEM PUT\n")
		}
	}
	data := sb.String()
	for range nKetNoi {
		wg.Add(1)
		go func() { // goroutine gửi
			defer wg.Done()
			if c, err := net.Dial("tcp", net.JoinHostPort(host, port)); err == nil {
				c.Write([]byte(data))
				c.Close()
			}
		}()
	}
	for k := 1; k <= nKetNoi; k++ {
		conn, err := ln.Accept()
		if err != nil {
			break
		}
		wg.Add(1)
		go func(n *NhatKy) { // goroutine xử lý một kết nối
			defer wg.Done()
			defer conn.Close()
			sc := bufio.NewScanner(conn)
			soTin := 0
			for sc.Scan() {
				if p := strings.Fields(sc.Text()); len(p) == 2 && p[0] == "DEM" {
					bd.capNhat(p[1])
					soTin++
				}
			}
			n.Log("kết nối đóng: đã xử lý %d thông điệp", soTin)
		}(nk(fmt.Sprintf("xử-lý-%d", k)))
	}
	wg.Wait()
	ln.Close()
	dung := int64(nKetNoi * nTin)
	get, put := bd.layBang("GET"), bd.layBang("PUT")
	say("")
	say("  %-32s%9s%6s%13s", "Cách cập nhật biến chung", "Giá trị", "Đúng", "Mất cập nhật")
	hang := []struct {
		ten, gt string
		so      int64
	}{
		{"không khóa (đọc – nhường – ghi)", fmt.Sprint(bd.khongKhoa), bd.khongKhoa},
		{"khóa (mutex/lock)", fmt.Sprint(bd.coKhoa), bd.coKhoa},
		{"nguyên tử (atomic)", fmt.Sprint(bd.nguyenTu.Load()), bd.nguyenTu.Load()},
		{"bảng đồng thời: GET + PUT", fmt.Sprintf("%d+%d", get, put), get + put},
	}
	for _, h := range hang {
		say("  %-32s%9s%6d%13d", h.ten, h.gt, dung, dung-h.so)
	}
	say("Nhận xét: \"đọc – làm việc khác – ghi\" không khóa làm mất cập nhật; khóa, thao tác nguyên tử")
	say("  hoặc cấu trúc dữ liệu đồng thời đều cho kết quả đúng.")
}

// khoaKenh – khóa làm bằng kênh có bộ đệm 1, hỗ trợ chờ có hạn.
type khoaKenh chan struct{}

func (k khoaKenh) lay(han time.Duration) bool {
	select {
	case k <- struct{}{}:
		return true
	case <-time.After(han):
		return false
	}
}

func (k khoaKenh) nha() { <-k }

func beTac() {
	say("")
	say("(f) Bế tắc: hai goroutine chuyển tin giữa phiên A và phiên B, mỗi goroutine cần khóa của cả hai phiên")
	khoa := map[string]khoaKenh{"A": make(khoaKenh, 1), "B": make(khoaKenh, 1)}
	type kqLan struct {
		ten      string
		xong, bo int
	}
	var bang []kqLan
	for lan, coThuTu := range []bool{false, true} {
		if coThuTu {
			say("Lần %d – mọi goroutine lấy khóa theo cùng thứ tự (A trước B):", lan+1)
		} else {
			say("Lần %d – lấy khóa theo thứ tự ngược nhau:", lan+1)
		}
		var mu sync.Mutex
		kq := kqLan{ten: "ngược thứ tự"}
		if coThuTu {
			kq.ten = "cùng thứ tự"
		}
		chuyen := func(n *NhatKy, nguon, dich string) {
			dau, sau := nguon, dich
			if coThuTu {
				s := []string{nguon, dich}
				sort.Strings(s)
				dau, sau = s[0], s[1]
			}
			khoa[dau].lay(time.Hour)
			defer khoa[dau].nha()
			n.Log("giữ khóa phiên %s, chờ khóa phiên %s", dau, sau)
			time.Sleep(giuMs * time.Millisecond) // giả lập ghi vào socket của phiên đang giữ
			if !khoa[sau].lay(hanChoMs * time.Millisecond) {
				n.Log("chờ khóa phiên %s quá %d ms → nghi bế tắc, nhả khóa phiên %s, bỏ cuộc", sau, hanChoMs, dau)
				mu.Lock()
				kq.bo++
				mu.Unlock()
				return
			}
			n.Log("đã chuyển tin từ phiên %s sang phiên %s", nguon, dich)
			khoa[sau].nha()
			mu.Lock()
			kq.xong++
			mu.Unlock()
		}
		var wg sync.WaitGroup
		wg.Add(2)
		go func() { defer wg.Done(); chuyen(nk("chuyển-AB"), "A", "B") }()
		time.Sleep(10 * time.Millisecond)
		go func() { defer wg.Done(); chuyen(nk("chuyển-BA"), "B", "A") }()
		wg.Wait()
		bang = append(bang, kq)
	}
	say("")
	say("  %-20s%10s%24s", "Cách lấy khóa", "Hoàn tất", "Bỏ cuộc (hết hạn chờ)")
	for _, kq := range bang {
		say("  %-20s%10d%24d", kq.ten, kq.xong, kq.bo)
	}
	say("Nhận xét: cùng một thứ tự lấy khóa cho mọi goroutine loại bỏ vòng chờ – điều kiện cần của bế tắc.")
}

func runKhoa(host, port string) {
	tranhChap(host, port)
	beTac()
}
