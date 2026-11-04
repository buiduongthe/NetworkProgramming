package main

import (
	"encoding/json"
	"fmt"
	"net"
	"os"
	"strconv"
)

// Server tra cứu thuật ngữ trên UDP – Mã giả 4.3 (Mục 4.3.1).
// Kế thừa: Mã giả 4.1 (vòng RECVFROM/SENDTO), Mã giả 3.7 (PROCESS_REQUEST, DICT).
// Mỗi yêu cầu và mỗi phản hồi là một datagram chứa đối tượng JSON; server chép trường id
// của yêu cầu vào phản hồi và giới hạn phản hồi trong MAX_DGRAM = 1200 byte.
// Tham số thứ ba (thêm cho bài thực hành): bỏ qua N datagram đầu tiên để mô phỏng mất gói.

// MaxDgram – CONST MAX_DGRAM ← 1200 (Mã giả 4.3, dòng 1).
const MaxDgram = 1200

// Glossary – từ điển thuật ngữ; Keys giữ thứ tự trong tệp.
type Glossary struct {
	Text map[string]string
	Keys []string
}

// LoadGlossary – DICT ← LOAD_JSON_FILE("glossary.json"), giữ thứ tự khóa.
func LoadGlossary(path string) (*Glossary, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	dec := json.NewDecoder(f)
	if _, err := dec.Token(); err != nil {
		return nil, err
	}
	g := &Glossary{Text: map[string]string{}}
	for dec.More() {
		tok, err := dec.Token()
		if err != nil {
			return nil, err
		}
		key, _ := tok.(string)
		var text string
		if err := dec.Decode(&text); err != nil {
			return nil, err
		}
		g.Text[key] = text
		g.Keys = append(g.Keys, key)
	}
	return g, nil
}

var dict *Glossary

// ProcessRequest – PROCESS_REQUEST (kế thừa nguyên vẹn từ server tra cứu TCP): nghiệp vụ, độc lập với socket.
func ProcessRequest(req map[string]any) map[string]any {
	op, _ := GetString(req, "op")
	switch op {
	case "lookup":
		term, ok := GetString(req, "term")
		if text, found := dict.Text[term]; ok && found {
			return map[string]any{"status": 200, "term": term, "text": text}
		}
		return map[string]any{"status": 404, "error": "Không có thuật ngữ"}
	case "list":
		return map[string]any{"status": 200, "terms": dict.Keys}
	}
	return map[string]any{"status": 400, "error": "Sai thao tác"}
}

// RunUdpLookupServer – Mã giả 4.3 UDP_LOOKUP_SERVER.
func RunUdpLookupServer(args []string) int {
	var err error
	if dict, err = LoadGlossary("glossary.json"); err != nil {
		fmt.Printf("[server] Không đọc được glossary.json: %v\n", err)
		return 1
	}
	host, port, err := ParseEndpoint(args[:min(len(args), 2)], "0.0.0.0", 9100) // dòng 3
	if err != nil {
		fmt.Println(err)
		return 2
	}
	drop := 0 // mô phỏng mất gói (thêm)
	if len(args) > 2 {
		if drop, err = strconv.Atoi(args[2]); err != nil || drop < 0 {
			fmt.Printf("Số datagram bỏ qua không hợp lệ: %s\n", args[2])
			return 2
		}
	}
	s, err := net.ListenPacket("udp4", net.JoinHostPort(host, strconv.Itoa(port))) // dòng 4–5: SOCKET(INET, DGRAM), BIND
	if err != nil {
		fmt.Printf("[server] Không mở được cổng: %v\n", err)
		return 1
	}
	fmt.Printf("[server] Đang lắng nghe UDP tại %s:%d, từ điển %d thuật ngữ\n", host, port, len(dict.Keys))
	if drop > 0 {
		fmt.Printf("[server] Mô phỏng mất gói: bỏ qua %d datagram đầu tiên\n", drop)
	}
	StopOnCtrlC(s)
	buf := make([]byte, 65535)
	count := 0
	for { // dòng 6: LOOP
		n, addr, err := s.ReadFrom(buf) // dòng 7: RECVFROM(s, 65535)
		if isReset(err) {
			continue // Windows: ICMP port unreachable
		}
		if err != nil {
			break // socket đã đóng (Ctrl+C)
		}
		count++
		if count <= drop {
			fmt.Printf("[server] Bỏ qua datagram thứ %d từ %s (mô phỏng mất gói)\n", count, addr)
			continue
		}
		var resp map[string]any
		req, err := FromBytes(buf[:n]) // dòng 8–9: JSON_DECODE(DECODE(data, "UTF-8"))
		if err == nil {
			resp = ProcessRequest(req) // dòng 10
		} else { // dòng 11–13: CATCH JsonError
			req = map[string]any{"id": nil}
			resp = map[string]any{"status": 400, "error": "JSON sai"}
		}
		resp["id"] = req["id"]   // dòng 15: ghép phản hồi với yêu cầu
		out := ToBytes(resp)     // dòng 16
		if len(out) > MaxDgram { // dòng 17–20
			resp = map[string]any{"id": req["id"], "status": 413, "error": "Quá lớn"}
			out = ToBytes(resp)
		}
		s.WriteTo(out, addr) // dòng 21: SENDTO(s, out, addr)
		op, ok := GetString(req, "op")
		if !ok {
			op = "None"
		}
		fmt.Printf("[server] Yêu cầu id=%s op=%s từ %s -> status=%v, %d byte\n", ToBytes(req["id"]), op, addr, resp["status"], len(out))
	}
	s.Close()
	fmt.Println("[server] Dừng server")
	return 0
}
