package main

// Dịch vụ thử (máy chủ phụ trợ) cho các vai trò luong, hang-doi. Giao thức giống bản Python:
//
//	VIEC <mã> <ms>   → chờ <ms> rồi trả "XONG <mã>"
//	PHAT <n>         → gửi n dòng "GOI <k> <dữ liệu>", đọc n dòng "KQ <k> <tổng byte>", trả "NHAN <đúng>/<n>"
//	THONGKE          → trả "TK <số yêu cầu VIEC> <đồng thời tối đa>", in tóm tắt "đợt", đặt lại

import (
	"bufio"
	"fmt"
	"net"
	"strconv"
	"strings"
	"sync"
	"time"
)

// duLieu – dữ liệu của gói thứ k (giống nhau ở mọi bản ngôn ngữ).
func duLieu(k int) string { return strings.Repeat(fmt.Sprintf("goi-so-%d-", k), 3) }

// tongByte – kết quả xử lý một gói: tổng giá trị các byte UTF-8, lấy dư 65536.
func tongByte(s string) int {
	t := 0
	for _, b := range []byte(s) {
		t += int(b)
	}
	return t % 65536
}

type thongKe struct {
	mu                  sync.Mutex
	tong, dangChay, max int
}

func (t *thongKe) vao() {
	t.mu.Lock()
	t.tong++
	t.dangChay++
	t.max = max(t.max, t.dangChay)
	t.mu.Unlock()
}

func (t *thongKe) ra() { t.mu.Lock(); t.dangChay--; t.mu.Unlock() }

func (t *thongKe) layVaDatLai() (int, int) {
	t.mu.Lock()
	defer t.mu.Unlock()
	a, b := t.tong, t.max
	t.tong, t.max = 0, 0
	return a, b
}

func phucVuDichVu(conn net.Conn, tk *thongKe) {
	defer conn.Close()
	r := bufio.NewReader(conn)
	w := bufio.NewWriter(conn)
	defer w.Flush()
	line, _ := r.ReadString('\n')
	p := strings.Fields(line)
	if len(p) == 0 {
		return
	}
	switch p[0] {
	case "VIEC":
		ms, _ := strconv.Atoi(p[2])
		tk.vao()
		time.Sleep(time.Duration(ms) * time.Millisecond)
		tk.ra()
		fmt.Fprintf(w, "XONG %s\n", p[1])
	case "PHAT":
		n, _ := strconv.Atoi(p[1])
		for k := 1; k <= n; k++ {
			fmt.Fprintf(w, "GOI %d %s\n", k, duLieu(k))
		}
		w.Flush()
		dung := 0
		for range n {
			l, _ := r.ReadString('\n')
			kq := strings.Fields(l)
			if len(kq) == 3 {
				k, _ := strconv.Atoi(kq[1])
				v, _ := strconv.Atoi(kq[2])
				if v == tongByte(duLieu(k)) {
					dung++
				}
			}
		}
		fmt.Fprintf(w, "NHAN %d/%d\n", dung, n)
		say("[dịch vụ] PHAT %d: nhận %d kết quả, đúng %d/%d", n, n, dung, n)
	case "THONGKE":
		tong, mx := tk.layVaDatLai()
		fmt.Fprintf(w, "TK %d %d\n", tong, mx)
		say("[dịch vụ] đợt: %d yêu cầu VIEC, đồng thời tối đa %d", tong, mx)
	}
}

func runDichVu(host, port string) {
	ln, err := net.Listen("tcp", net.JoinHostPort(host, port))
	if err != nil {
		say("Lỗi: %v", err)
		return
	}
	say("[dịch vụ] lắng nghe %s:%s (mỗi kết nối một goroutine)", host, port)
	tk := &thongKe{}
	for {
		conn, err := ln.Accept()
		if err != nil {
			return
		}
		go phucVuDichVu(conn, tk)
	}
}
