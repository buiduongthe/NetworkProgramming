package main

// Ghi lại khoảng thời gian chạy của từng tác vụ và vẽ biểu đồ Gantt bằng văn bản.
//
// Mỗi hàng của biểu đồ là một worker (goroutine, nhóm worker); mỗi cột là một khoảng thời gian
// bằng nhau, ký tự lấy theo thời điểm giữa cột:
//
//	chữ cái  = đúng một tác vụ đang chạy trên worker đó (A = tác vụ 1, B = tác vụ 2, ...)
//	*        = nhiều tác vụ cùng lúc trên một hàng (ví dụ nhiều goroutine cùng chờ mạng)

import (
	"fmt"
	"sort"
	"strings"
	"sync"
	"time"
)

const (
	cols    = 48
	letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
)

type seg struct {
	task   int
	worker string
	a, b   float64 // giây kể từ lúc bắt đầu ghi
}

type Recorder struct {
	t0   time.Time
	mu   sync.Mutex
	segs []seg
}

func NewRecorder() *Recorder { return &Recorder{t0: time.Now()} }

func (r *Recorder) Add(task int, worker string, start, end time.Time) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.segs = append(r.segs, seg{task, worker, start.Sub(r.t0).Seconds(), end.Sub(r.t0).Seconds()})
}

func (r *Recorder) Total() float64 {
	t := 0.0
	for _, s := range r.segs {
		t = max(t, s.b)
	}
	return t
}

// Names – đặt tên dễ đọc cho worker theo thứ tự xuất hiện: goroutine-1, goroutine-2, ...
type Names struct {
	mu     sync.Mutex
	prefix string
	n      int
}

func (nm *Names) Next() string {
	nm.mu.Lock()
	defer nm.mu.Unlock()
	nm.n++
	return fmt.Sprintf("%s-%d", nm.prefix, nm.n)
}

func gantt(title string, r *Recorder, scale float64, kind string) {
	const maxRows = 8
	fmt.Printf("%s: %.2f s\n", title, r.Total())
	segs := append([]seg(nil), r.segs...)
	sort.SliceStable(segs, func(i, j int) bool { return segs[i].a < segs[j].a })
	var workers []string
	for _, s := range segs {
		found := false
		for _, w := range workers {
			found = found || w == s.worker
		}
		if !found {
			workers = append(workers, s.worker)
		}
	}
	for i, w := range workers {
		if i == maxRows {
			break
		}
		var b strings.Builder
		for c := range cols {
			lo, hi := scale*float64(c)/cols, scale*float64(c+1)/cols
			mid := (lo + hi) / 2
			var now, on []seg
			for _, s := range segs {
				if s.worker != w {
					continue
				}
				if s.a <= mid && mid < s.b {
					now = append(now, s) // đang chạy giữa cột
				}
				if s.a < hi && s.b > lo {
					on = append(on, s)
				}
			}
			switch {
			case len(now) >= 2:
				b.WriteString("*") // nhiều tác vụ cùng lúc
			case len(now) == 1:
				b.WriteByte(letters[now[0].task])
			case len(on) > 0: // tác vụ ngắn hơn một cột
				b.WriteByte(letters[on[0].task])
			default:
				b.WriteString(" ")
			}
		}
		fmt.Printf("  %-12s|%s|\n", w, b.String())
	}
	if len(workers) > maxRows {
		fmt.Printf("  … và %d %s khác, mỗi %s chạy 1 tác vụ\n", len(workers)-maxRows, kind, kind)
	}
}

func axis(scale float64) {
	fmt.Printf("  %-12s 0%*s\n", "", cols, fmt.Sprintf("%.1f s", scale))
}

type result struct {
	name string
	rec  *Recorder
	kind string
}

func show(results []result) {
	scale := 0.0
	for _, r := range results {
		scale = max(scale, r.rec.Total())
	}
	fmt.Println()
	for _, r := range results {
		gantt(r.name, r.rec, scale, r.kind)
	}
	axis(scale)
	fmt.Println()
	base := results[0].rec.Total()
	fmt.Printf("  %-30s %9s %9s\n", "Mô hình", "Thời gian", "Tăng tốc")
	for _, r := range results {
		t := r.rec.Total()
		fmt.Printf("  %-30s %7.2f s %8.1f×\n", r.name, t, base/t)
	}
	fmt.Println()
}
