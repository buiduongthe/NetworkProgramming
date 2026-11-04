package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"math/rand"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"time"
)

// Tải tệp "bất đồng bộ" có báo cáo tiến trình và hủy – Mã giả 5.6 PUT_FILE_ASYNC (Mục 5.3.4).
// Kế thừa Mã giả 3.10 (PUT_CLIENT): giữ nguyên giao thức và trình tự 4 bước; thêm hàm báo tiến trình
// onProgress (tối đa ~10 lần/giây) và tín hiệu hủy ct (context). Nếu tệp chưa tồn tại, chương trình
// tạo một tệp thử 4 MiB.

// sha256OfFileAsync – SHA256_OF_FILE_ASYNC(path, ct): đọc từng khối, kiểm tra ct giữa các khối.
func sha256OfFileAsync(ctx context.Context, path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	buf := make([]byte, 65536)
	for {
		if err := ctx.Err(); err != nil {
			return "", err // ct báo hủy
		}
		n, err := f.Read(buf)
		h.Write(buf[:n])
		if errors.Is(err, io.EOF) {
			return hex.EncodeToString(h.Sum(nil)), nil
		}
		if err != nil {
			return "", err
		}
	}
}

// PutFileAsync – Mã giả 5.6 PUT_FILE_ASYNC(s, path, on_progress, ct).
func PutFileAsync(ctx context.Context, conn net.Conn, path string, onProgress func(sent, size int64)) (map[string]any, error) {
	info, err := os.Stat(path)
	if err != nil {
		return nil, err
	}
	size := info.Size()                         // FILE_SIZE(path)
	digest, err := sha256OfFileAsync(ctx, path) // AWAIT SHA256_OF_FILE_ASYNC: tệp lớn: lâu
	if err != nil {
		return nil, err
	}
	hdr := map[string]any{"op": "put", "name": filepath.Base(path), "size": size, "sha256": digest}
	if err := SendJSONAsync(ctx, conn, hdr); err != nil {
		return nil, err
	}
	resp, err := RecvJSONAsync(ctx, conn)
	if err != nil {
		return nil, err
	}
	if GetInt(resp, "status") != 100 {
		return nil, &ProtocolError{"Server từ chối"}
	}
	f, err := os.Open(path) // OPEN_FILE(path, READ)
	if err != nil {
		return nil, err
	}
	defer f.Close() // FINALLY CLOSE_FILE(f)
	var sent int64
	last := time.Time{}
	buf := make([]byte, 65536)
	for { // LOOP
		n, rerr := f.Read(buf) // AWAIT READ_ASYNC(f, 65536, ct)
		if n > 0 {
			if err := SendAllAsync(ctx, conn, buf[:n]); err != nil { // AWAIT SEND_ALL_ASYNC: điểm có thể hủy
				return nil, err
			}
			sent += int64(n)                              // đã vào bộ đệm gửi
			if time.Since(last) >= 100*time.Millisecond { // tối đa ~10 lần/giây
				onProgress(sent, size)
				last = time.Now()
			}
		}
		if errors.Is(rerr, io.EOF) { // chunk = EMPTY_BYTES
			break
		}
		if rerr != nil {
			return nil, rerr
		}
	}
	resp, err = RecvJSONAsync(ctx, conn) // server đã nhận đủ
	if err != nil {
		return nil, err
	}
	if GetInt(resp, "status") == 200 {
		onProgress(size, size) // 100% chỉ được báo sau khi server xác nhận
	}
	return resp, nil
}

func makeTestFile(path string, mib int) error {
	data := make([]byte, mib*1048576)
	rand.New(rand.NewSource(5)).Read(data) // nội dung ngẫu nhiên cố định
	if err := os.WriteFile(path, data, 0o644); err != nil {
		return err
	}
	fmt.Printf("[client] Tạo tệp thử %s (%d MiB)\n", path, mib)
	return nil
}

func showProgress(sent, size int64) { // on_progress
	fmt.Printf("[client] Tiến trình: %.2f/%.2f MiB (%d%%)\n", float64(sent)/1048576, float64(size)/1048576, sent*100/size)
}

// RunPutClient – client [host] [port] [tệp] [hủy_sau_ms].
func RunPutClient(args []string) int {
	host, port, err := ParseEndpoint(args[:min(len(args), 2)], "127.0.0.1", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	path := "du_lieu_thu.bin"
	if len(args) > 2 {
		path = args[2]
	}
	cancelMs := 0
	if len(args) > 3 {
		if cancelMs, err = strconv.Atoi(args[3]); err != nil || cancelMs < 0 {
			fmt.Printf("Thời gian hủy không hợp lệ: %s\n", args[3])
			return 2
		}
	}
	if _, err := os.Stat(path); err != nil {
		if err := makeTestFile(path, 4); err != nil {
			fmt.Printf("[client] Không tạo được tệp thử: %v\n", err)
			return 1
		}
	}
	ctx, cancel := context.WithCancel(context.Background()) // cts: nguồn tín hiệu hủy
	defer cancel()
	conn, err := SafeConnectAsync(ctx, host, port, 3000)
	if err != nil {
		fmt.Printf("[client] Không kết nối được: %v\n", err)
		return 1
	}
	defer conn.Close() // hủy giữa chừng: đóng kết nối (Mục 5.4.1)
	// Bộ đệm gửi nhỏ (64 KiB) để số byte "đã gửi" bám sát số byte server đã nhận (minh họa)
	conn.(*net.TCPConn).SetWriteBuffer(65536)
	note := ""
	if cancelMs > 0 {
		note = fmt.Sprintf(", sẽ bấm Hủy sau %d ms", cancelMs)
		time.AfterFunc(time.Duration(cancelMs)*time.Millisecond, cancel) // nút Hủy: CANCEL(cts)
	}
	fmt.Printf("[client] Đã kết nối %s:%d, tải lên %s%s\n", host, port, path, note)
	t0 := time.Now()
	resp, err := PutFileAsync(ctx, conn, path, showProgress)
	sec := time.Since(t0).Seconds()
	switch {
	case errors.Is(err, context.Canceled):
		fmt.Printf("[client] Đã hủy sau %.2f s; đóng kết nối (trạng thái giao thức không còn tin cậy)\n", sec)
	case err != nil:
		fmt.Printf("[client] Lỗi: %v\n", err)
		return 1
	default:
		fmt.Printf("[client] Kết quả: %s sau %.2f s\n", JsonBytes(resp), sec)
	}
	return 0
}
