// Bài 6.1 – Nghiệp vụ tra cứu thuật ngữ, chép gọn từ Mã giả 3.7 (Mục 3.5.1).
//
// processRequest độc lập với socket; handleLookup là vòng phục vụ một client.
// Cả hai server của bài (thread_server.go, pool_server.go) dùng lại nguyên vẹn.
package main

import (
	"bytes"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
)

var DICT map[string]string // từ điển thuật ngữ
var TERMS []string         // các thuật ngữ theo đúng thứ tự trong tệp

// loadGlossary: LOAD_JSON_FILE("glossary.json"); tìm tệp ở thư mục hiện hành hoặc thư mục bài.
func loadGlossary() {
	path := "glossary.json"
	if _, err := os.Stat(path); err != nil {
		exe, _ := os.Executable()
		path = filepath.Join(filepath.Dir(exe), "..", "glossary.json") // out/app → thư mục bài
	}
	data, err := os.ReadFile(path)
	if err != nil {
		fail("Không đọc được glossary.json: " + err.Error())
	}
	if err := json.Unmarshal(data, &DICT); err != nil {
		fail("glossary.json sai định dạng: " + err.Error())
	}
	// map của Go không giữ thứ tự: đọc lại các khóa theo thứ tự xuất hiện bằng json.Decoder
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.Token() // dấu {
	for dec.More() {
		k, _ := dec.Token()
		var v string
		dec.Decode(&v)
		TERMS = append(TERMS, k.(string))
	}
}

// processRequest – Mã giả 3.7, dòng 2–12: ánh xạ thao tác → phản hồi.
func processRequest(req map[string]any) map[string]any {
	switch req["op"] {
	case "lookup":
		term, _ := req["term"].(string)
		if text, ok := DICT[term]; ok {
			return map[string]any{"status": 200, "term": term, "text": text}
		}
		return map[string]any{"status": 404, "error": "Không có thuật ngữ"}
	case "list":
		return map[string]any{"status": 200, "terms": TERMS}
	}
	return map[string]any{"status": 400, "error": "Sai thao tác"}
}

// handleLookup – Mã giả 3.7, dòng 14–27: nhận yêu cầu, trả phản hồi đến khi client gửi bye hoặc đóng.
func handleLookup(conn io.ReadWriter) error {
	for {
		req, err := recvJSON(conn)
		if cc, ok := err.(*ConnectionClosed); ok {
			if cc.Received == 0 { // đóng tại ranh giới khung
				return nil
			}
			return err // đóng giữa khung: lỗi
		}
		if _, ok := err.(*JsonError); ok { // JSON sai: trả 400 rồi tiếp tục
			if err := sendJSON(conn, map[string]any{"status": 400, "error": "JSON sai"}); err != nil {
				return err
			}
			continue
		}
		if err != nil {
			return err
		}
		if req["op"] == "bye" {
			return nil
		}
		if err := sendJSON(conn, processRequest(req)); err != nil {
			return err
		}
	}
}
