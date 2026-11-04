package main

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"strconv"
	"time"
)

// Bên nhận của giao thức dừng và chờ trên UDP – Mã giả 4.7 SAW_RECEIVE (Mục 4.4.1).
// Kế thừa: Mã giả 4.1 (vòng RECVFROM/SENDTO).
// Luôn gửi ACK cho datagram nhận được; datagram trùng bị bỏ dữ liệu nhưng vẫn được ACK;
// sau FIN chờ thêm 2 × MAX_RTO (như TIME-WAIT của TCP) để trả lời FIN lặp lại khi ACK bị mất.

// SawReceive – Mã giả 4.7 SAW_RECEIVE; trả về (số byte đã ghi, số datagram trùng).
func SawReceive(s *net.UDPConn, out io.Writer, loss *LossSim) (int, int, error) {
	var expected uint32 // dòng 2: expected ← 0
	written, dups := 0, 0
	var recvTimeout time.Duration // 0 = chưa đặt RECV_TIMEOUT (chờ vô hạn)
	buf := make([]byte, 65535)
	for { // dòng 3: LOOP
		if recvTimeout > 0 {
			s.SetReadDeadline(time.Now().Add(recvTimeout)) // RECV_TIMEOUT áp dụng cho mỗi lần nhận
		}
		n, peer, err := s.ReadFromUDP(buf) // dòng 5: RECVFROM(s, 65535)
		var ne net.Error
		if errors.As(err, &ne) && ne.Timeout() { // dòng 6: CATCH Timeout – chỉ xảy ra sau FIN
			fmt.Printf("[nhận] Hết %d ms không có datagram mới: kết thúc\n", 2*RtoMax)
			return written, dups, nil // dòng 7
		}
		if isReset(err) { // Windows: ICMP port unreachable
			continue
		}
		if err != nil {
			return written, dups, err
		}
		d := buf[:n]
		if len(d) < 5 { // dòng 9: datagram lỗi: bỏ
			continue
		}
		seq, typ := binary.BigEndian.Uint32(d[:4]), d[4] // dòng 10
		dropped := loss.Drop()
		if !dropped {
			ack := append(binary.BigEndian.AppendUint32(nil, seq), ACK)
			s.WriteToUDP(ack, peer) // dòng 11: luôn ACK
		}
		note := ""
		if dropped {
			note = " – mô phỏng mất ACK"
		}
		switch {
		case seq == expected && typ == FIN: // dòng 12
			recvTimeout = 2 * RtoMax * time.Millisecond // dòng 13: như TIME-WAIT
			fmt.Printf("[nhận] seq=%d FIN: gửi ACK %d%s; chờ %d ms (như TIME-WAIT)\n", seq, seq, note, 2*RtoMax)
		case seq == expected: // dòng 14
			out.Write(d[5:]) // dòng 15: WRITE(out, d[5 .. END])
			written += len(d) - 5
			expected++ // dòng 16
			fmt.Printf("[nhận] seq=%d DATA %d byte: ghi tệp, gửi ACK %d%s\n", seq, len(d)-5, seq, note)
		default: // seq < expected: trùng
			dups++
			fmt.Printf("[nhận] seq=%d trùng: bỏ dữ liệu, vẫn gửi ACK %d%s\n", seq, seq, note)
		}
	}
}

// RunRecv – bên nhận: recv [host] [port] [tep_ra] [mat_goi].
func RunRecv(args []string) int {
	host, port, err := ParseEndpoint(args[:min(len(args), 2)], "0.0.0.0", 9000)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	path := "tep_nhan.bin"
	if len(args) > 2 {
		path = args[2]
	}
	spec := ""
	if len(args) > 3 {
		spec = args[3]
	}
	loss := NewLossSim(spec)
	s, err := net.ListenUDP("udp4", &net.UDPAddr{IP: net.ParseIP(host), Port: port}) // SOCKET(INET, DGRAM) + BIND
	if err != nil {
		fmt.Printf("[nhận] Không mở được cổng %s: %v\n", strconv.Itoa(port), err)
		return 1
	}
	defer s.Close()
	fmt.Printf("[nhận] Đang chờ dữ liệu UDP tại %s:%d, ghi vào %s\n", host, port, path)
	fmt.Printf("[nhận] Mô phỏng mất ACK: %s\n", loss.SpecText())
	out, err := os.Create(path)
	if err != nil {
		fmt.Printf("[nhận] Không tạo được tệp: %v\n", err)
		return 1
	}
	written, dups, err := SawReceive(s, out, loss)
	out.Close()
	if err != nil {
		fmt.Printf("[nhận] Lỗi: %v\n", err)
		return 1
	}
	fmt.Printf("[nhận] Hoàn tất: %d byte, %d datagram trùng; SHA-256:\n", written, dups)
	fmt.Printf("[nhận]   %s\n", sha256File(path))
	return 0
}
