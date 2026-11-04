// Bài mở rộng MR6.4 – Bàn cờ Caro 15×15: kiểm tra thắng (5 quân liên tiếp), hiển thị, bot đơn giản.
//
// Server dùng fiveLine() để PHÁN XỬ (server là bên có thẩm quyền); client dùng render() để vẽ bàn cờ
// và chooseMove() khi chạy ở chế độ bot. Bot tất định (chỉ dùng bộ sinh LCG có hạt giống) nên hai bản
// Python, Go cho cùng ván đấu.
package main

import (
	"fmt"
	"sort"
	"strconv"
	"strings"
)

// N – kích thước bàn cờ.
const N = 15

const empty = '.'

// dirs – ngang, dọc, chéo xuống, chéo lên.
var dirs = [4][2]int{{0, 1}, {1, 0}, {1, 1}, {1, -1}}

type board [N][N]byte

func newBoard() *board {
	var b board
	for r := range b {
		for c := range b[r] {
			b[r][c] = empty
		}
	}
	return &b
}

func inside(r, c int) bool { return r >= 0 && r < N && c >= 0 && c < N }

// cellName: (7, 7) -> "h8": cột là chữ a–o, hàng là số 1–15.
func cellName(r, c int) string { return fmt.Sprintf("%c%d", 'a'+c, r+1) }

// parseCell: "h8" -> (7, 7); ok = false nếu sai cú pháp hoặc ngoài bàn cờ.
func parseCell(text string) (int, int, bool) {
	text = strings.ToLower(strings.TrimSpace(text))
	if len(text) < 2 || text[0] < 'a' || text[0] > 'o' {
		return 0, 0, false
	}
	for _, ch := range text[1:] {
		if ch < '0' || ch > '9' {
			return 0, 0, false
		}
	}
	n, _ := strconv.Atoi(text[1:])
	r, c := n-1, int(text[0]-'a')
	return r, c, inside(r, c)
}

// fiveLine: nếu quân vừa đặt ở (r, c) tạo thành dãy ≥ 5 quân cùng màu thì trả các ô của dãy, ngược lại nil.
func fiveLine(b *board, r, c int) [][2]int {
	mark := b[r][c]
	for _, d := range dirs {
		line := [][2]int{{r, c}}
		for _, sgn := range []int{1, -1} { // đi về hai phía của ô vừa đặt
			rr, cc := r+d[0]*sgn, c+d[1]*sgn
			for inside(rr, cc) && b[rr][cc] == mark {
				line = append(line, [2]int{rr, cc})
				rr, cc = rr+d[0]*sgn, cc+d[1]*sgn
			}
		}
		if len(line) >= 5 {
			sort.Slice(line, func(i, j int) bool {
				return line[i][0] < line[j][0] || (line[i][0] == line[j][0] && line[i][1] < line[j][1])
			})
			return line
		}
	}
	return nil
}

// render vẽ bàn cờ thành các dòng chữ; ô vừa đánh (last) được đánh dấu bằng chữ thường x/o.
func render(b *board, lastR, lastC int) []string {
	head := make([]string, N)
	for c := range head {
		head[c] = string(rune('a' + c))
	}
	lines := []string{"    " + strings.Join(head, " ")}
	for r := 0; r < N; r++ {
		cells := make([]string, N)
		for c := 0; c < N; c++ {
			v := string(b[r][c])
			if r == lastR && c == lastC {
				v = strings.ToLower(v)
			}
			cells[c] = v
		}
		lines = append(lines, fmt.Sprintf("%3d %s", r+1, strings.Join(cells, " ")))
	}
	return lines
}

// ---------------------------------------------------------------- bot

// value – giá trị của một hướng theo (độ dài dãy, số đầu còn trống): dãy 4 hở hai đầu gần như chắc thắng.
var value = map[[2]int]int64{{4, 2}: 100000, {4, 1}: 10000, {3, 2}: 5000, {3, 1}: 500,
	{2, 2}: 200, {2, 1}: 50, {1, 2}: 10, {1, 1}: 2}

// styles – kiểu bot: (hệ số tấn công, hệ số phòng thủ, độ "nhiễu"). Bot tập sự cộng thêm điểm ngẫu nhiên nên
// hay bỏ sót thế cờ nguy hiểm (nhưng vẫn luôn thắng ngay khi có thể và luôn chặn dãy 4).
var styles = map[string][3]int64{"tan_cong": {10, 7, 0}, "phong_thu": {7, 10, 0},
	"can_bang": {10, 9, 0}, "tap_su": {10, 9, 50000}}

// lcg – bộ sinh số giả ngẫu nhiên LCG (cùng công thức với MR3.3 và bản Python).
type lcg struct{ x uint32 }

func newLcg(seed int64) *lcg { return &lcg{uint32(seed) & 0x7FFFFFFF} }

func (r *lcg) nextInt(lo, hi int) int {
	r.x = (r.x*1103515245 + 12345) & 0x7FFFFFFF
	return lo + int(r.x>>8)%(hi-lo+1)
}

// lineValue – giá trị của việc đặt quân mark vào ô trống (r, c); trả -1 nếu tạo được 5 quân (thắng ngay).
func lineValue(b *board, r, c int, mark byte) int64 {
	var total int64
	for _, d := range dirs {
		run, ends := 1, 0
		for _, sgn := range []int{1, -1} {
			rr, cc := r+d[0]*sgn, c+d[1]*sgn
			for inside(rr, cc) && b[rr][cc] == mark {
				run++
				rr, cc = rr+d[0]*sgn, cc+d[1]*sgn
			}
			if inside(rr, cc) && b[rr][cc] == empty {
				ends++
			}
		}
		if run >= 5 {
			return -1
		}
		total += value[[2]int{run, ends}]
	}
	return total
}

// nearStone – ô có quân nào trong phạm vi 2 ô không (chỉ xét các ô này cho nhanh).
func nearStone(b *board, r, c int) bool {
	for rr := max(0, r-2); rr < min(N, r+3); rr++ {
		for cc := max(0, c-2); cc < min(N, c+3); cc++ {
			if b[rr][cc] != empty {
				return true
			}
		}
	}
	return false
}

func abs(x int) int {
	if x < 0 {
		return -x
	}
	return x
}

// chooseMove – thắng ngay nếu được, chặn nếu đối thủ sắp thắng, còn lại cộng điểm tấn công + phòng thủ.
func chooseMove(b *board, me byte, style string, rng *lcg) (int, int) {
	opp := byte('X')
	if me == 'X' {
		opp = 'O'
	}
	w := styles[style]
	bestR, bestC := N/2, N/2 // bàn trống: đánh vào giữa
	var bestScore int64 = -1
	bestDist := 0
	for r := 0; r < N; r++ {
		for c := 0; c < N; c++ {
			if b[r][c] != empty || !nearStone(b, r, c) {
				continue
			}
			mine, theirs := lineValue(b, r, c, me), lineValue(b, r, c, opp)
			var score int64
			switch {
			case mine < 0:
				score = 1_000_000_000_000 // thắng ngay
			case theirs < 0:
				score = 100_000_000_000 // bắt buộc chặn
			default:
				score = w[0]*mine + w[1]*theirs
				if w[2] > 0 {
					score += int64(rng.nextInt(0, int(w[2])))
				}
			}
			// hòa điểm: gần tâm hơn, rồi theo hàng, cột (duyệt r, c tăng dần nên chỉ cần so "<")
			dist := abs(r-N/2) + abs(c-N/2)
			if score > bestScore || (score == bestScore && dist < bestDist) {
				bestR, bestC, bestScore, bestDist = r, c, score, dist
			}
		}
	}
	return bestR, bestC
}
