package main

// Phần dùng chung: hằng số, việc CPU (băm), hàm "thư viện cũ" kiểu chặn, nhật ký.
//
// Giao thức (mỗi kết nối một yêu cầu) giống bản Python:
//
//	BAM <mã> <chế_độ> <số_byte> <số_vòng> <hạn_ms>\n + dữ liệu   chế_độ: chan | tach | gioihan
//	    → "OK <mã> <16 hex đầu SHA-256>" hoặc "HETGIO <mã>"
//	CAMBIEN <mã>   → "CAMBIEN <mã> <giá trị> <tên goroutine/luồng đã giao kết quả>"
//	DATLAI         → "OK"
//	THONGKE <nhãn> → "TK <trễ nhịp ms> <số mẫu cảm biến> <trễ giao mẫu ms> <việc ở nhóm tính tối đa>"

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"sync"
	"time"
)

const (
	nYC       = 16        // số yêu cầu đồng thời mỗi đợt
	soByte    = 64 * 1024 // dữ liệu mỗi yêu cầu
	soVong    = 400       // số vòng băm
	msCu      = 20        // hàm "thư viện cũ" chặn 20 ms
	nhomTinh  = 2         // số worker tính
	gioiHan   = 4         // chế độ C: tối đa 4 yêu cầu ở nhóm tính
	nhipMs    = 10        // nhịp tim của vòng lặp
	camBienMs = 50        // chu kỳ luồng cảm biến
)

var (
	t0     = time.Now()
	khoaIn sync.Mutex
)

// duLieu – byte thứ i = i mod 251 (giống mọi bản ngôn ngữ).
func duLieu(n int) []byte {
	b := make([]byte, n)
	for i := range b {
		b[i] = byte(i % 251)
	}
	return b
}

// bam – SHA-256 của dữ liệu lặp soVong lần; kiểm tra ctx trước mỗi vòng (select với context).
// Trả về (hex16, -1) khi xong, ("", vòng đã dừng) khi bị hủy.
func bam(ctx context.Context, data []byte, vong int) (string, int) {
	h := sha256.New()
	for r := range vong {
		select {
		case <-ctx.Done(): // hạn chót đã qua hoặc bị hủy → dừng ngay
			return "", r
		default:
		}
		h.Write(data)
	}
	return hex.EncodeToString(h.Sum(nil))[:16], -1
}

// traCuuCu – giả lập hàm của thư viện CŨ chỉ có API chặn (chờ msCu ms).
func traCuuCu() { time.Sleep(msCu * time.Millisecond) }

func logAi(ai, format string, a ...any) {
	khoaIn.Lock()
	defer khoaIn.Unlock()
	fmt.Printf("[%7.3f s] %-16s| %s\n", time.Since(t0).Seconds(), ai, fmt.Sprintf(format, a...))
}

func say(format string, a ...any) {
	khoaIn.Lock()
	defer khoaIn.Unlock()
	fmt.Printf(format+"\n", a...)
}
