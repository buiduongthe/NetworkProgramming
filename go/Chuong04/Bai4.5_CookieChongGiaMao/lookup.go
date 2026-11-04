// Nghiệp vụ tra cứu kế thừa từ Bài 4.2 (Mã giả 3.7: DICT, PROCESS_REQUEST), chép nguyên để bài tự chứa.

package main

import (
	"encoding/json"
	"os"
)

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
