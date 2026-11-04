package main

import (
	"encoding/binary"
	"errors"
	"fmt"
	"math"
	"net"
	"os"
	"path/filepath"
	"time"
)

// Bên gửi của giao thức dừng và chờ (stop-and-wait ARQ) trên UDP – Mã giả 4.6 SAW_SEND (Mục 4.4.1)
// với RTO ước lượng theo RFC 6298 (Mục 4.4.2): SRTT, RTTVAR, RTO = SRTT + max(G, K·RTTVAR);
// nhân đôi RTO khi hết thời gian chờ; thuật toán Karn.
// Kế thừa: Mã giả 4.4 (vòng chờ phản hồi đúng địa chỉ, đúng mã định danh trong phần thời gian còn lại).

// errTimeout – hết thời gian chờ ACK.
var errTimeout = errors.New("hết thời gian chờ")

// RttEstimator – RTO() / UPDATE_RTT() / BACKOFF_RTO() theo RFC 6298 (Mục 4.4.2).
type RttEstimator struct {
	srtt, rttvar, rto float64
	hasSample         bool
}

const (
	alpha = 1.0 / 8
	beta  = 1.0 / 4
	kRto  = 4
	gClk  = 1.0 // độ phân giải đồng hồ 1 ms
)

// NewRttEstimator – chưa có mẫu đo: RTO = RTO ban đầu.
func NewRttEstimator() *RttEstimator { return &RttEstimator{rto: RtoInit} }

// Update – UPDATE_RTT(R').
func (e *RttEstimator) Update(r float64) {
	if !e.hasSample {
		e.srtt, e.rttvar, e.hasSample = r, r/2, true // mẫu đầu tiên
	} else {
		e.rttvar = (1-beta)*e.rttvar + beta*math.Abs(e.srtt-r) // tính trước, với SRTT cũ
		e.srtt = (1-alpha)*e.srtt + alpha*r
	}
	e.rto = math.Min(RtoMax, math.Max(RtoMin, e.srtt+math.Max(gClk, kRto*e.rttvar)))
}

// Backoff – BACKOFF_RTO(): RTO ← MIN(2 × RTO, MAX_RTO).
func (e *RttEstimator) Backoff() { e.rto = math.Min(RtoMax, 2*e.rto) }

// RTO – thời gian chờ hiện hành (ms).
func (e *RttEstimator) RTO() float64 { return e.rto }

// waitAck – WAIT_ACK: chờ ACK mang đúng seq từ đúng peer trong rtoMs; bỏ ACK cũ, ACK lạ; hết giờ → errTimeout.
func waitAck(s *net.UDPConn, peer *net.UDPAddr, seq uint32, rtoMs float64) error {
	deadline := time.Now().Add(time.Duration(rtoMs * float64(time.Millisecond)))
	buf := make([]byte, 65535)
	for {
		s.SetReadDeadline(deadline) // chờ trong phần thời gian còn lại
		n, addr, err := s.ReadFromUDP(buf)
		var ne net.Error
		if errors.As(err, &ne) && ne.Timeout() {
			return errTimeout
		}
		if isReset(err) { // Windows: ICMP port unreachable
			continue
		}
		if err != nil {
			return err
		}
		d := buf[:n]
		if addr.IP.Equal(peer.IP) && addr.Port == peer.Port && len(d) >= 5 && d[4] == ACK && binary.BigEndian.Uint32(d[:4]) == seq {
			return nil
		}
	}
}

// SawSend – Mã giả 4.6 SAW_SEND; trả về (số datagram đã gửi, số lần gửi lại).
func SawSend(s *net.UDPConn, peer *net.UDPAddr, chunks [][]byte, est *RttEstimator, loss *LossSim) (int, int, error) {
	var seq uint32 // dòng 3: seq ← 0
	sent, retrans := 0, 0
	for i := 0; i <= len(chunks); i++ { // dòng 4: FOR EACH c IN APPEND(chunks, FIN_MARK)
		typ, body := byte(DATA), []byte(nil) // dòng 5
		if i == len(chunks) {
			typ = FIN // c = FIN_MARK
		} else {
			body = chunks[i]
		}
		tries := 0 // dòng 6
		for {      // dòng 7: LOOP
			dgram := binary.BigEndian.AppendUint32(nil, seq) // UINT32_BE(seq) + BYTE(type) + c
			dgram = append(dgram, typ)
			dgram = append(dgram, body...)
			dropped := loss.Drop()
			if !dropped {
				s.WriteToUDP(dgram, peer) // dòng 8: SENDTO
			}
			sent++
			kind, note := "DATA", ""
			if typ == FIN {
				kind = "FIN"
			}
			if dropped {
				note = " – mô phỏng mất gói"
			}
			fmt.Printf("[gửi] seq=%d %s %d byte, RTO %.0f ms%s\n", seq, kind, len(dgram), est.RTO(), note)
			t0 := time.Now()                        // dòng 9
			err := waitAck(s, peer, seq, est.RTO()) // dòng 11: WAIT_ACK(s, peer, seq, RTO())
			if err == nil {
				if tries == 0 { // dòng 12: Karn – chỉ đo khi chưa gửi lại
					est.Update(float64(time.Since(t0).Microseconds()) / 1000)
				}
				break // dòng 13
			}
			if !errors.Is(err, errTimeout) {
				return sent, retrans, err
			}
			tries++ // dòng 14: CATCH Timeout
			retrans++
			fmt.Printf("[gửi] Hết %.0f ms chờ ACK %d, gửi lại (lần %d)\n", est.RTO(), seq, tries)
			est.Backoff()         // dòng 16
			if tries > MaxTries { // dòng 17
				return sent, retrans, fmt.Errorf("Không nhận được ACK %d sau %d lần gửi lại", seq, MaxTries)
			}
		}
		seq++ // dòng 20
	}
	return sent, retrans, nil
}

// RunSend – bên gửi: send <tep> [host] [port] [mat_goi].
func RunSend(args []string) int {
	path := args[0]
	host, port, err := ParseEndpoint(args[1:min(len(args), 3)], "127.0.0.1", 9000)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	spec := ""
	if len(args) > 3 {
		spec = args[3]
	}
	loss := NewLossSim(spec)
	data, err := os.ReadFile(path)
	if err != nil {
		fmt.Printf("[gửi] Không đọc được tệp: %v\n", err)
		return 1
	}
	var chunks [][]byte // các khối ≤ 1195 byte
	for i := 0; i < len(data); i += Chunk {
		chunks = append(chunks, data[i:min(i+Chunk, len(data))])
	}
	fmt.Printf("[gửi] Tệp %s: %d byte, %d khối; SHA-256:\n", filepath.Base(path), len(data), len(chunks))
	fmt.Printf("[gửi]   %s\n", sha256File(path))
	fmt.Printf("[gửi] Gửi tới %s:%d; mô phỏng mất DATA/FIN: %s\n", host, port, loss.SpecText())
	ips, err := net.LookupIP(host)
	if err != nil {
		fmt.Printf("[gửi] Không phân giải được %s: %v\n", host, err)
		return 1
	}
	peer := &net.UDPAddr{IP: ips[0], Port: port}
	network := "udp4"
	if ips[0].To4() == nil {
		network = "udp6"
	}
	s, err := net.ListenUDP(network, nil)
	if err != nil {
		fmt.Printf("[gửi] Không tạo được socket: %v\n", err)
		return 1
	}
	defer s.Close()
	t0 := time.Now()
	sent, retrans, err := SawSend(s, peer, chunks, NewRttEstimator(), loss)
	if err != nil {
		fmt.Printf("[gửi] Lỗi: %v\n", err)
		return 1
	}
	fmt.Printf("[gửi] Hoàn tất: %d datagram đã gửi (%d lần gửi lại), %d ms\n", sent, retrans, time.Since(t0).Milliseconds())
	return 0
}
