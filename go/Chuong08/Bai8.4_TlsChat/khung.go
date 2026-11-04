// Bài 8.4 – Gửi, nhận khung JSON (chép gọn Mã giả 5.1, kế thừa Mã giả 3.6).
//
// Khung = 4 byte độ dài (big-endian) + JSON UTF-8. Logic không đổi khi chạy trên TLS:
// *tls.Conn cũng là net.Conn (io.Reader, io.Writer) nên đọc ghi giống trên socket gốc (Mục 8.4.5).
package main

import (
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
)

const MAX_FRAME = 1048576 // 1 MiB

// recvJSON – RECV_JSON_ASYNC: io.ReadFull = RECV_EXACT_ASYNC; kết nối đóng -> io.EOF / io.ErrUnexpectedEOF.
func recvJSON(r io.Reader) (map[string]any, error) {
	head := make([]byte, 4)
	if _, err := io.ReadFull(r, head); err != nil {
		return nil, err
	}
	n := binary.BigEndian.Uint32(head) // FROM_UINT32_BE
	if n > MAX_FRAME {
		return nil, errors.New("Khung quá lớn") // ProtocolError
	}
	data := make([]byte, n)
	if _, err := io.ReadFull(r, data); err != nil {
		return nil, err
	}
	var obj map[string]any
	if err := json.Unmarshal(data, &obj); err != nil || obj == nil {
		return nil, errors.New("Thông điệp không phải đối tượng JSON")
	}
	return obj, nil
}

// sendJSON – SEND_JSON_ASYNC: ghép tiêu đề độ dài với nội dung rồi gửi một lần.
func sendJSON(w io.Writer, obj any) error {
	p, err := json.Marshal(obj)
	if err != nil {
		return err
	}
	_, err = w.Write(append(binary.BigEndian.AppendUint32(nil, uint32(len(p))), p...))
	return err
}
