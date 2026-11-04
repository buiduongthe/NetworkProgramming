package main

// Tranh chấp dữ liệu (race condition), khóa, thao tác nguyên tử và tránh bế tắc (deadlock).
//
//  1. Tranh chấp: 4 goroutine cùng "đọc – nhường CPU – ghi" lên một biến đếm chung; runtime.Gosched()
//     (giống gọi mạng hay ghi nhật ký giữa chừng) cho goroutine khác chen vào → mất lần cộng.
//     Sửa bằng sync.Mutex, hoặc bằng atomic.Int64 (cộng trong một lệnh, không cần khóa).
//     Công cụ `go run -race .` phát hiện được lỗi này (cần cgo).
//  2. Bế tắc: T1 giữ khóa X rồi chờ Y, T2 giữ Y rồi chờ X. Khóa ở đây là kênh có bộ đệm 1 để chờ có
//     hạn bằng select; cách tránh là luôn lấy khóa theo cùng một thứ tự.

import (
	"fmt"
	"runtime"
	"sync"
	"sync/atomic"
	"time"
)

const nG, nIter = 4, 5000

func raceDemo() {
	fmt.Printf("Tranh chấp dữ liệu: %d goroutine × %d lần \"đọc – nhường CPU – ghi\"\n", nG, nIter)
	fmt.Printf("  lên một biến đếm chung (kết quả đúng phải là %d).\n", nG*nIter)
	total := int64(nG * nIter)
	run := func(body func()) { // chạy body trên nG goroutine, chờ tất cả xong
		var wg sync.WaitGroup
		for range nG {
			wg.Add(1)
			go func() { defer wg.Done(); body() }()
		}
		wg.Wait()
	}
	report := func(name string, v int64, t time.Time) {
		verdict := "đúng"
		if v != total {
			verdict = fmt.Sprintf("mất %d lần cộng", total-v)
		}
		fmt.Printf("  %-16s: %6d / %d → %s (%.2f s)\n", name, v, total, verdict, time.Since(t).Seconds())
	}

	var plain int64 // 1. không khóa
	t := time.Now()
	run(func() {
		for range nIter {
			v := plain        // đọc
			runtime.Gosched() // nhường CPU: goroutine khác có thể chen vào đây
			plain = v + 1     // ghi – có thể đè lên kết quả của goroutine khác
		}
	})
	report("không khóa", plain, t)

	var locked int64 // 2. sync.Mutex
	var mu sync.Mutex
	t = time.Now()
	run(func() {
		for range nIter {
			mu.Lock() // đoạn găng: chỉ một goroutine được vào
			v := locked
			runtime.Gosched()
			locked = v + 1
			mu.Unlock()
		}
	})
	report("có khóa (Mutex)", locked, t)

	var at atomic.Int64 // 3. thao tác nguyên tử
	t = time.Now()
	run(func() {
		for range nIter {
			at.Add(1) // đọc – cộng – ghi trong một lệnh của CPU
			runtime.Gosched()
		}
	})
	report("atomic.Int64", at.Load(), t)
}

// chLock – khóa làm bằng kênh có bộ đệm 1: gửi = lấy khóa, nhận = nhả khóa.
type chLock chan struct{}

func (l chLock) lock()   { l <- struct{}{} }
func (l chLock) unlock() { <-l }
func (l chLock) tryLock(d time.Duration) bool {
	select {
	case l <- struct{}{}:
		return true
	case <-time.After(d):
		return false
	}
}

func deadlockDemo() {
	fmt.Println()
	fmt.Println("Bế tắc (deadlock): T1 chuyển X→Y, T2 chuyển Y→X, mỗi luồng cần cả")
	fmt.Println("  hai khóa; số dư ban đầu X = 100, Y = 100.")
	bal := map[string]int{"X": 100, "Y": 100}
	locks := map[string]chLock{"X": make(chLock, 1), "Y": make(chLock, 1)}
	var outMu sync.Mutex
	say := func(format string, a ...any) {
		outMu.Lock()
		fmt.Printf("  "+format+"\n", a...)
		outMu.Unlock()
	}
	transfer := func(name, src, dst string, amount int, ordered bool) {
		first, second := src, dst
		if ordered && first > second {
			first, second = second, first
		}
		locks[first].lock()
		defer locks[first].unlock()
		say("%s giữ khóa %s, chờ khóa %s", name, first, second)
		time.Sleep(50 * time.Millisecond) // để luồng kia kịp giữ khóa còn lại
		if !locks[second].tryLock(500 * time.Millisecond) {
			say("%s: chờ %s quá 0,5 s → nghi bế tắc, nhả %s, bỏ cuộc", name, second, first)
			return
		}
		defer locks[second].unlock()
		bal[src] -= amount
		bal[dst] += amount
		say("%s: đã chuyển %d từ %s sang %s", name, amount, src, dst)
	}
	for _, ordered := range []bool{false, true} {
		if ordered {
			fmt.Println("Cách tránh: mọi luồng lấy khóa theo cùng thứ tự (X trước, Y sau):")
		}
		var wg sync.WaitGroup
		wg.Add(2)
		go func() { defer wg.Done(); transfer("T1", "X", "Y", 10, ordered) }()
		time.Sleep(10 * time.Millisecond)
		go func() { defer wg.Done(); transfer("T2", "Y", "X", 5, ordered) }()
		wg.Wait()
		fmt.Printf("  Số dư: X = %d, Y = %d (tổng %d)\n", bal["X"], bal["Y"], bal["X"]+bal["Y"])
	}
}
