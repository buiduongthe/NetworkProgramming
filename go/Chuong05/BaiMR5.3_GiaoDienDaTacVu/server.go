package main

// Máy chủ thử cục bộ cho ứng dụng giao diện đa tác vụ (chỉ chạy trong máy). Ba dịch vụ giả lập:
//   - HTTP  <port>/tcp : GET /tep?kb=<số KB>&tre=<ms> – gửi tệp theo khối 16 KB, nghỉ <tre> ms mỗi khối;
//   - "DNS" <port>/udp : nhận "A <tên>", trả "<tên> <địa chỉ>" hoặc "<tên> NXDOMAIN" sau 100–400 ms
//     (bảng tên giả lập, không phải giao thức DNS thật – xem Bài 4.3);
//   - cổng mở <port+1>, <port+2>/tcp: chỉ nhận rồi đóng kết nối, để ứng dụng kiểm tra cổng.

import (
	"bytes"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"
)

const chunk = 16 * 1024

type fakeName struct {
	ip string
	ms int
}

var fakeNames = map[string]fakeName{
	"router.lab": {"10.10.0.1", 100}, "may-in.lab": {"10.10.0.21", 250},
	"camera-1.lab": {"10.10.0.31", 400}, "nas.lab": {"10.10.0.40", 150},
}

var outMu sync.Mutex

func slog(format string, a ...any) {
	outMu.Lock()
	defer outMu.Unlock()
	fmt.Printf("[server] "+format+"\n", a...)
}

func fileHandler(w http.ResponseWriter, r *http.Request) {
	kb, _ := strconv.Atoi(r.URL.Query().Get("kb"))
	delay, _ := strconv.Atoi(r.URL.Query().Get("tre"))
	if kb <= 0 {
		kb = 256
	}
	size := kb * 1024
	slog("HTTP GET /tep %d KB, nghỉ %d ms mỗi khối", kb, delay)
	w.Header().Set("Content-Type", "application/octet-stream")
	w.Header().Set("Content-Length", strconv.Itoa(size))
	block := bytes.Repeat([]byte("x"), chunk)
	for sent := 0; sent < size; {
		n := min(chunk, size-sent)
		if _, err := w.Write(block[:n]); err != nil {
			slog("HTTP: client đóng kết nối giữa chừng (%d/%d KB)", sent/1024, kb)
			return
		}
		w.(http.Flusher).Flush()
		sent += n
		if sent >= size {
			break // khối cuối: không cần nghỉ
		}
		select {
		case <-time.After(time.Duration(delay) * time.Millisecond):
		case <-r.Context().Done(): // client hủy: dừng sớm
			slog("HTTP: client đóng kết nối giữa chừng (%d/%d KB)", sent/1024, kb)
			return
		}
	}
}

func dnsServer(pc net.PacketConn) {
	buf := make([]byte, 512)
	for {
		n, addr, err := pc.ReadFrom(buf)
		if err != nil {
			return
		}
		name := strings.TrimSpace(strings.TrimPrefix(string(buf[:n]), "A "))
		go func() {
			fn, ok := fakeNames[name]
			if !ok {
				fn = fakeName{"NXDOMAIN", 300}
			}
			time.Sleep(time.Duration(fn.ms) * time.Millisecond)
			pc.WriteTo([]byte(name+" "+fn.ip), addr)
		}()
	}
}

func openPort(ln net.Listener) {
	for {
		c, err := ln.Accept()
		if err != nil {
			return
		}
		c.Close()
	}
}

func runTestServer(host string, port int) {
	addr := net.JoinHostPort(host, strconv.Itoa(port))
	pc, err := net.ListenPacket("udp", addr)
	if err != nil {
		slog("lỗi: %v", err)
		os.Exit(1)
	}
	go dnsServer(pc)
	for _, p := range []int{port + 1, port + 2} {
		ln, err := net.Listen("tcp", net.JoinHostPort(host, strconv.Itoa(p)))
		if err != nil {
			slog("lỗi: %v", err)
			os.Exit(1)
		}
		go openPort(ln)
	}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /tep", fileHandler)
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		slog("lỗi: %v", err)
		os.Exit(1)
	}
	slog("HTTP %s/tcp, \"DNS\" %s/udp, cổng mở %d, %d", addr, addr, port+1, port+2)
	go func() {
		sig := make(chan os.Signal, 1)
		signal.Notify(sig, os.Interrupt, syscall.SIGTERM)
		<-sig
		slog("dừng")
		os.Exit(0)
	}()
	http.Serve(ln, mux)
}
