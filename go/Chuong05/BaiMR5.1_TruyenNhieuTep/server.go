// Server truyền nhiều tệp: mỗi kết nối phục vụ một lượt tải xuống/tải lên (xem khung.go).
// Server KHÔNG tự giới hạn tốc độ – client điều tiết, TCP tự tạo áp lực ngược.
package main

import (
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
)

func phucVu(conn net.Conn, kho string) {
	defer conn.Close()
	req, err := nhanKhung(conn)
	if err != nil {
		return
	}
	name := filepath.Base(req.Name)
	switch req.Op {
	case "download":
		path := filepath.Join(kho, name)
		info, err := os.Stat(path)
		if err != nil || info.IsDir() {
			guiKhung(conn, Khung{OK: false, Loi: "Không có tệp " + name})
			return
		}
		guiKhung(conn, Khung{OK: true, Size: info.Size(), Sha256: sha256Tep(path)})
		f, err := os.Open(path)
		if err != nil {
			return
		}
		defer f.Close()
		f.Seek(req.Offset, io.SeekStart)
		io.Copy(conn, f) // client chậm -> TCP áp lực ngược làm server gửi chậm theo
	case "upload":
		part := filepath.Join(kho, name+".part")
		guiKhung(conn, Khung{OK: true})
		flag := os.O_CREATE | os.O_WRONLY
		if req.Offset == 0 {
			flag |= os.O_TRUNC
		}
		f, err := os.OpenFile(part, flag, 0o644)
		if err != nil {
			return
		}
		f.Seek(req.Offset, io.SeekStart)
		nhan := req.Offset
		buf := make([]byte, chunk)
		for nhan < req.Size {
			want := int64(chunk)
			if req.Size-nhan < want {
				want = req.Size - nhan
			}
			m, err := io.ReadFull(conn, buf[:want])
			if m > 0 {
				f.Write(buf[:m])
				nhan += int64(m)
			}
			if err != nil {
				break
			}
		}
		f.Close()
		shaOK := sha256Tep(part) == req.Sha256
		if shaOK {
			os.Rename(part, filepath.Join(kho, name))
		}
		guiKhung(conn, Khung{OK: true, Sha256OK: shaOK})
	default:
		guiKhung(conn, Khung{OK: false, Loi: "op lạ"})
	}
}

func chayServer(host string, port int, kho string) {
	os.MkdirAll(kho, 0o755)
	ln, err := net.Listen("tcp", fmt.Sprintf("%s:%d", host, port))
	if err != nil {
		fmt.Println("Không mở được listener:", err)
		os.Exit(1)
	}
	fmt.Printf("[server] Nghe tại %s:%d, thư mục lưu: %s\n", host, port, kho)
	for {
		conn, err := ln.Accept()
		if err != nil {
			return
		}
		go phucVu(conn, kho)
	}
}
