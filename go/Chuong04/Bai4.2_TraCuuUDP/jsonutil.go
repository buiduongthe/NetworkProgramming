package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"unicode/utf8"
)

// ToBytes – ENCODE(JSON_ENCODE(obj), "UTF-8"), dạng gọn, giữ nguyên chữ có dấu.
func ToBytes(obj any) []byte {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	enc.Encode(obj)
	return bytes.TrimSuffix(buf.Bytes(), []byte("\n"))
}

// ErrJson – JsonError: dữ liệu không phải đối tượng JSON hợp lệ mã hóa UTF-8.
var ErrJson = errors.New("JSON sai")

// FromBytes – JSON_DECODE(DECODE(data, "UTF-8")); lỗi UTF-8, lỗi cú pháp, JSON không phải đối tượng → ErrJson.
func FromBytes(data []byte) (map[string]any, error) {
	if !utf8.Valid(data) {
		return nil, ErrJson
	}
	var v any
	if err := json.Unmarshal(data, &v); err != nil {
		return nil, ErrJson
	}
	obj, ok := v.(map[string]any)
	if !ok {
		return nil, ErrJson
	}
	return obj, nil
}

// GetString lấy trường chuỗi của đối tượng JSON; ok = false nếu thiếu hoặc không phải chuỗi.
func GetString(obj map[string]any, key string) (string, bool) {
	v, ok := obj[key].(string)
	return v, ok
}

// GetInt lấy trường số nguyên của đối tượng JSON (số JSON được giải mã thành float64); 0 nếu thiếu.
func GetInt(obj map[string]any, key string) int {
	switch v := obj[key].(type) {
	case float64:
		return int(v)
	case int:
		return v
	}
	return 0
}
