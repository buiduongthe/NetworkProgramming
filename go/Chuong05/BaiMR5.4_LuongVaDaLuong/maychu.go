package main

// (b) Server TCP nhiều goroutine: mỗi client một goroutine và nhóm worker cố định.
//
//	cổng P   – máy chủ A: goroutine "nhận-A" Accept(), mỗi kết nối một goroutine MỚI "kết-nối-k";
//	cổng P+1 – máy chủ B: 2 goroutine worker cố định "nhóm-1", "nhóm-2" nhận việc từ kênh có bộ đệm 2;
//	           "nhận-B" gửi việc bằng select { case viec <- v: ... default: } – kênh đầy → "503 BAN <mã>".
//
// Vai trò tai: 6 goroutine "khách-k" (cách nhau 20 ms) gửi "VIEC k 300" tới từng máy chủ.

import (
	"bufio"
	"fmt"
	"net"
	"strconv"
	"strings"
	"sync"
	"time"
)

const (
	nKhach, msViec, cachMs = 6, 300, 20
	nhomB, hangDoiB        = 2, 2
)

type viecB struct {
	conn net.Conn
	r    *bufio.Reader
	p    []string
}

// xuLy – phần việc giống nhau ở hai máy chủ: chờ <ms> rồi trả lời kèm tên goroutine đang chạy.
func xuLy(n *NhatKy, conn net.Conn, p []string) {
	n.Log("xử lý VIEC %s %s", p[1], p[2])
	ms, _ := strconv.Atoi(p[2])
	time.Sleep(time.Duration(ms) * time.Millisecond)
	fmt.Fprintf(conn, "XONG %s %s\n", p[1], n.Ten)
}

func docYeuCau(r *bufio.Reader) []string {
	line, _ := r.ReadString('\n')
	p := strings.Fields(line)
	if len(p) != 3 || p[0] != "VIEC" {
		return nil
	}
	return p
}

func mayChuA(ln net.Listener) {
	n := nk("nhận-A")
	for dem := 1; ; dem++ {
		conn, err := ln.Accept()
		if err != nil {
			return
		}
		n.Log("kết nối mới → tạo goroutine kết-nối-%d", dem)
		go func(conn net.Conn, ten string) { // mỗi client một goroutine
			defer conn.Close()
			if p := docYeuCau(bufio.NewReader(conn)); p != nil {
				xuLy(nk(ten), conn, p)
			}
		}(conn, fmt.Sprintf("kết-nối-%d", dem))
	}
}

func mayChuB(ln net.Listener) {
	viec := make(chan viecB, hangDoiB) // kênh có bộ đệm = hàng đợi có giới hạn
	for i := 1; i <= nhomB; i++ {
		go func(n *NhatKy) { // worker của nhóm: lặp mãi, mỗi lần nhận một việc
			for v := range viec {
				xuLy(n, v.conn, v.p)
				v.conn.Close()
			}
		}(nk(fmt.Sprintf("nhóm-%d", i)))
	}
	n := nk("nhận-B")
	for {
		conn, err := ln.Accept()
		if err != nil {
			return
		}
		r := bufio.NewReader(conn)
		p := docYeuCau(r)
		if p == nil {
			conn.Close()
			continue
		}
		select {
		case viec <- viecB{conn, r, p}: // còn chỗ trong kênh
			n.Log("VIEC %s vào hàng đợi", p[1])
		default: // kênh đầy: không chờ, từ chối ngay
			n.Log("hàng đợi đầy (%d/%d) → trả 503 cho VIEC %s", hangDoiB, hangDoiB, p[1])
			fmt.Fprintf(conn, "503 BAN %s\n", p[1])
			conn.Close()
		}
	}
}

func runServer(host, port string) {
	pa, _ := strconv.Atoi(port)
	lnA, err1 := net.Listen("tcp", net.JoinHostPort(host, strconv.Itoa(pa)))
	lnB, err2 := net.Listen("tcp", net.JoinHostPort(host, strconv.Itoa(pa+1)))
	if err1 != nil || err2 != nil {
		say("Lỗi mở cổng: %v %v", err1, err2)
		return
	}
	say("[server] cổng %d: máy chủ A – mỗi client một goroutine", pa)
	say("[server] cổng %d: máy chủ B – nhóm %d goroutine cố định, hàng đợi tối đa %d (đầy → 503)", pa+1, nhomB, hangDoiB)
	go mayChuA(lnA)
	mayChuB(lnB)
}

// ---------------- Máy tạo tải ----------------

func khach(host string, port, k int, kq []string, wg *sync.WaitGroup) {
	defer wg.Done()
	n := nk(fmt.Sprintf("khách-%d", k))
	time.Sleep(time.Duration((k-1)*cachMs) * time.Millisecond)
	t := time.Now()
	conn, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(port)), 10*time.Second)
	if err != nil {
		n.Log("lỗi: %v", err)
		return
	}
	defer conn.Close()
	fmt.Fprintf(conn, "VIEC %d %d\n", k, msViec)
	line, _ := bufio.NewReader(conn).ReadString('\n')
	line = strings.TrimSpace(line)
	kq[k-1] = line
	dt := time.Since(t).Seconds()
	if p := strings.Fields(line); len(p) == 3 && p[0] == "XONG" {
		n.Log("XONG %d bởi %s sau %.2f s", k, p[2], dt)
	} else {
		n.Log("%s (bị từ chối) sau %.2f s", line, dt)
	}
}

func motDot(host string, port int) (int, int, float64) {
	kq := make([]string, nKhach)
	var wg sync.WaitGroup
	t := time.Now()
	for k := 1; k <= nKhach; k++ {
		wg.Add(1)
		go khach(host, port, k, kq, &wg)
	}
	wg.Wait()
	ok := 0
	for _, v := range kq {
		if strings.HasPrefix(v, "XONG") {
			ok++
		}
	}
	return ok, nKhach - ok, time.Since(t).Seconds()
}

func runTai(host, port string) {
	pa, _ := strconv.Atoi(port)
	say("(b) Máy chủ nhiều goroutine: %d client đồng thời (cách nhau %d ms), mỗi yêu cầu xử lý %d ms", nKhach, cachMs, msViec)
	say("Máy chủ A – mỗi client một goroutine (cổng %d):", pa)
	a1, a2, a3 := motDot(host, pa)
	say("Máy chủ B – nhóm %d goroutine, hàng đợi %d (cổng %d):", nhomB, hangDoiB, pa+1)
	b1, b2, b3 := motDot(host, pa+1)
	say("")
	say("  %-29s%11s%9s%10s", "Máy chủ", "Thành công", "Từ chối", "Thời gian")
	say("  %-29s%11d%9d%8.2f s", "A. mỗi client một goroutine", a1, a2, a3)
	say("  %-29s%11d%9d%8.2f s", fmt.Sprintf("B. nhóm %d worker + hàng đợi %d", nhomB, hangDoiB), b1, b2, b3)
	say("Nhận xét: A tạo goroutine không giới hạn – rẻ hơn luồng nhiều nhưng quá tải vẫn cạn bộ nhớ, socket;")
	say("  B giữ số worker cố định, chỉ nhận %d việc cùng lúc, phần vượt bị từ chối ngay (503)", nhomB+hangDoiB)
	say("  thay vì xếp hàng vô hạn – client biết để thử lại sau.")
}
