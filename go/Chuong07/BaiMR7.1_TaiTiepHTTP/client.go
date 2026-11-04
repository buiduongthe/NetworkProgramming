package main

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"os"
	"strconv"
	"time"
)

// Tải tệp có tải tiếp: HEAD lấy kích thước/ETag/SHA-256; GET với "Range: bytes=<đã_có>-" và
// "If-Range: <ETag>"; kết nối đứt thì chờ lùi thời gian rồi thử lại từ chỗ dừng (Bài 5.6).
// Khác http.client của Python, Body của Go trả io.ErrUnexpectedEOF khi nhận thiếu Content-Length.

var httpClient = &http.Client{Timeout: 5 * time.Second}

func fileSize(path string) int64 {
	if fi, err := os.Stat(path); err == nil {
		return fi.Size()
	}
	return 0
}

// getRange – một lần GET; trả về số byte có trong tệp cục bộ sau lần này.
func getRange(url, path string, have int64, etag string) (int64, error) {
	req, _ := http.NewRequest(http.MethodGet, url, nil)
	if have > 0 {
		req.Header.Set("Range", fmt.Sprintf("bytes=%d-", have))
		req.Header.Set("If-Range", etag)
	}
	resp, err := httpClient.Do(req)
	if err != nil {
		return have, err
	}
	defer resp.Body.Close()
	line := fmt.Sprintf("[tai] -> %s", resp.Status)
	if resp.StatusCode == http.StatusPartialContent {
		line += ", " + resp.Header.Get("Content-Range")
	}
	fmt.Println(line)
	flag := os.O_WRONLY | os.O_CREATE
	switch resp.StatusCode {
	case http.StatusRequestedRangeNotSatisfiable: // dải không thỏa mãn được: giữ nguyên phần đã có
		return have, nil
	case http.StatusOK: // server gửi toàn bộ: ghi đè từ đầu
		flag |= os.O_TRUNC
		have = 0
	case http.StatusPartialContent:
		flag |= os.O_APPEND
	default:
		return have, fmt.Errorf("mã trạng thái không mong đợi %d", resp.StatusCode)
	}
	f, err := os.OpenFile(path, flag, 0o644)
	if err != nil {
		return have, err
	}
	got, err := io.Copy(f, resp.Body) // ghi dần ra tệp: phần đã nhận được giữ lại khi đứt
	f.Close()
	if err != nil {
		fmt.Printf("[tai] Kết nối đứt: nhận %d/%d byte của lần này\n", got, resp.ContentLength)
	}
	return have + got, nil
}

func runClient(url, path string, attempts int) {
	resp, err := httpClient.Head(url)
	if err != nil || resp.StatusCode != http.StatusOK {
		fmt.Println("[tai] HEAD thất bại:", err)
		os.Exit(1)
	}
	resp.Body.Close()
	size, etag, sha := resp.ContentLength, resp.Header.Get("ETag"), resp.Header.Get("X-Checksum-SHA256")
	fmt.Printf("[tai] HEAD: %d byte, ETag %s\n", size, etag)
	have := fileSize(path)
	for attempt := 1; attempt <= attempts && have < size; attempt++ {
		want := "toàn bộ"
		if have > 0 {
			want = "Range: bytes=" + strconv.FormatInt(have, 10) + "-"
		}
		fmt.Printf("[tai] Lần %d: đã có %d byte, xin %s\n", attempt, have, want)
		if have, err = getRange(url, path, have, etag); err != nil {
			fmt.Println("[tai] Lỗi:", err)
			have = fileSize(path)
		}
		if have < size {
			delay := 0.2 * float64(int(1)<<(attempt-1)) // lùi thời gian theo cấp số nhân
			fmt.Printf("[tai] Chờ %.1f s rồi tải tiếp\n", delay)
			time.Sleep(time.Duration(delay * float64(time.Second)))
		}
	}
	if have < size {
		fmt.Printf("[tai] THẤT BẠI sau %d lần thử: có %d/%d byte\n", attempts, have, size)
		os.Exit(1)
	}
	data, _ := os.ReadFile(path)
	sum := sha256.Sum256(data)
	digest := hex.EncodeToString(sum[:])
	ok := "SAI"
	if digest == sha {
		ok = "KHỚP"
	}
	fmt.Printf("[tai] Đủ %d/%d byte; SHA-256 %s (%s...)\n", have, size, ok, digest[:16])
}
