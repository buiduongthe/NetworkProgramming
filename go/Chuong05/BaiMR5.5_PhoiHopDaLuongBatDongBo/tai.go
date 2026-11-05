package main

// Máy tạo tải: 16 goroutine gửi 16 yêu cầu đồng thời cho từng chế độ, in bảng so sánh.

import (
	"bufio"
	"fmt"
	"net"
	"strconv"
	"strings"
	"sync"
	"time"
)

func gui(host, port, line string, data []byte) string {
	conn, err := net.DialTimeout("tcp", net.JoinHostPort(host, port), 10*time.Second)
	if err != nil {
		return "LOI"
	}
	defer conn.Close()
	conn.Write(append([]byte(line+"\n"), data...))
	s, _ := bufio.NewReader(conn).ReadString('\n')
	return strings.TrimSpace(s)
}

func runTai(host, port string) {
	data := duLieu(soByte)
	say("Máy tạo tải: %d yêu cầu đồng thời mỗi đợt, mỗi yêu cầu %d KB, băm SHA-256 %d vòng (≈ %.0f MB dữ liệu băm)",
		nYC, soByte/1024, soVong, float64(soByte*soVong)/1e6)
	cheDo := [][2]string{{"chan", "A. CPU ngay trên vòng lặp"}, {"tach", "B. đẩy sang nhóm luồng"},
		{"gioihan", "C. nhóm luồng + áp lực ngược"}}
	type hang struct {
		nhan string
		dt   float64
		tk   []string
	}
	var bang []hang
	mauHex := ""
	for _, cd := range cheDo {
		gui(host, port, "DATLAI", nil)
		kq := make([]string, nYC)
		var wg sync.WaitGroup
		t := time.Now()
		for k := 1; k <= nYC; k++ {
			wg.Add(1)
			go func(k int) {
				defer wg.Done()
				kq[k-1] = gui(host, port, fmt.Sprintf("BAM %d %s %d %d 10000", k, cd[0], soByte, soVong), data)
			}(k)
		}
		wg.Wait()
		dt := time.Since(t).Seconds()
		ok := 0
		hexSet := map[string]bool{}
		for _, x := range kq {
			if p := strings.Fields(x); len(p) == 3 && p[0] == "OK" {
				ok++
				hexSet[p[2]] = true
				if mauHex == "" {
					mauHex = p[2]
				}
			}
		}
		tk := strings.Fields(gui(host, port, "THONGKE "+cd[1], nil))
		say("Đợt %s: %d/%d OK, %.2f s, %.1f yc/s, %d giá trị băm khác nhau", cd[1], ok, nYC, dt, float64(nYC)/dt, len(hexSet))
		bang = append(bang, hang{cd[1], dt, tk})
	}
	say("")
	say("  %-18s%10s%7s%10s%10s%6s", "Chế độ", "Thời gian", "yc/s", "Trễ nhịp", "Cảm biến", "Nhóm")
	ngan := []string{"A. chặn vòng lặp", "B. nhóm luồng", "C. + áp lực ngược"} // nhãn ngắn trong bảng
	for i, h := range bang {
		tn, _ := strconv.ParseFloat(h.tk[1], 64)
		tm, _ := strconv.ParseFloat(h.tk[3], 64)
		nm, _ := strconv.Atoi(h.tk[4])
		say("  %-18s%8.2f s%7.1f%7.1f ms%7.1f ms%6d", ngan[i], h.dt, float64(nYC)/h.dt, tn, tm, nm)
	}
	say("  (Trễ nhịp, Cảm biến: độ trễ tối đa của nhịp tim 10 ms và của")
	say("   mẫu cảm biến; Nhóm: số việc ở nhóm tính cùng lúc, tối đa)")
	say("Băm kiểm tra (16 hex đầu): %s – giống nhau ở mọi bản ngôn ngữ", mauHex)
	say("Nhận xét: A chặn vòng lặp → nhịp tim, mẫu cảm biến và mọi kết nối khác đều đứng chờ; B, C giữ")
	say("  vòng lặp thông suốt; C chỉ cho %d việc vào nhóm %d luồng, phần còn lại chờ", gioiHan, nhomTinh)
	say("  bất đồng bộ ở cổng vào – hàng đợi của nhóm luồng không phình theo tải.")
	say("")
	say("(d) Hạn chót và hủy xuyên ranh giới async → luồng (chế độ tach):")
	for _, x := range [][3]int{{101, soVong, 1000}, {102, soVong * 8, 30}} {
		t := time.Now()
		tl := gui(host, port, fmt.Sprintf("BAM %d tach %d %d %d", x[0], soByte, x[1], x[2]), data)
		say("  yêu cầu %d (%d vòng, hạn %d ms): %s sau %.0f ms", x[0], x[1], x[2], strings.Fields(tl)[0],
			float64(time.Since(t).Microseconds())/1000)
	}
	say("")
	say("(c) Luồng nền giao kết quả vào vòng lặp sự kiện:")
	for _, ma := range []int{201, 202} {
		tl := strings.Fields(gui(host, port, fmt.Sprintf("CAMBIEN %d", ma), nil))
		say("  CAMBIEN %d → giá trị %s, future do luồng %s hoàn tất", ma, tl[2], tl[3])
	}
}
