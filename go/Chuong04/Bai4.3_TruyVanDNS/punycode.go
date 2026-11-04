package main

// Mã hóa Punycode (RFC 3492) cho một nhãn tên miền quốc tế, ví dụ "bücher" → "xn--bcher-kva".
// Thư viện chuẩn Go không có gói IDNA nên bài tự cài đặt phần mã hóa tối thiểu:
// chữ được đổi về chữ thường rồi mã hóa; không kiểm tra đầy đủ các quy tắc IDNA 2008.

import (
	"strings"
	"unicode/utf8"
)

const (
	pcBase        = 36
	pcTMin        = 1
	pcTMax        = 26
	pcSkew        = 38
	pcDamp        = 700
	pcInitialBias = 72
	pcInitialN    = 128
)

func pcAdapt(delta, numPoints int, first bool) int {
	if first {
		delta /= pcDamp
	} else {
		delta /= 2
	}
	delta += delta / numPoints
	k := 0
	for delta > ((pcBase-pcTMin)*pcTMax)/2 {
		delta /= pcBase - pcTMin
		k += pcBase
	}
	return k + (pcBase-pcTMin+1)*delta/(delta+pcSkew)
}

func pcDigit(d int) byte {
	if d < 26 {
		return byte('a' + d)
	}
	return byte('0' + d - 26)
}

// ToASCIILabel – ENCODE(label, "ASCII"): nhãn ASCII giữ nguyên, nhãn có ký tự ngoài ASCII → "xn--" + Punycode.
func ToASCIILabel(label string) string {
	label = strings.ToLower(label)
	runes := []rune(label)
	var out []byte
	for _, r := range runes {
		if r < 0x80 {
			out = append(out, byte(r))
		}
	}
	basic := len(out)
	if basic == len(runes) {
		return label // toàn ASCII
	}
	if basic > 0 {
		out = append(out, '-')
	}
	n, delta, bias, h := pcInitialN, 0, pcInitialBias, basic
	for h < len(runes) {
		m := int(utf8.MaxRune)
		for _, r := range runes { // điểm mã nhỏ nhất ≥ n chưa xử lý
			if int(r) >= n && int(r) < m {
				m = int(r)
			}
		}
		delta += (m - n) * (h + 1)
		n = m
		for _, r := range runes {
			if int(r) < n {
				delta++
			}
			if int(r) == n {
				q := delta
				for k := pcBase; ; k += pcBase {
					t := k - bias
					if t < pcTMin {
						t = pcTMin
					} else if t > pcTMax {
						t = pcTMax
					}
					if q < t {
						break
					}
					out = append(out, pcDigit(t+(q-t)%(pcBase-t)))
					q = (q - t) / (pcBase - t)
				}
				out = append(out, pcDigit(q))
				bias = pcAdapt(delta, h+1, h == basic)
				delta = 0
				h++
			}
		}
		delta++
		n++
	}
	return "xn--" + string(out)
}
