// Bài 8.4 – Cấp và kiểm tra JWT ký HMAC-SHA-256 (HS256) cho xác thực người dùng chat.
//
// VERIFY_JWT của Mã giả 8.6 theo Mục 7.2.4: kiểm tra đúng thuật toán (từ chối alg "none"), chữ ký,
// exp chưa quá hạn, iss là ISSUER, aud là đối tượng mong đợi. Khóa ký lấy qua SECRET("CHAT_JWT_KEY").
// Cấp token thử nghiệm (vai trò server ủy quyền giản lược):
//
//	CHAT_JWT_KEY=<khóa> out/app token <sub> <tệp_token> [aud=chat] [thời_hạn_giây=3600]
package main

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

const ISSUER = "auth.lab.local"

// JwtError: token không hợp lệ, kèm lý do.
type JwtError struct{ reason string }

func (e *JwtError) Error() string { return e.reason }

// secret – SECRET(name): bí mật chỉ lấy từ biến môi trường.
func secret(name string) (string, error) {
	if v := os.Getenv(name); v != "" {
		return v, nil
	}
	return "", errors.New("Thiếu biến môi trường " + name)
}

var b64url = base64.RawURLEncoding // Base64URL không có dấu '=' ở cuối

func hs256(key, data string) []byte {
	m := hmac.New(sha256.New, []byte(key))
	m.Write([]byte(data))
	return m.Sum(nil)
}

// Claims: các trường của token do server ủy quyền cấp.
type Claims struct {
	Iss string `json:"iss"`
	Sub string `json:"sub"`
	Aud string `json:"aud"`
	Iat int64  `json:"iat"`
	Exp int64  `json:"exp"`
}

func signJWT(c Claims, key string) string {
	head := b64url.EncodeToString([]byte(`{"alg":"HS256","typ":"JWT"}`))
	body, _ := json.Marshal(c)
	payload := b64url.EncodeToString(body)
	return head + "." + payload + "." + b64url.EncodeToString(hs256(key, head+"."+payload))
}

// verifyJWT – VERIFY_JWT(token, issuer, audience) -> claims; lỗi -> JwtError kèm lý do.
func verifyJWT(token any, key, issuer, audience string) (map[string]any, error) {
	s, _ := token.(string)
	parts := strings.Split(s, ".")
	if len(parts) != 3 {
		return nil, &JwtError{"định dạng sai"}
	}
	var header, claims map[string]any
	hb, err1 := b64url.DecodeString(strings.TrimRight(parts[0], "="))
	cb, err2 := b64url.DecodeString(strings.TrimRight(parts[1], "="))
	sig, err3 := b64url.DecodeString(strings.TrimRight(parts[2], "="))
	if err1 != nil || err2 != nil || err3 != nil || json.Unmarshal(hb, &header) != nil || json.Unmarshal(cb, &claims) != nil {
		return nil, &JwtError{"định dạng sai"}
	}
	if header["alg"] != "HS256" { // đúng thuật toán mong đợi
		return nil, &JwtError{"thuật toán không được chấp nhận"}
	}
	if !hmac.Equal(sig, hs256(key, parts[0]+"."+parts[1])) { // so sánh thời gian hằng
		return nil, &JwtError{"chữ ký sai"}
	}
	if exp, ok := claims["exp"].(float64); !ok || exp <= float64(time.Now().Unix()) {
		return nil, &JwtError{"đã hết hạn"}
	}
	if claims["iss"] != issuer {
		return nil, &JwtError{"iss không đúng"}
	}
	audOK := claims["aud"] == audience
	if list, isList := claims["aud"].([]any); isList {
		for _, a := range list {
			audOK = audOK || a == audience
		}
	}
	if !audOK {
		return nil, &JwtError{"aud không đúng"}
	}
	if sub, ok := claims["sub"].(string); !ok || sub == "" {
		return nil, &JwtError{"thiếu sub"}
	}
	return claims, nil
}

// tokenMain: cấp token thử nghiệm và ghi ra tệp.
func tokenMain(args []string) {
	if len(args) < 2 {
		fail("Cách dùng: out/app token <sub> <tệp_token> [aud] [thời_hạn_giây]")
	}
	sub, path, aud, ttl := args[0], args[1], "chat", 3600
	if len(args) > 2 {
		aud = args[2]
	}
	if len(args) > 3 {
		ttl, _ = strconv.Atoi(args[3])
	}
	key, err := secret("CHAT_JWT_KEY")
	if err != nil {
		fail(err.Error())
	}
	now := time.Now().Unix()
	token := signJWT(Claims{ISSUER, sub, aud, now, now + int64(ttl)}, key)
	os.MkdirAll(filepath.Dir(path), 0o755)
	if err := os.WriteFile(path, []byte(token), 0o600); err != nil {
		fail(err.Error())
	}
	fmt.Printf("[token] Đã cấp token sub=%s, aud=%s, iss=%s, hết hạn sau %d s -> %s\n", sub, aud, ISSUER, ttl, path)
}
