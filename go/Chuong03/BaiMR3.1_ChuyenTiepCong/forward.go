package main

import (
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"sync"
	"sync/atomic"
	"syscall"
	"time"
)

var connCounter atomic.Int64

// runForwarder – nghe ở cổng cục bộ; mỗi client được ghép với một kết nối mới tới đích.
func runForwarder(lhost, lport, thost, tport string) {
	ln, err := net.Listen("tcp", net.JoinHostPort(lhost, lport))
	if err != nil {
		fmt.Println("[chuyen-tiep] Lỗi:", err)
		os.Exit(1)
	}
	target := net.JoinHostPort(thost, tport)
	fmt.Printf("[chuyen-tiep] %s:%s -> %s\n", lhost, lport, target)
	stopOnInterrupt(ln, "[chuyen-tiep] Dừng")
	for {
		client, err := ln.Accept()
		if err != nil {
			return
		}
		go serve(client.(*net.TCPConn), target)
	}
}

// pump – chép src -> dst tới khi src hết dữ liệu, rồi CloseWrite (gửi FIN) cho dst.
// io.Copy tự lặp Read/Write và Write của Go luôn gửi hết (tương đương SEND_ALL).
func pump(dst, src *net.TCPConn, label string, total *int64, wg *sync.WaitGroup) {
	defer wg.Done()
	n, err := io.Copy(dst, src)
	*total = n
	if err != nil {
		fmt.Printf("[chuyen-tiep] %s: lỗi %v\n", label, err)
	}
	dst.CloseWrite() // chuyển tiếp FIN, không đóng hẳn socket: chiều kia có thể còn dữ liệu
}

func serve(client *net.TCPConn, target string) {
	id := connCounter.Add(1)
	up, err := net.DialTimeout("tcp", target, 3*time.Second)
	if err != nil {
		reason := err.Error()
		if errors.Is(err, syscall.ECONNREFUSED) {
			reason = "Connection refused"
		}
		fmt.Printf("[chuyen-tiep] #%d %s: không nối được đích %s (%s)\n", id, client.RemoteAddr(), target, reason)
		client.Close() // client chưa được đọc dữ liệu nên sẽ nhận RST
		return
	}
	upstream := up.(*net.TCPConn)
	fmt.Printf("[chuyen-tiep] #%d %s <-> đích %s\n", id, client.RemoteAddr(), target)
	var upBytes, downBytes int64
	var wg sync.WaitGroup
	wg.Add(2)
	go pump(upstream, client, fmt.Sprintf("#%d lên", id), &upBytes, &wg)
	go pump(client, upstream, fmt.Sprintf("#%d xuống", id), &downBytes, &wg)
	wg.Wait() // chờ cả hai chiều kết thúc
	client.Close()
	upstream.Close()
	fmt.Printf("[chuyen-tiep] #%d xong: lên %d byte, xuống %d byte\n", id, upBytes, downBytes)
}
