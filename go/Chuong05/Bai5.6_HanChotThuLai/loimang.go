// Nhận biết các lỗi mạng thường gặp trên mọi hệ điều hành.
// Trên Windows, Go trả mã lỗi Winsock (WSAECONNREFUSED = 10061...) thay cho mã POSIX (ECONNREFUSED...),
// và gói syscall không có hằng số cho các mã này, nên bài so sánh cả hai.

package main

import (
	"errors"
	"syscall"
)

func isErrno(err error, posix syscall.Errno, winsock syscall.Errno) bool {
	var e syscall.Errno
	return errors.As(err, &e) && (e == posix || e == winsock)
}

// isRefused – ConnectionRefused: nhận gói RST khi kết nối.
func isRefused(err error) bool { return isErrno(err, syscall.ECONNREFUSED, 10061) }

// isReset – ConnectionReset (với UDP trên Windows: ICMP port unreachable).
func isReset(err error) bool { return isErrno(err, syscall.ECONNRESET, 10054) }

// isUnreachable – NetworkUnreachable, HostUnreachable.
func isUnreachable(err error) bool {
	return isErrno(err, syscall.ENETUNREACH, 10051) || isErrno(err, syscall.EHOSTUNREACH, 10065)
}
