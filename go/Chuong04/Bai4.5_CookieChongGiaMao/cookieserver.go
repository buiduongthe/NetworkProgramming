package main

import (
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"fmt"
	"net"
	"strconv"
	"time"
)

// Server tra cứu UDP xác thực địa chỉ nguồn bằng mã thách thức (cookie) không trạng thái – Mã giả 4.8 (Mục 4.6.1).
// Kế thừa: Mã giả 4.3 (server tra cứu UDP) – thay thân vòng lặp bằng HandleDatagram.
// Cookie = HMAC-SHA256(KEY, "ip:port:w"), w là khung thời gian 1 phút; server không lưu trạng thái.
// Với địa chỉ chưa xác thực, server không gửi phản hồi lớn hơn MAX_RATIO lần yêu cầu mà gửi thách thức 401.

// Mã giả 4.8, dòng 1: CONST KEY ← RANDOM_BYTES(32), MAX_RATIO ← 3, MAX_DGRAM ← 1200.
const (
	MaxRatio = 3
	MaxDgram = 1200
)

var key = make([]byte, 32)

// Cookie – Mã giả 4.8, dòng 2–5: mã thách thức gắn với địa chỉ (ip, cổng) và khung thời gian w.
func Cookie(addr *net.UDPAddr, w int64) string {
	msg := fmt.Sprintf("%s:%d:%d", addr.IP, addr.Port, w) // addr.ip + ":" + addr.port + ":" + w
	mac := hmac.New(sha256.New, key)
	mac.Write([]byte(msg))
	return hex.EncodeToString(mac.Sum(nil)) // HEX(HMAC_SHA256(KEY, msg))
}

// safeEqual – SAFE_EQUAL: so sánh thời gian hằng.
func safeEqual(a, b string) bool {
	return subtle.ConstantTimeCompare([]byte(a), []byte(b)) == 1
}

// HandleDatagram – Mã giả 4.8, dòng 7–24.
func HandleDatagram(s net.PacketConn, data []byte, addr *net.UDPAddr) {
	if len(data) > MaxDgram { // quá lớn: bỏ
		fmt.Printf("[server] Bỏ datagram quá lớn (%d byte) từ %s\n", len(data), addr)
		return
	}
	req, err := FromBytes(data) // JSON_DECODE(DECODE(data, "UTF-8")); gồm cả IF NOT IS_OBJECT(req)
	if err != nil {             // không phải JSON hoặc không phải đối tượng: bỏ im lặng
		fmt.Printf("[server] Bỏ datagram sai định dạng từ %s\n", addr)
		return
	}
	w := time.Now().Unix() / 60 // FLOOR(NOW_SECONDS() / 60)
	c, hasCookie := GetString(req, "cookie")
	ok := hasCookie && (safeEqual(c, Cookie(addr, w)) || safeEqual(c, Cookie(addr, w-1)))
	state := "không có"
	if _, present := req["cookie"]; present {
		state = "không hợp lệ"
		if ok {
			state = "hợp lệ"
		}
	}
	fmt.Printf("[server] Nhận %d byte từ %s (id=%s, cookie: %s)\n", len(data), addr, ToBytes(req["id"]), state)
	resp := ProcessRequest(req) // resp ← PROCESS_REQUEST(req); resp.id ← req.id
	resp["id"] = req["id"]
	out := ToBytes(resp)     // out ← ENCODE(JSON_ENCODE(resp), "UTF-8")
	if len(out) > MaxDgram { // giữ kiểm tra của Mã giả 4.3
		resp = map[string]any{"id": req["id"], "status": 413, "error": "Quá lớn"}
		out = ToBytes(resp)
	}
	if !ok && len(out) > MaxRatio*len(data) { // IF NOT ok AND LENGTH(out) > MAX_RATIO * LENGTH(data)
		ch := map[string]any{"id": req["id"], "status": 401, "cookie": Cookie(addr, w)}
		small := ToBytes(ch) // phản hồi nhỏ
		if len(small) > MaxRatio*len(data) {
			fmt.Printf("[server]   -> cả thách thức (%d byte) cũng vượt %d × %d byte: bỏ, không trả lời\n", len(small), MaxRatio, len(data))
			return // IF LENGTH(out) > MAX_RATIO * LENGTH(data) THEN RETURN
		}
		fmt.Printf("[server]   -> phản hồi %d byte > %d × %d byte: gửi thách thức 401 (%d byte)\n", len(out), MaxRatio, len(data), len(small))
		out = small
	} else {
		fmt.Printf("[server]   -> gửi phản hồi status=%v (%d byte)\n", resp["status"], len(out))
	}
	s.WriteTo(out, addr) // SENDTO(s, out, addr)
}

// RunCookieServer – vòng RECVFROM của Mã giả 4.3 với thân là HandleDatagram.
func RunCookieServer(args []string) int {
	var err error
	if dict, err = LoadGlossary("glossary.json"); err != nil {
		fmt.Printf("[server] Không đọc được glossary.json: %v\n", err)
		return 1
	}
	rand.Read(key) // KEY ← RANDOM_BYTES(32)
	host, port, err := ParseEndpoint(args, "0.0.0.0", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	s, err := net.ListenPacket("udp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fmt.Printf("[server] Không mở được cổng: %v\n", err)
		return 1
	}
	fmt.Printf("[server] Đang lắng nghe UDP tại %s:%d, từ điển %d thuật ngữ, MAX_RATIO = %d\n", host, port, len(dict.Keys), MaxRatio)
	StopOnCtrlC(s)
	buf := make([]byte, 65535)
	for {
		n, addr, err := s.ReadFrom(buf) // RECVFROM(s, 65535)
		if isReset(err) {
			continue // Windows: ICMP port unreachable
		}
		if err != nil {
			break
		}
		HandleDatagram(s, buf[:n], addr.(*net.UDPAddr))
	}
	s.Close()
	fmt.Println("[server] Dừng server")
	return 0
}
