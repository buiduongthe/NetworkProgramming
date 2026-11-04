package main

import (
	"crypto/rand"
	"encoding/binary"
	"fmt"
	"net"
	"strings"
	"time"
)

// Tự tạo và phân tích truy vấn DNS qua UDP – Mã giả 4.5 (Mục 4.3.2): BUILD_QUERY, SKIP_NAME, DNS_QUERY.
// Kế thừa: client tra cứu DNS của Mục 2.5 – thay RESOLVE của hệ điều hành bằng truy vấn tự tạo.
// Bổ sung so với mã giả (theo đoạn văn sau Mã giả 4.5): kiểm tra mọi chỉ số pos không vượt độ dài
// thông điệp và kiểm tra cờ TC trước khi đọc phần trả lời.

// ProtocolError – phản hồi sai định dạng hoặc không khớp truy vấn.
type ProtocolError struct{ Msg string }

func (e *ProtocolError) Error() string { return e.Msg }

var typeNames = map[uint16]string{1: "A", 28: "AAAA"}
var rcodes = map[byte]string{0: "NOERROR", 1: "FORMERR", 2: "SERVFAIL", 3: "NXDOMAIN", 4: "NOTIMP", 5: "REFUSED"}

func hexdump(b []byte) string {
	parts := make([]string, len(b))
	for i, x := range b {
		parts[i] = fmt.Sprintf("%02X", x)
	}
	return strings.Join(parts, " ")
}

// BuildQuery – Mã giả 4.5, dòng 1–9.
func BuildQuery(id uint16, name string, qtype uint16) ([]byte, error) {
	q := binary.BigEndian.AppendUint16(nil, id)                               // dòng 2: UINT16_BE(id)
	q = binary.BigEndian.AppendUint16(q, 0x0100)                              // cờ RD = 1
	q = binary.BigEndian.AppendUint16(q, 1)                                   // dòng 3: QD = 1
	q = append(q, make([]byte, 6)...)                                         // AN = NS = AR = 0
	for _, label := range strings.Split(strings.TrimSuffix(name, "."), ".") { // dòng 4
		b := ToASCIILabel(label) // dòng 5: ENCODE(label, "ASCII"); tên quốc tế → Punycode
		if len(b) < 1 || len(b) > 63 {
			return nil, fmt.Errorf("Nhãn không hợp lệ: '%s'", label)
		}
		q = append(q, byte(len(b))) // dòng 6: BYTE(LENGTH(b)) + b
		q = append(q, b...)
	}
	q = append(q, 0)                                // dòng 8: kết thúc tên
	q = binary.BigEndian.AppendUint16(q, qtype)     // QTYPE
	return binary.BigEndian.AppendUint16(q, 1), nil // QCLASS = IN
}

// need – kiểm tra chỉ số (bổ sung): phản hồi sai định dạng không được làm đọc ngoài thông điệp.
func need(m []byte, end int) error {
	if end > len(m) {
		return &ProtocolError{"Phản hồi sai định dạng: vượt quá độ dài thông điệp"}
	}
	return nil
}

// SkipName – Mã giả 4.5, dòng 10–17: nhảy qua một tên (không giải mã).
func SkipName(m []byte, pos int) (int, error) {
	for {
		if err := need(m, pos+1); err != nil {
			return 0, err
		}
		n := m[pos]
		if n == 0 {
			return pos + 1, nil // hết tên
		}
		if n&0xC0 == 0xC0 {
			return pos + 2, need(m, pos+2) // con trỏ nén 2 byte
		}
		if n&0xC0 != 0 {
			return 0, &ProtocolError{"Phản hồi sai định dạng: nhãn kiểu lạ"}
		}
		pos += 1 + int(n)
	}
}

// DnsQuery – Mã giả 4.5, dòng 18–38; trả về (rcode, answers).
func DnsQuery(serverIP net.IP, port int, name string, qtype uint16) (byte, []string, error) {
	network := "udp4" // dòng 19: SOCKET(FAMILY(server.ip), DGRAM) – IPv4 hoặc IPv6
	if serverIP.To4() == nil {
		network = "udp6"
	}
	s, err := net.ListenUDP(network, nil)
	if err != nil {
		return 0, nil, err
	}
	defer s.Close()                                            // CLOSE(s)
	s.SetReadDeadline(time.Now().Add(2000 * time.Millisecond)) // RECV_TIMEOUT 2000 ms
	var rnd [2]byte
	rand.Read(rnd[:])
	id := binary.BigEndian.Uint16(rnd[:]) // dòng 20: RANDOM_UINT16()
	q, err := BuildQuery(id, name, qtype)
	if err != nil {
		return 0, nil, err
	}
	server := &net.UDPAddr{IP: serverIP, Port: port}
	fmt.Printf("[client] Truy vấn %s cho %s tới %s (id=0x%04X, %d byte)\n", typeNames[qtype], name, server, id, len(q))
	fmt.Printf("[client] Truy vấn (hex): %s\n", hexdump(q))
	if _, err := s.WriteToUDP(q, server); err != nil { // dòng 21: SENDTO
		return 0, nil, err
	}
	buf := make([]byte, 4096)
	n, addr, err := s.ReadFromUDP(buf) // dòng 22: RECVFROM(s, 4096)
	if err != nil {
		return 0, nil, err
	}
	m := buf[:n]
	if !addr.IP.Equal(serverIP) || addr.Port != port { // dòng 23: IF addr ≠ server
		return 0, nil, &ProtocolError{fmt.Sprintf("Phản hồi từ địa chỉ lạ %s", addr)}
	}
	if err := need(m, 12); err != nil {
		return 0, nil, err
	}
	if binary.BigEndian.Uint16(m[0:2]) != id { // dòng 24
		return 0, nil, &ProtocolError{"ID của phản hồi không khớp truy vấn"}
	}
	rcode := m[3] & 0x0F                            // dòng 25: 3 = NXDOMAIN
	ancount := int(binary.BigEndian.Uint16(m[6:8])) // dòng 26
	rname, ok := rcodes[rcode]
	if !ok {
		rname = "?"
	}
	fmt.Printf("[client] Phản hồi %d byte từ %s, RCODE=%d (%s), ANCOUNT=%d\n", len(m), addr, rcode, rname, ancount)
	fmt.Printf("[client] Phản hồi (hex): %s\n", hexdump(m))
	if m[2]&0x02 != 0 { // bổ sung: cờ TC
		return 0, nil, &ProtocolError{"Phản hồi bị cắt (TC = 1): cần hỏi lại qua TCP cổng 53"}
	}
	pos, err := SkipName(m, 12) // dòng 27: bỏ tên câu hỏi
	if err != nil {
		return 0, nil, err
	}
	pos += 4 // bỏ QTYPE, QCLASS
	var answers []string
	for range ancount { // dòng 29: REPEAT ancount TIMES
		if pos, err = SkipName(m, pos); err != nil { // dòng 30
			return 0, nil, err
		}
		if err := need(m, pos+10); err != nil {
			return 0, nil, err
		}
		rtype := binary.BigEndian.Uint16(m[pos : pos+2])         // dòng 31
		rdlen := int(binary.BigEndian.Uint16(m[pos+8 : pos+10])) // dòng 32
		if err := need(m, pos+10+rdlen); err != nil {
			return 0, nil, err
		}
		rdata := m[pos+10 : pos+10+rdlen] // dòng 33
		if rtype == 1 && rdlen == 4 {     // dòng 34: IPV4_TEXT
			answers = append(answers, net.IP(rdata).String())
		}
		if rtype == 28 && rdlen == 16 { // dòng 35: IPV6_TEXT
			answers = append(answers, net.IP(rdata).String())
		}
		pos += 10 + rdlen // dòng 36
	}
	return rcode, answers, nil
}
