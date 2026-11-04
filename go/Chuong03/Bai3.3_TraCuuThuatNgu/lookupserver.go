package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"os"
	"strconv"
)

// Server tra cứu thuật ngữ dùng khung JSON – Mã giả 3.7 (Mục 3.5.1) theo đặc tả giao thức dịch vụ tra cứu:
// TCP cổng 9100, khung tiền tố độ dài 4 byte, nội dung JSON UTF-8.
// Kế thừa: khung sườn ECHO_SERVER (Mã giả 3.1) với HANDLE_ECHO thay bằng HandleLookup;
// SEND_JSON/RECV_JSON (Mã giả 3.6, tệp framing.go).

// Glossary – từ điển thuật ngữ; Keys giữ thứ tự trong tệp để thao tác "list" trả đúng thứ tự.
type Glossary struct {
	Text map[string]string
	Keys []string
}

// LoadGlossary – DICT ← LOAD_JSON_FILE("glossary.json"), đọc từng cặp khóa–giá trị để giữ thứ tự.
func LoadGlossary(path string) (*Glossary, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	dec := json.NewDecoder(f)
	if _, err := dec.Token(); err != nil { // dấu '{'
		return nil, err
	}
	g := &Glossary{Text: map[string]string{}}
	for dec.More() {
		tok, err := dec.Token() // khóa
		if err != nil {
			return nil, err
		}
		key, _ := tok.(string)
		var text string
		if err := dec.Decode(&text); err != nil { // giá trị
			return nil, err
		}
		g.Text[key] = text
		g.Keys = append(g.Keys, key)
	}
	return g, nil
}

var dict *Glossary

// ProcessRequest – nghiệp vụ, độc lập với socket: nhận đối tượng yêu cầu, trả đối tượng phản hồi.
func ProcessRequest(req map[string]any) map[string]any {
	op, _ := GetString(req, "op")
	switch op { // CASE req.op OF
	case "lookup":
		term, ok := GetString(req, "term")
		if text, found := dict.Text[term]; ok && found { // req.term IN DICT
			return map[string]any{"status": 200, "term": term, "text": text}
		}
		return map[string]any{"status": 404, "error": "Không có thuật ngữ"}
	case "list":
		return map[string]any{"status": 200, "terms": dict.Keys} // KEYS(DICT)
	}
	return map[string]any{"status": 400, "error": "Sai thao tác"} // OTHERWISE
}

// HandleLookup – HANDLE_LOOKUP(conn): vòng nhận khung JSON của một client.
func HandleLookup(conn net.Conn, addr string) error {
	for {
		req, err := RecvJSON(conn)
		var cc *ConnectionClosed
		var je *JsonError
		switch {
		case errors.As(err, &cc) && cc.Received == 0:
			return nil // đóng tại ranh giới
		case errors.As(err, &je): // CATCH JsonError, gồm cả IF NOT IS_OBJECT(req)
			if err := SendJSON(conn, map[string]any{"status": 400, "error": "JSON sai"}); err != nil {
				return err
			}
			fmt.Printf("[server] %s: (JSON sai) -> 400\n", addr)
			continue
		case err != nil:
			return err // đóng giữa khung hoặc ProtocolError: lỗi
		}
		op, _ := GetString(req, "op")
		if op == "bye" {
			return nil
		}
		resp := ProcessRequest(req)
		if err := SendJSON(conn, resp); err != nil {
			return err
		}
		if op == "" { // chỉ để ghi nhật ký
			op = "?"
		}
		term := ""
		if op == "lookup" {
			t, _ := GetString(req, "term")
			term = " '" + t + "'"
		}
		fmt.Printf("[server] %s: %s%s -> %v\n", addr, op, term, resp["status"])
	}
}

// RunLookupServer – khung sườn ECHO_SERVER (Mã giả 3.1), cổng mặc định 9100.
func RunLookupServer(args []string) int {
	var err error
	if dict, err = LoadGlossary("glossary.json"); err != nil {
		fmt.Printf("[server] Không đọc được glossary.json: %v\n", err)
		return 1
	}
	host, port, err := ParseEndpoint(args, "0.0.0.0", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port))) // SOCKET, REUSEADDR, BIND, LISTEN
	if err != nil {
		fmt.Printf("[server] Không mở được cổng: %v\n", err)
		return 1
	}
	fmt.Printf("[server] Đang lắng nghe tại %s:%d (%d thuật ngữ)\n", host, port, len(dict.Keys))
	StopOnCtrlC(srv)
	for { // vòng chấp nhận kết nối
		conn, err := srv.Accept() // ACCEPT
		if err != nil {
			break
		}
		addr := conn.RemoteAddr().String()
		fmt.Printf("[server] Client kết nối: %s\n", addr)
		if err := HandleLookup(conn, addr); err != nil { // NetError, ConnectionClosed, ProtocolError
			fmt.Printf("[server] Lỗi với %s: %v\n", addr, err) // không dừng server
		}
		conn.Close() // CLOSE(conn)
		fmt.Printf("[server] Client %s đã đóng kết nối\n", addr)
	}
	fmt.Println("[server] Dừng server")
	return 0
}
