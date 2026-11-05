package main

// Nhật ký theo goroutine.
//
// Mỗi dòng: "[  0.123 s] tải-1 yc=1      | nội dung".
// Go KHÔNG có tên luồng hay biến cục bộ luồng (thread-local): goroutine không có danh tính mà chương trình
// đọc được, và có thể chạy trên luồng hệ điều hành khác nhau theo thời gian. Cách làm của Go là truyền
// tường minh một giá trị ngữ cảnh – ở đây là *NhatKy (tên + ngữ cảnh "yc=...") – cho goroutine; với
// chương trình lớn thường dùng context.Context hoặc log/slog với thuộc tính.

import (
	"fmt"
	"strings"
	"sync"
	"time"
)

var (
	t0     = time.Now()
	khoaIn sync.Mutex // không cho hai goroutine in xen lẫn trên một dòng
)

// NhatKy – "danh tính" của một goroutine trong nhật ký.
type NhatKy struct {
	Ten     string
	NguCanh string
}

func nk(ten string) *NhatKy { return &NhatKy{Ten: ten} }

func bayGio() float64 { return time.Since(t0).Seconds() }

// Log in một dòng có thời điểm, tên goroutine và ngữ cảnh.
func (n *NhatKy) Log(format string, a ...any) {
	ai := strings.TrimSpace(n.Ten + " " + n.NguCanh)
	khoaIn.Lock()
	defer khoaIn.Unlock()
	fmt.Printf("[%7.3f s] %-16s| %s\n", bayGio(), ai, fmt.Sprintf(format, a...))
}

// say in một dòng bình thường (tiêu đề, bảng).
func say(format string, a ...any) {
	khoaIn.Lock()
	defer khoaIn.Unlock()
	fmt.Printf(format+"\n", a...)
}
