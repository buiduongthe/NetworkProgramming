//go:build linux

// Bài 6.3 – Server echo một luồng dùng dồn kênh vào/ra (mô hình reactor).
//
// Cài đặt Mã giả 6.4 (Mục 6.2.4, Giáo trình Lập trình mạng). Kế thừa Mã giả 3.1: cùng chức năng echo,
// vòng ACCEPT/RECV chặn được thay bằng vòng SELECT trên tập socket không chặn.
//
// Gói net của Go đã tự dùng epoll bên trong và che giấu mô tả tệp (file descriptor), nên để thấy rõ
// vòng SELECT của mã giả, bài gọi thẳng các lời gọi hệ thống của gói syscall (Linux):
// Socket, Bind, Listen, SetNonblock, Select, Accept, Read, Write, Close – đúng tên lệnh trong mã giả.
//
// Chạy:  out/app server [host] [port]      (mặc định 0.0.0.0 9000)
package main

import (
	"fmt"
	"net"
	"sort"
	"syscall"
)

const MAX_BUF = 65536 // MAX_BUF ← 65536: ngưỡng bộ đệm gửi của mỗi kết nối

// fdSet, fdIsSet: bật và kiểm tra bit của mô tả tệp fd trong tập FdSet (FD_SET, FD_ISSET của C).
func fdSet(fd int, set *syscall.FdSet) {
	set.Bits[fd/64] |= int64(1) << (uint(fd) % 64)
}

func fdIsSet(fd int, set *syscall.FdSet) bool {
	return set.Bits[fd/64]&(int64(1)<<(uint(fd)%64)) != 0
}

func sockaddrText(sa syscall.Sockaddr) string {
	if a, ok := sa.(*syscall.SockaddrInet4); ok {
		return fmt.Sprintf("%d.%d.%d.%d:%d", a.Addr[0], a.Addr[1], a.Addr[2], a.Addr[3], a.Port)
	}
	return "?"
}

// reactorServer – REACTOR_SERVER(args).
func reactorServer(args []string) {
	host, port := parseEndpoint(args, "0.0.0.0", 9000)
	ip := net.ParseIP(host).To4()
	if ip == nil {
		fail("Địa chỉ IPv4 không hợp lệ: " + host)
	}
	srv, err := syscall.Socket(syscall.AF_INET, syscall.SOCK_STREAM, 0) // SOCKET(INET, STREAM)
	if err != nil {
		fail("[server] SOCKET lỗi: " + err.Error())
	}
	syscall.SetsockoptInt(srv, syscall.SOL_SOCKET, syscall.SO_REUSEADDR, 1) // SETOPT(REUSEADDR)
	sa := &syscall.SockaddrInet4{Port: port}
	copy(sa.Addr[:], ip)
	if err := syscall.Bind(srv, sa); err != nil { // BIND
		fail("[server] BIND lỗi: " + err.Error())
	}
	syscall.Listen(srv, 16)                          // LISTEN, backlog 16
	syscall.SetNonblock(srv, true)                   // SETOPT(srv, NONBLOCKING, TRUE)
	outBuf := map[int][]byte{}                       // socket → dữ liệu chờ gửi
	closing := map[int]bool{}                        // client đã đóng chiều gửi (đã nhận FIN)
	names, echoed := map[int]string{}, map[int]int{} // chỉ để ghi nhật ký
	onInterrupt("[server] Dừng server")
	logLine("[server] Reactor một luồng đang lắng nghe tại %s:%d", host, port)

	closeConn := func(s int) {
		delete(outBuf, s)  // REMOVE(out_buf, s)
		delete(closing, s) // REMOVE(closing, s)
		syscall.Close(s)   // CLOSE(s)
		logLine("[server] Đóng %s: đã echo %d byte, còn mở %d", names[s], echoed[s], len(outBuf))
		delete(names, s)
		delete(echoed, s)
	}

	buf := make([]byte, 4096)
	for {
		conns := make([]int, 0, len(outBuf)) // các socket kết nối, xếp tăng dần cho dễ theo dõi
		for c := range outBuf {
			conns = append(conns, c)
		}
		sort.Ints(conns)
		var readSet, writeSet syscall.FdSet
		fdSet(srv, &readSet)
		maxFd := srv
		for _, c := range conns {
			if !closing[c] && len(outBuf[c]) < MAX_BUF { // room: chưa nhận FIN, bộ đệm gửi chưa đầy → theo dõi đọc
				fdSet(c, &readSet)
			}
			if len(outBuf[c]) > 0 { // pending: còn dữ liệu chờ gửi → theo dõi ghi
				fdSet(c, &writeSet)
			}
			maxFd = max(maxFd, c)
		}
		// readable, writable ← SELECT({srv} ∪ room, pending): chặn đến khi có socket sẵn sàng
		if _, err := syscall.Select(maxFd+1, &readSet, &writeSet, nil, nil); err != nil {
			continue // EINTR: bị tín hiệu ngắt, chọn lại
		}
		if fdIsSet(srv, &readSet) { // có kết nối mới
			conn, peer, err := syscall.Accept(srv) // ACCEPT
			if err == nil {
				syscall.SetNonblock(conn, true) // SETOPT(conn, NONBLOCKING, TRUE)
				outBuf[conn] = []byte{}         // EMPTY_BYTES
				names[conn], echoed[conn] = sockaddrText(peer), 0
				logLine("[server] Client %s kết nối (đang mở: %d)", names[conn], len(outBuf))
			}
		}
		for _, s := range conns { // các socket kết nối sẵn sàng đọc
			if !fdIsSet(s, &readSet) {
				continue
			}
			n, err := syscall.Read(s, buf) // RECV(s, 4096): socket đã sẵn sàng, không chờ
			if err != nil {                // lỗi (ví dụ ConnectionReset): không gửi được nữa, đóng ngay
				closeConn(s)
			} else if n == 0 { // client gửi FIN: ADD(closing, s) – gửi nốt rồi mới đóng
				closing[s] = true
			} else {
				outBuf[s] = append(outBuf[s], buf[:n]...) // chờ gửi
			}
		}
		for _, s := range conns { // FOR EACH s IN writable
			if _, open := outBuf[s]; !open || !fdIsSet(s, &writeSet) {
				continue // bỏ s vừa đóng vì lỗi
			}
			n, err := syscall.Write(s, outBuf[s]) // SEND: có thể gửi một phần
			if err != nil {
				closeConn(s)
				continue
			}
			outBuf[s] = outBuf[s][n:]
			echoed[s] += n
		}
		for _, s := range conns { // FOR EACH s IN COPY(closing)
			if _, open := outBuf[s]; open && closing[s] && len(outBuf[s]) == 0 {
				closeConn(s) // đã gửi hết: đóng
			}
		}
	}
}
