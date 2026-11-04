// Server HTTP/1.1 nhỏ ở hai chế độ vulnerable/protected, dùng cùng tham số phòng thủ với bản Python.
package main

import (
	"bufio"
	"bytes"
	"fmt"
	"net"
	"os"
	"os/signal"
	"sync"
	"syscall"
	"time"
)

// Tham số phòng thủ (chế độ protected) – giống bản Python.
const (
	hanDocTieuDe  = 3 * time.Second // toàn bộ phần đầu phải xong trong hạn này
	hanChotRoi    = 2 * time.Second // không nhận thêm byte trong khoảng này -> đóng
	coTieuDeToiDa = 8 * 1024        // tổng kích thước phần đầu tối đa
	gioiHanIP     = 10              // số kết nối đồng thời tối đa mỗi IP
	tranToanCuc   = 64              // trần kết nối đồng thời toàn server (vượt -> 503)
	soLuongWorker = 40              // nhóm goroutine cố định (giống MaxRequestWorkers)
)

var traLoi = []byte("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n" +
	"Connection: close\r\nContent-Length: 20\r\n\r\nXin chao tu server!\n")

type thongKe struct {
	mu                               sync.Mutex
	chapNhan, xong, dongCham, tuChoi int
}

func (s *thongKe) tang(p *int) { s.mu.Lock(); *p++; s.mu.Unlock() }

func chayServer(mode, host, port string) {
	if mode != "vulnerable" && mode != "protected" {
		fmt.Println("Chế độ phải là vulnerable hoặc protected")
		os.Exit(1)
	}
	ln, err := net.Listen("tcp", net.JoinHostPort(host, port))
	if err != nil {
		fmt.Println("Không mở được listener:", err)
		os.Exit(1)
	}
	st := &thongKe{}
	demIP := map[string]int{}
	fmt.Printf("[server:%s] Nghe tại %s:%s, nhóm %d worker. Ctrl+C để dừng.\n",
		mode, host, port, soLuongWorker)
	if mode == "protected" {
		fmt.Printf("[server:protected] Phòng thủ: hạn đọc tiêu đề %.1fs, hạn rỗi %.1fs, "+
			"tiêu đề <= %dB, <= %d kết nối/IP, trần %d kết nối.\n",
			hanDocTieuDe.Seconds(), hanChotRoi.Seconds(), coTieuDeToiDa, gioiHanIP, tranToanCuc)
	} else {
		fmt.Println("[server:vulnerable] KHÔNG có hạn đọc tiêu đề: mỗi kết nối chậm giữ một worker.")
	}

	// Dừng êm khi nhận SIGINT/SIGTERM: in tổng kết rồi thoát.
	sig := make(chan os.Signal, 1)
	signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		<-sig
		// In tổng kết TRƯỚC khi đóng listener: tránh để main thoát trước khi dòng này kịp in.
		st.mu.Lock()
		fmt.Printf("[server:%s] Dừng. Tổng kết: chấp nhận %d, phục vụ xong %d, "+
			"đóng vì chậm %d, từ chối 503 %d.\n",
			mode, st.chapNhan, st.xong, st.dongCham, st.tuChoi)
		st.mu.Unlock()
		os.Exit(0)
	}()

	// Nhóm worker cố định: chỉ accept khi có một chỗ trống (sem) -> mô phỏng trần worker.
	sem := make(chan struct{}, soLuongWorker)
	var muIP sync.Mutex
	for {
		sem <- struct{}{} // chiếm một chỗ trước khi accept (như Python: worker gọi accept)
		conn, err := ln.Accept()
		if err != nil {
			<-sem
			return
		}
		st.tang(&st.chapNhan)
		go func(c net.Conn) {
			defer func() { <-sem }()
			if mode == "protected" {
				phucVuProtected(c, st, demIP, &muIP)
			} else {
				phucVuVulnerable(c, st)
			}
		}(conn)
	}
}

// phucVuProtected: đọc phần đầu có hạn chót, kiểm tra trần toàn cục và giới hạn theo IP.
func phucVuProtected(c net.Conn, st *thongKe, demIP map[string]int, muIP *sync.Mutex) {
	defer c.Close()
	ip, _, _ := net.SplitHostPort(c.RemoteAddr().String())
	muIP.Lock()
	tong := 0
	for _, v := range demIP {
		tong += v
	}
	quaTai := tong >= tranToanCuc || demIP[ip] >= gioiHanIP
	if !quaTai {
		demIP[ip]++
	}
	muIP.Unlock()
	if quaTai {
		st.tang(&st.tuChoi)
		c.Write([]byte("HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\n" +
			"Content-Length: 0\r\n\r\n"))
		return
	}
	defer func() {
		muIP.Lock()
		demIP[ip]--
		if demIP[ip] <= 0 {
			delete(demIP, ip)
		}
		muIP.Unlock()
	}()
	if docTieuDeCoHan(c) {
		c.Write(traLoi)
		st.tang(&st.xong)
	} else {
		st.tang(&st.dongCham) // đóng kết nối gửi tiêu đề quá chậm
	}
}

// phucVuVulnerable: đọc phần đầu KHÔNG đặt hạn -> client nhỏ giọt giữ worker mãi.
func phucVuVulnerable(c net.Conn, st *thongKe) {
	defer c.Close()
	if docTieuDeKhongHan(c) {
		c.Write(traLoi)
		st.tang(&st.xong)
	}
}

// docTieuDeCoHan: hạn chót tuyệt đối + hạn chót rỗi + cỡ tiêu đề tối đa.
func docTieuDeCoHan(c net.Conn) bool {
	hanTuyetDoi := time.Now().Add(hanDocTieuDe)
	var buf bytes.Buffer
	r := bufio.NewReader(c)
	tmp := make([]byte, 1024)
	for !bytes.Contains(buf.Bytes(), []byte("\r\n\r\n")) {
		conLai := time.Until(hanTuyetDoi)
		if conLai <= 0 {
			return false // (1) quá hạn tổng
		}
		han := hanChotRoi
		if conLai < han {
			han = conLai
		}
		c.SetReadDeadline(time.Now().Add(han)) // (2) hạn chót rỗi
		n, err := r.Read(tmp)
		if n > 0 {
			buf.Write(tmp[:n])
		}
		if err != nil {
			return false
		}
		if buf.Len() > coTieuDeToiDa { // (3) tiêu đề quá to
			return false
		}
	}
	return true
}

func docTieuDeKhongHan(c net.Conn) bool {
	c.SetReadDeadline(time.Time{}) // không hạn
	var buf bytes.Buffer
	r := bufio.NewReader(c)
	tmp := make([]byte, 1024)
	for !bytes.Contains(buf.Bytes(), []byte("\r\n\r\n")) {
		n, err := r.Read(tmp)
		if n > 0 {
			buf.Write(tmp[:n])
		}
		if err != nil {
			return false
		}
	}
	return true
}
