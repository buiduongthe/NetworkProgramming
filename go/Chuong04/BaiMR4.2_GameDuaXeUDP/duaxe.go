// Bài mở rộng MR4.2 – Phần dùng chung của trò chơi đua xe UDP: hằng số, vật lý, giả lập mất gói, bot, vẽ khung.
//
// Đơn vị: vị trí x và vận tốc v là số nguyên, tính bằng 1/100 ô (đường đua dài 40 ô = 4000).
// Dùng số nguyên để server, client Python và client Go tính ra CÙNG một kết quả (không sai số dấu phẩy động).
//
// Luật chơi: mỗi tick (50 ms) xe nhận một đầu vào: 1 = ga, 0 = nhả ga, -1 = phanh.
// Đoạn đường từ ô 20 đến ô 26 là khúc cua: đi vào khúc cua nhanh hơn 0,50 ô/tick thì xe bị văng (tụt tốc độ).
package main

import (
	"fmt"
	"strings"
)

const (
	tickMs    = 50   // server mô phỏng 20 tick mỗi giây
	start     = 20   // đếm ngược 20 tick (1 s) trước khi xuất phát
	track     = 4000 // vạch đích
	zoneStart = 2000 // khúc cua
	zoneEnd   = 2600
	vCua      = 50 // tốc độ tối đa an toàn trong khúc cua
	vVang     = 10 // tốc độ còn lại sau khi văng
	acc       = 6
	drag      = 2
	brake     = 12
	vMax      = 90
	maxRace   = 600 // giới hạn 30 s
	lead      = 2   // client gửi đầu vào cho tick k+2 khi nhận ảnh chụp tick k (bù trễ, có dư thời gian)
	window    = 3   // mỗi gói đầu vào mang kèm 3 đầu vào gần nhất (dư thừa chống mất gói)
	endTicks  = 10  // sau khi đua xong, server gửi thêm 10 ảnh chụp kết quả rồi mới dừng
)

var inputText = map[int]string{1: "ga", 0: "nhả", -1: "phanh"}

// step – một tick vật lý; trả (x, v, có_văng). Server và client dùng chung hàm này.
func step(x, v, inp int) (int, int, bool) {
	switch {
	case inp > 0:
		v = min(v+acc, vMax)
	case inp < 0:
		v = max(v-brake, 0)
	default:
		v = max(v-drag, 0)
	}
	x += v
	crashed := x >= zoneStart && x < zoneEnd && v > vCua
	if crashed {
		v = vVang
	}
	return x, v, crashed
}

// lossy – giả lập mất gói: true nếu gói (hướng, xe, số thứ tự) bị "mất".
// Quyết định là hàm băm của các tham số, không phụ thuộc thời điểm, nên cùng seed cho cùng các gói bị mất
// ở mọi lần chạy và ở cả hai bản Python, Go.
func lossy(seed int64, direction, car, key, pct int) bool {
	if pct <= 0 {
		return false
	}
	h := uint32(seed)*2654435761 + uint32(direction)*40503 + uint32(car)*9973 + uint32(key)*2246822519
	h ^= h >> 16
	h *= 0x45D9F3B
	h ^= h >> 16
	h *= 0x45D9F3B
	h ^= h >> 16
	return int(h%100) < pct
}

// ---------------------------------------------------------------- bot

// styles – (khoảng an toàn trước khúc cua, tốc độ trong cua).
var styles = map[string][2]int{"lieu": {0, vCua}, "can_than": {300, vCua - 10}}

// wouldCrash – nếu tick này dùng inp rồi phanh liên tục, xe có vào khúc cua quá nhanh không?
func wouldCrash(x, v, inp, zs, limit int) bool {
	switch inp {
	case 1:
		v = min(v+acc, vMax)
	case 0:
		v = max(v-drag, 0)
	default:
		v = max(v-brake, 0)
	}
	x += v
	for {
		if x >= zoneEnd {
			return false
		}
		if x >= zs && v > limit {
			return true
		}
		if v <= limit {
			return false
		}
		v = max(v-brake, 0)
		x += v
	}
}

// botInput – ga nhiều nhất có thể mà vẫn kịp phanh trước khúc cua.
func botInput(x, v int, style string) int {
	safety, limit := styles[style][0], styles[style][1]
	zs := zoneStart - safety
	if x >= zs && x < zoneEnd { // đang ở khúc cua (hoặc vùng an toàn trước nó)
		if v+acc <= limit {
			return 1
		}
		if v-drag <= limit {
			return 0
		}
		return -1
	}
	for _, inp := range []int{1, 0, -1} {
		if !wouldCrash(x, v, inp, zs, limit) {
			return inp
		}
	}
	return -1
}

// ---------------------------------------------------------------- hiển thị

// fmt2 – số nguyên 1/100 ô -> chuỗi có hai chữ số thập phân (ví dụ 1234 -> "12.34").
func fmt2(n int) string { return fmt.Sprintf("%d.%02d", n/100, n%100) }

// trackLine – vẽ một làn đua 40 ký tự: '~' là khúc cua, '|' là vạch đích.
func trackLine(letter string, x int) string {
	pos := min(39, x/100)
	var b strings.Builder
	for i := 0; i < 40; i++ {
		switch {
		case i == pos:
			b.WriteString(letter)
		case i >= zoneStart/100 && i < zoneEnd/100:
			b.WriteByte('~')
		default:
			b.WriteByte('.')
		}
	}
	b.WriteByte('|')
	return b.String()
}
